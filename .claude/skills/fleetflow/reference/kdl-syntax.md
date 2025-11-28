# KDL構文リファレンス

FleetFlowの設定ファイル（flow.kdl）の構文詳細です。

## 基本構造

```kdl
// プロジェクト名（必須）
project "project-name"

// ステージ定義
stage "stage-name" {
    service "service-name"
}

// サービス詳細定義
service "service-name" {
    image "image-name"
    version "tag"
    // ...
}
```

## プロジェクト宣言

```kdl
project "myapp"
```

- **必須**: すべての設定ファイルで最初に宣言
- **用途**: コンテナ命名規則、ラベル付けに使用
- **命名規則**: `{project}-{stage}-{service}`

## ステージ定義

```kdl
stage "local" {
    service "db"
    service "redis"
    service "web"
}

stage "prod" {
    service "db"
    service "redis"
    service "web"
    service "cdn"
}
```

- **複数定義可能**: 環境ごとに異なるサービス構成
- **サービスは共通定義**: `service`ブロックで詳細を定義

## サービス定義

### イメージ指定

```kdl
service "db" {
    image "postgres"
    version "16"
    // → postgres:16 として解釈
}

service "custom" {
    image "ghcr.io/org/app:v1.0.0"
    // タグ付きイメージはそのまま使用
}

service "default" {
    // imageもversionも省略 → サービス名:latest
}
```

**解釈ルール**:

| image | version | 結果 |
|-------|---------|------|
| あり | あり | `image:version` |
| あり（タグ含む） | - | そのまま使用 |
| あり（タグなし） | - | `image:latest` |
| - | あり | `service-name:version` |
| - | - | `service-name:latest` |

### ポート設定

```kdl
ports {
    port host=8080 container=3000
    port host=5432 container=5432 protocol="tcp"
    port host=53 container=53 protocol="udp"
}
```

| パラメータ | 必須 | 説明 |
|-----------|------|------|
| `host` | ✅ | ホスト側のポート番号 |
| `container` | ✅ | コンテナ内のポート番号 |
| `protocol` | - | `tcp`（デフォルト）または `udp` |

### 環境変数

```kdl
env {
    DATABASE_URL "postgres://localhost:5432/mydb"
    DEBUG "true"
    NODE_ENV "development"
}
```

- キーと値をペアで指定
- 複数行で定義可能

### ボリュームマウント

```kdl
volumes {
    volume host="./data" container="/var/lib/postgresql/data"
    volume host="/config" container="/etc/config" read_only=true
}
```

| パラメータ | 必須 | 説明 |
|-----------|------|------|
| `host` | ✅ | ホスト側のパス（相対パスは自動で絶対パスに変換） |
| `container` | ✅ | コンテナ内のパス |
| `read_only` | - | 読み取り専用（デフォルト: false） |

### コマンド実行

```kdl
service "db" {
    image "postgres"
    version "16"
    command "postgres -c max_connections=200"
}
```

- コンテナ起動時のコマンドを上書き
- スペースで自動的に引数分割

### Dockerビルド設定

```kdl
service "api" {
    build {
        dockerfile "services/api/Dockerfile"
        context "."
        args {
            RUST_VERSION "1.75"
        }
        target "production"
        no_cache false
        image_tag "myapp/api:latest"
    }
}
```

| パラメータ | 説明 |
|-----------|------|
| `dockerfile` | Dockerfileのパス |
| `context` | ビルドコンテキスト（デフォルト: プロジェクトルート） |
| `args` | ビルド引数 |
| `target` | マルチステージビルドのターゲット |
| `no_cache` | キャッシュを使用しない |
| `image_tag` | イメージタグ |

**規約ベース検出**: `services/{name}/Dockerfile` が自動検出されます。

## 設定ファイル検索順序

FleetFlowは以下の優先順位で設定ファイルを検索します：

1. 環境変数 `FLOW_CONFIG_PATH`
2. カレントディレクトリ:
   - `flow.local.kdl` (ローカル専用)
   - `.flow.local.kdl`
   - `flow.kdl` (標準)
   - `.flow.kdl`
3. `.fleetflow/` ディレクトリ
4. `~/.config/fleetflow/flow.kdl` (グローバル)

## 完全な例

```kdl
project "myapp"

// ステージ定義
stage "local" {
    service "db"
    service "redis"
    service "web"
}

stage "prod" {
    service "db"
    service "redis"
    service "web"
    service "cdn"
}

// PostgreSQL
service "db" {
    image "postgres"
    version "16-alpine"
    ports {
        port host=5432 container=5432
    }
    env {
        POSTGRES_DB "myapp"
        POSTGRES_USER "myapp"
        POSTGRES_PASSWORD "secret"
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

// Webアプリ
service "web" {
    image "node"
    version "20-alpine"
    ports {
        port host=3000 container=3000
    }
    env {
        NODE_ENV "development"
        DATABASE_URL "postgres://myapp:secret@db:5432/myapp"
        REDIS_URL "redis://redis:6379"
    }
    volumes {
        volume host="./app" container="/app"
    }
    command "npm run dev"
}

// CDN（本番のみ）
service "cdn" {
    image "nginx"
    version "alpine"
    ports {
        port host=80 container=80
        port host=443 container=443
    }
    volumes {
        volume host="./nginx.conf" container="/etc/nginx/nginx.conf" read_only=true
    }
}
```
