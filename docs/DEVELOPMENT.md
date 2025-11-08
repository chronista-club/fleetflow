# 開発ガイド

このプロジェクトの開発に役立つツールとコマンドの使い方を説明します。

## 開発環境

このプロジェクトでは [mise](https://mise.jdx.dev/) を使用して開発ツールを管理しています。

### インストール済みツール

```bash
mise current
```

主要なツール:
- **rust**: 1.91.0 - Rustコンパイラ
- **rg**: 15.1.0 - 高速なコード検索ツール（ripgrep）
- **fd**: 10.3.0 - 高速なファイル検索ツール
- **lsd**: 1.2.0 - モダンなlsコマンド
- **gh**: 2.82.1 - GitHub CLI

## よく使うコマンド

### コード検索（ripgrep）

`rg`は`grep`よりも高速で使いやすいコード検索ツールです。

```bash
# 基本的な検索
rg "pattern"

# テスト関数を検索
rg "#\[test\]"

# 件数をカウント
rg "#\[test\]" --count

# 特定のファイルタイプのみ検索
rg "FlowError" --type rust

# 検索結果に前後の行を表示
rg "TODO" -C 3

# 大文字小文字を区別しない
rg "error" -i

# 正規表現パターン
rg "service \"(\w+)\"" -o
```

### ファイル検索（fd）

`fd`は`find`よりも高速で使いやすいファイル検索ツールです。

```bash
# 基本的な検索
fd "pattern"

# .kdlファイルを検索
fd -e kdl

# services/ディレクトリ内のファイル
fd . services/

# 隠しファイルも含めて検索
fd -H "pattern"

# ディレクトリのみ検索
fd -t d

# 実行可能ファイルのみ検索
fd -t x
```

### Cargoコマンド

#### ビルドとテスト

```bash
# ビルド
cargo build

# リリースビルド
cargo build --release

# テスト実行
cargo test

# 特定のテストを実行
cargo test test_name

# テストを詳細表示
cargo test -- --nocapture

# ワークスペース全体でテスト
cargo test --workspace
```

#### コード品質

```bash
# Clippy（Linter）
cargo clippy

# フォーマット確認
cargo fmt -- --check

# フォーマット適用
cargo fmt

# 未使用の依存関係チェック
cargo machete
```

#### 依存関係管理

```bash
# 依存関係を追加
cargo add <crate>

# 開発用依存関係を追加
cargo add --dev <crate>

# 依存関係を削除
cargo rm <crate>

# 依存関係を更新
cargo upgrade

# バージョンを設定
cargo set-version 1.0.0
```

### GitHub CLI（gh）

```bash
# プルリクエスト作成
gh pr create

# プルリクエスト一覧
gh pr list

# Issue作成
gh issue create

# リポジトリをブラウザで開く
gh repo view --web
```

## 開発ワークフロー

### 1. 新機能の開発

```bash
# ブランチ作成
git checkout -b feature/new-feature

# コードを書く
# ...

# テスト実行
cargo test

# フォーマット
cargo fmt

# Clippy
cargo clippy

# コミット
git add .
git commit -m "新機能を追加"

# プッシュ
git push origin feature/new-feature
```

### 2. テストの追加

テストファイルの構造:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        // テストコード
    }
}
```

### 3. デバッグ

```bash
# デバッグビルド（最適化なし）
cargo build

# ログレベル設定
RUST_LOG=debug cargo run

# バックトレース有効化
RUST_BACKTRACE=1 cargo run
```

## ディレクトリ構造

```
unison-flow/
├── crates/
│   ├── flow-atom/      # Atom定義とパーサー
│   ├── flow-cli/       # CLIツール
│   ├── flow-config/    # 設定管理
│   └── flow-container/ # コンテナ管理
├── spec/               # 仕様ドキュメント
├── docs/               # ドキュメント
└── examples/           # サンプル
```

## トラブルシューティング

### ビルドエラー

```bash
# クリーンビルド
cargo clean
cargo build
```

### テストが失敗する

```bash
# 詳細なログ出力
cargo test -- --nocapture

# 特定のテストのみ実行
cargo test test_name -- --nocapture
```

### フォーマットエラー

```bash
# 自動フォーマット
cargo fmt
```

## 参考リンク

- [Rust Book](https://doc.rust-lang.org/book/)
- [Cargo Book](https://doc.rust-lang.org/cargo/)
- [ripgrep User Guide](https://github.com/BurntSushi/ripgrep/blob/master/GUIDE.md)
- [fd User Guide](https://github.com/sharkdp/fd)
