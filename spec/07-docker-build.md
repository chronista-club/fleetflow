# 仕様書: Dockerイメージビルド機能

**Issue**: #10
**作成日**: 2025-11-23
**ステータス**: Phase 3 - 仕様策定中

## What & Why - 何を作るか、なぜ作るか

### 概要

FleetFlowにDockerイメージのビルド機能を追加し、カスタムアプリケーションの開発・デプロイを可能にします。

### 背景

現在のFleetFlowは既存のDockerイメージ（PostgreSQL、Redis等）を使ったコンテナ起動のみサポートしています。しかし、実際の開発では以下のニーズがあります：

- **カスタムアプリケーションイメージのビルド**
  - 自社開発のWebアプリケーション
  - マイクロサービスの各サービス
  - 独自の設定を含むミドルウェア

- **開発サイクルでの頻繁なリビルド**
  - コード変更後の動作確認
  - 依存関係の更新
  - 設定ファイルの調整

### 目的

1. **開発体験の向上**
   - Docker Composeと同等のビルド機能を提供
   - flow.kdl一つで完結する開発環境構築

2. **柔軟性の確保**
   - 既存イメージとカスタムビルドの両対応
   - 環境ごとに異なるビルド設定

3. **簡潔な記述**
   - 規約ベースで自動検出
   - 必要な場合のみ明示的指定

## コンセプト

### 「規約優先、設定も可能」

FleetFlowの哲学に従い、デフォルトの規約を提供しつつ、柔軟な設定も可能にします。

**規約ベース（Convention over Configuration）**:
```kdl
service "api" {
    // Dockerfileの指定なし
    // → ./services/api/Dockerfile を自動検出
}
```

**明示的設定（Configuration when needed）**:
```kdl
service "api" {
    dockerfile "./backend/api/Dockerfile"
    context "./backend"
}
```

### 「変数展開との統合」

既存のIssue #7で実装した変数展開機能と統合し、ビルド引数にも変数を使用できます。

```kdl
variables {
    NODE_VERSION "20"
    APP_ENV "development"
}

service "api" {
    build_args {
        NODE_VERSION "{NODE_VERSION}"
        APP_ENV "{APP_ENV}"
    }
}
```

## 機能仕様

### 1. ビルド対象の指定

#### 1.1 明示的パス指定

```kdl
service "api" {
    dockerfile "./path/to/Dockerfile"
}
```

- `dockerfile` フィールドでDockerfileのパスを指定
- プロジェクトルート（flow.kdlがある場所）からの相対パス
- 絶対パスも可能

#### 1.2 規約ベース自動検出

```kdl
service "api" {
    // dockerfile未指定
}
```

**検索順序**:
1. `./services/{service-name}/Dockerfile`
2. `./{service-name}/Dockerfile`
3. `./Dockerfile.{service-name}`

最初に見つかったファイルを使用。見つからない場合は`image`フィールドで指定されたイメージをpull。

### 2. ビルドコンテキスト

#### 2.1 デフォルト動作

**プロジェクトルートを常にコンテキストとする**:

```kdl
service "api" {
    dockerfile "./services/api/Dockerfile"
    // context: プロジェクトルート（flow.kdlがある場所）
}
```

**理由**:
- モノレポ構成での共通ファイル参照
- `COPY ../common` のような相対パス参照の回避
- シンプルで予測可能な挙動

#### 2.2 明示的コンテキスト指定

```kdl
service "api" {
    dockerfile "./services/api/Dockerfile"
    context "./services/api"  // Dockerfileと同じディレクトリ
}
```

- `context` フィールドで明示的に指定可能
- プロジェクトルートからの相対パス

### 3. ビルド引数（ARG）

#### 3.1 変数展開との統合

```kdl
variables {
    NODE_VERSION "20"
    REGISTRY "ghcr.io/myorg"
}

service "api" {
    dockerfile "./services/api/Dockerfile"

    build_args {
        NODE_VERSION "{NODE_VERSION}"
        BASE_IMAGE "{REGISTRY}/base:latest"
    }
}
```

