---
name: fleetflow
description: FleetFlow（KDLベースのコンテナオーケストレーションツール）を効果的に使用するためのガイド
version: 0.2.0
---

# FleetFlow スキル

このスキルは、FleetFlowをプロジェクトで効果的に活用するための包括的なガイドです。
他のプロジェクトに持ち込んで、FleetFlowによる環境構築を簡単に始められます。

## エイリアス

このスキルは以下のように呼ぶことができます：

- `fleetflow` → FleetFlow全般
- `flow` → 設定ファイル（flow.kdl）
- `ff` → FleetFlowの略称

## 目的

FleetFlowは、KDL（KDL Document Language）をベースにした、超シンプルなコンテナオーケストレーション・環境構築ツールです。
Docker Composeの手軽さはそのままに、より少ない記述で、より強力な設定管理を実現します。

**コンセプト**: 「宣言だけで、開発も本番も」

## スキルの起動タイミング

このスキルは以下の場合に参照してください：

- ✅ プロジェクトにFleetFlowを導入する際
- ✅ `flow.kdl` 設定ファイルを作成・編集する際
- ✅ コンテナ環境の構築・管理を行う際
- ✅ ローカル開発環境のセットアップ時
- ✅ ステージ管理（local/dev/staging/prod）が必要な際

**ユーザーがスキルを明示的に呼び出す方法**:

- `/fleetflow` または `/flow` コマンド
- 「FleetFlow」「flow.kdl」「コンテナ環境」などのキーワードを含む質問

## FleetFlowの主要な特徴

### 1. 超シンプルな構文

Docker Composeと比較して同等かそれ以下の記述量で環境を定義できます。

**Docker Compose (YAML)**:
```yaml
version: '3'
services:
  db:
    image: postgres:16
    ports:
      - "5432:5432"
    environment:
      POSTGRES_PASSWORD: postgres
    volumes:
      - ./data:/var/lib/postgresql/data
```

**FleetFlow (KDL)**:
```kdl
project "myapp"

stage "local" {
    service "db"
}

service "db" {
    image "postgres"
    version "16"

    ports {
        port host=5432 container=5432
    }

    env {
        POSTGRES_PASSWORD "postgres"
    }

    volumes {
        volume host="./data" container="/var/lib/postgresql/data"
    }
}
```

### 2. 可読性

- YAMLのインデント地獄から解放
- ブロック構造が明確
- コメントが書きやすい

### 3. モジュール化（Phase 2以降）

```kdl
// 共通設定を分離
include "common/database.kdl"
include "services/*.kdl"

// 変数展開
variables {
    registry "ghcr.io/myorg"
    version "1.0.0"
}

service "api" {
    image "{registry}/api:{version}"
}
```

### 4. ステージ管理

開発環境から本番環境まで、同じ設定ファイルで統一管理：

```kdl
stage "local" {
    service "db"
    service "redis"
}

stage "prod" {
    service "db"
    service "redis"
    service "cache"
}
```

## CLIコマンド

### 基本コマンド

#### `fleetflow up [stage]`

指定したステージのコンテナを起動します。

```bash
# localステージを起動
fleetflow up local

# デフォルトステージ（local）を起動
fleetflow up
```

**動作**:
- 設定ファイルを読み込み
- コンテナが存在しなければ作成
- 既に存在する場合は起動のみ
- サービスごとに進捗を表示

#### `fleetflow down [stage]`

指定したステージのコンテナを停止・削除します。

```bash
# localステージを停止・削除
fleetflow down local
```

**動作**:
- コンテナを停止
- コンテナを削除
- ボリュームは削除しない（データ保持）

#### `fleetflow ps [stage]`

指定したステージのコンテナ状態を表示します。

```bash
# localステージの状態確認
fleetflow ps local
```

**表示内容**:
- コンテナ名
- 状態（Running/Stopped）
- ポートマッピング

#### `fleetflow validate`

設定ファイルの構文チェックを行います。

```bash
fleetflow validate
```

**チェック内容**:
- KDL構文エラー
- 必須フィールドの欠落
- 論理的な矛盾

### 設定ファイルの検索順序

FleetFlowは以下の優先順位で設定ファイルを検索します：

1. 環境変数 `FLOW_CONFIG_PATH`（直接パス指定）
2. カレントディレクトリ:
   - `flow.local.kdl` (ローカル専用、.gitignore推奨)
   - `.flow.local.kdl` (隠しファイル版)
   - `flow.kdl` (標準設定)
   - `.flow.kdl` (隠しファイル版)
3. `.fleetflow/` ディレクトリ内（上記と同じ順序）
4. `~/.config/fleetflow/flow.kdl` (グローバル設定)

**推奨構成**:

```
プロジェクトルート/
├── flow.kdl              # バージョン管理対象（共通設定）
├── flow.local.kdl        # ローカル専用（.gitignore推奨）
└── .gitignore            # flow.local.kdl を含める
```

