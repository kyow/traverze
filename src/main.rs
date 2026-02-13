use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "traverze")]
#[command(about = "File full-text search with Tantivy")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Index {
        #[arg(long, default_value = ".traverze-index")]
        index_dir: PathBuf,
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    Search {
        #[arg(long, default_value = ".traverze-index")]
        index_dir: PathBuf,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        query: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Index { index_dir, files } => {
            let engine = traverze::Traverze::open_or_create(&index_dir)?;
            let indexed = engine.index_files(&files)?;
            println!("indexed {} file(s)", indexed);
        }
        Commands::Search {
            index_dir,
            limit,
            query,
        } => {
            let engine = traverze::Traverze::open_or_create(&index_dir)?;
            let hits = engine.search(&query, limit)?;
            for hit in hits {
                println!("{:.3}\t{}", hit.score, hit.path);
            }
        }
    }

    Ok(())
}
