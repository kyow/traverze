# traverze

[English](README.md) | 日本語

Tantivy と Lindera をベースにした全文検索のためのユーティリティライブラリ・CLI。日本語サポートを標準搭載しています。

## 特徴

- **全文検索:** Tantivy による高速なインデックス作成とクエリ実行。
- **日本語サポート:** 幅広い言語に対応する N-gram トークナイザーと、IPADIC を用いた高精度な日本語形態素解析を行う Lindera から選択可能。
- **スニペット抽出:** 検索結果とあわせてハイライト付きのテキストスニペットを取得。
- **デュアルインターフェース:** CLI ツールとしても、Rust ライブラリとしても利用可能。
- **クロスプラットフォーム:** Rust 製で、Windows・macOS・Linux で動作。

### トークナイザーの feature フラグ

- `tokenizer-ngram`(デフォルト)— 文字ベースの 2〜3 gram トークナイザー。CJK を含むすべての言語に対応。追加の辞書データが不要で軽量。
- `tokenizer-lindera-ipadic`(オプション)— IPADIC 辞書を用いた日本語形態素解析器。日本語テキストに対してより正確なトークンを生成しますが、バイナリサイズが増加します(約 50 MB)。

## 動作要件

- [Rust](https://www.rust-lang.org/tools/install) 1.85 以上(edition 2024)

## インストール

```bash
cargo install traverze
```

Lindera(IPADIC)トークナイザーを使う場合:

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

補足:
- `index` のデフォルトは高速モードです(`contents` を保存しません)。
- ファイルを指定せずに `index --reset` を実行すると、インデックスディレクトリの削除のみを行います。
- スニペットを有効にするには、`index --with-snippet` でインデックスを作成してください。
- スニペット非対応のインデックスに対して `search --with-snippet` を使う場合は、`index --reset --with-snippet` でインデックスを再作成してください。
- `list` はインデックス済みファイルのパスを 1 行に 1 件ずつ出力します。
- `--query-preprocess` は、検索前にクエリをどのようにトークン化するかを制御します:
  - `plain` — クエリ文字列を Tantivy のクエリパーサーにそのまま渡します。
  - `auto`(デフォルト)— インデックスのアナライザーでクエリをトークン化し、トークンを AND で結合します。CJK の部分文字列は単語境界を保つためフレーズクエリとして扱われます。すべてのトークンはクォート・エスケープされるため、Tantivy のクエリ構文(`AND`、`OR`、`NOT` などの予約語や特殊文字)はリテラルとして扱われます。

### 検索結果の出力フォーマット

検索結果はタブ区切り(1 ヒットにつき 1 行)で標準出力に表示されます:

```
<score>\t<path>                   # --with-snippet なし
<score>\t<path>\t<snippet>        # --with-snippet あり
```

スニペット内の改行・タブ・キャリッジリターンは `\n`、`\t`、`\r` にエスケープされます。
実行時間の情報は標準エラー出力に表示されます。

## ライブラリとしての利用

### 依存関係の追加

```toml
[dependencies]
traverze = "0.3"
```

Lindera(IPADIC)トークナイザーを使う場合:

```toml
[dependencies]
traverze = { version = "0.3", features = ["tokenizer-lindera-ipadic"] }
```

### 最小構成の例

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

### スニペット付き検索

```rust
use traverze::{SearchOptions, SnippetFormat, SnippetOptions, Traverze};

fn main() -> anyhow::Result<()> {
    let engine = Traverze::new()?; // デフォルトの ".traverze-index" を使用

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

> **注意:** スニペット検索には、`--with-snippet`(CLI)または
> `Traverze::builder().with_snippet(true).open()`(ライブラリ)で作成したインデックスが必要です。
> 実行時の確認には `engine.has_snippet()` を使用してください。

### インデックス済みファイルの一覧表示

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

### インデックスからのファイル削除

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

### トークナイザーモードの明示的な指定

```rust
use traverze::{TokenizerMode, Traverze};

fn main() -> anyhow::Result<()> {
    // Lindera IPADIC トークナイザーを使用(`tokenizer-lindera-ipadic` feature が必要)
    let engine = Traverze::builder()
        .mode(TokenizerMode::LinderaIpadic)
        .open()?;
    // ...
    Ok(())
}
```

## サードパーティ表記

バイナリやソース成果物(crates.io パッケージを含む)を配布する際は、
`THIRD_PARTY_NOTICES.md` を確認のうえ同梱してください。

`tokenizer-lindera-ipadic` を有効にする場合は、IPADIC 辞書データの
表記条件が適用されるため、特に重要です。

## ライセンス

以下のいずれかのライセンスを選択できます。

- Apache License, Version 2.0([LICENSE-APACHE](LICENSE-APACHE) または <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT License([LICENSE-MIT](LICENSE-MIT) または <http://opensource.org/licenses/MIT>)
