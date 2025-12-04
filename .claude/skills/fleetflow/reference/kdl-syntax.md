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
    image "image-name:tag"  // ⚠️ 必須
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

### イメージ指定（必須）

```kdl
service "db" {
    image "postgres:16"
    // imageフィールドは必須です
}

service "custom" {
    image "ghcr.io/org/app:v1.0.0"
    // レジストリ付きイメージも使用可能
}
```

**重要**: `image`フィールドは**必須**です。省略するとエラーになります：

```kdl
// ❌ エラー: imageが必須
service "db" {
    version "16"
}
// Error: サービス 'db' に image が指定されていません
```

### versionフィールド（オプション）

`version`は別途指定可能ですが、通常は`image`にタグを含めます：

```kdl
service "db" {
    image "postgres"
    version "16"
    // → 内部的に postgres:16 として扱われる
}

// より一般的な書き方
service "db" {
    image "postgres:16"
}
```

### ポート設定

```kdl
ports {
    port 8080 3000
    port 5432 5432 protocol="tcp"
    port 53 53 protocol="udp"
    port 8443 443 host_ip="127.0.0.1"
}
```

**構文**: `port <host_port> <container_port> [options]`

| パラメータ | 必須 | 説明 |
|-----------|------|------|
| 第1引数 | ✅ | ホスト側のポート番号 |
| 第2引数 | ✅ | コンテナ内のポート番号 |
| `protocol` | - | `tcp`（デフォルト）または `udp` |
| `host_ip` | - | バインドするホストIP |

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
- **マージ時は両方の値が結合される**（後の定義が優先）

### ボリュームマウント

```kdl
volumes {
    volume "./data" "/var/lib/postgresql/data"
    volume "/config" "/etc/config" read_only=true
}
```

**構文**: `volume <host_path> <container_path> [options]`

| パラメータ | 必須 | 説明 |
|-----------|------|------|
| 第1引数 | ✅ | ホスト側のパス（相対パスは自動で絶対パスに変換） |
| 第2引数 | ✅ | コンテナ内のパス |
| `read_only` | - | 読み取り専用（デフォルト: false） |

### コマンド実行

```kdl
service "db" {
    image "postgres:16"
    command "postgres -c max_connections=200"
}
```

- コンテナ起動時のコマンドを上書き
- スペースで自動的に引数分割

### 依存関係

```kdl
service "web" {
    image "node:20-alpine"
    depends_on "db" "redis"
}
```

- 起動順序の制御に使用
- スペース区切りで複数指定可能

### Dockerビルド設定

```kdl
service "api" {
    image "myapp/api:latest"  // ビルド後のイメージタグ
    build {
        dockerfile "services/api/Dockerfile"
        context "."
        args {
            RUST_VERSION "1.75"
        }
        target "production"
        no_cache false
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

**規約ベース検出**: `services/{name}/Dockerfile` が自動検出されます。

### ヘルスチェック設定

```kdl
service "db" {
    image "postgres:16"
    healthcheck {
        test "pg_isready -U postgres"
        interval 30
        timeout 3
        retries 3
        start_period 10
    }
}
```

| パラメータ | デフォルト | 説明 |
|-----------|-----------|------|
| `test` | - | ヘルスチェックコマンド（必須） |
| `interval` | 30 | チェック間隔（秒） |
| `timeout` | 3 | タイムアウト（秒） |
| `retries` | 3 | リトライ回数 |
| `start_period` | 10 | 起動待機時間（秒） |

## サービスマージ機能

複数ファイルで同じサービスを定義した場合、設定がマージされます：

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

| フィールドタイプ | ルール | 例 |
|----------------|--------|-----|
| `Option<T>` | 後の定義が`Some`なら上書き | image, version, command, build, healthcheck |
| `Vec<T>` | 後の定義が空でなければ上書き | ports, volumes, depends_on |
| `HashMap<K, V>` | 両方をマージ（後の定義が優先） | env (environment) |

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
    image "postgres:16-alpine"
    ports {
        port 5432 5432
    }
    env {
        POSTGRES_DB "myapp"
        POSTGRES_USER "myapp"
        POSTGRES_PASSWORD "secret"
    }
    volumes {
        volume "./data/postgres" "/var/lib/postgresql/data"
    }
    healthcheck {
        test "pg_isready -U myapp"
        interval 10
        timeout 5
        retries 5
    }
}

// Redis
service "redis" {
    image "redis:7-alpine"
    ports {
        port 6379 6379
    }
    healthcheck {
        test "redis-cli ping"
    }
}

// Webアプリ
service "web" {
    image "node:20-alpine"
    ports {
        port 3000 3000
    }
    env {
        NODE_ENV "development"
        DATABASE_URL "postgres://myapp:secret@db:5432/myapp"
        REDIS_URL "redis://redis:6379"
    }
    volumes {
        volume "./app" "/app"
    }
    command "npm run dev"
    depends_on "db" "redis"
}

// CDN（本番のみ）
service "cdn" {
    image "nginx:alpine"
    ports {
        port 80 80
        port 443 443
    }
    volumes {
        volume "./nginx.conf" "/etc/nginx/nginx.conf" read_only=true
    }
}
```
