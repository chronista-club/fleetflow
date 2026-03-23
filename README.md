# FleetFlow

KDL で設定を書き、ローカルから本番まで同じファイルでコンテナ環境を管理する。

[![CI](https://github.com/chronista-club/fleetflow/actions/workflows/ci.yml/badge.svg)](https://github.com/chronista-club/fleetflow/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/chronista-club/fleetflow/graph/badge.svg)](https://codecov.io/gh/chronista-club/fleetflow)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)

## インストール・更新

```bash
# インストール
curl -sSf https://raw.githubusercontent.com/chronista-club/fleetflow/main/install.sh | sh

# 更新
fleet self-update
```

---

## fleet up すると何が起こるか

`fleet up local` を実行すると、FleetFlow は以下の流れでコンテナを起動する。

```mermaid
flowchart LR
    A["fleet up local"] --> B["fleet.kdl を読む"]
    B --> C["local ステージの\nサービスを解決"]
    C --> D["Docker ネットワーク作成"]
    D --> E["依存順にコンテナ起動\npostgres → redis → app"]
```

1. `.fleetflow/fleet.kdl` をパースし、指定ステージのサービス定義を取得する
2. `depends_on` を解析して起動順序を決定する
3. ステージ専用の Docker ネットワークを作成する
4. 依存順にコンテナを起動する（postgres → redis → app）

Docker Compose は使わない。Docker API（Bollard）を直接操作している。

設定ファイルがない状態で実行すると、対話的な初期化ウィザード（TUI）が起動する。

---

## 設定ファイル

プロジェクトルートに `.fleetflow/fleet.kdl` を作成する。

```kdl
// ステージ: どの環境でどのサービスを動かすか
stage "local" {
    service "postgres"
    service "redis"
    service "app"
    variables {
        APP_ENV "development"
    }
}

stage "live" {
    service "postgres"
    service "redis"
    variables {
        APP_ENV "production"
    }
}

// サービス: 各コンテナの定義
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
    }
    depends_on "postgres" "redis"
}
```

環境変数は `.env` ファイルでステージごとに分離できる:

```
.fleetflow/
├── fleet.kdl      # メイン設定
├── .env           # 全ステージ共通
├── .env.local     # local 固有
├── .env.dev       # dev 固有
└── .env.live      # live 固有
```

---

## コマンド

### 日常操作（Daily）

```bash
fleet up [stage]              # ステージを起動
fleet up local --pull         # 最新イメージを pull してから起動
fleet up local --dry-run      # 実行せず計画のみ表示（設定検証にも使える）
fleet down [stage]            # 停止
fleet down local --remove     # 停止 + コンテナ削除
fleet restart [stage]         # 再起動
fleet restart -n web          # 特定サービスだけ再起動
fleet ps [stage]              # コンテナ一覧・状態表示
fleet logs [stage]            # ログ表示
fleet logs local -n app       # 特定サービスのログ
fleet logs local --follow     # リアルタイム追跡
fleet exec -n <svc> -- <cmd>  # コンテナ内でコマンド実行
```

ステージ指定は位置引数、`-s` フラグ、または `FLEET_STAGE` 環境変数:

```bash
fleet exec -n app -s local -- npm run migrate
```

### ビルド・デプロイ（Ship）

```bash
fleet build [stage]                                      # Docker イメージをビルド
fleet build local -n app --push --registry ghcr.io/owner # ビルド + push
fleet deploy [stage]                                     # デプロイ（停止 → pull → 再起動）
fleet deploy local --yes                                 # 確認なしで実行
```

### Control Plane 管理（CP）

`fleet cp` 配下に管理系コマンドを集約:

```bash
fleet cp login                # CP にログイン（Auth0 Device Flow）
fleet cp logout               # ログアウト
fleet cp auth                 # 認証状態を確認
fleet cp daemon start/stop    # デーモン管理
fleet cp tenant list/create   # テナント管理
fleet cp project list/create  # プロジェクト管理
fleet cp server list/register # サーバー管理
fleet cp cost list/summary    # コスト管理
fleet cp dns list/create/sync # DNS 管理
fleet cp remote deploy        # リモートデプロイ
fleet cp registry list/deploy # 複数 Fleet 統合管理
```

### ユーティリティ

```bash
fleet mcp           # MCP サーバーを起動（Claude Code 連携用）
fleet self-update    # FleetFlow を最新版に更新
fleet --version      # バージョン表示
```

---

## Claude Code 連携

MCP サーバーを内蔵しており、Claude Code から直接コンテナ操作ができる。

```bash
claude mcp add fleetflow -- fleet mcp
```

登録後は Claude Code 上で `fleet up`, `fleet logs`, `fleet deploy` などを AI 経由で実行できる。

---

## プロジェクト構成

```
fleetflow/
├── crates/
│   ├── fleetflow/                  # CLI (bin: fleet)
│   ├── fleetflow-core/             # KDL パーサー・データモデル
│   ├── fleetflow-config/           # 設定管理
│   ├── fleetflow-container/        # コンテナ操作 (Bollard)
│   ├── fleetflow-build/            # Docker ビルド
│   ├── fleetflow-cloud/            # クラウドインフラ抽象化
│   ├── fleetflow-cloud-sakura/     # さくらのクラウド
│   ├── fleetflow-cloud-cloudflare/ # Cloudflare
│   ├── fleetflow-mcp/              # MCP サーバー
│   ├── fleetflow-registry/         # 複数 fleet 管理
│   ├── fleetflow-controlplane/     # Control Plane ライブラリ
│   ├── fleetflowd/                 # CP デーモン
│   └── fleet-agent/                # サーバーエージェント
├── docs/
│   └── guide/                     # 利用ガイド
```

## ライセンス

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE)
