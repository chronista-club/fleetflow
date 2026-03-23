# FleetFlow プロジェクトガイド

## プロジェクト概要

FleetFlowは、KDL（KDL Document Language）をベースにした超シンプルなコンテナオーケストレーション・環境構築ツール。

**コンセプト**: 環境構築は、対話になった。伝えれば、動く。

## 技術スタック

- **言語**: Rust (edition 2024)
- **パーサー**: `kdl` crate
- **コンテナAPI**: `bollard` (Docker API client)
- **CLI**: `clap` (derive features)
- **非同期ランタイム**: `tokio`
- **エラーハンドリング**: `anyhow`(CLI), `thiserror`(ライブラリ)
- **ロギング**: `tracing`, `tracing-subscriber`
- **その他**: `config`, `serde`, `tera`(テンプレート)

## プロジェクト構造

```
fleetflow/
├── crates/
│   ├── fleetflow/                 # CLIエントリーポイント
│   ├── fleetflow-core/            # KDLパーサー・データモデル
│   │   ├── src/model/             # データ構造
│   │   └── src/parser/            # パーサー
│   ├── fleetflow-registry/        # Fleet Registry（複数fleet統合管理）
│   ├── fleetflow-config/          # 設定管理
│   ├── fleetflow-container/       # コンテナ操作
│   ├── fleetflow-build/           # Dockerビルド機能
│   ├── fleetflow-cloud/           # クラウドインフラ抽象化
│   ├── fleetflow-cloud-sakura/    # さくらクラウド連携
│   ├── fleetflow-cloud-cloudflare/ # Cloudflare連携
│   └── fleetflow-mcp/            # MCPサーバー
├── docs/
│   └── guide/                     # 利用ガイド（Usage）
├── .claude/
│   ├── CLAUDE.md                  # このファイル
│   └── ports.md                   # ポート設定ガイド
```

## ビルド・テスト・実行

```bash
cargo build             # ビルド
cargo test              # 全テスト実行
cargo test --lib        # ライブラリテストのみ
cargo clippy            # リント
cargo fmt               # フォーマット
cargo run -- --help     # 開発用実行
fleet --help            # インストール後
```

## 名称・環境変数の規約

| 用途 | 名称 |
|------|------|
| プロジェクト名 | `FleetFlow`（FとF大文字） |
| CLIコマンド | `fleet` |
| crate名・ディレクトリ名 | `fleetflow` |
| クレート命名 | `fleetflow-*`（kebab-case） |
| 環境変数プレフィックス | `FLEETFLOW_` |
| ステージ環境変数 | `FLEET_STAGE`（local/dev/pre/live） |

## コーディング規約

### Rustスタイル
- `rustfmt`標準設定 + `clippy`遵守
- ライブラリ: `thiserror`でカスタムエラー型、CLIレベル: `anyhow`
- 各クレートは単一責任、依存関係は一方向、公開APIは最小限

### コミットメッセージ
```
feat: / fix: / refactor: / docs: / spec: / test:
```

## 設計原則

- **最小限の概念**: `project`, `stage`, `service` の3つで全てを表現
- **YAGNI**: 今必要でない機能は実装しない
- **Straightforward**: 入力→処理→出力を直線的に
- **不要な抽象化を避ける**: 過度なトレイトやジェネリクスを使わない

## KDL設定ファイル仕様

```kdl
project "my-project"          // 必須: プロジェクト名宣言

stage "local" {               // ステージ定義
    service "web"
    service "db"
}

service "web" {               // サービス詳細（image必須）
    image "node:20-alpine"
    ports { port host=3000 container=3000 }
    env { NODE_ENV "development" }
}
```

### コア概念
- **コンテナ命名**: `{project}-{stage}-{service}`
- **ステージ**: `local`, `dev`, `pre`, `live`
- **image**: 必須フィールド（自動推測なし）
- **サービスマージ**: 複数ファイルで同名サービス定義時、後の定義が優先マージ

## OrbStack連携

主にローカル開発環境（macOS）での利用を想定。

Dockerラベルで自動グループ化:
- `com.docker.compose.project` = `{project}-{stage}` (OrbStackグループ化)
- `com.docker.compose.service` = `{service}`
- `fleetflow.project` / `fleetflow.stage` / `fleetflow.service`

詳細: Creo Memories (fleetflow atlas) の S4: OrbStack Integration / D2: OrbStack Integration Design を参照

## CLIコマンド

```bash
# Daily（日常操作）
fleet up/down/restart [-s stage] [-n svc]  # 起動/停止/再起動
fleet ps/logs/exec [-s stage]              # 一覧/ログ/コンテナ実行

# Ship（ビルド・デプロイ）
fleet build/deploy [-s stage]

# Admin（CP管理 — fleet cp 配下）
fleet cp login/logout/auth                 # 認証
fleet cp daemon/tenant/project/server      # リソース管理
fleet cp cost/dns/remote/registry          # コスト・DNS・デプロイ・Registry

# Util
fleet mcp / fleet self-update / fleet --version
```

## 開発フェーズ

| Phase | 状態 | 内容 |
|-------|------|------|
| 1: MVP | 完了 | KDLパーサー、基本CLI、OrbStack連携、Docker API統合 |
| 2: ビルド | 完了 | Dockerビルド、個別サービス操作、複数設定ファイル |
| 3: クラウド | 完了 | クラウド抽象化、さくら/Cloudflare連携 |
| 4: 拡張 | 完了 | variables、include、MCP、Registry、SSH、Playbook |
| 5: Platform | 全5Phase完了 | Control Plane、Multi-Project、MCP v2、WebUI Dashboard |

## ドキュメント

- **仕様書 (spec)**: Creo Memories (fleetflow atlas, category: "spec") — S1〜S10
- **設計書 (design)**: Creo Memories (fleetflow atlas, category: "design-decision") — D1〜D8
- **利用ガイド (guide)**: `docs/guide/` + Creo Memories (fleetflow atlas, category: "guide")

## トラブルシューティング

- **ビルドエラー**: `rustc --version` 確認 / `cargo clean && cargo build`
- **Docker接続**: OrbStack起動確認 / Docker socketパーミッション確認
- **テスト失敗**: 残存コンテナ確認 / ポート競合確認
