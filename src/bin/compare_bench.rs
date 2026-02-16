use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use traverze::{TokenizerMode, Traverze};

const FILE_COUNT: usize = 300;
const SEARCH_REPEAT: usize = 200;
const QUERIES: [&str; 6] = [
    "検索",
    "Rust",
    "インデックス",
    "性能",
    "tokenizer",
    "形態素解析",
];

fn main() -> Result<()> {
    if !cfg!(feature = "tokenizer-lindera-ipadic") {
        bail!(
            "このベンチは Lindera が必要です。`--features tokenizer-lindera-ipadic` を指定してください。"
        );
    }

    let base_dir = Path::new("target").join("compare-bench");
    fs::create_dir_all(&base_dir)
        .with_context(|| format!("failed to create bench dir: {}", base_dir.display()))?;

    let seeds = load_seed_texts()?;
    let files = create_bench_files(&base_dir, FILE_COUNT, &seeds)?;

    println!("mode\tindex_ms\tsearch_ms\ttotal_hits");
    run_mode(&base_dir, &files, TokenizerMode::Ngram, "ngram")?;
    run_mode(
        &base_dir,
        &files,
        TokenizerMode::LinderaIpadic,
        "lindera_ipadic",
    )?;

    Ok(())
}

fn load_seed_texts() -> Result<Vec<String>> {
    let mut files: Vec<PathBuf> = fs::read_dir("benchdata/small")
        .context("failed to read benchdata/small")?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("txt"))
        .collect();
    files.sort();

    if files.is_empty() {
        bail!("benchdata/small に *.txt がありません");
    }

    files
        .iter()
        .map(|p| fs::read_to_string(p).with_context(|| format!("failed to read {}", p.display())))
        .collect()
}

fn create_bench_files(base_dir: &Path, file_count: usize, seeds: &[String]) -> Result<Vec<PathBuf>> {
    let docs_dir = base_dir.join("docs");
    if docs_dir.exists() {
        fs::remove_dir_all(&docs_dir)
            .with_context(|| format!("failed to cleanup {}", docs_dir.display()))?;
    }
    fs::create_dir_all(&docs_dir).with_context(|| format!("failed to create {}", docs_dir.display()))?;

    let mut files = Vec::with_capacity(file_count);
    for i in 0..file_count {
        let path = docs_dir.join(format!("doc_{i:04}.txt"));
        let seed = &seeds[i % seeds.len()];
        let body = format!(
            "{seed}\n\n文書番号: {i}\nこの文書は tokenizer 比較ベンチ用のデータです。\n"
        );
        fs::write(&path, body).with_context(|| format!("failed to write {}", path.display()))?;
        files.push(path);
    }
    Ok(files)
}

fn run_mode(base_dir: &Path, files: &[PathBuf], mode: TokenizerMode, label: &str) -> Result<()> {
    let index_dir = base_dir.join(format!("index_{label}"));
    if index_dir.exists() {
        fs::remove_dir_all(&index_dir)
            .with_context(|| format!("failed to cleanup {}", index_dir.display()))?;
    }

    let engine = Traverze::open_or_create_with_mode(&index_dir, mode)?;

    let start = Instant::now();
    let indexed = engine.index_files(files)?;
    let index_elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    if indexed != files.len() {
        bail!("indexed files mismatch: expected {}, got {}", files.len(), indexed);
    }

    let start = Instant::now();
    let mut total_hits = 0usize;
    for _ in 0..SEARCH_REPEAT {
        for query in QUERIES {
            total_hits += engine.search(query, 20)?.len();
        }
    }
    let search_elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

    println!("{label}\t{index_elapsed_ms:.3}\t{search_elapsed_ms:.3}\t{total_hits}");
    Ok(())
}
