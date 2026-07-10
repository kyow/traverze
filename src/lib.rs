use std::fs;
use std::path::{Path, PathBuf};

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
use tantivy::query::QueryParser;
use tantivy::schema::{
    Field, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions, Value,
};
use tantivy::snippet::SnippetGenerator;
use tantivy::tokenizer::{LowerCaser, NgramTokenizer, RemoveLongFilter, TextAnalyzer, TokenStream};
use tantivy::{Index, ReloadPolicy, Term, doc};

const TOKENIZER_NAME: &str = "traverze_ja";
pub const DEFAULT_INDEX_DIR: &str = ".traverze-index";

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
        let mut writer = self
            .index
            .writer::<tantivy::schema::TantivyDocument>(50_000_000)
            .context("failed to create index writer")?;

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

        writer.commit().context("failed to commit index")?;
        Ok(count)
    }

    pub fn remove(&self, files: &[PathBuf]) -> Result<usize> {
        let mut writer = self
            .index
            .writer::<tantivy::schema::TantivyDocument>(50_000_000)
            .context("failed to create index writer")?;

        let mut count = 0usize;
        for file in files {
            let abs = normalize_path(file);
            let path_text = abs.to_string_lossy().to_string();
            writer.delete_term(Term::from_field_text(self.path_field, &path_text));
            count += 1;
        }

        writer.commit().context("failed to commit index")?;
        Ok(count)
    }

    pub fn search(&self, query: &str, options: SearchOptions) -> Result<Vec<SearchHit>> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .context("failed to build index reader")?;
        let searcher = reader.searcher();

        let query_parser = QueryParser::for_index(&self.index, vec![self.contents_field]);
        let processed_query = preprocess_query(&self.index, query, options.query_preprocess)?;
        let (parsed_query, parse_errors) = query_parser.parse_query_lenient(&processed_query);
        if !parse_errors.is_empty() {
            eprintln!("warning: query parse errors (ignored): {:?}", parse_errors);
        }

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

fn preprocess_query(index: &Index, query: &str, mode: QueryPreprocess) -> Result<String> {
    match mode {
        QueryPreprocess::Plain => Ok(query.to_string()),
        QueryPreprocess::Auto => {
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
                // eprintln!(
                //     "query_preprocess\tmode={mode:?}\tinput={query}\ttokens=[]\toutput={query}"
                // );
                Ok(query.to_string())
            } else {
                // Build an AND query where each morphological token is expanded
                // with a character-level phrase fallback.  This handles the case
                // where the index tokenizer splits a word differently from the
                // query tokenizer due to context-dependent morphological analysis.
                //
                // For a CJK token with >1 char (e.g. "日付") we emit:
                //   ("日付" OR "日 付")
                // The phrase query "日 付" matches when the index has the
                // individual characters as adjacent tokens.
                //
                // Every token is emitted double-quoted (with `\` and `"`
                // escaped): a quoted token that analyzes to a single term
                // parses as a plain term query, and quoting keeps embedded
                // Tantivy syntax characters (`:`, `(`, `-`, ...) and reserved
                // keywords (AND, OR, ...) from being parsed as query structure.
                let expanded_parts: Vec<String> = terms
                    .iter()
                    .map(|term| {
                        let chars: Vec<char> = term.chars().collect();
                        if chars.len() > 1 && chars.iter().all(|c| is_cjk_like(*c)) {
                            let char_phrase = chars
                                .iter()
                                .map(|c| c.to_string())
                                .collect::<Vec<_>>()
                                .join(" ");
                            format!("(\"{term}\" OR \"{char_phrase}\")")
                        } else {
                            format!("\"{}\"", escape_for_phrase(term))
                        }
                    })
                    .collect();
                let and_query = expanded_parts.join(" AND ");
                // eprintln!(
                //     "query_preprocess\tmode={mode:?}\tinput={query}\ttokens={}\texpanded={}\toutput={and_query}",
                //     terms.join("|"),
                //     expanded_parts.join("|")
                // );
                Ok(and_query)
            }
        }
    }
}

