# ガイド: Dockerイメージビルド機能の使い方

**Issue**: #10
**関連**: `spec/07-docker-build.md`, `design/03-docker-build.md`
**作成日**: 2025-11-23

## このガイドについて

FleetFlowでカスタムDockerイメージをビルドする方法を、実践的な例とともに解説します。

## 対象読者

- FleetFlowを使ってカスタムアプリケーションを開発したい方
- Docker Composeのビルド機能からFleetFlowに移行したい方
- マイクロサービスやモノレポでのビルドを効率化したい方

## 前提知識

- Dockerfileの基本的な書き方
- FleetFlowの基本的な使い方（`flow up`, `flow down`）
- flow.kdlの基本構文

## 基本的な使い方

### 1. 最小限の設定

**プロジェクト構造**:
```
my-app/
├── flow.kdl
├── services/
│   └── api/
│       ├── Dockerfile
│       ├── package.json
│       └── src/
│           └── index.js
```

**flow.kdl**:
```kdl
project "my-app"

stage "local" {
    service "api"
}

service "api" {
    // Dockerfileの指定は不要（規約で自動検出）

    ports {
        port host=3000 container=3000
    }
}
```

**Dockerfile** (`services/api/Dockerfile`):
```dockerfile
FROM node:20-alpine
WORKDIR /app
COPY package*.json ./
RUN npm install
COPY . .
EXPOSE 3000
CMD ["npm", "start"]
```

**起動**:
```bash
flow up local
```

→ 自動的に`services/api/Dockerfile`が検出され、ビルド後にコンテナが起動します。

### 2. 明示的なDockerfile指定

Dockerfileが規約と異なる場所にある場合：

**プロジェクト構造**:
```
my-app/
├── flow.kdl
├── backend/
│   ├── api.dockerfile
│   └── src/
```

**flow.kdl**:
```kdl
service "api" {
    dockerfile "./backend/api.dockerfile"

    ports {
        port host=3000 container=3000
    }
}
```

## ビルド引数の使用

### 1. 基本的なビルド引数

**flow.kdl**:
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

    ports {
        port host=3000 container=3000
    }
}
```

**Dockerfile**:
```dockerfile
ARG NODE_VERSION=18
FROM node:${NODE_VERSION}-alpine

ARG APP_ENV
ENV APP_ENV=${APP_ENV}

WORKDIR /app
COPY . .
RUN npm install
CMD ["npm", "start"]
```

### 2. ステージごとに異なるビルド引数

```kdl
variables {
    NODE_VERSION "20"
}

stage "local" {
    variables {
        APP_ENV "development"
        DEBUG "true"
    }
    service "api"
}

stage "live" {
    variables {
        APP_ENV "production"
        DEBUG "false"
    }
    service "api"
}

service "api" {
    build_args {
        NODE_VERSION "{NODE_VERSION}"
        APP_ENV "{APP_ENV}"
        DEBUG "{DEBUG}"
    }
}
```

**使用例**:
```bash
# 開発環境でビルド・起動
flow up local
# → APP_ENV=development, DEBUG=true

# ライブ環境でビルド・起動
flow up live
# → APP_ENV=production, DEBUG=false
```

## マルチステージビルドの活用

### 1. 開発用と本番用でビルドターゲットを切り替え

**Dockerfile**:
```dockerfile
# 開発用ステージ
FROM node:20 AS development
WORKDIR /app
COPY package*.json ./
RUN npm install
COPY . .
EXPOSE 3000
CMD ["npm", "run", "dev"]

# 本番用ステージ（最適化）
FROM node:20-alpine AS production
WORKDIR /app
COPY package*.json ./
RUN npm install --production
COPY . .
RUN npm run build
EXPOSE 3000
CMD ["npm", "start"]
```

**flow.kdl**:
```kdl
stage "local" {
    service "api"
}

stage "live" {
    service "api"
}

