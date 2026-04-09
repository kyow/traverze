# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Japanese language support using Lindera tokenizer (IPAdic feature flag)
- Command to list all indexed files with their paths
- Query preprocessing for improved search accuracy
- Apache 2.0 and MIT dual licensing
- Builder pattern for Traverze initialization
- `DEFAULT_INDEX_DIR` public constant for consistent index directory usage

### Changed

- Renamed public API methods for indexing, removing, and listing for consistency
- Refactored search methods to use `SearchOptions` for consistency
- Updated query preprocessing options to use `plain` and `auto` for improved clarity

### Fixed

- Query parsing with lenient error handling and keyword quoting
- Snippet support check method name for consistency
- Copyright name corrected to 'kyow' in LICENSE files

### Removed

- `AnalyzeOriginalOrAnd` option
- Debug tokenization options from CLI and indexing

## [0.2.0] - 2026-02-27

### Added

- Options to include snippets in search results
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
