use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(not(feature = "tokenizer-lindera-ipadic"))]
use anyhow::bail;
use anyhow::{Context, Result, anyhow};
#[cfg(feature = "tokenizer-lindera-ipadic")]
use lindera::dictionary::load_dictionary;
#[cfg(feature = "tokenizer-lindera-ipadic")]
use lindera::mode::Mode;
#[cfg(feature = "tokenizer-lindera-ipadic")]
use lindera::segmenter::Segmenter;
#[cfg(feature = "tokenizer-lindera-ipadic")]
use lindera_tantivy::tokenizer::LinderaTokenizer;
use tantivy::collector::TopDocs;
use tantivy::directory::error::{OpenReadError, OpenWriteError};
use tantivy::query::{BooleanQuery, Occur, PhraseQuery, Query, QueryParser, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions, Value,
};
use tantivy::snippet::SnippetGenerator;
use tantivy::tokenizer::{LowerCaser, NgramTokenizer, RemoveLongFilter, TextAnalyzer, TokenStream};
use tantivy::{Index, IndexWriter, ReloadPolicy, TantivyError, Term, doc};

const TOKENIZER_NAME: &str = "traverze_ja";
pub const DEFAULT_INDEX_DIR: &str = ".traverze-index";

const WRITER_HEAP_SIZE: usize = 50_000_000;
/// Number of times a commit is replayed after a transient failure (issue #23).
const COMMIT_RETRIES: usize = 2;
const COMMIT_RETRY_BACKOFF: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenizerMode {
    Ngram,
    LinderaIpadic,
}

#[cfg(feature = "tokenizer-lindera-ipadic")]
pub fn default_tokenizer_mode() -> TokenizerMode {
    // Prefer Lindera when both features are enabled.
    TokenizerMode::LinderaIpadic
}

