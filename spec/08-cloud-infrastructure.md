# FleetFlow Cloud Infrastructure

## 概要

FleetFlowにクラウドインフラ管理機能を追加し、**複数のクラウドプロバイダー**をKDLで宣言的に管理する。

## 背景と目的

### Why（なぜ必要か）

- 現在のFleetFlowはローカルDocker（OrbStack）のみ対象
- クラウドへのデプロイは手作業（usacloudコマンド、SSH、手順書）
- インフラとアプリを統一的に管理したい
- Terraformのような「宣言した状態に収束させる」動作を実現

### ゴール

- `fleetflow up --stage dev` でクラウド環境を宣言的に構築
- さくらのクラウド、Cloudflareなど複数プロバイダー対応
- stageによってデプロイターゲットが変わる設計

## 対応プロバイダー

| プロバイダー | サービス | 用途 |
|-------------|---------|------|
| **さくらのクラウド** | Server, Disk | コンピュート |
| **Cloudflare** | R2 | オブジェクトストレージ |
| | Workers | エッジコンピュート |
| | DNS | ドメイン管理 |
| | Pages | 静的サイトホスティング |

## スコープ

### Phase 0（初期実装）

- さくらのクラウドのサーバー作成・削除
- SSH鍵設定
- 基本的なサーバースペック定義
- 1サーバー1スタック構成

### Phase 1

- Cloudflare R2バケット管理
- Cloudflare DNS管理
- 複数サーバー対応

### Phase 2

- Cloudflare Workers対応
- 水平スケール（`scale`ノード）

## KDL構文定義

### providers

```kdl
providers {
    sakura-cloud {
        zone "tk1a"
        // 認証はusacloud configから自動取得
    }

    cloudflare {
        account-id env="CF_ACCOUNT_ID"
        // 認証は環境変数から
    }
}
```

### server（さくらのクラウド）

```kdl
server "creo-dev-01" {
    provider "sakura-cloud"
    plan core=4 memory=4
    disk size=100 os="ubuntu-24.04"
    ssh-key "~/.ssh/id_ed25519.pub"
    deploy-services "creo-stack"
}
```

### scale（将来）

```kdl
scale "creo-dev-01" {
    min 1
    max 3
    cpu-threshold 80
    cooldown 300
}
```

### service-group

```kdl
service-group "creo-stack" {
    service "surrealdb" {
        image "surrealdb/surrealdb:v1.5.4"
        port 8000
        volume "surrealdb-data:/data"
    }

    service "qdrant" {
        image "qdrant/qdrant:latest"
        port 6333
    }

    service "mcp-server" {
        build "./apps/creo-mcp-server"
        port 3000
        depends-on "surrealdb" "qdrant"
    }
}
```

### r2-bucket（Cloudflare R2）

```kdl
r2-bucket "creo-attachments" {
    provider "cloudflare"
    location "APAC"

    cors {
        allowed-origins "https://api.creo-memories.com"
        allowed-methods "GET" "PUT" "POST" "DELETE"
    }

    custom-domain "cdn.creo-memories.com"
}
```

### worker（Cloudflare Workers）

```kdl
worker "api-gateway" {
    provider "cloudflare"
    source "./workers/api-gateway"
    route "api.creo-memories.com/*"

    bindings {
        r2 "ATTACHMENTS" bucket="creo-attachments"
        kv "CACHE" namespace="creo-cache"
    }
}
```

### dns（Cloudflare DNS）

```kdl
dns "creo-memories.com" {
    provider "cloudflare"

    record "api" type="A" {
        value "xxx.xxx.xxx.xxx"  // さくらのクラウドIP
        proxied true
    }

    record "cdn" type="CNAME" {
        value "creo-attachments.r2.cloudflarestorage.com"
        proxied true
    }
}
```

## 完全な例（flow.kdl）

```kdl
providers {
    sakura-cloud { zone "tk1a" }
    cloudflare { account-id env="CF_ACCOUNT_ID" }
}

stage "dev" {
    // コンピュート（さくらのクラウド）
    server "creo-dev-01" {
        provider "sakura-cloud"
        plan core=4 memory=4
        disk size=100 os="ubuntu-24.04"
        deploy-services "creo-stack"
    }

    // ストレージ（Cloudflare R2）
    r2-bucket "creo-attachments" {
        provider "cloudflare"
        custom-domain "cdn.creo-memories.com"
    }

    // DNS（Cloudflare）
    dns "creo-memories.com" {
        provider "cloudflare"
        record "api" type="A" value=server.creo-dev-01.ip proxied=true
    }
}

service-group "creo-stack" {
    service "surrealdb" {
        image "surrealdb/surrealdb:v1.5.4"
        port 8000
    }
    service "qdrant" {
        image "qdrant/qdrant:latest"
        port 6333
    }
    service "mcp-server" {
        build "./apps/creo-mcp-server"
        port 3000
    }
}
```

## コマンド

```bash
# 環境を構築・更新
fleetflow up --stage dev

# 環境を削除
fleetflow down --stage dev

# 差分を確認（dry-run）
flow plan --stage dev

# 状態を確認
flow status --stage dev
```

## 設計原則

1. **フラットアプローチ**: server, scale, service-groupを分離して定義
2. **水平スケール志向**: scaleノードで将来の拡張に備える
3. **プロバイダー抽象化**: 複数クラウドを統一的に管理
4. **宣言的収束**: 現状と宣言の差分を計算して適用
