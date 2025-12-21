# ユースケース: Creo Memories

## 概要

Creo Memoriesは、AIエージェント向けのメモリシステムで、SurrealDB（構造化データ）、Qdrant（ベクトル検索）、MinIO（S3互換ストレージ）、PLaMo Embedding（日本語埋め込み）を組み合わせたハイブリッドアーキテクチャを採用しています。

FleetFlowを使用して、ローカル開発からクラウド本番まで一貫した環境定義を実現しています。

## プロジェクト構成

```
creo-memories/
├── .fleetflow/
│   ├── flow.kdl         # メイン設定
│   └── flow.local.kdl   # 機密情報（gitignore）
├── packages/            # ライブラリ
├── apps/               # アプリケーション
├── services/           # Dockerイメージ
└── data/               # 永続化データ
```

## サービス構成

| サービス | イメージ | 用途 |
|---------|---------|------|
| surrealdb | surrealdb/surrealdb:v2.4.0 | メタデータ・関係性DB |
| qdrant | qdrant/qdrant:latest | ベクトル検索 |
| minio | minio/minio:latest | S3互換ストレージ |
| plamo-embedding | creo-plamo-embedding:local | 日本語埋め込み生成 |
| creo-mcp-server | creo-mcp-server:latest | MCPサーバー |
| nginx | nginx:alpine | リバースプロキシ |

## FleetFlow設定

### flow.kdl

```kdl
project "creo-memories"

// ローカル開発環境
stage "local" {
    service "surrealdb"
    service "qdrant"
    service "minio"
    service "plamo-embedding"
    service "creo-mcp-server"
}

// 開発環境（nginx付き）
stage "dev" {
    service "surrealdb"
    service "qdrant"
    service "minio"
    service "plamo-embedding"
    service "creo-mcp-server"
    service "nginx"
}

// 本番環境
stage "prod" {
    service "surrealdb"
    service "qdrant"
    service "minio"
    service "plamo-embedding"
    service "creo-mcp-server"
    service "nginx"
}

// SurrealDB
service "surrealdb" {
    image "surrealdb/surrealdb"
    version "v2.4.0"

    ports {
        port host=12000 container=8000
    }

    environment {
        SURREAL_LOG "trace"
    }

    command "start --log trace --user root --pass root --bind 0.0.0.0:8000 rocksdb:///data/database.db"

    volumes {
        volume host="./data/surrealdb" container="/data"
    }
}

// Qdrant
service "qdrant" {
    image "qdrant/qdrant"
    version "latest"

    ports {
        port host=12001 container=6333
        port host=12002 container=6334
    }

    volumes {
        volume host="./data/qdrant" container="/qdrant/storage"
    }
}

// MinIO
service "minio" {
    image "minio/minio"
    version "latest"

    ports {
        port host=12066 container=9000  // S3 API
        port host=12067 container=9001  // Web Console
    }

    environment {
        MINIO_ROOT_USER "minioadmin"
        MINIO_ROOT_PASSWORD "minioadmin"
    }

    command "server /data --console-address :9001"

    volumes {
        volume host="./data/minio" container="/data"
    }
}

// PLaMo Embedding
service "plamo-embedding" {
    image "creo-plamo-embedding"
    version "local"

    ports {
        port host=12003 container=8080
    }

    environment {
        MODEL_NAME "pfnet/plamo-embedding-1b"
        DEVICE "cpu"
        MAX_LENGTH "1024"
        HOST "0.0.0.0"
        PORT "8080"
    }

    volumes {
        volume host="./data/hf-cache" container="/root/.cache/huggingface"
    }
}

// Creo MCP Server
service "creo-mcp-server" {
    image "creo-mcp-server"
    version "latest"

    ports {
        port host=12080 container=3000
    }

    environment {
        EMBEDDING_PROVIDER "http"
        PLAMO_EMBEDDING_URL "http://plamo-embedding:8080"
        SURREALDB_URL "ws://surrealdb:8000/rpc"
        SURREALDB_NAMESPACE "creo"
        SURREALDB_DATABASE "memories"
        SURREALDB_USERNAME "root"
        SURREALDB_PASSWORD "root"
        QDRANT_URL "http://qdrant:6333"
        QDRANT_COLLECTION "memories"
        MCP_SERVER_PORT "3000"
        NODE_ENV "development"
    }
}

// Nginx
service "nginx" {
    image "nginx"
    version "alpine"

    ports {
        port host=80 container=80
        port host=443 container=443
    }

    volumes {
        volume host="./nginx/nginx.conf" container="/etc/nginx/nginx.conf" read_only=#true
        volume host="./nginx/conf.d" container="/etc/nginx/conf.d" read_only=#true
    }
}
```