#[cfg(not(feature = "tokenizer-lindera-ipadic"))]
pub fn default_tokenizer_mode() -> TokenizerMode {
    TokenizerMode::Ngram
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub path: String,
    pub score: f32,
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnippetFormat {
    Text,
    Html,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QueryPreprocess {
    Plain,
    #[default]
    Auto,
}

#[derive(Debug, Clone, Copy)]
pub struct SnippetOptions {
    pub max_num_chars: usize,
    pub format: SnippetFormat,
}

impl Default for SnippetOptions {
    fn default() -> Self {
        Self {
            max_num_chars: 150,
            format: SnippetFormat::Text,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SearchOptions {
    pub limit: usize,
    pub snippet: Option<SnippetOptions>,
    pub query_preprocess: QueryPreprocess,
}

impl SearchOptions {
    pub fn with_limit(limit: usize) -> Self {
        Self {
            limit,
            snippet: None,
            query_preprocess: QueryPreprocess::default(),
        }
    }
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self::with_limit(20)
    }
}

#[derive(Clone)]
pub struct Traverze {
    index: Index,
    path_field: Field,
    contents_field: Field,
    contents_is_stored: bool,
}

pub struct TraverzeBuilder {
    index_dir: PathBuf,
    mode: TokenizerMode,
    with_snippet: bool,
}

impl TraverzeBuilder {
    pub fn index_dir(mut self, dir: &Path) -> Self {
        self.index_dir = dir.to_path_buf();
        self
    }

    pub fn mode(mut self, mode: TokenizerMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_snippet(mut self, enabled: bool) -> Self {
        self.with_snippet = enabled;
        self
    }

    pub fn open(self) -> Result<Traverze> {
        let engine =
            Traverze::open_or_create(&self.index_dir, self.mode, build_schema(self.with_snippet))?;
        if self.with_snippet && !engine.has_snippet() {
            return Err(anyhow!(
                "index snippet support mismatch: expected enabled, but existing index is disabled"
            ));
        }
        Ok(engine)
    }
}

impl Traverze {
    pub fn new() -> Result<Self> {
        Self::builder().open()
    }

    pub fn builder() -> TraverzeBuilder {
        TraverzeBuilder {
            index_dir: PathBuf::from(DEFAULT_INDEX_DIR),
            mode: default_tokenizer_mode(),
            with_snippet: false,
        }
    }

    fn open_or_create(index_dir: &Path, mode: TokenizerMode, schema: Schema) -> Result<Self> {
        fs::create_dir_all(index_dir)
            .with_context(|| format!("failed to create index dir: {}", index_dir.display()))?;

        let index = match Index::open_in_dir(index_dir) {
            Ok(index) => index,
            Err(_) => Index::create_in_dir(index_dir, schema)
                .with_context(|| format!("failed to create index: {}", index_dir.display()))?,
        };

        Self::from_index(index, mode)
    }

    fn from_index(index: Index, mode: TokenizerMode) -> Result<Self> {
        register_tokenizer(&index, mode)?;
        let schema = index.schema();
        let path_field = schema
            .get_field("path")
            .map_err(|_| anyhow!("`path` field is missing in schema"))?;
        let contents_field = schema
            .get_field("contents")
            .map_err(|_| anyhow!("`contents` field is missing in schema"))?;
        let contents_is_stored = schema.get_field_entry(contents_field).is_stored();

        Ok(Self {
            index,
            path_field,
            contents_field,
            contents_is_stored,
        })
    }

    pub fn index(&self, files: &[PathBuf]) -> Result<usize> {
        self.commit_with_retry(|writer| {
            let mut count = 0usize;
            for file in files {
                if !file.is_file() {
                    continue;
                }
                let abs = normalize_path(file);
                let content = fs::read_to_string(&abs)
                    .or_else(|_| fs::read(&abs).map(|b| String::from_utf8_lossy(&b).into_owned()))
                    .with_context(|| format!("failed to read file: {}", abs.display()))?;

                let path_text = abs.to_string_lossy().to_string();
                writer.delete_term(Term::from_field_text(self.path_field, &path_text));
                writer
                    .add_document(doc!(
                        self.path_field => path_text,
                        self.contents_field => content,
                    ))
                    .context("failed to add document")?;
                count += 1;
            }
            Ok(count)
        })
    }

    pub fn remove(&self, files: &[PathBuf]) -> Result<usize> {
        self.commit_with_retry(|writer| {
            let mut count = 0usize;
            for file in files {
                let abs = normalize_path(file);
                let path_text = abs.to_string_lossy().to_string();
                writer.delete_term(Term::from_field_text(self.path_field, &path_text));
                count += 1;
            }
            Ok(count)
        })
    }

    /// Runs `apply` on a fresh writer and commits, retrying the whole
    /// operation on a transient `PermissionDenied` commit failure (issue #23:
    /// on Windows, antivirus real-time scanning can briefly deny access to
    /// freshly written segment files). Replaying is safe because `index` and
    /// `remove` both issue `delete_term` before any `add_document`, making
    /// them idempotent. The failed writer must be dropped before the next
    /// attempt so its lockfile is released.
    fn commit_with_retry<F>(&self, apply: F) -> Result<usize>
    where
        F: Fn(&mut IndexWriter) -> Result<usize>,
    {
        for attempt in 0..=COMMIT_RETRIES {
            let mut writer = self
                .index
                .writer::<tantivy::schema::TantivyDocument>(WRITER_HEAP_SIZE)
                .context("failed to create index writer")?;
            let count = apply(&mut writer)?;
            match writer.commit() {
                Ok(_) => return Ok(count),
                Err(err) if attempt < COMMIT_RETRIES && is_transient_commit_error(&err) => {
                    drop(writer);
                    std::thread::sleep(COMMIT_RETRY_BACKOFF * (attempt as u32 + 1));
                }
                Err(err) => {
                    let context = if attempt > 0 {
                        format!("failed to commit index (still failing after {attempt} retries)")
                    } else {
                        "failed to commit index".to_string()
                    };
                    return Err(err).context(context);
                }
            }
        }
        unreachable!("commit_with_retry loop exits only via return")
    }

    pub fn search(&self, query: &str, options: SearchOptions) -> Result<Vec<SearchHit>> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .context("failed to build index reader")?;
        let searcher = reader.searcher();

        // Auto mode builds the query programmatically (issue #28); the raw
        // string only reaches the parser in plain mode or when the query
        // analyzes to no tokens.
        let auto_query = match options.query_preprocess {
            QueryPreprocess::Auto => build_auto_query(&self.index, self.contents_field, query)?,
            QueryPreprocess::Plain => None,
        };
        let parsed_query = match auto_query {
            Some(built) => built,
            None => {
                let query_parser = QueryParser::for_index(&self.index, vec![self.contents_field]);
                let (parsed, parse_errors) = query_parser.parse_query_lenient(query);
                if !parse_errors.is_empty() {
                    eprintln!("warning: query parse errors (ignored): {:?}", parse_errors);
                }
                parsed
            }
        };

        let top_docs = searcher
            .search(&parsed_query, &TopDocs::with_limit(options.limit))
            .context("failed to run search")?;

        let mut snippet_generator = if let Some(snippet_options) = options.snippet {
            if !self.contents_is_stored {
                return Err(anyhow!(
                    "snippet is not available for this index. recreate index with snippet storage enabled"
                ));
            }
            let mut generator =
                SnippetGenerator::create(&searcher, &*parsed_query, self.contents_field)
                    .context("failed to create snippet generator")?;
            generator.set_max_num_chars(snippet_options.max_num_chars);
            Some((generator, snippet_options.format))
        } else {
            None
        };

        let mut hits = Vec::with_capacity(top_docs.len());
        for (score, doc_addr) in top_docs {
            let retrieved = searcher
                .doc::<tantivy::schema::TantivyDocument>(doc_addr)
                .context("failed to load document")?;
            let path = retrieved
                .get_first(self.path_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !path.is_empty() {
                let snippet = snippet_generator.as_mut().map(|(generator, format)| {
                    let snippet = generator.snippet_from_doc(&retrieved);
                    match format {
                        SnippetFormat::Text => snippet.fragment().to_string(),
                        SnippetFormat::Html => snippet.to_html(),
                    }
                });
                hits.push(SearchHit {
                    path,
                    score,
                    snippet,
                });
            }
        }

        Ok(hits)
    }

    pub fn list(&self) -> Result<Vec<String>> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .context("failed to build index reader")?;
        let searcher = reader.searcher();

        let mut paths = Vec::new();
        for segment_reader in searcher.segment_readers() {
            let store_reader = segment_reader
                .get_store_reader(0)
                .context("failed to open store reader")?;
            for doc_id in 0..segment_reader.max_doc() {
                if segment_reader.is_deleted(doc_id) {
                    continue;
                }
                let doc: tantivy::schema::TantivyDocument = store_reader
                    .get(doc_id)
                    .context("failed to load document")?;
                if let Some(path) = doc.get_first(self.path_field).and_then(|v| v.as_str()) {
                    if !path.is_empty() {
                        paths.push(path.to_string());
                    }
                }
            }
        }

        paths.sort();
        Ok(paths)
    }

    pub fn has_snippet(&self) -> bool {
        self.contents_is_stored
    }
}

/// Builds the `QueryPreprocess::Auto` query directly as a `BooleanQuery`
/// instead of assembling a query-language string for re-parsing (issue #28),
/// so no quoting/escaping layer sits between the analyzed tokens and the
/// executed query and token text is always matched literally.
///
/// Each morphological token becomes a `Must` clause, expanded with a
/// character-level phrase fallback. This handles the case where the index
/// tokenizer splits a word differently from the query tokenizer due to
/// context-dependent morphological analysis: for a CJK token with >1 char
/// (e.g. "日付") the clause is `term("日付") OR phrase(["日", "付"])`, and
/// the phrase matches when the index has the individual characters as
/// adjacent tokens.
///
/// Returns `None` when the query analyzes to no tokens; the caller falls
/// back to parsing the raw query string, as before.
fn build_auto_query(index: &Index, field: Field, query: &str) -> Result<Option<Box<dyn Query>>> {
    let mut analyzer = index
        .tokenizers()
        .get(TOKENIZER_NAME)
        .ok_or_else(|| anyhow!("`{TOKENIZER_NAME}` tokenizer is not registered"))?;
    let mut stream = analyzer.token_stream(query);
    let mut terms = Vec::new();
    stream.process(&mut |token| {
        // Drop tokens containing whitespace. The ngram tokenizer emits
        // grams spanning word boundaries (e.g. "c d" for "abc def");
        // requiring them would turn a multi-word query into a substring
        // search, while the per-word grams alone give the intended
        // "AND across words" semantics.
        if !token.text.is_empty() && !token.text.chars().any(char::is_whitespace) {
            terms.push(token.text.to_string());
        }
    });
    if terms.is_empty() {
        return Ok(None);
    }
    let clauses = terms
        .iter()
        .map(|text| {
            let term_query: Box<dyn Query> = Box::new(TermQuery::new(
                Term::from_field_text(field, text),
                IndexRecordOption::WithFreqs,
            ));
            let chars: Vec<char> = text.chars().collect();
            if chars.len() > 1 && chars.iter().all(|c| is_cjk_like(*c)) {
                let char_phrase = PhraseQuery::new(
                    chars
                        .iter()
                        .map(|c| Term::from_field_text(field, &c.to_string()))
                        .collect(),
                );
                let expanded = BooleanQuery::new(vec![
                    (Occur::Should, term_query),
                    (Occur::Should, Box::new(char_phrase) as Box<dyn Query>),
                ]);
                (Occur::Must, Box::new(expanded) as Box<dyn Query>)
            } else {
                (Occur::Must, term_query)
            }
        })
        .collect();
    Ok(Some(Box::new(BooleanQuery::new(clauses))))
}

/// Returns `true` for commit errors caused by a transient
/// `PermissionDenied`, e.g. an antivirus scanner briefly holding a freshly
/// written segment file on Windows (issue #23). Tantivy's error variants do
/// not expose the underlying `io::Error` via `source()`, so the variants
/// carrying one are matched directly.
fn is_transient_commit_error(err: &TantivyError) -> bool {
    let kind = match err {
        TantivyError::IoError(io_error) => Some(io_error.kind()),
        TantivyError::OpenWriteError(OpenWriteError::IoError { io_error, .. })
        | TantivyError::OpenReadError(OpenReadError::IoError { io_error, .. }) => {
            Some(io_error.kind())
        }
        _ => None,
    };
    kind == Some(io::ErrorKind::PermissionDenied)
}

/// Returns `true` for CJK ideographs, Hiragana, and Katakana characters
/// that are likely to appear as individual tokens in a morphological index.
fn is_cjk_like(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{309F}'   // Hiragana
        | '\u{30A0}'..='\u{30FF}' // Katakana
        | '\u{4E00}'..='\u{9FFF}' // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}' // CJK Extension A
        | '\u{F900}'..='\u{FAFF}' // CJK Compatibility Ideographs
        | '\u{FF65}'..='\u{FF9F}' // Halfwidth Katakana
    )
}

fn normalize_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
    })
}

