# FleetFlow

> Docker Composeよりシンプル。KDLで書く、次世代の環境構築ツール。

[![CI](https://github.com/chronista-club/fleetflow/actions/workflows/ci.yml/badge.svg)](https://github.com/chronista-club/fleetflow/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)

## コンセプト

**「宣言だけで、開発も本番も」**

FleetFlowは、KDL（KDL Document Language）をベースにした、革新的で超シンプルなコンテナオーケストレーション・環境構築ツールです。
Docker Composeの手軽さはそのままに、より少ない記述で、より強力な設定管理を実現します。

### なぜFleetFlow？

- **超シンプル**: Docker Composeと同等かそれ以下の記述量
- **可読性**: YAMLよりも読みやすいKDL構文
- **モジュール化**: include機能で設定を分割・再利用
- **統一管理**: 開発環境から本番環境まで同じツールで
- **OrbStack連携**: macOSでの開発体験を最適化
- **再起動ポリシー**: ホスト再起動後のコンテナ自動復旧
- **クラウド対応**: さくらのクラウド、Cloudflareなど複数プロバイダーをサポート

## クイックスタート

### インストール

```bash
cargo install --git https://github.com/chronista-club/fleetflow
```

### 基本的な使い方

1. `flow.kdl` を作成:

```kdl
// flow.kdl
project "myapp"

stage "local" {
    service "web"
    service "db"
}

service "web" {
    image "node:20-alpine"
    ports {
        port host=3000 container=3000
    }
    environment {
        NODE_ENV "development"
    }
}

service "db" {
    image "postgres:16"
    restart "unless-stopped"
    ports {
        port host=5432 container=5432
    }
    environment {
        POSTGRES_PASSWORD "password"
    }
}
```

2. 起動:

```bash
# ステージを起動
fleetflow up local

# ログを確認
fleetflow logs

# 状態を確認
fleetflow ps

# 停止
fleetflow down local
```

## 特徴

### 1. KDLベースの直感的な記述

YAMLの冗長さから解放され、読みやすく書きやすい設定ファイルを実現。

```kdl
service "api" {
    image "myapp:latest"
    ports {
        port host=8080 container=8080
    }
    environment {
        DATABASE_URL "postgresql://localhost/mydb"
        REDIS_URL "redis://localhost:6379"
    }
}
```

### 2. ステージベースの環境管理

開発（local）、検証（dev）、本番（prd）など、複数の環境を1つのファイルで管理。

```kdl
project "myapp"

stage "local" {
    service "api"
    service "db"
    variables {
        LOG_LEVEL "debug"
    }
}

stage "prd" {
    service "api"
    service "db"
    variables {
        LOG_LEVEL "info"
    }
}
```

### 3. Dockerビルド機能

Dockerfileからのイメージビルドをサポート。規約ベースの自動検出と明示的指定の両方に対応。

```kdl
// 規約ベース: ./services/api/Dockerfile を自動検出
service "api" {
    build_args {
        NODE_VERSION "20"
    }
}

// 明示的指定
service "worker" {
    dockerfile "./backend/worker/Dockerfile"
    context "./backend"
    target "production"  // マルチステージビルド対応
}
```

### 4. OrbStack連携

macOSのOrbStackと連携し、プロジェクト・ステージごとにコンテナをグループ化。

- コンテナ名: `{project}-{stage}-{service}`
- OrbStackグループ: `{project}-{stage}`

```
📁 vantage-local
  ├── surrealdb
  ├── qdrant
  └── api

📁 fleetflow-dev
  ├── postgres
  └── redis
```

### 5. 自動設定読み込み

`flow.kdl` または `flow/` ディレクトリ内の `.kdl` ファイルを自動検出。

```
project/
├── flow.kdl              # 単一ファイル
# または
├── flow/
│   ├── main.kdl         # メイン設定
│   ├── services.kdl     # サービス定義
│   └── stages.kdl       # ステージ定義
```

### 6. クラウドインフラ管理

複数のクラウドプロバイダーをKDLで宣言的に管理。

```kdl
providers {
    sakura-cloud { zone "tk1a" }
    cloudflare { account-id env="CF_ACCOUNT_ID" }
}

stage "dev" {
    // さくらのクラウドでサーバー作成
    server "app-server" {
        provider "sakura-cloud"
        plan core=4 memory=4
        disk size=100 os="ubuntu-24.04"
    }

    // Cloudflare DNSを自動設定
    dns "example.com" {
        provider "cloudflare"
        record "api" type="A" value=server.app-server.ip
    }
}
```

## コマンド

```bash
# ステージを起動
fleetflow up <stage>

# ステージを停止
fleetflow down <stage>

# サービスを再起動
fleetflow restart <stage> [service]

# サービスを停止（コンテナは保持）
fleetflow stop <stage> [service]

# サービスを起動（停止中のコンテナ）
fleetflow start <stage> [service]

# ログを表示
fleetflow logs [--follow] [--lines N] [service]

# コンテナ一覧
fleetflow ps [--all]

# 設定を検証
fleetflow validate

# イメージをビルド
fleetflow build [service] <stage>

# イメージを再ビルドして再起動
fleetflow rebuild <service> [stage]

# クラウドインフラ管理
fleetflow cloud up --stage <stage>
fleetflow cloud down --stage <stage>

# バージョン表示
fleetflow version
```

## プロジェクト構造

```
fleetflow/
├── crates/
│   ├── fleetflow-cli/              # CLIエントリーポイント
│   ├── fleetflow-atom/             # KDLパーサー・データモデル
│   ├── fleetflow-container/        # コンテナ操作
│   ├── fleetflow-config/           # 設定管理
│   ├── fleetflow-build/            # Dockerビルド機能
│   ├── fleetflow-cloud/            # クラウドインフラ抽象化
│   ├── fleetflow-cloud-sakura/     # さくらクラウド連携
│   └── fleetflow-cloud-cloudflare/ # Cloudflare連携
├── spec/                           # 仕様書（What & Why）
├── design/                         # 設計書（How）
└── guides/                         # 利用ガイド（Usage）
```

## ロードマップ

### Phase 1: MVP ✅
- [x] KDLパーサーの実装
- [x] 基本的なCLIコマンド（up/down/ps/logs）
- [x] Docker API統合（bollard）
- [x] OrbStack連携
- [x] 自動イメージpull

### Phase 2: ビルド機能 ✅
- [x] Dockerビルド機能（fleetflow-build）
- [x] 個別サービス操作（start/stop/restart）
- [x] 複数設定ファイル対応
- [x] マルチステージビルド対応

### Phase 3: クラウドインフラ 🚧
- [x] クラウドプロバイダー抽象化
- [x] さくらクラウド連携（usacloud）
- [x] Cloudflare連携
- [x] DNS自動管理（Cloudflare）
- [ ] CLI統合

### Phase 4: 拡張機能
- [ ] 環境変数の参照
- [ ] 変数定義と展開
- [ ] 環境継承（include-from）
- [ ] ヘルスチェック機能

## 技術スタック

- **言語**: Rust (Edition 2024)
- **パーサー**: `kdl` crate
- **コンテナAPI**: `bollard` (Docker API client)
- **CLI**: `clap`
- **非同期**: `tokio`

## 開発に参加する

Issue、Pull Requestは大歓迎です！

### 開発環境のセットアップ

```bash
git clone https://github.com/chronista-club/fleetflow.git
cd fleetflow
cargo build
cargo test
```

### 開発コマンド

```bash
# テスト実行
cargo test

# リント
cargo clippy

# フォーマット
cargo fmt

# ローカルインストール
cargo install --path crates/fleetflow-cli
```

## ドキュメント

- [仕様書](spec/) - 機能の詳細仕様（What & Why）
  - [Core Concepts](spec/01-core-concepts.md) - 基本概念
  - [KDL Parser](spec/02-kdl-parser.md) - パーサー仕様
  - [CLI Commands](spec/03-cli-commands.md) - コマンド仕様
  - [OrbStack Integration](spec/06-orbstack-integration.md) - OrbStack連携
  - [Docker Build](spec/07-docker-build.md) - ビルド機能
  - [Cloud Infrastructure](spec/08-cloud-infrastructure.md) - クラウドインフラ
  - [DNS Integration](spec/09-dns-integration.md) - DNS連携
- [設計書](design/) - 実装の設計詳細（How）
- [利用ガイド](guides/) - ユースケース別のガイド（Usage）

## ライセンス

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

## 関連リンク

- [KDL - The KDL Document Language](https://kdl.dev/)
- [kdl-rs](https://github.com/kdl-org/kdl-rs)
- [bollard](https://docs.rs/bollard/) - Docker API client for Rust

---

**FleetFlow** - シンプルに、統一的に、環境を構築する。
