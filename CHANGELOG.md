# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- CJK-aware query preprocessing with morphological token expansion (`QueryPreprocess::Auto`)
- Command to list all indexed files with their paths
- Builder pattern for Traverze initialization (`Traverze::builder()`)
- `DEFAULT_INDEX_DIR` public constant for consistent index directory usage
- Apache 2.0 and MIT dual licensing

### Changed

- Renamed `index_files` to `index`, `remove_files` to `remove` for consistency
- Consolidated `search` and `search_with_options` into a single `search` method that takes `SearchOptions`
- Replaced constructors (`new_in_dir`, `new_in_dir_with_mode`, `new_in_dir_for_indexing`) with builder pattern
- Renamed `supports_snippet()` to `has_snippet()`
- Updated query preprocessing options to use `plain` and `auto` for improved clarity

### Fixed

- Query parsing now uses lenient mode
- Auto query preprocessing now preserves AND semantics on the ngram index (every token is quoted and escaped, tokens containing whitespace are dropped)
- Copyright name corrected to 'kyow' in LICENSE files

### Removed

- `AnalyzeOriginalOrAnd` option
- Debug tokenization options from CLI and indexing
- `search_with_options` method (merged into `search`)

## [0.2.0] - 2026-02-27

### Added

- Options to include snippets in search results (`--with-snippet`, `SnippetOptions`, `SnippetFormat`)
- `search_with_options` method and `SearchOptions` struct
- `--reset` flag for index command to recreate index
- `--version` flag for CLI
- Index reset function

### Fixed

- Error message for snippet support in index command
- Indexing functionality retained when snippets are unnecessary

## [0.1.0] - 2026-02-17

### Added

- Initial release
- Full-text search engine built on Tantivy
- CLI for indexing, searching, and removing files
- N-gram tokenizer support (default)
- Lindera morphological analysis library integration
- Benchmark for tokenizer comparison
- Remove command for unindexing files
- Processing time measurement for indexing and search operations

[Unreleased]: https://github.com/kyow/traverze/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/kyow/traverze/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/kyow/traverze/releases/tag/v0.1.0
