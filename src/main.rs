use std::path::PathBuf;
use std::time::Instant;

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
    Remove {
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
            let engine = traverze::Traverze::new_in_dir(&index_dir)?;
            let (indexed, elapsed) = time_block(|| engine.index_files(&files))?;
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
            query,
        } => {
            let engine = traverze::Traverze::new_in_dir(&index_dir)?;
            let (hits, elapsed) = time_block(|| engine.search(&query, limit))?;
            for hit in hits {
                println!("{:.3}\t{}", hit.score, hit.path);
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
