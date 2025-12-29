# FleetFlow

> **環境構築は、対話になった。伝えれば、動く。**

[![CI](https://github.com/chronista-club/fleetflow/actions/workflows/ci.yml/badge.svg)](https://github.com/chronista-club/fleetflow/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)

## コンセプト

FleetFlow は、KDL (KDL Document Language) をベースにした革新的な環境構築ツールです。
「設定より規約 (Convention over Configuration)」を徹底し、最小限の記述でローカル開発から本番デプロイまでをシームレスにつなぎます。

### なぜ FleetFlow？

- **超シンプル**: Docker Compose 同等かそれ以下の記述量
- **AI ネイティブ**: MCP (Model Context Protocol) を標準サポート。AI があなたの代わりにインフラを操作
- **環境統一**: 開発から本番まで同じ設定ファイルで管理
- **美しい構文**: YAML のインデント地獄から解放される、構造的で読みやすい KDL 構文

---

## クイックスタート

### 1. インストール

```bash
# Homebrew (macOS)
brew install chronista-club/tap/fleetflow

# または curl
curl -sSf https://raw.githubusercontent.com/chronista-club/fleetflow/main/install.sh | sh
```

### 2. プロジェクトの初期化

```bash
mkdir my-project && cd my-project
fleet init
```

### 3. 環境のセットアップと起動

```bash
# 環境をセットアップ（初回）
fleet setup local

# 起動
fleet up local
```

---

## CLI コマンド

```bash
# セットアップ（冪等）
fleet setup <stage>    # ステージのインフラを構築

# ライフサイクル
fleet up <stage>       # ステージを起動
fleet down <stage>     # ステージを停止
fleet restart <stage>  # ステージを再起動
fleet deploy <stage>   # デプロイ（CI/CD向け）

# 状態確認
fleet ps               # コンテナ一覧
fleet logs             # ログ表示

# ビルド
fleet build <stage>    # Dockerイメージをビルド

# AI連携
fleet mcp              # MCPサーバーを起動

# 検証
fleet validate         # 設定ファイルを検証
```

---

## 設定ファイル

### flow.kdl

```kdl
project "my-app"

// ステージ定義
stage "local" {
    service "app"
    service "db"
}

stage "dev" {
    server "my-dev-server"
    service "app"
    service "db"
}

// サービス定義
service "app" {
    image "node:22-alpine"
    ports {
        port host=3000 container=3000
    }
    env {
        NODE_ENV "{{ NODE_ENV }}"
    }
}

service "db" {
    image "postgres:16"
    ports {
        port host=5432 container=5432
    }
    volumes {
        volume host="{{ PROJECT_ROOT }}/data/postgres" container="/var/lib/postgresql/data"
    }
}
```

### 環境変数（.fleetflow/）

```
.fleetflow/
├── .env           # グローバル（共通）
├── .env.local     # local 固有
├── .env.dev       # dev 固有
└── .env.live      # live 固有
```

---

## プロジェクト構造

```
fleetflow/
├── crates/
│   ├── fleetflow/              # CLI エントリーポイント (bin: fleet)
│   ├── fleetflow-core/         # KDL パーサー・データモデル
│   ├── fleetflow-container/    # コンテナ操作
│   ├── fleetflow-build/        # Docker ビルド
│   ├── fleetflow-cloud/        # クラウドインフラ抽象化
│   └── fleetflow-mcp/          # AI エージェント連携 (MCP)
├── spec/                       # 仕様書 (What & Why)
├── design/                     # 設計書 (How)
└── guides/                     # 利用ガイド (Usage)
```

## ライセンス

Licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).

---

**FleetFlow** - シンプルに、統一的に、環境を構築する。
