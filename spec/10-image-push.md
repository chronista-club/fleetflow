# 仕様書: イメージプッシュ機能

**作成日**: 2025-12-13
**ステータス**: 仕様策定中

## What & Why - 何を作るか、なぜ作るか

### 概要

`flow build` コマンドに `--push` オプションを追加し、ビルドしたイメージをコンテナレジストリにプッシュできるようにする。

### 背景

現在のFleetFlowは以下の機能を持っている：
- ローカルでのDockerイメージビルド（`flow build`）
- クラウドサーバーへのデプロイ（`flow cloud up`）

しかし、CI/CDパイプラインや本番更新では以下のフローが必要：

```
ビルド → レジストリにプッシュ → 本番サーバーでpull
```

現状ではプッシュ部分が欠けており、手動で `docker push` を実行する必要がある。

### 目的

1. **CI/CD対応**: GitHub Actions等からワンコマンドでビルド＆プッシュ
2. **手動デプロイ対応**: 開発者がローカルから本番イメージを更新可能
3. **タグ管理**: コミットハッシュやバージョンタグでイメージを管理

### ユースケース

#### UC-1: GitHub Actionsでの自動ビルド

```yaml
# .github/workflows/deploy.yml
jobs:
  deploy:
    steps:
      - uses: actions/checkout@v4
      - name: Login to GHCR
        run: echo "${{ secrets.GITHUB_TOKEN }}" | docker login ghcr.io -u ${{ github.actor }} --password-stdin
      - name: Build and Push
        run: flow build --push --tag ${{ github.sha }} prod
      - name: Deploy
        run: flow cloud up --stage prod
```

#### UC-2: 手動でのリリース

```bash
# ローカルからリリース
docker login ghcr.io
flow build --push --tag v1.2.0 prod
flow cloud up --stage prod
```

#### UC-3: 開発中のテストデプロイ

```bash
# 開発ブランチのイメージをdevサーバーにデプロイ
flow build --push --tag feature-xxx dev
flow cloud up --stage dev
```

## 機能仕様

### 1. コマンド構文

```bash
flow build [OPTIONS] [SERVICE] <STAGE>
```

#### 新規オプション

| オプション | 説明 |
|-----------|------|
| `--push` | ビルド後にレジストリにプッシュ |
| `--tag <TAG>` | イメージタグを指定（`--push`と併用） |

#### 使用例

```bash
# ステージ内の全サービスをビルド＆プッシュ
flow build --push prod

# 特定サービスのみ
flow build --push api prod

# タグを指定
flow build --push --tag v1.2.0 prod
flow build --push --tag abc123def prod

# キャッシュなしでビルド＆プッシュ
flow build --push --no-cache prod
```

### 2. タグ解決ルール

タグは以下の優先順位で決定される：

1. `--tag` オプション（最優先）
2. KDLの `image` フィールドに含まれるタグ
3. デフォルト: `latest`

#### 例

```kdl
service "api" {
    image "ghcr.io/org/myapp:main"
    dockerfile "./Dockerfile"
}
```

| コマンド | プッシュされるイメージ |
|---------|---------------------|
| `flow build --push prod` | `ghcr.io/org/myapp:main` |
| `flow build --push --tag v1.0 prod` | `ghcr.io/org/myapp:v1.0` |
| `flow build --push --tag abc123 prod` | `ghcr.io/org/myapp:abc123` |

### 3. レジストリ認証

#### 認証情報の取得元

Docker CLIの標準的な認証情報を使用：

1. `~/.docker/config.json`
2. 環境変数 `DOCKER_CONFIG` で指定されたパス
3. credential helper（docker-credential-osxkeychain等）

#### 対応レジストリ

- Docker Hub (`docker.io`)
- GitHub Container Registry (`ghcr.io`)
- Google Container Registry (`gcr.io`)
- Amazon ECR (`*.dkr.ecr.*.amazonaws.com`)
- その他Docker互換レジストリ

#### エラーハンドリング

```
エラー: レジストリへの認証に失敗しました

対象: ghcr.io/org/myapp

解決方法:
  • docker login ghcr.io を実行してください
  • CIの場合は DOCKER_USERNAME, DOCKER_PASSWORD を設定してください
```

### 4. KDL設定

#### 基本形式

```kdl
service "api" {
    image "ghcr.io/org/myapp:latest"
    dockerfile "./Dockerfile"
}
```

#### レジストリ別の設定例

```kdl
// GitHub Container Registry
service "api" {
    image "ghcr.io/myorg/myapp:latest"
    dockerfile "./services/api/Dockerfile"
}

// Docker Hub
service "web" {
    image "myuser/myapp:latest"
    dockerfile "./services/web/Dockerfile"
}

// Amazon ECR
service "worker" {
    image "123456789.dkr.ecr.ap-northeast-1.amazonaws.com/myapp:latest"
    dockerfile "./services/worker/Dockerfile"
}
```

### 5. 出力形式

#### 成功時

```
Building api...
  → Dockerfile: ./services/api/Dockerfile
  → Context: .
  → Target: production
  [####################################] 100%

Pushing api...
  → ghcr.io/org/myapp:v1.2.0
  [####################################] 100%

✓ api: ghcr.io/org/myapp:v1.2.0
```

#### 複数サービスの場合

```
Building api...
  [####################################] 100%
Building worker...
  [####################################] 100%

Pushing api...
  [####################################] 100%
Pushing worker...
  [####################################] 100%

✓ api: ghcr.io/org/myapp:v1.2.0
✓ worker: ghcr.io/org/myapp-worker:v1.2.0
```

### 6. エラーハンドリング

| エラー | メッセージ | 解決方法 |
|--------|----------|---------|
| 認証失敗 | `レジストリへの認証に失敗しました` | `docker login` を実行 |
| イメージ名不正 | `イメージ名にレジストリが含まれていません` | `image` フィールドを修正 |
| プッシュ失敗 | `イメージのプッシュに失敗しました` | ネットワーク/権限を確認 |
| タグ不正 | `タグに使用できない文字が含まれています` | タグを修正 |

## 非機能要件

### 1. パフォーマンス

- レイヤーキャッシュを活用した効率的なプッシュ
- 並列プッシュ（複数サービスの場合）

### 2. セキュリティ

- 認証情報はDocker標準の仕組みを使用（FleetFlow独自の保存なし）
- ログに認証情報を出力しない

### 3. 互換性

- Docker CLI と同じ認証フローを使用
- 既存の `flow build` コマンドとの後方互換性

## 制約

### Phase 1（本実装）

- シングルプラットフォーム（ホストのアーキテクチャのみ）
- 単一タグのプッシュ
- Docker CLI 標準の認証のみ

### 将来の拡張（Phase 2以降）

- マルチプラットフォームビルド（`--platform linux/amd64,linux/arm64`）
- 複数タグの同時プッシュ（`--tag v1.2.0 --tag latest`）
- 環境変数による認証（`DOCKER_USERNAME`, `DOCKER_PASSWORD`）

## 関連ドキュメント

- [Dockerビルド仕様](./07-docker-build.md)
- [クラウドインフラ仕様](./08-cloud-infrastructure.md)
- [イメージプッシュ設計](../design/06-image-push.md)
- [CI/CDデプロイガイド](../guides/03-ci-deployment.md)
