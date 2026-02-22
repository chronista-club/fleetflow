# FleetFlow

KDL で設定を書き、ローカルから本番まで同じファイルでコンテナ環境を管理する。

[![CI](https://github.com/chronista-club/fleetflow/actions/workflows/ci.yml/badge.svg)](https://github.com/chronista-club/fleetflow/actions/workflows/ci.yml)
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

### コンテナ操作

```bash
fleet up [stage]              # ステージのコンテナを起動
fleet up local --pull         # 最新イメージを pull してから起動
fleet down [stage]            # 停止
fleet down local --remove     # 停止 + コンテナ削除
fleet ps [stage]              # コンテナ一覧
fleet logs [stage]            # ログ表示
fleet logs local -n app       # 特定サービスのログ
fleet logs local --follow     # リアルタイム追跡
```

### サービス単体

```bash
fleet start <service>                    # 起動
fleet stop <service>                     # 停止
fleet restart <service>                  # 再起動
fleet exec -n <service> -- <command>     # コンテナ内でコマンド実行
```

ステージ指定は `--stage` / `-s` または `FLEET_STAGE` 環境変数:

```bash
fleet exec -n app -s local -- npm run migrate
```

### ビルド・デプロイ

```bash
fleet build [stage]                                      # Docker イメージをビルド
fleet build local -n app --push --registry ghcr.io/owner # ビルド + push
fleet deploy [stage]                                     # デプロイ（停止 → pull → 再起動）
fleet deploy local --yes                                 # 確認なしで実行
```

### クラウド連携

`fleet stage` はクラウドサーバーの作成からコンテナのデプロイまでを一括で行う。さくらのクラウドと Cloudflare に対応。

```mermaid
flowchart LR
    A["fleet stage up live"] --> B["クラウドにサーバー作成"]
    B --> C["SSH で接続"]
    C --> D["Docker セットアップ"]
    D --> E["コンテナをデプロイ"]
```

```bash
fleet stage up <stage>        # サーバー作成 + コンテナ起動
fleet stage down <stage>      # コンテナ停止
fleet stage status <stage>    # 状態表示
```

### Playbook

リモートサーバーへの操作手順を KDL で定義し、再実行する:

```bash
fleet play <playbook>         # 実行
fleet play deploy-app --yes   # 確認なし
```

### Registry

複数プロジェクトの fleet を一元管理する:

```bash
fleet registry list                              # 全 fleet とサーバーの一覧
fleet registry deploy <fleet> -s live --yes      # SSH 経由でデプロイ
```

### その他

```bash
fleet validate      # 設定ファイルを検証
fleet mcp           # MCP サーバーを起動（Claude Code 連携用）
fleet version       # バージョン表示
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
│   └── fleetflow-registry/         # 複数 fleet 管理
├── examples/                       # 設定ファイルのサンプル
├── spec/                           # 仕様書
├── design/                         # 設計書
└── guides/                         # 利用ガイド
```

## ライセンス

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE)
