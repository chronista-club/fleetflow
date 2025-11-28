# 設定パターン集

FleetFlowの実践的な設定パターンです。

## フルスタックWebアプリケーション

フロントエンド + バックエンド + データベース + キャッシュの構成です。

```kdl
project "webapp"

stage "local" {
    service "frontend"
    service "backend"
    service "db"
    service "redis"
}

// フロントエンド（React/Vue/Next.js等）
service "frontend" {
    image "node"
    version "20-alpine"
    ports {
        port host=3000 container=3000
    }
    env {
        NODE_ENV "development"
        NEXT_PUBLIC_API_URL "http://localhost:8080"
    }
    volumes {
        volume host="./frontend" container="/app"
    }
    command "npm run dev"
}

// バックエンドAPI
service "backend" {
    image "node"
    version "20-alpine"
    ports {
        port host=8080 container=8080
    }
    env {
        NODE_ENV "development"
        DATABASE_URL "postgres://user:pass@db:5432/app"
        REDIS_URL "redis://redis:6379"
    }
    volumes {
        volume host="./backend" container="/app"
    }
    command "npm run dev"
}

// PostgreSQL
service "db" {
    image "postgres"
    version "16-alpine"
    ports {
        port host=5432 container=5432
    }
    env {
        POSTGRES_DB "app"
        POSTGRES_USER "user"
        POSTGRES_PASSWORD "pass"
    }
    volumes {
        volume host="./data/postgres" container="/var/lib/postgresql/data"
    }
}

// Redis
service "redis" {
    image "redis"
    version "7-alpine"
    ports {
        port host=6379 container=6379
    }
}
```

## マイクロサービス構成

複数のサービスがAPI Gatewayを通じて連携する構成です。

```kdl
project "microservices"

stage "local" {
    service "gateway"
    service "user-service"
    service "order-service"
    service "db-users"
    service "db-orders"
}

// API Gateway
service "gateway" {
    image "nginx"
    version "alpine"
    ports {
        port host=80 container=80
    }
    volumes {
        volume host="./gateway/nginx.conf" container="/etc/nginx/nginx.conf" read_only=true
    }
}

// ユーザーサービス
service "user-service" {
    build {
        dockerfile "services/user/Dockerfile"
    }
    ports {
        port host=8001 container=8000
    }
    env {
        DATABASE_URL "postgres://user:pass@db-users:5432/users"
    }
}

// 注文サービス
service "order-service" {
    build {
        dockerfile "services/order/Dockerfile"
    }
    ports {
        port host=8002 container=8000
    }
    env {
        DATABASE_URL "postgres://user:pass@db-orders:5432/orders"
        USER_SERVICE_URL "http://user-service:8000"
    }
}

// ユーザーDB
service "db-users" {
    image "postgres"
    version "16-alpine"
    ports {
        port host=5433 container=5432
    }
    env {
        POSTGRES_DB "users"
        POSTGRES_USER "user"
        POSTGRES_PASSWORD "pass"
    }
}

// 注文DB
service "db-orders" {
    image "postgres"
    version "16-alpine"
    ports {
        port host=5434 container=5432
    }
    env {
        POSTGRES_DB "orders"
        POSTGRES_USER "user"
        POSTGRES_PASSWORD "pass"
    }
}
```

## Rustバックエンド + SurrealDB

Rust APIサーバーとSurrealDBの構成です。

```kdl
project "rust-api"

stage "local" {
    service "api"
    service "surrealdb"
}

// Rust APIサーバー
service "api" {
    build {
        dockerfile "Dockerfile"
        target "development"
    }
    ports {
        port host=3000 container=3000
    }
    env {
        RUST_LOG "debug"
        DATABASE_URL "ws://surrealdb:8000"
        DATABASE_NS "app"
        DATABASE_DB "main"
    }
    volumes {
        volume host="./src" container="/app/src"
        volume host="./Cargo.toml" container="/app/Cargo.toml" read_only=true
    }
}

// SurrealDB
service "surrealdb" {
    image "surrealdb/surrealdb"
    version "latest"
    ports {
        port host=8000 container=8000
    }
    command "start --user root --pass root file:/data/database.db"
    volumes {
        volume host="./data/surreal" container="/data"
    }
}
```

## 静的サイト + リバースプロキシ

静的ファイル配信とSSL終端の構成です。

```kdl
project "static-site"

stage "local" {
    service "nginx"
}

service "nginx" {
    image "nginx"
    version "alpine"
    ports {
        port host=80 container=80
        port host=443 container=443
    }
    volumes {
        volume host="./public" container="/usr/share/nginx/html" read_only=true
        volume host="./nginx.conf" container="/etc/nginx/nginx.conf" read_only=true
        volume host="./certs" container="/etc/nginx/certs" read_only=true
    }
}
```

## ローカルDockerビルド

Dockerfileからビルドするパターンです。

### 規約ベース（自動検出）

```kdl
// services/api/Dockerfile が自動検出される
service "api" {
    ports {
        port host=3000 container=3000
    }
}
```

### 明示的なビルド設定

```kdl
service "api" {
    build {
        dockerfile "docker/api.Dockerfile"
        context "."
        args {
            RUST_VERSION "1.75"
            BUILD_MODE "release"
        }
        target "production"
        image_tag "myapp/api:latest"
    }
    ports {
        port host=3000 container=3000
    }
}
```

## 複数ステージ構成

開発・ステージング・本番で異なるサービス構成を持つパターンです。

```kdl
project "multi-stage"

// ローカル開発環境
stage "local" {
    service "web"
    service "db"
    service "mailhog"  // メールテスト用
}

// ステージング環境
stage "staging" {
    service "web"
    service "db"
}

// 本番環境
stage "prod" {
    service "web"
    service "db"
    service "cdn"
}

service "web" {
    image "node"
    version "20-alpine"
    ports {
        port host=3000 container=3000
    }
}

service "db" {
    image "postgres"
    version "16-alpine"
    ports {
        port host=5432 container=5432
    }
}

// ローカル開発用メールサーバー
service "mailhog" {
    image "mailhog/mailhog"
    ports {
        port host=1025 container=1025  // SMTP
        port host=8025 container=8025  // Web UI
    }
}

// 本番用CDN/リバースプロキシ
service "cdn" {
    image "nginx"
    version "alpine"
    ports {
        port host=80 container=80
        port host=443 container=443
    }
}
```

## ベストプラクティス

### 命名規則

- プロジェクト名: 短く、ハイフン区切り（`my-app`）
- ステージ名: 用途を表す（`local`, `staging`, `prod`）
- サービス名: 役割を表す（`db`, `api`, `web`）

### ポート管理

| サービス | デフォルトポート |
|---------|----------------|
| PostgreSQL | 5432 |
| MySQL | 3306 |
| Redis | 6379 |
| SurrealDB | 8000 |
| HTTP | 80, 3000, 8080 |
| HTTPS | 443 |

### ボリューム管理

```kdl
// 開発用: ソースコードをマウント
volumes {
    volume host="./src" container="/app/src"
}

// データ永続化
volumes {
    volume host="./data/postgres" container="/var/lib/postgresql/data"
}

// 設定ファイル（読み取り専用）
volumes {
    volume host="./config.yml" container="/etc/app/config.yml" read_only=true
}
```

### 環境変数

```kdl
env {
    // データベース接続
    DATABASE_URL "postgres://user:pass@db:5432/app"

    // 開発モード
    NODE_ENV "development"
    RUST_LOG "debug"

    // サービス間通信
    API_URL "http://api:3000"
}
```