**Dockerfile例**:
```dockerfile
ARG NODE_VERSION=18
ARG BASE_IMAGE=node:${NODE_VERSION}

FROM ${BASE_IMAGE}
```

#### 3.2 ステージ固有の引数

```kdl
variables {
    APP_ENV "development"
}

stage "local" {
    variables {
        APP_ENV "local"
        DEBUG "true"
    }
    service "api"
}

service "api" {
    build_args {
        APP_ENV "{APP_ENV}"
        DEBUG "{DEBUG}"
    }
}
```

- ステージごとに異なるビルド引数を渡せる
- 開発環境と本番環境で異なる最適化が可能

### 4. イメージタグ管理

#### 4.1 自動タグ生成

```kdl
project "myapp"

service "api" {
    dockerfile "./services/api/Dockerfile"
    // 自動生成タグ: myapp-api:local
}
```

**タグ命名規則**:
```
{project-name}-{service-name}:{stage-name}
```

#### 4.2 明示的タグ指定

```kdl
service "api" {
    dockerfile "./services/api/Dockerfile"
    image_tag "myapp-api:v1.0.0"
}
```

- `image_tag` フィールドで明示的に指定
- セマンティックバージョニングに対応

### 5. キャッシュ制御

#### 5.1 デフォルト動作

- Dockerのビルドキャッシュを活用
- レイヤーキャッシュによる高速ビルド

#### 5.2 キャッシュ無効化

```bash
fleetflow up --build --no-cache local
```

または

```kdl
service "api" {
    build {
        no_cache true
    }
}
```

### 6. コンテナレジストリ設定

#### 6.1 レジストリ階層

レジストリは3つのレベルで設定でき、以下の優先順位で解決されます：

```
CLI --registry > Service.registry > Stage.registry > Flow.registry
```

#### 6.2 Flow レベル（プロジェクト全体）

```kdl
project "myapp"

// プロジェクト全体のデフォルトレジストリ
registry "ghcr.io/myorg"
```

このレジストリは、下位レベルで上書きされない限り、全てのサービスに適用されます。

#### 6.3 Stage レベル（ステージ/環境ごと）

```kdl
stage "dev" {
    registry "gcr.io/dev-project"
    service "api"
}

stage "prod" {
    registry "ghcr.io/prod-org"
    service "api"
}
```

ステージごとに異なるレジストリを使用できます（例：開発と本番で異なるレジストリ）。

#### 6.4 Service レベル（サービスごと）

```kdl
service "api" {
    registry "ghcr.io/special-org"
    dockerfile "./Dockerfile"
}

// registryを指定しない → 上位レベルの設定を継承、または Docker Hub
service "db" {
    image "postgres:16"
}
```

特定のサービスのみ別のレジストリを使用する場合に指定します。

#### 6.5 CLI オプション

```bash
# CLI引数が最優先
fleetflow build api prod --registry ghcr.io/override
```

### 7. ビルドターゲット（マルチステージビルド対応）

```kdl
service "api" {
    dockerfile "./Dockerfile"
    target "production"  // マルチステージビルドのターゲット
}
```

**Dockerfile例**:
```dockerfile
FROM node:20 AS development
WORKDIR /app
COPY package.json .
RUN npm install
COPY . .
CMD ["npm", "run", "dev"]

FROM node:20 AS production
WORKDIR /app
COPY package.json .
RUN npm install --production
COPY . .
RUN npm run build
CMD ["npm", "start"]
```

## コマンド仕様

### 1. `fleetflow up`

既存の`up`コマンドに`--build`フラグを追加。

#### 1.1 基本動作（既存）

```bash
fleetflow up local
```

- Dockerfileが存在する場合、**初回のみ**自動ビルド
- 既にイメージがある場合はそれを使用
- イメージがない場合はDocker Hubからpull

#### 1.2 強制リビルド

```bash
fleetflow up --build local
```

- 既存イメージの有無にかかわらず、必ずビルドを実行
- キャッシュは利用

