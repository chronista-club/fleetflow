---
name: fleetflow
description: FleetFlow（KDLベースのコンテナオーケストレーションツール）を効果的に使用するためのガイド
version: 0.2.5
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
| イメージプッシュ | ビルド後のレジストリプッシュを自動化 |
| サービスマージ | 複数ファイルでの設定オーバーライド |
| クラウド対応 | さくらのクラウド、Cloudflareなど複数プロバイダー |
| DNS自動管理 | Cloudflare DNSとの自動連携 |

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
    image "postgres:16"  // image は必須
    ports {
        port 5432 5432
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
| `build <stage> [-n service]` | イメージをビルド |
| `build <stage> --push [--tag <tag>]` | ビルド＆レジストリへプッシュ |
| `rebuild <service> [stage]` | リビルドして再起動 |
| `validate` | 設定を検証 |
| `cloud up --stage <stage>` | クラウド環境を構築 |
| `cloud down --stage <stage>` | クラウド環境を削除 |
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
    image "postgres:16"     // 必須
    ports { ... }
    env { ... }
    volumes { ... }
    build { ... }           // Dockerビルド設定
    healthcheck { ... }     // ヘルスチェック設定
}

// クラウドインフラ（オプション）
providers {
    sakura-cloud { zone "tk1a" }
    cloudflare { account-id env="CF_ACCOUNT_ID" }
}

server "app-server" {       // クラウドサーバー定義
    provider "sakura-cloud"
    plan core=4 memory=4
}
```

詳細: [reference/kdl-syntax.md](reference/kdl-syntax.md)

## 重要な仕様

### imageフィールドは必須

v0.2.4以降、`image`フィールドは**必須**です。省略するとエラーになります：

```kdl
// 正しい定義
service "db" {
    image "postgres:16"
}

// エラー: imageが必須
service "db" {
    version "16"  // これだけではダメ
}
// Error: サービス 'db' に image が指定されていません
```

### サービスマージ機能

複数ファイルで同じサービスを定義すると、設定がマージされます：

```kdl
// flow.kdl（ベース設定）
service "api" {
    image "myapp:latest"
    ports { port 8080 3000 }
    env { NODE_ENV "production" }
}

// flow.local.kdl（ローカルオーバーライド）
service "api" {
    env { DATABASE_URL "localhost:5432" }
}

// 結果:
// - image: "myapp:latest" (保持)
// - ports: [8080:3000] (保持)
// - env: { NODE_ENV: "production", DATABASE_URL: "localhost:5432" } (マージ)
```

**マージルール**:

| フィールドタイプ | ルール |
|----------------|--------|
| `Option<T>` | 後の定義が`Some`なら上書き、`None`なら保持 |
| `Vec<T>` | 後の定義が空でなければ上書き、空なら保持 |
| `HashMap<K, V>` | 両方をマージ（後の定義が優先） |

### Dockerビルド機能

規約ベースの自動検出と明示的指定の両方に対応：

```kdl
// 規約ベース: ./services/api/Dockerfile を自動検出
service "api" {
    image "myapp/api:latest"
    build_args {
        NODE_VERSION "20"
    }
}

// 明示的指定
service "worker" {
    image "myapp/worker:latest"
    dockerfile "./backend/worker/Dockerfile"
    context "./backend"
    target "production"  // マルチステージビルド
}
```

### イメージプッシュ機能

ビルドしたイメージをレジストリにプッシュ：

```bash
# ビルドのみ
fleetflow build local -n api

# ビルド＆プッシュ
fleetflow build local -n api --push

# タグを指定してビルド＆プッシュ
fleetflow build local -n api --push --tag v1.0.0
```

**認証方式**:
- Docker標準の `~/.docker/config.json` から認証情報を取得
- credential helper（osxkeychain, desktop など）も自動対応
- 環境変数 `DOCKER_CONFIG` でパスをカスタマイズ可能

**対応レジストリ**:
- Docker Hub (docker.io)
- GitHub Container Registry (ghcr.io)
- Amazon ECR (*.dkr.ecr.*.amazonaws.com)
- Google Container Registry (gcr.io)
- プライベートレジストリ (localhost:5000 など)

**タグ解決の優先順位**:
1. `--tag` CLIオプション
2. KDL設定の `image` フィールドのタグ
3. デフォルト: `latest`

### クラウドインフラ管理

複数のクラウドプロバイダーをKDLで宣言的に管理：

```kdl
providers {
    sakura-cloud { zone "tk1a" }
    cloudflare { account-id env="CF_ACCOUNT_ID" }
}

stage "dev" {
    server "app-server" {
        provider "sakura-cloud"
        plan core=4 memory=4
        disk size=100 os="ubuntu-24.04"
        dns_aliases "app" "api"  // DNSエイリアス
    }
}
```

### DNS自動管理（Cloudflare）

`cloud up`/`cloud down`時にDNSレコードを自動管理：

- サーバー作成時: `{service}-{stage}.{domain}` のAレコードを自動追加
- サーバー削除時: DNSレコードを自動削除
- `dns_aliases`でCNAMEエイリアスも自動作成

必要な環境変数:
- `CLOUDFLARE_API_TOKEN`: Cloudflare APIトークン
- `CLOUDFLARE_ZONE_ID`: ドメインのZone ID

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

- プロジェクトにFleetFlowを導入する際
- `flow.kdl` 設定ファイルを作成・編集する際
- コンテナ環境の構築・管理を行う際
- ローカル開発環境のセットアップ時
- クラウドインフラを宣言的に管理する際

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
