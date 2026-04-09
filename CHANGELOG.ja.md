# 変更履歴

このプロジェクトに対するすべての重要な変更は、このファイルに記録されます。

このファイルのフォーマットは [Keep a Changelog](https://keepachangelog.com/ja/1.1.0/) に基づいており、
このプロジェクトは [セマンティック バージョニング](https://semver.org/lang/ja/spec/v2.0.0.html) に準拠しています。

## [Unreleased]

### Added

- Lindera トークナイザーによる日本語対応（IPAdic フィーチャーフラグ）
- インデックス済みファイルの一覧表示コマンド
- 検索精度向上のためのクエリ前処理
- Apache 2.0 および MIT デュアルライセンスの追加
- Traverze 初期化のビルダーパターン導入
- `DEFAULT_INDEX_DIR` パブリック定数の公開

### Changed

- インデックス・削除・一覧のパブリック API メソッド名を統一
- 検索メソッドを `SearchOptions` を使用する形にリファクタリング
- クエリ前処理オプションを `plain` と `auto` に変更

### Fixed

- 寛容なエラーハンドリングとキーワードクォートによるクエリ解析の修正
- スニペットサポートチェックのメソッド名を修正
- LICENSE ファイルの著作者名を 'kyow' に修正

### Removed

- `AnalyzeOriginalOrAnd` オプションの削除
- CLI およびインデックス処理からデバッグ用トークナイズオプションを削除

## [0.2.0] - 2026-02-27

### Added

- 検索結果にスニペットを含めるオプション
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