/// Escapes `\` and `"` so an arbitrary token can be embedded inside a
/// double-quoted Tantivy phrase without terminating or altering it.
fn escape_for_phrase(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
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
mod tests {
    fn engine_with_docs(
        dir: &std::path::Path,
        mode: crate::TokenizerMode,
        docs: &[(&str, &str)],
    ) -> crate::Traverze {
        let engine = crate::Traverze::builder()
            .index_dir(&dir.join("index"))
            .mode(mode)
            .open()
            .unwrap();
        let files: Vec<std::path::PathBuf> = docs
            .iter()
            .map(|(name, content)| {
                let path = dir.join(name);
                std::fs::write(&path, content).unwrap();
                path
            })
            .collect();
        engine.index(&files).unwrap();
        engine
    }

    fn search_names(
        engine: &crate::Traverze,
        query: &str,
        mode: crate::QueryPreprocess,
    ) -> Vec<String> {
        let options = crate::SearchOptions {
            limit: 10,
            snippet: None,
            query_preprocess: mode,
        };
        let mut names: Vec<String> = engine
            .search(query, options)
            .unwrap()
            .iter()
            .map(|hit| {
                std::path::Path::new(&hit.path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        names.sort();
        names
    }

    #[cfg(not(feature = "tokenizer-lindera-ipadic"))]
    #[test]
    fn default_mode_is_ngram_without_lindera_feature() {
        assert_eq!(crate::default_tokenizer_mode(), crate::TokenizerMode::Ngram);
    }

    #[cfg(feature = "tokenizer-lindera-ipadic")]
    #[test]
    fn default_mode_is_lindera_with_feature() {
        assert_eq!(
            crate::default_tokenizer_mode(),
            crate::TokenizerMode::LinderaIpadic
        );
    }

    #[test]
    fn list_files_empty_index() {
        let dir = tempfile::tempdir().unwrap();
        let engine = crate::Traverze::builder()
            .index_dir(dir.path())
            .mode(crate::TokenizerMode::Ngram)
            .open()
            .unwrap();
        let files = engine.list().unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn list_files_returns_indexed_paths() {
        let dir = tempfile::tempdir().unwrap();
        let index_dir = dir.path().join("index");
        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");
        std::fs::write(&file_a, "hello world").unwrap();
        std::fs::write(&file_b, "foo bar").unwrap();

        let engine = crate::Traverze::builder()
            .index_dir(&index_dir)
            .mode(crate::TokenizerMode::Ngram)
            .open()
            .unwrap();
        let count = engine.index(&[file_a.clone(), file_b.clone()]).unwrap();
        assert_eq!(count, 2);

        let files = engine.list().unwrap();
        assert_eq!(files.len(), 2);
        // list returns sorted paths
        let canonical_a = std::fs::canonicalize(&file_a)
            .unwrap()
            .to_string_lossy()
            .to_string();
        let canonical_b = std::fs::canonicalize(&file_b)
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(files.contains(&canonical_a));
        assert!(files.contains(&canonical_b));
    }

    // Issue #24: on an ngram index, auto mode must AND the query words even
    // though the analyzer emits ngram tokens spanning word boundaries.
    #[test]
    fn auto_multiword_query_is_and_on_ngram_index() {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine_with_docs(
            dir.path(),
            crate::TokenizerMode::Ngram,
            &[
                ("a.txt", "abc xyz"),
                ("b.txt", "def xyz"),
                ("c.txt", "abc def xyz"),
            ],
        );

        let auto = search_names(&engine, "abc def", crate::QueryPreprocess::Auto);
        assert_eq!(auto, vec!["c.txt"]);

        // plain mode keeps Tantivy's default OR semantics
        let plain = search_names(&engine, "abc def", crate::QueryPreprocess::Plain);
        assert_eq!(plain, vec!["a.txt", "b.txt", "c.txt"]);
    }

    // Issue #24: the AND/OR/... operators inside the user query are tokenized
    // like any other word and must be matched literally, not parsed as syntax.
    #[test]
    fn auto_and_keyword_is_literal_on_ngram_index() {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine_with_docs(
            dir.path(),
            crate::TokenizerMode::Ngram,
            &[
                ("plain.txt", "search replace tool"),
                ("with_and.txt", "search and replace tool"),
            ],
        );

        let hits = search_names(&engine, "search AND replace", crate::QueryPreprocess::Auto);
        assert_eq!(hits, vec!["with_and.txt"]);
    }

    // Issue #24: Tantivy syntax characters inside tokens must not corrupt the
    // assembled query structure.
    #[test]
    fn auto_syntax_chars_do_not_corrupt_query_on_ngram_index() {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine_with_docs(
            dir.path(),
            crate::TokenizerMode::Ngram,
            &[
                ("colon.txt", "foo:bar baz"),
                ("nocolon.txt", "foo bar baz"),
                ("quoted.txt", "say \"hi\" there"),
                ("unquoted.txt", "say hi there"),
            ],
        );

        let hits = search_names(&engine, "foo:bar", crate::QueryPreprocess::Auto);
        assert_eq!(hits, vec!["colon.txt"]);

        let hits = search_names(&engine, "say \"hi\"", crate::QueryPreprocess::Auto);
        assert_eq!(hits, vec!["quoted.txt"]);
    }

    #[test]
    fn auto_cjk_query_on_ngram_index() {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine_with_docs(
            dir.path(),
            crate::TokenizerMode::Ngram,
            &[
                ("date.txt", "日付を確認する"),
                ("other.txt", "無関係な内容"),
            ],
        );

        let hits = search_names(&engine, "日付", crate::QueryPreprocess::Auto);
        assert_eq!(hits, vec!["date.txt"]);

        // AND semantics: a query with an unknown word must not match
        let hits = search_names(&engine, "日付 未知語ワード", crate::QueryPreprocess::Auto);
        assert!(hits.is_empty());
    }

    #[test]
    fn preprocess_auto_drops_cross_word_ngrams_and_quotes_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let engine = crate::Traverze::builder()
            .index_dir(dir.path())
            .mode(crate::TokenizerMode::Ngram)
            .open()
            .unwrap();

        let out = crate::preprocess_query(&engine.index, "abc def", crate::QueryPreprocess::Auto)
            .unwrap();
        assert_eq!(
            out,
            r#""ab" AND "abc" AND "bc" AND "de" AND "def" AND "ef""#
        );

        let out =
            crate::preprocess_query(&engine.index, "a\"b", crate::QueryPreprocess::Auto).unwrap();
        assert_eq!(out, r#""a\"" AND "a\"b" AND "\"b""#);
    }

    #[cfg(feature = "tokenizer-lindera-ipadic")]
    #[test]
    fn auto_multiword_query_is_and_on_lindera_index() {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine_with_docs(
            dir.path(),
            crate::TokenizerMode::LinderaIpadic,
            &[
                ("a.txt", "abc xyz"),
                ("b.txt", "def xyz"),
                ("c.txt", "abc def xyz"),
            ],
        );

        let hits = search_names(&engine, "abc def", crate::QueryPreprocess::Auto);
        assert_eq!(hits, vec!["c.txt"]);
    }

    #[cfg(feature = "tokenizer-lindera-ipadic")]
    #[test]
    fn auto_cjk_query_on_lindera_index() {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine_with_docs(
            dir.path(),
            crate::TokenizerMode::LinderaIpadic,
            &[
                ("date.txt", "日付を確認する"),
                ("other.txt", "無関係な内容"),
            ],
        );

        let hits = search_names(&engine, "日付 確認", crate::QueryPreprocess::Auto);
        assert_eq!(hits, vec!["date.txt"]);

        let hits = search_names(&engine, "日付 未知語ワード", crate::QueryPreprocess::Auto);
        assert!(hits.is_empty());
    }

    #[test]
    fn list_files_excludes_removed() {
        let dir = tempfile::tempdir().unwrap();
        let index_dir = dir.path().join("index");
        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");
        std::fs::write(&file_a, "hello").unwrap();
        std::fs::write(&file_b, "world").unwrap();

        {
            let engine = crate::Traverze::builder()
                .index_dir(&index_dir)
                .mode(crate::TokenizerMode::Ngram)
                .open()
                .unwrap();
            engine.index(&[file_a.clone(), file_b.clone()]).unwrap();
        }
        {
            let engine = crate::Traverze::builder()
                .index_dir(&index_dir)
                .mode(crate::TokenizerMode::Ngram)
                .open()
                .unwrap();
            engine.remove(&[file_a]).unwrap();

            let files = engine.list().unwrap();
            assert_eq!(files.len(), 1);
            let canonical_b = std::fs::canonicalize(&file_b)
                .unwrap()
                .to_string_lossy()
                .to_string();
            assert_eq!(files[0], canonical_b);
        }
    }
}
