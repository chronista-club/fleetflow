# フィードバック: Creo Memoriesでの利用経験

## 概要

[Creo Memories](https://github.com/chronista-club/creo-memories) プロジェクトでFleetFlowを採用した際に発見した課題と改善提案をまとめています。

## 環境

- FleetFlow: v0.2.x
- KDL: v2
- Docker: Docker Desktop / OrbStack
- OS: macOS (Apple Silicon)

## 発見した課題

### Issue 1: ネットワーク自動設定の欠如

**重要度**: 高

**症状**:
- `fleet up` 後、コンテナ間でホスト名解決ができない
- `ws://surrealdb:8000` などのサービス名でのアクセスが失敗

**根本原因**:
FleetFlowがDockerネットワークを自動作成・接続しないため、各コンテナがデフォルトの`bridge`ネットワークで起動し、相互にDNS解決できない。

**現在のワークアラウンド**:

```bash
# 手動でネットワーク作成
docker network create creo-memories-local

# 各コンテナを手動で接続
docker network connect --alias surrealdb creo-memories-local creo-memories-local-surrealdb
docker network connect --alias qdrant creo-memories-local creo-memories-local-qdrant
# ... 他のサービスも同様
```

**提案**:

```kdl
// Option A: 暗黙的ネットワーク
// stageごとに自動的にネットワークを作成（`{project}-{stage}`）
// 例: creo-memories-local

stage "local" {
    service "surrealdb"
    service "qdrant"
    service "creo-mcp-server"
}

// Option B: 明示的ネットワーク設定
stage "local" {
    network "app-network" {
        driver "bridge"
    }

    service "surrealdb" {
        network "app-network"
    }
}
```

**期待される動作**:
1. `fleet up --stage local` でネットワーク自動作成
2. 定義されたサービスを自動接続
3. サービス名でのDNS解決が可能
4. `fleet down` でネットワークも削除

---

### Issue 2: 環境変数キーワードの混乱

**重要度**: 中

**症状**:
- `env` ブロックで定義した環境変数がコンテナに渡らない
- 設定が無視されるが、エラーメッセージが表示されない

**根本原因**:
KDL v2構文では `env` ではなく `environment` キーワードを使用する必要があるが、`env` を使用してもパースエラーにならず、暗黙的に無視される。

**提案**:

```
// Option A: 警告表示
[WARN] Unknown block 'env' in service definition. Did you mean 'environment'?

// Option B: 両方のキーワードをサポート
env { ... }      // エイリアスとして許可
environment { ... }  // 正式なキーワード
```

---

### Issue 3: KDL v2ブール値の認識

**重要度**: 低

**症状**:
- `true` / `false` がブール値として認識されない

**現状**:
KDL v2仕様に準拠して `#true` / `#false` を使用する必要がある。これは正しい動作だが、初見では混乱しやすい。

**提案**:
- ドキュメントでKDL v2構文の違いを明記
- エラーメッセージで「Did you mean `#true`?」と提案

---

### Issue 4: さくらのクラウド連携

**重要度**: 高（将来機能）

**現状**:
[spec/08-cloud-infrastructure.md](../spec/08-cloud-infrastructure.md) でクラウド連携が計画されているが、未実装。

**ユースケース要件**:

1. **サーバー作成**
   - さくらのクラウドにサーバー作成（usacloud経由）
   - SSH鍵自動設定
   - パケットフィルタ適用

2. **リモートデプロイ**
   ```bash
   # ローカルで設定を定義し、リモートにデプロイ
   fleet up --stage dev --remote
   ```

3. **状態管理**
   - サーバーIP、ステータスなどを状態ファイルで管理
   - 宣言的な収束（Terraform的アプローチ）

**usacloudとの統合案**:

```kdl
providers {
    sakura-cloud {
        zone "tk1a"
        // 認証は usacloud config から取得
    }
}

server "creo-dev-01" {
    provider "sakura-cloud"
    plan core=4 memory=4
    disk size=100 os="ubuntu-22.04"
    ssh-key "~/.ssh/creo-cloud.pub"

    // このサーバーで起動するサービス群
    deploy-services "creo-stack"
}
```

---

## 良かった点

### 1. KDL構文の可読性

Docker Composeに比べて設定が簡潔で読みやすい。

```kdl
// FleetFlow (KDL)
service "surrealdb" {
    image "surrealdb/surrealdb"
    version "v2.4.0"
    ports {
        port host=12000 container=8000
    }
}
```

```yaml
# Docker Compose (YAML)
services:
  surrealdb:
    image: surrealdb/surrealdb:v2.4.0
    ports:
      - "12000:8000"
```

### 2. ステージ分離

`local` / `dev` / `live` のステージ分離が明確で、環境ごとの設定差分が管理しやすい。

### 3. flow.local.kdlによる機密情報分離

機密情報を別ファイルに分離し、gitignoreで管理できる設計が良い。

---

## 優先度まとめ

| 優先度 | Issue | 影響 |
|-------|-------|------|
| P1 | ネットワーク自動設定 | 全プロジェクトでワークアラウンドが必要 |
| P2 | さくらのクラウド連携 | クラウドデプロイに手作業が残る |
| P3 | envキーワード警告 | 初期設定時の混乱 |
| P4 | ブール値エラー改善 | マイナーな UX 改善 |

---

## 関連ドキュメント

- [ユースケース詳細](./use-case-creo-memories.md)
- [Cloud Infrastructure Spec](../spec/08-cloud-infrastructure.md)
- [Cloud Infrastructure Design](../design/04-cloud-infrastructure.md)
