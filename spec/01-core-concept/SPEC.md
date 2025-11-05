# Core Concept - 仕様書

## コンセプト

### プロセスとFlow

#### 全てはプロセスである

**プロセス**とは、OS上で実行される最小単位です。全てのアプリケーションは最終的にプロセスとして実行されます。

```mermaid
graph LR
    A[アプリケーション] --> B[プロセス群]
    B --> C[PostgreSQLプロセス]
    B --> D[Redisプロセス]
    B --> E[APIプロセス]

    C --> F[PID: 1234<br/>メモリ: 256MB<br/>CPU: 5%]
    D --> G[PID: 1235<br/>メモリ: 64MB<br/>CPU: 2%]
    E --> H[PID: 1236<br/>メモリ: 128MB<br/>CPU: 10%]
```

#### FlowとProcessの関係

**Flow**は、プロセスの**設計図**です。Flowを実行すると、複数のプロセスが起動します。

```mermaid
sequenceDiagram
    participant User
    participant Flow
    participant Docker
    participant OS

    User->>Flow: unison up --stage=local
    Flow->>Flow: flow.kdl を解析
    Flow->>Docker: コンテナ作成リクエスト
    Docker->>OS: プロセス起動
    OS-->>Docker: PID: 1234 (postgres)
    Docker-->>Flow: コンテナID: abc123
    OS-->>Docker: PID: 1235 (redis)
    Docker-->>Flow: コンテナID: def456
    Flow-->>User: ✓ 起動完了

    Note over OS: プロセスが実行中
```

#### 抽象化レイヤー

Unison Flowは、プロセス管理を段階的に抽象化します：

```mermaid
graph TD
    subgraph "抽象度: 高い（人間に優しい）"
        A[Flow/Stage定義<br/>flow.kdl]
    end

    subgraph "抽象度: 中程度"
        B[Service定義<br/>論理的なサービス]
        C[Docker Image<br/>パッケージ化]
    end

    subgraph "抽象度: 低い（機械に近い）"
        D[Container<br/>分離された実行環境]
        E[Process<br/>OS上の実行単位]
    end

    A --> B
    B --> C
    C --> D
    D --> E

    style A fill:#e1f5ff
    style E fill:#ffe1e1
```

#### プロセスライフサイクル

```mermaid
stateDiagram-v2
    [*] --> 定義: flow.kdlに記述
    定義 --> 解析: unison up
    解析 --> イメージPull: Dockerイメージ取得
    イメージPull --> コンテナ作成: docker create
    コンテナ作成 --> プロセス起動: docker start
    プロセス起動 --> 実行中: プロセスが動作

    実行中 --> 停止中: unison down
    停止中 --> プロセス起動: unison up

    実行中 --> 異常終了: クラッシュ
    異常終了 --> プロセス起動: 再起動

    停止中 --> 削除: unison down --remove
    削除 --> [*]

    note right of 実行中
        プロセスがCPU/メモリを使用
        PID, ポート, ファイルを占有
    end note
```

### 基本概念の定義

Unison Flowでは2つの基本概念を使用します：

#### サービス（Service）

**サービス**とは、Unison Flowにおける最小単位の実行可能なコンポーネントです。

サービスは以下の特性を持ちます：

1. **独立性**: 各サービスは独自のコンテナで実行される
2. **命名**: 一意な名前により識別される
3. **宣言的**: 望む状態を記述する（どう起動するかではなく、何が必要か）
4. **組み合わせ可能**: 他のサービスと組み合わせて環境を構築

#### 例

```kdl
// データベースサービス
service "postgres" {
    version "16"
}

// キャッシュサービス
service "redis" {
    version "7"
}

// アプリケーションサービス
service "api" {
    image "myapp:1.0.0"
    depends_on "postgres" "redis"
}
```

#### サービスの責務

- **単一責任**: 1つのサービスは1つの役割を持つ
  - ✅ Good: `service "postgres"` - データベース専用
  - ❌ Bad: `service "all-in-one"` - DB + API + フロントエンド

- **疎結合**: サービス間は疎結合を保つ
  - ネットワーク経由で通信
  - 明示的な依存関係（`depends_on`）

