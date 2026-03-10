# traverze

A utility library and CLI for full-text search built on Tantivy and Lindera.

## Features

- `tokenizer-ngram` (default)
- `tokenizer-lindera-ipadic` (optional)

## Requirements

- Rust 1.85+ (edition 2024)

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
traverze search [--index-dir <DIR>] [--limit <N>] [--with-snippet] [--snippet-max-chars <N>] [--snippet-format text|html] [--query-preprocess none|analyze-and] <QUERY>
```

Notes:
- `index` default is fast mode (no stored `contents`).
- `index --reset` without files only deletes the index directory.
- To enable snippets, build index with `index --with-snippet`.
- If `search --with-snippet` is used on a non-snippet index, recreate with `index --reset --with-snippet`.
- `list` outputs one indexed file path per line.

## Library Usage

### Add dependency

```toml
[dependencies]
traverze = "0.2"
```

Use Lindera (IPADIC) tokenizer:

```toml
[dependencies]
traverze = { version = "0.2", features = ["tokenizer-lindera-ipadic"] }
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
> Use `engine.supports_snippet()` to check at runtime.

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