service "api" {
    dockerfile "./Dockerfile"

    # ステージごとに異なるターゲットを指定したい場合
    # （将来対応）
}
```

**workaround（現時点）**:

ステージごとに異なるサービス定義を使用：

```kdl
stage "local" {
    service "api-dev"
}

stage "live" {
    service "api-live"
}

service "api-dev" {
    dockerfile "./Dockerfile"
    target "development"

    ports {
        port host=3000 container=3000
    }

    volumes {
        volume host="./src" container="/app/src"
    }
}

service "api-live" {
    dockerfile "./Dockerfile"
    target "production"

    ports {
        port host=3000 container=3000
    }
}
```

## リビルドコマンドの使い方

### 1. 特定のサービスをリビルド

コード変更後、すぐにリビルドして動作確認：

```bash
# apiサービスをリビルド
flow rebuild api

# キャッシュなしでリビルド
flow rebuild api --no-cache
```

### 2. upコマンドでのリビルド

全サービスをリビルドして起動：

```bash
# 全サービスをリビルド
flow up --build local

# キャッシュなしでリビルド
flow up --build --no-cache local
```

## 実践的なパターン

### パターン1: フルスタックWebアプリケーション

**プロジェクト構造**:
```
webapp/
├── flow.kdl
├── frontend/
│   ├── Dockerfile
│   ├── package.json
│   └── src/
├── backend/
│   ├── Dockerfile
│   ├── go.mod
│   └── main.go
└── docker-compose.yml  # 削除可能
```

**flow.kdl**:
```kdl
project "webapp"

variables {
    NODE_VERSION "20"
    GO_VERSION "1.21"
}

stage "local" {
    service "frontend"
    service "backend"
    service "db"
}

service "frontend" {
    # 規約で自動検出: ./frontend/Dockerfile

    build_args {
        NODE_VERSION "{NODE_VERSION}"
    }

    ports {
        port host=3000 container=3000
    }

    env {
        API_URL "http://backend:8000"
    }

    volumes {
        volume host="./frontend/src" container="/app/src"
    }
}

service "backend" {
    # 規約で自動検出: ./backend/Dockerfile

    build_args {
        GO_VERSION "{GO_VERSION}"
    }

    ports {
        port host=8000 container=8000
    }

    env {
        DATABASE_URL "postgres://db:5432/webapp"
    }
}

service "db" {
    image "postgres"
    version "16"

    env {
        POSTGRES_DB "webapp"
        POSTGRES_PASSWORD "postgres"
    }

    volumes {
        volume host="./data/postgres" container="/var/lib/postgresql/data"
    }
}
```

**frontend/Dockerfile**:
```dockerfile
ARG NODE_VERSION=18
FROM node:${NODE_VERSION}-alpine

WORKDIR /app
COPY package*.json ./
RUN npm install

COPY . .
EXPOSE 3000
CMD ["npm", "run", "dev"]
```

**backend/Dockerfile**:
```dockerfile
ARG GO_VERSION=1.20
FROM golang:${GO_VERSION}-alpine AS builder

WORKDIR /app
COPY go.* ./
RUN go mod download

COPY . .
RUN go build -o server .

FROM alpine:latest
WORKDIR /app
COPY --from=builder /app/server .
EXPOSE 8000
CMD ["./server"]
```

**開発フロー**:
```bash
# 初回起動
flow up local

# フロントエンド変更後
# （ホットリロードが効いているので不要）

# バックエンド変更後
flow rebuild backend

# 全体リビルド
flow up --build local
```

### パターン2: マイクロサービス

**プロジェクト構造**:
```
microservices/
├── flow.kdl
├── services/
│   ├── auth-service/
│   │   ├── Dockerfile
│   │   └── main.go
│   ├── user-service/
│   │   ├── Dockerfile
│   │   └── main.go
│   └── order-service/
│       ├── Dockerfile
│       └── main.go
└── gateway/
    ├── nginx.conf
    └── Dockerfile
```

**flow.kdl**:
```kdl
project "microservices"

