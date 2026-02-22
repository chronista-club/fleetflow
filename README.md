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
curl -sSf https://raw.githubusercontent.com/chronista-club/fleetflow/main/install.sh | sh
```

> **注意**: インストール後、Claude Code との連携設定で `fleet mcp` コマンドを使用します:
> ```bash
> claude mcp add fleetflow -- fleet mcp
> ```

### 2. 設定ファイルの作成

`.fleetflow/fleet.kdl` を作成します（設定例は下記参照）。

### 3. 起動

```bash
# ステージを起動
fleet up local
```

設定ファイルが存在しない状態でコマンドを実行すると、対話的な初期化ウィザードが起動します。

---

## CLI コマンド

### ステージのライフサイクル

```bash
# ステージを起動（ステージ省略時はデフォルトステージを使用）
fleet up [stage]              # コンテナを起動
fleet up local --pull         # 起動前に最新イメージをpull
fleet down [stage]            # コンテナを停止
fleet down local --remove     # 停止 + コンテナ削除
```

### 個別サービスの操作

```bash
fleet start <service>         # サービスを起動
fleet stop <service>          # サービスを停止
fleet restart <service>       # サービスを再起動
fleet exec -n <service> -- <command>  # コンテナ内でコマンド実行（省略時: /bin/sh）
```

`--stage` / `-s` オプションまたは `FLEET_STAGE` 環境変数でステージ指定が可能:

```bash
fleet start db --stage local
fleet exec -n app -s local -- npm run migrate
FLEET_STAGE=local fleet restart app
```

### 状態確認

```bash
fleet ps [stage]              # コンテナ一覧（全ステージ or 指定ステージ）
fleet logs [stage]            # ログ表示
fleet logs local -n app       # 特定サービスのログ
fleet logs local --follow     # リアルタイム追跡
```

### ビルド・デプロイ

```bash
fleet build [stage]                 # Dockerイメージをビルド
fleet build local -n app            # 特定サービスのみビルド
fleet build local --push --registry ghcr.io/owner  # ビルド＋レジストリにプッシュ
fleet deploy [stage]                # デプロイ（CI/CD向け: 強制停止→最新イメージ→再起動）
fleet deploy local --yes            # 確認なしで実行
```

### ステージ管理（stage サブコマンド）

`stage` サブコマンドはインフラとコンテナを統一的に操作します:

```bash
fleet stage up <stage>        # ステージを起動（インフラ＋コンテナ）
fleet stage down <stage>      # ステージを停止
fleet stage down <stage> --suspend   # サーバー電源をOFF（リモート）
fleet stage down <stage> --destroy --yes  # サーバーを削除（課金完全停止）
fleet stage status <stage>    # ステージの状態を表示
fleet stage logs <stage>      # ステージのログを表示
fleet stage ps [stage]        # コンテナ一覧
```

### Playbook（リモートサーバー操作）

```bash
fleet play <playbook>         # Playbookを実行
fleet play deploy-app --yes   # 確認なしで実行
```

### Fleet Registry（複数プロジェクト統合管理）

```bash
fleet registry list           # 全fleetとサーバーの一覧
fleet registry status         # 各fleet x serverの稼働状態
fleet registry deploy <fleet> # Registry定義に従ってSSH経由でデプロイ
fleet registry deploy <fleet> -s live --yes  # ステージ指定＋確認なし
```

### 検証・AI連携・その他

```bash
fleet validate                # 設定ファイルを検証
fleet mcp                     # MCPサーバーを起動（AI エージェント連携）
fleet version                 # バージョン情報を表示
fleet self-update             # FleetFlow自体を最新版に更新
```

---

## 設定ファイル

### fleet.kdl

```kdl
// ステージ定義
stage "local" {
    service "postgres"
    service "redis"
    service "app"
    variables {
        APP_ENV "development"
        DEBUG "true"
    }
}

stage "live" {
    service "postgres"
    service "redis"
    variables {
        APP_ENV "production"
        DEBUG "false"
    }
}

// サービス定義
service "postgres" {
    version "16"
    ports {
        port host=11432 container=5432
    }
    environment {
        POSTGRES_USER "flowuser"
        POSTGRES_PASSWORD "flowpass"
        POSTGRES_DB "flowdb"
    }
    volumes {
        volume "./data/postgres" "/var/lib/postgresql/data"
    }
}

service "redis" {
    version "7"
    ports {
        port host=11379 container=6379
    }
    volumes {
        volume "./data/redis" "/data"
    }
}

service "app" {
    image "myapp"
    version "latest"
    ports {
        port host=11080 container=8080
    }
    environment {
        DATABASE_URL "postgresql://flowuser:flowpass@postgres:5432/flowdb"
        REDIS_URL "redis://redis:6379"
        APP_ENV "development"
    }
    depends_on "postgres" "redis"
}
```

### ディレクトリ構造（.fleetflow/）

```
.fleetflow/
├── fleet.kdl      # メイン設定ファイル
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
│   ├── fleetflow/                 # CLI エントリーポイント (bin: fleet)
│   ├── fleetflow-core/            # KDL パーサー・データモデル
│   ├── fleetflow-config/          # 設定ファイル管理
│   ├── fleetflow-container/       # コンテナ操作
│   ├── fleetflow-build/           # Docker ビルド
│   ├── fleetflow-cloud/           # クラウドインフラ抽象化
│   ├── fleetflow-cloud-sakura/    # さくらのクラウド プロバイダー
│   ├── fleetflow-cloud-cloudflare/# Cloudflare プロバイダー
│   ├── fleetflow-mcp/             # AI エージェント連携 (MCP)
│   └── fleetflow-registry/        # 複数fleet統合管理
├── examples/                      # 設定ファイルのサンプル
├── spec/                          # 仕様書 (What & Why)
├── design/                        # 設計書 (How)
└── guides/                        # 利用ガイド (Usage)
```

## ライセンス

Licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).

---

**FleetFlow** - シンプルに、統一的に、環境を構築する。