## 設定ファイルフォーマット（flow.kdl）

### 基本構造

```kdl
// プロジェクト名（必須）
project "project-name"

// ステージ定義
stage "stage-name" {
    service "service-name"
    // 複数のサービスを列挙
}

// サービス詳細定義
service "service-name" {
    // イメージとバージョン
    image "image-name"
    version "tag"

    // ポート設定
    ports {
        port host=ホストポート container=コンテナポート
    }

    // 環境変数
    env {
        KEY "value"
    }

    // ボリュームマウント
    volumes {
        volume host="ホストパス" container="コンテナパス"
    }

    // コマンド（オプション）
    command "start --options"
}
```

### 詳細な構文

#### 1. プロジェクト宣言

```kdl
project "myapp"
```

- **必須**: すべての設定ファイルで最初に宣言
- **用途**: コンテナ命名規則、ラベル付けに使用
- **命名規則**: `{project}-{stage}-{service}`

#### 2. ステージ定義

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
- **サービスは共通定義**: 後述の`service`ブロックで詳細を定義

#### 3. サービス定義

##### イメージ指定

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
    // → default:latest として解釈
}
```

**解釈ルール**:
1. `image`と`version`の両方指定 → `image:version`
2. `image`のみでタグ含む → そのまま使用
3. `image`のみでタグなし → `image:latest`
4. `version`のみ → `service-name:version`
5. 両方なし → `service-name:latest`

##### ポート設定

```kdl
ports {
    port host=8080 container=3000
    port host=5432 container=5432 protocol="tcp"
    port host=53 container=53 protocol="udp"
}
```

**パラメータ**:
- `host`: ホスト側のポート番号（必須）
- `container`: コンテナ内のポート番号（必須）
- `protocol`: プロトコル（オプション、デフォルト: tcp）
  - 指定可能: `tcp`, `udp`

##### 環境変数

```kdl
env {
    DATABASE_URL "postgres://localhost:5432/mydb"
    DEBUG "true"
    NODE_ENV "development"
}
```

- キーと値をペアで指定
- 複数行で定義可能

##### ボリュームマウント

```kdl
volumes {
    volume host="./data" container="/var/lib/postgresql/data"
    volume host="/config" container="/etc/config" read_only=true
}
```

**パラメータ**:
- `host`: ホスト側のパス（必須）
  - 相対パスは自動的に絶対パスに変換
- `container`: コンテナ内のパス（必須）
- `read_only`: 読み取り専用（オプション、デフォルト: false）

##### コマンド実行

```kdl
service "db" {
    image "postgres"
    version "16"
    command "postgres -c max_connections=200"
}
```

- コンテナ起動時のコマンドを上書き
- スペースで自動的に引数分割

### 完全な例

```kdl
// プロジェクト名
project "myapp"

// ローカル開発環境
stage "local" {
    service "db"
    service "redis"
    service "web"
}

// 本番環境
stage "prod" {
    service "db"
    service "redis"
    service "web"
    service "cdn"
}

// PostgreSQLデータベース
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

// Redisキャッシュ
service "redis" {
    image "redis"
    version "7-alpine"

    ports {
        port host=6379 container=6379
    }

    volumes {
        volume host="./data/redis" container="/data"
    }
}

// Webアプリケーション
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

## OrbStack連携（macOS推奨）

FleetFlowは主に**ローカル開発環境（macOS）**での利用を想定しており、OrbStackとの連携に最適化されています。

### コンテナのグループ化

FleetFlowで起動したコンテナは、自動的にプロジェクト・ステージごとにグループ化されます。

**命名規則**:
```
{project}-{stage}-{service}
```

**例**:
```
myapp-local-db
myapp-local-redis
myapp-local-web
```

### OrbStackでの表示

OrbStackのUIでは、以下のラベルによってグループ化されます：

- `com.docker.compose.project`: `{project}-{stage}`
- `com.docker.compose.service`: `{service}`

これにより、プロジェクト・ステージごとに整理された表示が可能です。

### メタデータラベル

各コンテナには以下のFleetFlow固有のラベルも付与されます：

- `fleetflow.project`: プロジェクト名
- `fleetflow.stage`: ステージ名
- `fleetflow.service`: サービス名

これらのラベルは、フィルタリングや識別に利用できます。

## ベストプラクティス

### 1. ローカル専用設定の分離

```kdl
// flow.kdl（Git管理対象）
project "myapp"

stage "local" {
    service "db"
}

service "db" {
    image "postgres"
    version "16"
}
```

```kdl
// flow.local.kdl（.gitignore推奨）
project "myapp"

stage "local" {
    service "db"
}

service "db" {
    image "postgres"
    version "16"

    env {
        POSTGRES_PASSWORD "local-dev-password"  // ローカル専用
    }

    volumes {
        volume host="/Users/yourname/data" container="/var/lib/postgresql/data"
    }
}
```

**.gitignore**:
```
flow.local.kdl
```

### 2. 環境ごとのサービス構成

