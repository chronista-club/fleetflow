# Contributing to FleetFlow

## 開発環境のセットアップ

```bash
# リポジトリのクローン
git clone https://github.com/chronista-club/fleetflow.git
cd fleetflow

# ビルド
cargo build

# テスト
cargo test --lib

# リント
cargo clippy

# フォーマット
cargo fmt
```

## 必要なツール

- Rust (latest stable)
- Docker または OrbStack（統合テスト用）

## 開発フロー

1. Issue を確認し、作業対象を決める
2. ブランチを作成: `git checkout -b feat/issue-name`
3. 実装 + テスト
4. `cargo build && cargo test --lib && cargo clippy` が全て通ることを確認
5. PR を作成

## コミットメッセージ

[Conventional Commits](https://www.conventionalcommits.org) に準拠:

```
feat: 新機能の説明
fix: バグ修正の説明
refactor: リファクタリングの説明
docs: ドキュメント変更
test: テスト追加・修正
chore: その他（CI, 依存更新等）
```

## プロジェクト構成

```
crates/
├── fleetflow/           # CLI エントリーポイント
├── fleetflow-core/      # KDL パーサー・データモデル
├── fleetflow-container/ # コンテナ操作
├── fleetflow-config/    # 設定管理
├── fleetflow-build/     # Docker ビルド
├── fleetflow-cloud/     # クラウド抽象化
└── fleetflow-mcp/       # MCP サーバー
```

## テスト

```bash
cargo test --lib          # ユニットテストのみ（Docker 不要）
cargo test                # 全テスト（Docker 必要）
```

## コードスタイル

- `rustfmt` 標準設定
- `clippy` 警告ゼロ
- ライブラリ: `thiserror` でカスタムエラー型
- CLI: `anyhow` でエラーハンドリング
