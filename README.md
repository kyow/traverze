# traverze

A utility library and CLI for full-text search built on Tantivy and Lindera.

## Features

- `tokenizer-ngram` (default)
- `tokenizer-lindera-ipadic` (optional)

## CLI

```bash
traverze index [--index-dir <DIR>] [--with-snippet] [--reset] [FILES...]
traverze remove [--index-dir <DIR>] <FILES...>
traverze search [--index-dir <DIR>] [--limit <N>] [--with-snippet] [--snippet-max-chars <N>] [--snippet-format text|html] <QUERY>
```

Notes:
- `index` default is fast mode (no stored `contents`).
- `index --reset` without files only deletes the index directory.
- To enable snippets, build index with `index --with-snippet`.
- If `search --with-snippet` is used on a non-snippet index, recreate with `index --reset --with-snippet`.

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
use traverze::Traverze;

fn main() -> anyhow::Result<()> {
    let index_dir = PathBuf::from("./.traverze-index");
    let engine = Traverze::new_in_dir(&index_dir)?;

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