#### 1.3 キャッシュなしビルド

```bash
fleetflow up --build --no-cache local
```

- キャッシュを使わずにフルビルド
- 依存関係の更新時などに使用

### 2. `fleetflow rebuild`（新規コマンド）

特定のサービスを再ビルドする専用コマンド。

#### 2.1 基本構文

```bash
fleetflow rebuild <service> [stage]
```

#### 2.2 使用例

```bash
# apiサービスをリビルド
fleetflow rebuild api

# localステージのapiサービスをリビルド
fleetflow rebuild api local

# キャッシュなしでリビルド
fleetflow rebuild api --no-cache
```

#### 2.3 動作

1. 既存のコンテナを停止（実行中の場合）
2. イメージをリビルド
3. コンテナを再作成・起動

### 3. `fleetflow build`（新規コマンド）

ビルドのみを行い、コンテナは起動しない。

```bash
# 全サービスをビルド
fleetflow build local

# 特定のサービスのみビルド
fleetflow build api local

# キャッシュなしでビルド
fleetflow build --no-cache local
```

## flow.kdl記法

### 完全な例

```kdl
project "myapp"

// グローバル変数
variables {
    NODE_VERSION "20"
    REGISTRY "ghcr.io/myorg"
    APP_VERSION "1.0.0"
}

// ローカル開発環境
stage "local" {
    variables {
        APP_ENV "local"
        DEBUG "true"
    }
    service "api"
    service "worker"
}

// 本番環境
stage "prod" {
    variables {
        APP_ENV "production"
        DEBUG "false"
    }
    service "api"
    service "worker"
}

// APIサービス
service "api" {
    // ビルド設定
    dockerfile "./services/api/Dockerfile"
    context "."  // プロジェクトルート（デフォルト）

    build_args {
        NODE_VERSION "{NODE_VERSION}"
        APP_ENV "{APP_ENV}"
        DEBUG "{DEBUG}"
    }

    target "production"  // マルチステージビルドのターゲット

    // ランタイム設定
    ports {
        port host=3000 container=3000
    }

    env {
        DATABASE_URL "postgres://db:5432/myapp"
    }

    volumes {
        volume host="./services/api" container="/app"
    }
}

// Workerサービス（規約ベース）
service "worker" {
    // dockerfile未指定 → ./services/worker/Dockerfileを自動検出

    build_args {
        NODE_VERSION "{NODE_VERSION}"
        APP_ENV "{APP_ENV}"
    }

    env {
        REDIS_URL "redis://redis:6379"
    }
}

// データベース（既存イメージ）
service "db" {
    image "postgres"
    version "16"

    env {
        POSTGRES_DB "myapp"
        POSTGRES_PASSWORD "postgres"
    }
}
```

## ユースケース

### 1. フルスタックWebアプリケーション

```kdl
project "webapp"

stage "local" {
    service "frontend"
    service "backend"
    service "db"
}

service "frontend" {
    dockerfile "./frontend/Dockerfile"
    build_args {
        NODE_VERSION "20"
    }
    ports {
        port host=3000 container=3000
    }
}

service "backend" {
    dockerfile "./backend/Dockerfile"
    build_args {
        PYTHON_VERSION "3.12"
    }
    ports {
        port host=8000 container=8000
    }
}

service "db" {
    image "postgres"
    version "16"
}
```

### 2. マイクロサービスアーキテクチャ

```kdl
project "microservices"

variables {
    GO_VERSION "1.21"
    BASE_IMAGE "alpine:3.19"
}

stage "local" {
    service "auth-service"
    service "user-service"
    service "order-service"
    service "gateway"
}

service "auth-service" {
    // 規約ベース: ./services/auth-service/Dockerfile
    build_args {
        GO_VERSION "{GO_VERSION}"
        BASE_IMAGE "{BASE_IMAGE}"
    }
}

service "user-service" {
    // 規約ベース: ./services/user-service/Dockerfile
    build_args {
        GO_VERSION "{GO_VERSION}"
        BASE_IMAGE "{BASE_IMAGE}"
    }
}

// ... その他のサービス
```

