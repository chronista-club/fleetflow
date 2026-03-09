# CI/CDデプロイガイド

FleetFlowを使ったCI/CDパイプラインの構築ガイドです。

## 概要

FleetFlowは以下のワークフローをサポートします：

```
コード変更 → CI (ビルド & プッシュ) → 本番サーバー (デプロイ)
```

## 前提条件

- FleetFlowがインストールされていること
- Dockerレジストリへのアクセス権があること
- （クラウドデプロイの場合）クラウドプロバイダーの認証設定

## GitHub Actionsでの使用

### 基本的なワークフロー

```yaml
# .github/workflows/deploy.yml
name: Deploy

on:
  push:
    branches: [main]
  workflow_dispatch:

env:
  REGISTRY: ghcr.io

jobs:
  build-and-deploy:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write

    steps:
      - uses: actions/checkout@v4

      - name: Install FleetFlow
        run: cargo install --git https://github.com/chronista-club/fleetflow

      - name: Login to Container Registry
        run: echo "${{ secrets.GITHUB_TOKEN }}" | docker login ghcr.io -u ${{ github.actor }} --password-stdin

      - name: Build and Push
        run: fleet build live --push --tag ${{ github.sha }}

      - name: Deploy to Live
        run: flow cloud up --stage live --yes
        env:
          CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}
          CLOUDFLARE_ZONE_ID: ${{ secrets.CLOUDFLARE_ZONE_ID }}
```

### タグ付きリリース

```yaml
# .github/workflows/release.yml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  release:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write

    steps:
      - uses: actions/checkout@v4

      - name: Install FleetFlow
        run: cargo install --git https://github.com/chronista-club/fleetflow

      - name: Login to Container Registry
        run: echo "${{ secrets.GITHUB_TOKEN }}" | docker login ghcr.io -u ${{ github.actor }} --password-stdin

      - name: Get version from tag
        id: version
        run: echo "VERSION=${GITHUB_REF#refs/tags/}" >> $GITHUB_OUTPUT

      - name: Build and Push
        run: fleet build live --push --tag ${{ steps.version.outputs.VERSION }}
```

### プレ → ライブのフロー

```yaml
# .github/workflows/pre.yml
name: Deploy to Pre

on:
  push:
    branches: [develop]

jobs:
  deploy-pre:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install FleetFlow
        run: cargo install --git https://github.com/chronista-club/fleetflow
      - name: Login to GHCR
        run: echo "${{ secrets.GITHUB_TOKEN }}" | docker login ghcr.io -u ${{ github.actor }} --password-stdin
      - name: Build and Push
        run: fleet build pre --push --tag pre-${{ github.sha }}
      - name: Deploy
        run: flow cloud up --stage pre --yes
```

```yaml
# .github/workflows/live.yml
name: Deploy to Live

on:
  workflow_dispatch:
    inputs:
      tag:
        description: 'Image tag to deploy'
        required: true

jobs:
  deploy-live:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install FleetFlow
        run: cargo install --git https://github.com/chronista-club/fleetflow
      - name: Deploy
        run: flow cloud up --stage live --yes
        env:
          IMAGE_TAG: ${{ github.event.inputs.tag }}
```

## fleet.kdl の設定例

### 基本設定

```kdl
project "myapp"

// プレ環境
stage "pre" {
    service "api"
    service "worker"
}

// ライブ環境
stage "live" {
    service "api"
    service "worker"
}

// APIサービス
service "api" {
    image "ghcr.io/myorg/myapp-api:latest"
    dockerfile "./services/api/Dockerfile"
    target "production"

    ports {
        port 3000 3000
    }

    env {
        NODE_ENV "production"
    }
}

// Workerサービス
service "worker" {
    image "ghcr.io/myorg/myapp-worker:latest"
    dockerfile "./services/worker/Dockerfile"
    target "production"

    env {
        NODE_ENV "production"
    }
}
```

### クラウドインフラ設定

```kdl
project "myapp"

providers {
    sakura-cloud { zone "tk1a" }
    cloudflare { account-id env="CF_ACCOUNT_ID" }
}

stage "live" {
    server "app-server" {
        provider "sakura-cloud"
        plan core=4 memory=8
        disk size=100 os="ubuntu-24.04"
        dns_aliases "api" "app"
    }

    service "api"
    service "worker"
}

service "api" {
    image "ghcr.io/myorg/myapp-api"
    dockerfile "./Dockerfile"
}

service "worker" {
    image "ghcr.io/myorg/myapp-worker"
    dockerfile "./services/worker/Dockerfile"
}
```

## 手動デプロイ

### ローカルからのデプロイ

```bash
# 1. レジストリにログイン
docker login ghcr.io

# 2. ビルド & プッシュ
fleet build live --push --tag v1.2.0

# 3. ライブにデプロイ
flow cloud up --stage live
```

### 特定サービスのみ更新

```bash
# APIサービスのみビルド & プッシュ
fleet build live -n api --push --tag v1.2.1

# ライブサーバーでAPIのみ再起動
flow cloud restart api --stage live
```

## ベストプラクティス

### 1. イメージタグ戦略

| 環境 | タグ形式 | 例 |
|------|---------|-----|
| 開発 | `dev-{commit}` | `dev-abc123` |
| プレ | `pre-{commit}` | `pre-abc123` |
| ライブ | `v{version}` | `v1.2.0` |

### 2. シークレット管理

GitHub Secretsで管理すべき情報：

| シークレット | 用途 |
|-------------|------|
| `CLOUDFLARE_API_TOKEN` | Cloudflare DNS管理 |
| `CLOUDFLARE_ZONE_ID` | Cloudflare Zone ID |
| `SSH_PRIVATE_KEY` | サーバーへのSSH接続 |

### 3. Dockerfile最適化

```dockerfile
# マルチステージビルドで本番イメージを軽量化
FROM node:20 AS builder
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY . .
RUN npm run build

FROM node:20-alpine AS production
WORKDIR /app
COPY --from=builder /app/dist ./dist
COPY --from=builder /app/node_modules ./node_modules
CMD ["node", "dist/index.js"]
```

### 4. ヘルスチェック

```kdl
service "api" {
    image "ghcr.io/myorg/myapp-api"

    healthcheck {
        test "curl -f http://localhost:3000/health || exit 1"
        interval 30
        timeout 10
        retries 3
        start_period 60
    }
}
```

## トラブルシューティング

### プッシュに失敗する

```
エラー: レジストリへの認証に失敗しました
```

**解決方法**:
1. `docker login` が正しく実行されているか確認
2. GitHub Actionsの場合、`permissions.packages: write` を確認
3. トークンの有効期限を確認

### デプロイ後にサービスが起動しない

**確認手順**:
1. `fleet logs --stage live` でログを確認
2. イメージタグが正しいか確認
3. 環境変数が設定されているか確認

### DNSが更新されない

**確認事項**:
1. `CLOUDFLARE_API_TOKEN` が設定されているか
2. トークンに `Zone:DNS:Edit` 権限があるか
3. `CLOUDFLARE_ZONE_ID` が正しいか

## 関連ドキュメント

- [イメージプッシュ仕様](../spec/10-image-push.md)
- [クラウドインフラ仕様](../spec/08-cloud-infrastructure.md)
- [DNS連携仕様](../spec/09-dns-integration.md)