variables {
    GO_VERSION "1.21"
    ALPINE_VERSION "3.19"
}

stage "local" {
    service "auth-service"
    service "user-service"
    service "order-service"
    service "gateway"
    service "db"
    service "redis"
}

service "auth-service" {
    # 規約で自動検出: ./services/auth-service/Dockerfile

    build_args {
        GO_VERSION "{GO_VERSION}"
        ALPINE_VERSION "{ALPINE_VERSION}"
    }

    ports {
        port host=5001 container=5000
    }

    env {
        DATABASE_URL "postgres://db:5432/auth"
        REDIS_URL "redis://redis:6379"
    }
}

service "user-service" {
    build_args {
        GO_VERSION "{GO_VERSION}"
        ALPINE_VERSION "{ALPINE_VERSION}"
    }

    ports {
        port host=5002 container=5000
    }

    env {
        DATABASE_URL "postgres://db:5432/users"
    }
}

service "order-service" {
    build_args {
        GO_VERSION "{GO_VERSION}"
        ALPINE_VERSION "{ALPINE_VERSION}"
    }

    ports {
        port host=5003 container=5000
    }

    env {
        DATABASE_URL "postgres://db:5432/orders"
    }
}

service "gateway" {
    dockerfile "./gateway/Dockerfile"

    ports {
        port host=8080 container=80
    }
}

service "db" {
    image "postgres"
    version "16"

    env {
        POSTGRES_PASSWORD "postgres"
    }
}

service "redis" {
    image "redis"
    version "7-alpine"
}
```

**共通Dockerfile** (`services/*/Dockerfile`):
```dockerfile
ARG GO_VERSION=1.20
ARG ALPINE_VERSION=3.18

FROM golang:${GO_VERSION}-alpine${ALPINE_VERSION} AS builder
WORKDIR /app
COPY go.* ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 go build -o service .

FROM alpine:${ALPINE_VERSION}
WORKDIR /app
COPY --from=builder /app/service .
EXPOSE 5000
CMD ["./service"]
```

### パターン3: モノレポ構成

**プロジェクト構造**:
```
monorepo/
├── flow.kdl
├── package.json         # ルートのworkspace設定
├── pnpm-workspace.yaml
├── packages/
│   ├── shared/
│   │   └── src/
│   └── utils/
│       └── src/
└── apps/
    ├── web/
    │   ├── Dockerfile
    │   ├── package.json
    │   └── src/
    └── admin/
        ├── Dockerfile
        ├── package.json
        └── src/
```

**flow.kdl**:
```kdl
project "monorepo"

variables {
    PNPM_VERSION "8"
}

stage "local" {
    service "web"
    service "admin"
}

service "web" {
    dockerfile "./apps/web/Dockerfile"
    context "."  # モノレポルートをコンテキストに

    build_args {
        WORKSPACE "apps/web"
        PNPM_VERSION "{PNPM_VERSION}"
    }

    ports {
        port host=3000 container=3000
    }

    volumes {
        volume host="./apps/web/src" container="/monorepo/apps/web/src"
        volume host="./packages" container="/monorepo/packages"
    }
}

service "admin" {
    dockerfile "./apps/admin/Dockerfile"
    context "."

    build_args {
        WORKSPACE "apps/admin"
        PNPM_VERSION "{PNPM_VERSION}"
    }

    ports {
        port host=3001 container=3000
    }
}
```

**Dockerfile** (`apps/*/Dockerfile`):
```dockerfile
ARG PNPM_VERSION=8

FROM node:20-alpine AS base
RUN corepack enable pnpm
WORKDIR /monorepo

# 依存関係のインストール
FROM base AS deps
COPY pnpm-lock.yaml pnpm-workspace.yaml package.json ./
COPY packages ./packages
ARG WORKSPACE
COPY ${WORKSPACE}/package.json ./${WORKSPACE}/
RUN pnpm install --frozen-lockfile

