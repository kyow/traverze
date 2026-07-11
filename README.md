# traverze

English | [日本語](README.ja.md)

A utility library and CLI for full-text search built on Tantivy and Lindera, with built-in Japanese language support.

## Features

- **Full-Text Search:** Fast indexing and querying powered by Tantivy.
- **Japanese Support:** Choose between an N-gram tokenizer for broad language coverage or Lindera with IPADIC for accurate Japanese morphological analysis.
- **Snippet Extraction:** Retrieve highlighted text snippets alongside search results.
- **Dual Interface:** Use as a CLI tool or integrate directly as a Rust library.
- **Cross-Platform:** Built with Rust, runs on Windows, macOS, and Linux.

### Tokenizer feature flags

- `tokenizer-ngram` (default) — Character-based 2–3 gram tokenizer. Works with all languages including CJK. Lightweight with no additional dictionary data.
- `tokenizer-lindera-ipadic` (optional) — Japanese morphological analyzer using the IPADIC dictionary. Produces more accurate tokens for Japanese text but increases binary size (~50 MB).

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) 1.85+ (edition 2024)

## Installation

```bash
cargo install traverze
```

With Lindera (IPADIC) tokenizer:

```bash
cargo install traverze --features tokenizer-lindera-ipadic
```

## CLI

```bash
traverze index [--index-dir <DIR>] [--with-snippet] [--reset] [FILES...]
traverze list [--index-dir <DIR>]
traverze remove [--index-dir <DIR>] <FILES...>
traverze search [--index-dir <DIR>] [--limit <N>] [--with-snippet] [--snippet-max-chars <N>] [--snippet-format text|html] [--query-preprocess plain|auto] <QUERY>
```

Notes:
- `index` default is fast mode (no stored `contents`).
- `index --reset` without files only deletes the index directory.
- To enable snippets, build index with `index --with-snippet`.
- If `search --with-snippet` is used on a non-snippet index, recreate with `index --reset --with-snippet`.
- `list` outputs one indexed file path per line.
- `--query-preprocess` controls how the query is tokenized before searching:
  - `plain` — pass the query string directly to Tantivy's query parser.
  - `auto` (default) — tokenize the query with the index's analyzer, combine tokens with AND. CJK substrings are wrapped as phrase queries to preserve word boundaries. Every token is quoted and escaped, so Tantivy query syntax (reserved keywords such as `AND`/`OR`/`NOT` and special characters) is treated as literal text.

### Search output format

Results are printed as tab-separated values to stdout (one hit per line):

```
<score>\t<path>                   # without --with-snippet
<score>\t<path>\t<snippet>        # with --with-snippet
```

Newlines, tabs, and carriage returns in snippets are escaped as `\n`, `\t`, `\r`.
Timing information is printed to stderr.

## Library Usage

### Add dependency

```toml
[dependencies]
traverze = "0.3"
```

Use Lindera (IPADIC) tokenizer:

```toml
[dependencies]
traverze = { version = "0.3", features = ["tokenizer-lindera-ipadic"] }
```

### Minimal example

```rust
use std::path::PathBuf;
use traverze::{SearchOptions, Traverze};

fn main() -> anyhow::Result<()> {
    let engine = Traverze::new()?;

    let files = vec![
        PathBuf::from("README.md"),
        PathBuf::from("src/lib.rs"),
    ];
    engine.index(&files)?;

    let hits = engine.search("tantivy", SearchOptions::with_limit(10))?;
    for hit in hits {
        println!("{} ({:.3})", hit.path, hit.score);
    }

    Ok(())
}
```

### Search with snippets

```rust
use traverze::{SearchOptions, SnippetFormat, SnippetOptions, Traverze};

fn main() -> anyhow::Result<()> {
    let engine = Traverze::new()?; // uses default ".traverze-index"

    let options = SearchOptions {
        limit: 10,
        snippet: Some(SnippetOptions {
            max_num_chars: 150,
            format: SnippetFormat::Text,
        }),
        ..Default::default()
    };

    let hits = engine.search("tantivy", options)?;
    for hit in hits {
        println!("{} ({:.3})", hit.path, hit.score);
        if let Some(snippet) = &hit.snippet {
            println!("  {}", snippet);
        }
    }

    Ok(())
}
```

> **Note:** Snippet search requires the index to be built with `--with-snippet` (CLI) or
> `Traverze::builder().with_snippet(true).open()` (library).
> Use `engine.has_snippet()` to check at runtime.

### List indexed files

```rust
use traverze::Traverze;

fn main() -> anyhow::Result<()> {
    let engine = Traverze::new()?;
    let paths = engine.list()?;
    for path in &paths {
        println!("{}", path);
    }
    println!("{} file(s)", paths.len());
    Ok(())
}
```

### Remove files from the index

```rust
use std::path::PathBuf;
use traverze::Traverze;

fn main() -> anyhow::Result<()> {
    let engine = Traverze::new()?;
    let removed = engine.remove(&[PathBuf::from("old_file.txt")])?;
    println!("removed {} file(s)", removed);
    Ok(())
}
```

### Select tokenizer mode explicitly

```rust
use traverze::{TokenizerMode, Traverze};

fn main() -> anyhow::Result<()> {
    // Use Lindera IPADIC tokenizer (requires `tokenizer-lindera-ipadic` feature)
    let engine = Traverze::builder()
        .mode(TokenizerMode::LinderaIpadic)
        .open()?;
    // ...
    Ok(())
}
```

## Third-Party Notices

When distributing binaries or source artifacts (including crates.io packages),
review and include `THIRD_PARTY_NOTICES.md`.

This is especially important when `tokenizer-lindera-ipadic` is enabled,
because IPADIC dictionary data notice terms apply.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