- **置き換え可能**: サービスは他の実装に置き換え可能
  - PostgreSQL → MySQL
  - Redis → Memcached

#### サービス vs コンテナ

| 観点         | サービス（Service）  | コンテナ（Container）   |
| ------------ | -------------------- | ----------------------- |
| **抽象度**   | 高い（論理的）       | 低い（物理的）          |
| **定義場所** | flow.kdl             | 実行時にDockerが作成    |
| **スコープ** | アプリケーション全体 | 1つの実行インスタンス   |
| **例**       | "postgres"サービス   | flow-postgres-1コンテナ |

**関係性**:

```
Service (flow.kdl)
  ↓ 変換
Docker Image (postgres:16)
  ↓ 起動
Container (flow-postgres-1)
```

#### サービスの状態遷移

```
[定義] --parse--> [設定] --create--> [作成済み] --start--> [実行中]
                                        ↓                      ↓
                                     [削除] <---stop--- [停止済み]
```

#### ステージ（Stage）

**ステージ**とは、アプリケーションがデプロイされる物理的・論理的な実行環境です。

##### 定義

ステージは以下の責務を持ちます：

1. **実行場所**: コードが実際に動作する場所（開発マシン or クラウド）
2. **サービス構成**: どのサービスを起動するか
3. **設定管理**: ステージ固有の変数を定義
4. **分離レベル**: ステージ間の独立性
5. **目的**: 各ステージの役割と責任

##### 標準的なステージ

| ステージ  | 名称         | 場所       | 目的           | 特徴                       |
| --------- | ------------ | ---------- | -------------- | -------------------------- |
| **local** | ローカル     | 開発マシン | 開発・デバッグ | 高速なフィードバックループ |
| **dev**   | 開発         | クラウド   | 統合テスト     | チーム共有、CI/CD連携      |
| **stg**   | ステージング | クラウド   | 本番前検証     | 本番環境に近い構成         |
| **prd**   | 本番         | クラウド   | ユーザー提供   | 高可用性、監視             |

##### ステージの定義例

```kdl
// local: 開発マシン上
// Docker Desktop / Podman
stage "local" {
    service "postgres"
    service "redis"
    service "api"

    variables {
        DEBUG "true"
        LOG_LEVEL "debug"
    }
}

// dev: クラウド開発環境
// Cloud Run / ECS / Kubernetes
stage "dev" {
    service "postgres"
    service "redis"
    service "api"

    variables {
        DEBUG "true"
        LOG_LEVEL "info"
    }
}

// stg: ステージング環境
// 本番と同じインフラ構成
stage "stg" {
    service "postgres"
    service "redis"
    service "api"
    service "worker"

    variables {
        DEBUG "false"
        LOG_LEVEL "info"
    }
}

// prd: 本番環境
// 高可用性・監視・バックアップ
stage "prd" {
    service "postgres"
    service "redis"
    service "api"
    service "worker"

    variables {
        DEBUG "false"
        LOG_LEVEL "warn"
    }
}

// 共通のサービス定義
service "postgres" {
    version "16"
    port 5432:5432
}

service "redis" {
    version "7"
    port 6379:6379
}

service "api" {
    image "myapp"
    version "1.0.0"
    port 8080:3000
    depends_on "postgres" "redis"
}

service "worker" {
    image "myapp-worker"
    version "1.0.0"
    depends_on "postgres" "redis"
}
```

##### ステージの使い方

```bash
# ローカル開発
unison up --stage=local

# クラウド開発環境
unison up --stage=dev

# ステージング検証
unison up --stage=stg

# 本番デプロイ
unison up --stage=prd
```

##### ステージの責務

Stageは以下を統合的に管理します：

- **実行場所**: local（開発マシン）or cloud（クラウド）
- **サービス構成**: どのサービスを起動するか
- **環境変数**: ステージ固有の設定値

##### ステージ間の違い

**Local (開発マシン)**

- コンテナランタイム: Docker Desktop / Podman
- リソース: 開発マシンのCPU/メモリ
- データ: モックまたは最小セット
- アクセス: 開発者のみ

