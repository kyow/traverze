use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
#[cfg(not(feature = "tokenizer-lindera-ipadic"))]
use anyhow::bail;
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
use tantivy::tokenizer::{LowerCaser, NgramTokenizer, RemoveLongFilter, TextAnalyzer};
use tantivy::{Index, ReloadPolicy, Term, doc};

const TOKENIZER_NAME: &str = "traverze_ja";

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
}

#[derive(Clone)]
pub struct Traverze {
    index: Index,
    path_field: Field,
    contents_field: Field,
}

impl Traverze {
    pub fn open_or_create(index_dir: &Path) -> Result<Self> {
        fs::create_dir_all(index_dir)
            .with_context(|| format!("failed to create index dir: {}", index_dir.display()))?;

        let schema = build_schema();
        let index = match Index::open_in_dir(index_dir) {
            Ok(index) => index,
            Err(_) => Index::create_in_dir(index_dir, schema)
                .with_context(|| format!("failed to create index: {}", index_dir.display()))?,
        };

        register_tokenizer(&index, default_tokenizer_mode())?;
        let schema = index.schema();
        let path_field = schema
            .get_field("path")
            .map_err(|_| anyhow!("`path` field is missing in schema"))?;
        let contents_field = schema
            .get_field("contents")
            .map_err(|_| anyhow!("`contents` field is missing in schema"))?;

        Ok(Self {
            index,
            path_field,
            contents_field,
        })
    }

    pub fn index_files(&self, files: &[PathBuf]) -> Result<usize> {
        let mut writer = self
            .index
            .writer(50_000_000)
            .context("failed to create index writer")?;

        let mut count = 0usize;
        for file in files {
            if !file.is_file() {
                continue;
            }
            let abs = fs::canonicalize(file).unwrap_or_else(|_| file.clone());
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

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .context("failed to build index reader")?;
        let searcher = reader.searcher();

        let query_parser = QueryParser::for_index(&self.index, vec![self.contents_field]);
        let query = query_parser
            .parse_query(query)
            .context("failed to parse query")?;

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(limit))
            .context("failed to run search")?;

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
                hits.push(SearchHit { path, score });
            }
        }

        Ok(hits)
    }
}

fn build_schema() -> Schema {
    let mut builder = Schema::builder();
    builder.add_text_field("path", STRING | STORED);
    let text_indexing = TextFieldIndexing::default()
        .set_tokenizer(TOKENIZER_NAME)
        .set_index_option(IndexRecordOption::WithFreqsAndPositions);
    let contents_options = TextOptions::default().set_indexing_options(text_indexing);
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