# ビルド
FROM deps AS builder
ARG WORKSPACE
COPY ${WORKSPACE} ./${WORKSPACE}
RUN pnpm --filter ${WORKSPACE} build

# 実行環境
FROM node:20-alpine
WORKDIR /app
ARG WORKSPACE
COPY --from=builder /monorepo/${WORKSPACE}/dist ./
COPY --from=builder /monorepo/${WORKSPACE}/package.json ./
EXPOSE 3000
CMD ["node", "index.js"]
```

## トラブルシューティング

### ビルドが遅い

**原因**: ビルドコンテキストが大きすぎる

**解決策**: `.dockerignore`を作成して不要なファイルを除外

**.dockerignore**:
```
node_modules/
.git/
dist/
build/
*.log
.DS_Store
.env*
```

### ビルドキャッシュが効かない

**原因**: COPYコマンドの順序が最適でない

**解決策**: 変更が少ないファイルから先にCOPY

**❌ 悪い例**:
```dockerfile
FROM node:20-alpine
WORKDIR /app
COPY . .           # 全ファイルをコピー（キャッシュが効きにくい）
RUN npm install
```

**✅ 良い例**:
```dockerfile
FROM node:20-alpine
WORKDIR /app
COPY package*.json ./  # 依存関係ファイルのみ先にコピー
RUN npm install        # キャッシュが効きやすい
COPY . .              # ソースコードは後でコピー
```

### 機密情報の取り扱い

**❌ 悪い例**: ビルド引数で機密情報を渡す

```kdl
# これはNG: ビルド引数はイメージ履歴に残る
service "api" {
    build_args {
        DATABASE_PASSWORD "secret123"  # 危険！
    }
}
```

**✅ 良い例**: 環境変数で渡す

```kdl
service "api" {
    env {
        DATABASE_PASSWORD "secret123"  # ランタイム環境変数
    }
}
```

または、本番環境ではシークレット管理サービスを使用：

```kdl
service "api" {
    env {
        DATABASE_PASSWORD_FILE "/run/secrets/db_password"
    }
}
```

## ベストプラクティス

### 1. レイヤーキャッシュの最適化

```dockerfile
# 変更頻度の低いものから順に配置
FROM node:20-alpine

# 1. システムパッケージ（ほぼ変わらない）
RUN apk add --no-cache git

# 2. アプリケーションの依存関係（たまに変わる）
WORKDIR /app
COPY package*.json ./
RUN npm install

# 3. ソースコード（頻繁に変わる）
COPY . .

# 4. ビルド
RUN npm run build

CMD ["npm", "start"]
```

### 2. マルチステージビルドで最小化

```dockerfile
# ビルドステージ（大きくてもOK）
FROM node:20 AS builder
WORKDIR /app
COPY package*.json ./
RUN npm install
COPY . .
RUN npm run build

# 実行ステージ（最小限）
FROM node:20-alpine
WORKDIR /app
COPY --from=builder /app/dist ./dist
COPY --from=builder /app/node_modules ./node_modules
COPY package.json ./
CMD ["node", "dist/index.js"]
```

### 3. .dockerignoreで効率化

```
# 依存関係（ビルド時に再インストール）
node_modules/
vendor/

# ビルド成果物
dist/
build/
*.exe

# Git関連
.git/
.gitignore

# IDE設定
.vscode/
.idea/

# ログ・一時ファイル
*.log
tmp/
.DS_Store

# 環境設定ファイル
.env*
!.env.example
```

### 4. ビルド引数のバージョン管理

```kdl
variables {
    # 全体で使うバージョンを一元管理
    NODE_VERSION "20.11.0"  # 具体的なバージョン指定
    GO_VERSION "1.21.6"
    PYTHON_VERSION "3.12.1"
    ALPINE_VERSION "3.19"
}

service "frontend" {
    build_args {
        NODE_VERSION "{NODE_VERSION}"
    }
}

