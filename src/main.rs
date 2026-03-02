use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "traverze")]
#[command(version)]
#[command(about = "Full-text search CLI built on Tantivy and Lindera")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Index files for full-text search
    Index {
        /// Path to the index directory
        #[arg(long, default_value = ".traverze-index")]
        index_dir: PathBuf,
        /// Store file contents for snippet generation
        #[arg(long, default_value_t = false)]
        with_snippet: bool,
        /// Delete and recreate the index
        #[arg(long, default_value_t = false)]
        reset: bool,
        /// Print tokenization preview while indexing (index-side)
        #[arg(long, default_value_t = false)]
        debug_index_tokens: bool,
        /// Max number of tokens to print per file when --debug-index-tokens is enabled
        #[arg(long, default_value_t = 80)]
        debug_index_token_limit: usize,
        /// Files to index
        files: Vec<PathBuf>,
    },
    /// Remove files from the index
    Remove {
        /// Path to the index directory
        #[arg(long, default_value = ".traverze-index")]
        index_dir: PathBuf,
        /// Files to remove from the index
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Search the index for a query
    Search {
        /// Path to the index directory
        #[arg(long, default_value = ".traverze-index")]
        index_dir: PathBuf,
        /// Maximum number of results to return
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Include snippet in search results
        #[arg(long, default_value_t = false)]
        with_snippet: bool,
        /// Maximum number of characters in snippet
        #[arg(long, default_value_t = 150)]
        snippet_max_chars: usize,
        /// Output format for snippets
        #[arg(long, value_enum, default_value_t = SnippetFormatArg::Text)]
        snippet_format: SnippetFormatArg,
        /// Query preprocessing mode
        #[arg(long, value_enum, default_value_t = QueryPreprocessArg::AnalyzeAnd)]
        query_preprocess: QueryPreprocessArg,
        /// Search query string
        query: String,
    },
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum SnippetFormatArg {
    Text,
    Html,
}

impl From<SnippetFormatArg> for traverze::SnippetFormat {
    fn from(value: SnippetFormatArg) -> Self {
        match value {
            SnippetFormatArg::Text => Self::Text,
            SnippetFormatArg::Html => Self::Html,
        }
    }
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum QueryPreprocessArg {
    None,
    AnalyzeAnd,
    AnalyzeOriginalOrAnd,
}

impl From<QueryPreprocessArg> for traverze::QueryPreprocess {
    fn from(value: QueryPreprocessArg) -> Self {
        match value {
            QueryPreprocessArg::None => Self::None,
            QueryPreprocessArg::AnalyzeAnd => Self::AnalyzeAnd,
            QueryPreprocessArg::AnalyzeOriginalOrAnd => Self::AnalyzeOriginalOrAnd,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Index {
            index_dir,
            with_snippet,
            reset,
            debug_index_tokens,
            debug_index_token_limit,
            files,
        } => {
            if reset && files.is_empty() {
                if index_dir.exists() {
                    fs::remove_dir_all(&index_dir)?;
                }
                println!("reset index at {}", index_dir.display());
                return Ok(());
            }
            if reset && index_dir.exists() {
                fs::remove_dir_all(&index_dir)?;
            }
            let engine_result = traverze::Traverze::new_in_dir_for_indexing(
                &index_dir,
                traverze::default_tokenizer_mode(),
                with_snippet,
            );
            let engine = match engine_result {
                Ok(engine) => engine,
                Err(err) if err.to_string().contains("index snippet support mismatch") => {
                    return Err(anyhow!(
                        "index settings do not match existing index. recreate with `traverze index --index-dir {} --reset{} <FILES...>`",
                        index_dir.display(),
                        if with_snippet { " --with-snippet" } else { "" }
                    ));
                }
                Err(err) => return Err(err),
            };
            let debug_limit = debug_index_tokens.then_some(debug_index_token_limit);
            let (indexed, elapsed) = time_block(|| engine.index_files_with_debug(&files, debug_limit))?;
            println!("indexed {} file(s)", indexed);
            eprintln!("index_time_ms\t{:.3}", elapsed_ms(elapsed));
        }
        Commands::Remove { index_dir, files } => {
            let engine = traverze::Traverze::new_in_dir(&index_dir)?;
            let (removed, elapsed) = time_block(|| engine.remove_files(&files))?;
            println!("removed {} file(s)", removed);
            eprintln!("remove_time_ms\t{:.3}", elapsed_ms(elapsed));
        }
        Commands::Search {
            index_dir,
            limit,
            with_snippet,
            snippet_max_chars,
            snippet_format,
            query_preprocess,
            query,
        } => {
            let engine = traverze::Traverze::new_in_dir(&index_dir)?;
            if with_snippet && !engine.supports_snippet() {
                return Err(anyhow!(
                    "this index does not support snippet. run `traverze index --index-dir {} --reset` and then `traverze index --index-dir {} --with-snippet <FILES...>`",
                    index_dir.display(),
                    index_dir.display()
                ));
            }
            let search_options = traverze::SearchOptions {
                limit,
                snippet: with_snippet.then_some(traverze::SnippetOptions {
                    max_num_chars: snippet_max_chars,
                    format: snippet_format.into(),
                }),
                query_preprocess: query_preprocess.into(),
            };
            let (hits, elapsed) =
                time_block(|| engine.search_with_options(&query, search_options))?;
            for hit in hits {
                if let Some(snippet) = hit.snippet {
                    let escaped = snippet
                        .replace('\r', "\\r")
                        .replace('\n', "\\n")
                        .replace('\t', "\\t");
                    println!("{:.3}\t{}\t{}", hit.score, hit.path, escaped);
                } else {
                    println!("{:.3}\t{}", hit.score, hit.path);
                }
            }
            eprintln!("search_time_ms\t{:.3}", elapsed_ms(elapsed));
        }
    }

    Ok(())
}

fn time_block<T>(f: impl FnOnce() -> Result<T>) -> Result<(T, std::time::Duration)> {
    let start = Instant::now();
    let result = f()?;
    let elapsed = start.elapsed();
    Ok((result, elapsed))
}

fn elapsed_ms(elapsed: std::time::Duration) -> f64 {
    elapsed.as_secs_f64() * 1000.0
}