### flow.local.kdl（機密情報）

```kdl
service "creo-mcp-server" {
    environment {
        AUTH0_DOMAIN "your-tenant.auth0.com"
        AUTH0_CLIENT_ID "your-client-id"
        AUTH0_CLIENT_SECRET "your-client-secret"
        SESSION_SECRET "your-session-secret"
    }
}
```

## 運用フロー

### 起動

```bash
# ローカル開発環境
fleetfleetflow up --stage local

# 本番環境
fleetfleetflow up --stage prod
```

### 状態確認

```bash
fleetfleetflow ps
```

### 停止

```bash
fleetfleetflow down --stage local
```

## 発見した課題と対策

### 1. 環境変数キーワード

**問題**: `env` ブロックが認識されず、環境変数がコンテナに渡らなかった

**原因**: KDL v2構文では `env` ではなく `environment` を使用

**対策**: ドキュメントを確認し、正しいキーワード `environment` を使用

```kdl
// NG
env {
    KEY "value"
}

// OK
environment {
    KEY "value"
}
```

### 2. ネットワーク接続

**問題**: コンテナ間でホスト名解決ができない（`ws://surrealdb:8000` が接続できない）

**原因**: FleetFlowが自動的にDockerネットワークを作成・接続しない

**対策**: 手動でネットワークを作成し接続

```bash
# ネットワーク作成
docker network create creo-memories-local

# 各コンテナを接続（aliasでホスト名解決可能に）
docker network connect --alias surrealdb creo-memories-local creo-memories-local-surrealdb
docker network connect --alias qdrant creo-memories-local creo-memories-local-qdrant
docker network connect --alias minio creo-memories-local creo-memories-local-minio
docker network connect --alias plamo-embedding creo-memories-local creo-memories-local-plamo-embedding
docker network connect --alias creo-mcp-server creo-memories-local creo-memories-local-creo-mcp-server
```

### 3. KDL v2ブール値

**問題**: `true` がブール値として認識されない

**原因**: KDL v2では `#true` / `#false` を使用

**対策**:

```kdl
// NG
volume host="./conf" container="/etc/conf" read_only=true

// OK
volume host="./conf" container="/etc/conf" read_only=#true
```

## 改善提案

### P1: ネットワーク自動設定

各stageでコンテナ間通信用のネットワークを自動作成し、サービスを自動接続してほしい。

```kdl
stage "local" {
    // network設定がなければ、"creo-memories-local" を自動作成
    // 各serviceを自動的にネットワークに接続
    // aliasはサービス名をそのまま使用

    service "surrealdb"
    service "qdrant"
    service "creo-mcp-server"
}
```

期待動作：
- `fleetfleetflow up --stage local` で自動的にDockerネットワークを作成
- 各サービスをネットワークに接続
- サービス名でDNS解決可能

### P2: usacloud/さくらのクラウド連携

`spec/08-cloud-infrastructure.md` で計画されているクラウド連携機能。creo-memoriesのユースケースでは以下が必要：

1. さくらのクラウドでのサーバー作成
2. SSH鍵設定
3. Dockerインストール自動化
4. `fleetfleetflow up --stage dev --remote` でリモートデプロイ

## 関連リンク

- [Creo Memories GitHub](https://github.com/chronista-club/creo-memories)
- [FleetFlow Cloud Infrastructure Spec](../spec/08-cloud-infrastructure.md)
- [FleetFlow Cloud Infrastructure Design](../design/04-cloud-infrastructure.md)
