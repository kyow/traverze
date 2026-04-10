# 変更履歴

このプロジェクトに対するすべての重要な変更はこのファイルに記録されます。

このフォーマットは [Keep a Changelog](https://keepachangelog.com/ja/1.1.0/) に基づいており、
このプロジェクトは [Semantic Versioning](https://semver.org/lang/ja/spec/v2.0.0.html) に準拠しています。

## [Unreleased]

### Added

- CJK 対応のクエリ前処理（形態素トークン展開による `QueryPreprocess::Auto`）
- インデックス済みファイルの一覧表示コマンド
- Traverze 初期化のビルダーパターン導入（`Traverze::builder()`）
- `DEFAULT_INDEX_DIR` パブリック定数の公開
- Apache 2.0 および MIT デュアルライセンスの追加

### Changed

- `index_files` を `index`、`remove_files` を `remove` にリネーム
- `search` と `search_with_options` を `SearchOptions` を受け取る単一の `search` メソッドに統合
- コンストラクタ（`new_in_dir`、`new_in_dir_with_mode`、`new_in_dir_for_indexing`）をビルダーパターンに置き換え
- `supports_snippet()` を `has_snippet()` にリネーム
- クエリ前処理オプションを `plain` と `auto` に変更

### Fixed

- クエリ解析を寛容モードに変更し、予約語のキーワードクォートに対応
- LICENSE ファイルの著作者名を 'kyow' に修正

### Removed

- `AnalyzeOriginalOrAnd` オプションの削除
- CLI およびインデックス処理からデバッグ用トークナイズオプションを削除
- `search_with_options` メソッドの削除（`search` に統合）

## [0.2.0] - 2026-02-27

### Added

- 検索結果にスニペットを含めるオプション（`--with-snippet`、`SnippetOptions`、`SnippetFormat`）
- `search_with_options` メソッドと `SearchOptions` 構造体
- インデックスコマンドに `--reset` フラグを追加
- CLI に `--version` フラグを追加
- インデックスのリセット機能

### Fixed

- インデックスコマンドのスニペットサポートに関するエラーメッセージを修正
- スニペット不要時にもインデックス機能が正しく動作するよう修正

## [0.1.0] - 2026-02-17

### Added

- 初回リリース
- Tantivy ベースの全文検索エンジン
- ファイルのインデックス作成・検索・削除を行う CLI
- N-gram トークナイザーのサポート（デフォルト）
- Lindera 形態素解析ライブラリの統合
- トークナイザー比較用ベンチマーク
- ファイルをインデックスから削除するコマンド
- インデックスおよび検索の処理時間計測

[Unreleased]: https://github.com/kyow/traverze/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/kyow/traverze/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/kyow/traverze/releases/tag/v0.1.0