```kdl
// 開発環境には開発ツールを追加
stage "local" {
    service "db"
    service "redis"
    service "mailcatcher"  // 開発専用
}

// 本番環境は必要最小限
stage "prod" {
    service "db"
    service "redis"
}
```

### 3. ポート番号の整理

プロジェクト全体でポート番号を体系的に管理：

```kdl
// 3000番台: Webサービス
service "web" {
    ports {
        port host=3000 container=3000
    }
}

service "api" {
    ports {
        port host=3001 container=3000
    }
}

// 5000番台: データベース
service "postgres" {
    ports {
        port host=5432 container=5432
    }
}

// 6000番台: キャッシュ
service "redis" {
    ports {
        port host=6379 container=6379
    }
}
```

### 4. データ永続化

ボリュームマウントでデータを永続化：

```kdl
service "db" {
    volumes {
        // プロジェクトルートからの相対パス
        volume host="./data/db" container="/var/lib/postgresql/data"
    }
}
```

**.gitignore**:
```
data/
```

### 5. サービス間通信

Docker内部ネットワークを利用：

```kdl
service "web" {
    env {
        // サービス名でDNS解決
        DATABASE_URL "postgres://db:5432/myapp"
        REDIS_URL "redis://redis:6379"
    }
}
```

## 一般的なパターン

### フルスタックWebアプリ

```kdl
project "webapp"

stage "local" {
    service "db"
    service "redis"
    service "backend"
    service "frontend"
}

service "db" {
    image "postgres"
    version "16-alpine"
    ports {
        port host=5432 container=5432
    }
    env {
        POSTGRES_DB "webapp"
        POSTGRES_PASSWORD "postgres"
    }
    volumes {
        volume host="./data/postgres" container="/var/lib/postgresql/data"
    }
}

service "redis" {
    image "redis"
    version "7-alpine"
    ports {
        port host=6379 container=6379
    }
}

service "backend" {
    image "node"
    version "20-alpine"
    ports {
        port host=4000 container=4000
    }
    env {
        DATABASE_URL "postgres://postgres:postgres@db:5432/webapp"
        REDIS_URL "redis://redis:6379"
    }
    volumes {
        volume host="./backend" container="/app"
    }
    command "npm run dev"
}

service "frontend" {
    image "node"
    version "20-alpine"
    ports {
        port host=3000 container=3000
    }
    env {
        API_URL "http://backend:4000"
    }
    volumes {
        volume host="./frontend" container="/app"
    }
    command "npm run dev"
}
```

### マイクロサービス構成

```kdl
project "microservices"

stage "local" {
    service "db"
    service "redis"
    service "auth-service"
    service "user-service"
    service "api-gateway"
}

service "db" {
    image "postgres"
    version "16"
    // ...（省略）
}

service "redis" {
    image "redis"
    version "7"
    // ...（省略）
}

service "auth-service" {
    image "auth-app"
    version "latest"
    ports {
        port host=5001 container=5000
    }
}

service "user-service" {
    image "user-app"
    version "latest"
    ports {
        port host=5002 container=5000
    }
}

service "api-gateway" {
    image "nginx"
    version "alpine"
    ports {
        port host=8080 container=80
    }
    volumes {
        volume host="./nginx.conf" container="/etc/nginx/nginx.conf" read_only=true
    }
}
```

## トラブルシューティング

### 設定ファイルが見つからない

**エラー**: `Flow設定ファイルが見つかりません`

**解決方法**:
1. カレントディレクトリに`flow.kdl`が存在するか確認
2. 環境変数`FLOW_CONFIG_PATH`が正しく設定されているか確認
3. `fleetflow validate`で設定ファイルの検証

### イメージが見つからない

**エラー**: `イメージが見つかりません: xxx:yyy`

**解決方法**:
1. イメージ名とタグが正しいか確認
2. 必要であれば手動でpull: `docker pull image:tag`
3. プライベートレジストリの場合はDocker認証を確認

### ポートが既に使用されている

**エラー**: `ポート xxxx は既に使用されています`

**解決方法**:
1. 他のコンテナで同じポートを使っていないか確認: `docker ps`
2. ホスト側で他のプロセスがポートを使用していないか確認: `lsof -i :xxxx`
3. flow.kdlで別のポート番号を指定

### コンテナが起動しない

**解決方法**:
1. ログを確認: `docker logs {project}-{stage}-{service}`
2. 環境変数が正しいか確認
3. ボリュームマウントのパスが存在するか確認
4. コマンドが正しいか確認

## 参考資料

### 公式ドキュメント
- [KDL Document Language](https://kdl.dev/)
- [Docker Documentation](https://docs.docker.com/)
- [OrbStack](https://orbstack.dev/)

### FleetFlow関連
- [GitHub Repository](https://github.com/chronista-club/fleetflow)
- Issue・Pull Requestで質問・提案が可能

---

FleetFlow - シンプルに、統一的に、環境を構築する。