### 3. モノレポ構成

```kdl
project "monorepo"

service "api" {
    dockerfile "./apps/api/Dockerfile"
    context "."  // モノレポルートをコンテキストに

    build_args {
        WORKSPACE "apps/api"
    }
}

service "admin" {
    dockerfile "./apps/admin/Dockerfile"
    context "."

    build_args {
        WORKSPACE "apps/admin"
    }
}
```

**Dockerfile例**:
```dockerfile
ARG WORKSPACE

FROM node:20 AS builder
WORKDIR /monorepo
COPY package.json pnpm-workspace.yaml pnpm-lock.yaml ./
COPY packages ./packages
COPY ${WORKSPACE}/package.json ./${WORKSPACE}/
RUN corepack enable pnpm
RUN pnpm install --frozen-lockfile

COPY ${WORKSPACE} ./${WORKSPACE}
RUN pnpm --filter ${WORKSPACE} build

FROM node:20-alpine
WORKDIR /app
COPY --from=builder /monorepo/${WORKSPACE}/dist ./
CMD ["node", "index.js"]
```

## 非機能要件

### 1. パフォーマンス

- **ビルドキャッシュの活用**: Dockerのレイヤーキャッシュを最大限利用
- **並列ビルド**: 依存関係のないサービスは並列ビルド
- **増分ビルド**: 変更のあったサービスのみリビルド

### 2. UX（ユーザー体験）

- **進捗表示**: ビルド進行状況をリアルタイム表示
- **明確なエラーメッセージ**: ビルド失敗時の原因を分かりやすく表示
- **適切なデフォルト**: 規約ベースで最小限の記述

### 3. 互換性

- **Docker Compose相当**: Docker Composeのbuild機能と同等の機能性
- **既存設定との共存**: `image`指定と`dockerfile`指定の両立

## 制約と前提条件

### 制約

1. **Dockerfileベース**
   - Dockerfileを使ったビルドのみサポート
   - Buildpacksなど他のビルド方式は非対応

2. **ローカルビルド**
   - ローカル環境でのビルドのみ
   - リモートビルド（Buildkit remoteなど）は非対応

3. **シングルプラットフォーム**
   - ホストのアーキテクチャでのビルドのみ
   - マルチアーキテクチャビルドは将来対応

### 前提条件

1. **Docker/OrbStackの実行**
   - Docker Daemonが起動していること
   - Dockerfileが正しく記述されていること

2. **ビルドコンテキストのサイズ**
   - `.dockerignore`で不要なファイルを除外推奨
   - 大きすぎるコンテキストはビルド時間に影響

## セキュリティ考慮事項

### 1. ビルド引数の取り扱い

- **機密情報の非推奨**: ビルド引数にシークレットを含めない
- **警告表示**: パスワードのような文字列を検出したら警告

### 2. ベースイメージの検証

- **公式イメージ推奨**: 公式またはVerified Publisherのイメージを推奨
- **タグ固定**: `latest`タグではなく、特定バージョンを推奨

## 将来の拡張性

### Phase 2以降で検討

1. **BuildKit機能**
   - シークレットマウント
   - SSHマウント
   - キャッシュマウント

2. **マルチアーキテクチャ**
   - ARM64/AMD64のクロスビルド
   - `--platform`フラグのサポート

3. **リモートビルド**
   - Docker Buildxとの統合
   - リモートDockerホストでのビルド

4. **Buildpacks対応**
   - Cloud Native Buildpacksのサポート
   - Dockerfileなしでのビルド

## 参考資料

- [Docker Build Reference](https://docs.docker.com/engine/reference/commandline/build/)
- [Dockerfile Best Practices](https://docs.docker.com/develop/develop-images/dockerfile_best-practices/)
- [Bollard build_image API](https://docs.rs/bollard/latest/bollard/image/struct.Docker.html#method.build_image)
- Docker Compose build specification

---

**次のステップ**: 設計書の作成 (`design/03-docker-build.md`)
