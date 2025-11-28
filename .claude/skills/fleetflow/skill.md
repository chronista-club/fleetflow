---
name: fleetflow
description: FleetFlow（KDLベースのコンテナオーケストレーションツール）を効果的に使用するためのガイド
version: 0.2.0
---

# FleetFlow スキル

FleetFlowをプロジェクトで効果的に活用するための包括的なガイドです。

## エイリアス

- `fleetflow` / `flow` / `ff`

## 概要

FleetFlowは、KDL（KDL Document Language）をベースにした超シンプルなコンテナオーケストレーションツールです。

**コンセプト**: 「宣言だけで、開発も本番も」

### 主要な特徴

| 特徴 | 説明 |
|------|------|
| 超シンプル | Docker Composeと同等以下の記述量 |
| 可読性 | YAMLより読みやすいKDL構文 |
| ステージ管理 | local/dev/staging/prod を統一管理 |
| OrbStack連携 | macOSローカル開発に最適化 |
| Dockerビルド | Dockerfileからのビルドをサポート |

## クイックスタート

### インストール

```bash
cargo install fleetflow
# または
cargo install --git https://github.com/chronista-club/fleetflow
```

### 最小構成

```kdl
// flow.kdl
project "myapp"

stage "local" {
    service "db"
}

service "db" {
    image "postgres"
    version "16"
    ports {
        port host=5432 container=5432
    }
    env {
        POSTGRES_PASSWORD "postgres"
    }
}
```

### 基本操作

```bash
fleetflow up local      # 起動
fleetflow ps            # 状態確認
fleetflow logs          # ログ表示
fleetflow down local    # 停止・削除
```

## CLIコマンド一覧

| コマンド | 説明 |
|---------|------|
| `up <stage>` | ステージを起動 |
| `down <stage>` | ステージを停止・削除 |
| `ps [--all]` | コンテナ一覧 |
| `logs [--follow] [service]` | ログ表示 |
| `start <stage> [service]` | 停止中のサービスを起動 |
| `stop <stage> [service]` | サービスを停止（コンテナ保持） |
| `restart <stage> [service]` | サービスを再起動 |
| `validate` | 設定を検証 |
| `version` | バージョン表示 |

詳細: [reference/cli-commands.md](reference/cli-commands.md)

## 設定ファイル構造

```kdl
project "name"              // プロジェクト名（必須）

stage "local" {             // ステージ定義
    service "db"
    service "web"
}

service "db" {              // サービス定義
    image "postgres"
    version "16"
    ports { ... }
    env { ... }
    volumes { ... }
    build { ... }           // Dockerビルド設定
}
```

詳細: [reference/kdl-syntax.md](reference/kdl-syntax.md)

## コンテナ命名規則

FleetFlowは以下の命名規則でコンテナを作成します：

```
{project}-{stage}-{service}
```

例: `myapp-local-db`

OrbStackでは `{project}-{stage}` でグループ化されます。

## プロジェクト構造

```
fleetflow/
├── crates/
│   ├── fleetflow-cli/           # CLI
│   ├── fleetflow-atom/          # KDLパーサー
│   ├── fleetflow-container/     # コンテナ操作
│   ├── fleetflow-build/         # Dockerビルド
│   ├── fleetflow-cloud/         # クラウド抽象化
│   ├── fleetflow-cloud-sakura/  # さくらクラウド
│   └── fleetflow-cloud-cloudflare/ # Cloudflare
├── spec/                        # 仕様書
├── design/                      # 設計書
└── guides/                      # 利用ガイド
```

詳細: [reference/architecture.md](reference/architecture.md)

## スキルの起動タイミング

このスキルは以下の場合に参照してください：

- ✅ プロジェクトにFleetFlowを導入する際
- ✅ `flow.kdl` 設定ファイルを作成・編集する際
- ✅ コンテナ環境の構築・管理を行う際
- ✅ ローカル開発環境のセットアップ時

## リファレンス

- [KDL構文リファレンス](reference/kdl-syntax.md)
- [CLIコマンドリファレンス](reference/cli-commands.md)
- [アーキテクチャ](reference/architecture.md)
- [パターン集](examples/patterns.md)

## 外部リンク

- [GitHub Repository](https://github.com/chronista-club/fleetflow)
- [KDL Document Language](https://kdl.dev/)
- [OrbStack](https://orbstack.dev/)

---

FleetFlow - シンプルに、統一的に、環境を構築する。