service "backend" {
    build_args {
        GO_VERSION "{GO_VERSION}"
        ALPINE_VERSION "{ALPINE_VERSION}"
    }
}
```

## コンテナレジストリへのプッシュ

### 1. CLI からのレジストリ指定

```bash
# ghcr.io にプッシュ
flow build api live --push --registry ghcr.io/myorg --tag v1.0.0

# プラットフォームも指定（ARM Mac から x86 イメージを作成）
flow build api live --push --registry ghcr.io/myorg --platform linux/amd64
```

### 2. KDL でレジストリを設定

レジストリは3つのレベルで設定でき、優先順位は CLI > Service > Stage > Flow です。

**プロジェクト全体のデフォルト設定**:
```kdl
project "myapp"

// プロジェクト全体のデフォルトレジストリ
registry "ghcr.io/myorg"

stage "live" {
    service "api"
    service "worker"
}
```

**ステージごとに異なるレジストリ**:
```kdl
project "myapp"

stage "dev" {
    // 開発環境は別のレジストリを使用
    registry "gcr.io/dev-project"
    service "api"
}

stage "live" {
    // ライブ環境用レジストリ
    registry "ghcr.io/live-org"
    service "api"
}
```

**サービスごとのレジストリ指定**:
```kdl
project "myapp"

// プロジェクトデフォルト
registry "ghcr.io/myorg"

stage "live" {
    service "api"
    service "db"
}

// APIサービスはデフォルトレジストリ（ghcr.io/myorg）を使用
service "api" {
    dockerfile "./services/api/Dockerfile"
}

// DBサービスはDocker Hub（registry指定なし）から取得
service "db" {
    image "postgres:16"
    // registry を指定しない → Docker Hub
}
```

**ユースケース: 自社サービスと公開イメージの混在**:
```kdl
project "vantage"

// 自社サービスのデフォルトレジストリ
registry "ghcr.io/vantage-hub"

stage "live" {
    service "api"       // ghcr.io/vantage-hub からビルド・プッシュ
    service "surrealdb" // Docker Hub から取得（registryなし）
    service "redis"     // Docker Hub から取得（registryなし）
}

service "api" {
    dockerfile "./services/api/Dockerfile"
    // registry は Flow の設定を継承 → ghcr.io/vantage-hub
}

service "surrealdb" {
    image "surrealdb/surrealdb:v2.2.1"
    // registry 指定なし → Docker Hub
}

service "redis" {
    image "redis:7-alpine"
    // registry 指定なし → Docker Hub
}
```

### 3. レジストリ認証

レジストリへの認証は Docker CLI の認証情報を使用します。

```bash
# ghcr.io の認証
echo $GITHUB_TOKEN | docker login ghcr.io -u USERNAME --password-stdin

# gcr.io の認証
gcloud auth configure-docker

# ECR の認証
aws ecr get-login-password --region ap-northeast-1 | docker login --username AWS --password-stdin <account>.dkr.ecr.ap-northeast-1.amazonaws.com
```

### 4. CI/CDでの活用

GitHub Actions での例:

```yaml
- name: Login to GitHub Container Registry
  uses: docker/login-action@v3
  with:
    registry: ghcr.io
    username: ${{ github.actor }}
    password: ${{ secrets.GITHUB_TOKEN }}

- name: Build and push
  run: |
    flow build api live --push --tag ${{ github.sha }}
```

## 次のステップ

- [変数展開の詳細](./03-variables.md)（将来追加予定）
- [CI/CDとの統合](./04-ci-cd.md)（将来追加予定）
- [本番環境へのデプロイ](./05-production.md)（将来追加予定）

## 参考資料

- [Dockerfile Best Practices](https://docs.docker.com/develop/develop-images/dockerfile_best-practices/)
- [Multi-stage builds](https://docs.docker.com/build/building/multi-stage/)
- [.dockerignore file](https://docs.docker.com/engine/reference/builder/#dockerignore-file)

---

**フィードバック募集**: このガイドで分かりにくい点があれば、[Issue](https://github.com/chronista-club/fleetflow/issues)でお知らせください。
