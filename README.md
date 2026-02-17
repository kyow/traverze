# traverze

A small full-text search CLI/library built with Tantivy.

## Features

- `tokenizer-ngram` (default)
- `tokenizer-lindera-ipadic` (optional)

## CLI

```bash
traverze index [--index-dir <DIR>] <FILES...>
traverze remove [--index-dir <DIR>] <FILES...>
traverze search [--index-dir <DIR>] [--limit <N>] <QUERY>
```

## Third-Party Notices

When distributing binaries or source artifacts (including crates.io packages),
review and include `THIRD_PARTY_NOTICES.md`.

This is especially important when `tokenizer-lindera-ipadic` is enabled,
because IPADIC dictionary data notice terms apply.