fn build_schema(with_snippet: bool) -> Schema {
    let mut builder = Schema::builder();
    builder.add_text_field("path", STRING | STORED);
    let text_indexing = TextFieldIndexing::default()
        .set_tokenizer(TOKENIZER_NAME)
        .set_index_option(IndexRecordOption::WithFreqsAndPositions);
    let contents_options = if with_snippet {
        TextOptions::default()
            .set_stored()
            .set_indexing_options(text_indexing)
    } else {
        TextOptions::default().set_indexing_options(text_indexing)
    };
    builder.add_text_field("contents", contents_options);
    builder.build()
}

fn register_tokenizer(index: &Index, mode: TokenizerMode) -> Result<()> {
    match mode {
        TokenizerMode::Ngram => {
            let analyzer = TextAnalyzer::builder(NgramTokenizer::new(2, 3, false)?)
                .filter(RemoveLongFilter::limit(40))
                .filter(LowerCaser)
                .build();
            index.tokenizers().register(TOKENIZER_NAME, analyzer);
            Ok(())
        }
        TokenizerMode::LinderaIpadic => {
            #[cfg(feature = "tokenizer-lindera-ipadic")]
            {
                let dictionary = load_dictionary("embedded://ipadic")
                    .context("failed to load Lindera IPADIC dictionary")?;
                let segmenter = Segmenter::new(Mode::Normal, dictionary, None);
                let tokenizer = LinderaTokenizer::from_segmenter(segmenter);
                index.tokenizers().register(TOKENIZER_NAME, tokenizer);
                Ok(())
            }
            #[cfg(not(feature = "tokenizer-lindera-ipadic"))]
            {
                bail!(
                    "Lindera tokenizer is not enabled. Build with `--features tokenizer-lindera-ipadic`."
                )
            }
        }
    }
}

#[cfg(test)]
mod tests;