**Dev (クラウド開発)**

- コンテナランタイム: Cloud Run / ECS
- リソース: 小規模（コスト重視）
- データ: 共有の開発用データ
- アクセス: チーム全体

**Stg (ステージング)**

- コンテナランタイム: 本番と同じ
- リソース: 本番に近い構成
- データ: 本番の複製（匿名化）
- アクセス: QAチーム、承認者

**Prd (本番)**

- コンテナランタイム: 高可用性構成
- リソース: スケーラブル
- データ: 実データ
- アクセス: エンドユーザー

### ビジョン

Unison Flowは、**設定ファイルを書く喜び**を提供します。

Docker Composeの手軽さを保ちつつ、KDLの美しさと可読性により、開発者が直感的に理解できる環境構築ツールを目指します。「設定より規約」の哲学により、最小限の記述で最大限の機能を実現します。

### 哲学・設計原則

#### 1. Convention over Configuration（設定より規約）

**哲学**: 開発者の時間は貴重である。明らかなことは書かなくて良い。

```kdl
// Bad: 冗長
service "postgres" {
    image "postgres:latest"
    protocol "tcp"
    read_only false
}

// Good: 規約により省略
service "postgres" {
    // image, protocol, read_onlyは自動推測
}
```

**トレードオフ**:

- 利点: 簡潔、学習コストが低い
- 欠点: 暗黙的な動作、デバッグが難しい場合も
- 判断: 開発体験を優先。明示的にも書ける設計。

#### 2. Progressive Disclosure（段階的な開示）

**哲学**: シンプルなことはシンプルに。複雑なことは可能に。

```kdl
// レベル1: 初心者 - 最小構成
service "api" {}

// レベル2: 中級者 - 一般的な設定
service "api" {
    ports {
        port 8080 3000
    }
}

// レベル3: 上級者 - 詳細制御
service "api" {
    ports {
        port 8080 3000 protocol="tcp" host_ip="127.0.0.1"
    }
}
```

#### 3. Declarative over Imperative（宣言的 > 命令的）

**哲学**: 「何を」したいかを宣言する。「どう」するかはツールに任せる。

```kdl
// 宣言的: 望む状態を記述
environment "production" {
    services "api" "db" "redis"
}

// ツールが以下を自動処理:
// - サービスの起動順序（依存関係解決）
// - ネットワーク設定
// - ヘルスチェック
```

### 他との違い

| 観点             | Docker Compose | Unison Flow      |
| ---------------- | -------------- | ---------------- |
| **記述言語**     | YAML           | KDL              |
| **可読性**       | 中             | 高               |
| **規約**         | 少ない         | 多い（自動推測） |
| **モジュール化** | 限定的         | include/変数展開 |
| **学習曲線**     | 緩やか         | より緩やか       |

**独自性**:

- KDLによる美しい構文
- サービス名からのイメージ自動推測
- 階層的なincludeシステム（計画中）
- 変数展開とテンプレート（計画中）

## 仕様

### 機能仕様

#### FS-001: Service定義

**目的**: コンテナ化されたサービスを宣言的に定義

**入力**:

```kdl
service "api" {
    image "myapp:1.0.0"
    version "1.0.0"
    ports { ... }
    environment { ... }
    volumes { ... }
    depends_on "db" "redis"
}
```

**出力**: 内部の`Service`構造体

**振る舞い**:

1. サービス名を識別子として使用
2. imageが未指定の場合、サービス名+versionから推測
3. デフォルト値を適用
4. 依存関係を解決

**制約**:

- サービス名は一意
- 依存関係に循環参照は不可

#### FS-002: Environment定義

**目的**: 環境（dev/staging/prod等）ごとの設定を管理

**入力**:

```kdl
environment "production" {
    services "api" "worker" "db"
    variables {
        DEBUG "false"
        LOG_LEVEL "info"
    }
}
```

**出力**: 内部の`Environment`構造体

**振る舞い**:

1. 環境名を識別子として使用
2. 使用するサービスのリストを管理
3. 環境変数を定義

