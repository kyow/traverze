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
const DEFAULT_INDEX_DIR: &str = ".traverze-index";

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
    None,
    #[default]
    AnalyzeAnd,
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

impl Traverze {
    pub fn new() -> Result<Self> {
        Self::new_in_dir(Path::new(DEFAULT_INDEX_DIR))
    }

    pub fn new_in_dir(index_dir: &Path) -> Result<Self> {
        Self::new_in_dir_with_mode(index_dir, default_tokenizer_mode())
    }

    pub fn new_in_dir_with_mode(index_dir: &Path, mode: TokenizerMode) -> Result<Self> {
        Self::open_or_create(index_dir, mode, build_schema(false))
    }

    pub fn new_in_dir_for_indexing(
        index_dir: &Path,
        mode: TokenizerMode,
        with_snippet: bool,
    ) -> Result<Self> {
        let engine = Self::open_or_create(index_dir, mode, build_schema(with_snippet))?;
        if engine.supports_snippet() != with_snippet {
            let expected = if with_snippet { "enabled" } else { "disabled" };
            let actual = if engine.supports_snippet() {
                "enabled"
            } else {
                "disabled"
            };
            return Err(anyhow!(
                "index snippet support mismatch: expected {expected}, but existing index is {actual}"
            ));
        }
        Ok(engine)
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

    pub fn index_files(&self, files: &[PathBuf]) -> Result<usize> {
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

    pub fn remove_files(&self, files: &[PathBuf]) -> Result<usize> {
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

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        self.search_with_options(query, SearchOptions::with_limit(limit))
    }

    pub fn search_with_options(
        &self,
        query: &str,
        options: SearchOptions,
    ) -> Result<Vec<SearchHit>> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .context("failed to build index reader")?;
        let searcher = reader.searcher();

        let query_parser = QueryParser::for_index(&self.index, vec![self.contents_field]);
        let processed_query = preprocess_query(&self.index, query, options.query_preprocess)?;
        let parsed_query = query_parser
            .parse_query(&processed_query)
            .context("failed to parse query")?;

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

    pub fn supports_snippet(&self) -> bool {
        self.contents_is_stored
    }
}

fn preprocess_query(index: &Index, query: &str, mode: QueryPreprocess) -> Result<String> {
    match mode {
        QueryPreprocess::None => Ok(query.to_string()),
        QueryPreprocess::AnalyzeAnd => {
            let mut analyzer = index
                .tokenizers()
                .get(TOKENIZER_NAME)
                .ok_or_else(|| anyhow!("`{TOKENIZER_NAME}` tokenizer is not registered"))?;
            let mut stream = analyzer.token_stream(query);
            let mut terms = Vec::new();
            stream.process(&mut |token| {
                if !token.text.is_empty() {
                    terms.push(token.text.to_string());
                }
            });
            if terms.is_empty() {
                eprintln!(
                    "query_preprocess\tmode={mode:?}\tinput={query}\ttokens=[]\toutput={query}"
                );
                Ok(query.to_string())
            } else {
                // Build an AND query where each morphological token is expanded
                // with a character-level phrase fallback.  This handles the case
                // where the index tokenizer splits a word differently from the
                // query tokenizer due to context-dependent morphological analysis.
                //
                // For a CJK token with >1 char (e.g. "日付") we emit:
                //   (日付 OR "日 付")
                // The phrase query "日 付" matches when the index has the
                // individual characters as adjacent tokens.
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
                            format!("({term} OR \"{char_phrase}\")")
                        } else {
                            term.clone()
                        }
                    })
                    .collect();
                let and_query = expanded_parts.join(" AND ");
                eprintln!(
                    "query_preprocess\tmode={mode:?}\tinput={query}\ttokens={}\texpanded={}\toutput={and_query}",
                    terms.join("|"),
                    expanded_parts.join("|")
                );
                Ok(and_query)
            }
        }
    }
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
}
