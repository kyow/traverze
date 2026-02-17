# traverze

A utility library and CLI for full-text search built on Tantivy and Lindera.

## Features

- `tokenizer-ngram` (default)
- `tokenizer-lindera-ipadic` (optional)

## CLI

```bash
traverze index [--index-dir <DIR>] <FILES...>
traverze remove [--index-dir <DIR>] <FILES...>
traverze search [--index-dir <DIR>] [--limit <N>] <QUERY>
```

## Library Usage

### Add dependency

```toml
[dependencies]
traverze = "0.1"
```

Use Lindera (IPADIC) tokenizer:

```toml
[dependencies]
traverze = { version = "0.1", features = ["tokenizer-lindera-ipadic"] }
```

### Minimal example

```rust
use std::path::PathBuf;
use traverze::Traverze;

fn main() -> anyhow::Result<()> {
    let index_dir = PathBuf::from("./.traverze-index");
    let engine = Traverze::open_or_create(&index_dir)?;

    let files = vec![
        PathBuf::from("README.md"),
        PathBuf::from("src/lib.rs"),
    ];
    engine.index_files(&files)?;

    let hits = engine.search("tantivy", 10)?;
    for hit in hits {
        println!("{} ({:.3})", hit.path, hit.score);
    }

    Ok(())
}
```

## Third-Party Notices

When distributing binaries or source artifacts (including crates.io packages),
review and include `THIRD_PARTY_NOTICES.md`.

This is especially important when `tokenizer-lindera-ipadic` is enabled,
because IPADIC dictionary data notice terms apply.