#### FS-003: イメージ名の自動推測

**目的**: サービス名からDockerイメージ名を推測

**ロジック**:

```
サービス名 + ":" + (version OR "latest")
```

**例**:

- `service "postgres"` → `postgres:latest`
- `service "postgres" { version "16" }` → `postgres:16`
- `service "redis" { version "7-alpine" }` → `redis:7-alpine`

#### FS-004: Port定義

**目的**: ポートマッピングを定義

**形式**:

```kdl
port {host_port} {container_port} [protocol="tcp|udp"] [host_ip="IP"]
```

**デフォルト**:

- protocol: "tcp"
- host_ip: 0.0.0.0（全インターフェース）

#### FS-005: Volume定義

**目的**: ボリュームマウントを定義

**形式**:

```kdl
volume "{host_path}" "{container_path}" [read_only=true|false]
```

**デフォルト**:

- read_only: false

### インターフェース仕様

#### 最小構成

```kdl
service "postgres" {}
```

これだけで以下が自動設定:

- image: "postgres:latest"
- その他フィールド: デフォルト値

#### 標準構成

```kdl
service "api" {
    image "myapp:1.0.0"

    ports {
        port 8080 3000
    }

    environment {
        NODE_ENV "production"
    }

    depends_on "db"
}

service "db" {
    version "16"  // postgres:16
}

environment "production" {
    services "api" "db"
}
```

#### フル構成

```kdl
service "api" {
    image "myapp:1.0.0"
    version "1.0.0"

    ports {
        port 8080 3000 protocol="tcp"
        port 8443 3443 protocol="tcp" host_ip="127.0.0.1"
    }

    environment {
        NODE_ENV "production"
        DATABASE_URL "postgresql://db:5432/mydb"
    }

    volumes {
        volume "./data" "/app/data"
        volume "./config" "/app/config" read_only=true
    }

    depends_on "db" "redis"
}
```

### 非機能仕様

#### パフォーマンス

- 100サービス定義のパース: < 1秒
- メモリ使用量: O(n) (nはサービス数)

#### セキュリティ

- ファイルパスのサニタイズ
- 環境変数の機密情報は別管理を推奨

#### 互換性

- 後方互換性: メジャーバージョン内で保証
- 前方互換性: 未知のフィールドは警告して無視

## 哲学的考察

### なぜKDLか

**YAML**: 広く使われているが、インデントの曖昧さ、型の暗黙変換、アンカー/エイリアスの複雑さ

**TOML**: シンプルだが、ネストが深いと読みにくい

**KDL**:

- 明確な構造（ブレース）
- 人間に優しい構文
- 型が明確
- コメントが自然

```kdl
// KDL: 美しく、明確
service "api" {
    ports {
        port 8080 3000
    }
}
```

vs

```yaml
# YAML: インデントに依存
services:
  api:
    ports:
      - "8080:3000"
```

### ユーザー体験

#### 初めて使う開発者

```kdl
// これだけで動く
service "postgres" {}
service "redis" {}
```

「え、これだけ？簡単！」→ 成功体験 → 継続使用

#### 熟練した開発者

```kdl
// 詳細制御も可能
service "api" {
    ports {
        port 8080 3000 host_ip="127.0.0.1"
    }
}
```

「細かく制御できる。良い」→ 信頼 → 本番採用

### 進化の方向性

#### Phase 1: 基本機能（現在）

- Service/Environment定義
- イメージ名推測
- 基本的なPort/Volume

#### Phase 2: モジュール化

- include機能
- 変数定義と展開
- テンプレート

#### Phase 3: 高度な機能

- 条件分岐
- 環境間の継承
- プラグインシステム

#### Phase 4: エコシステム

- Kubernetes変換
- Terraform統合
- Web UI

## 参考資料

### 影響を受けた技術

- Docker Compose: シンプルさ
- Kubernetes: 宣言的な設計
- Nix: 再現性
- KDL: 美しい構文

### 設計哲学の参考

- The Zen of Python: "Simple is better than complex"
- Rails: Convention over Configuration
- Unix Philosophy: "Do one thing well"
