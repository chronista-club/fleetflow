# KDL構造ビジュアルガイド

FleetFlowのKDL設定ファイル構造をMermaidグラフで視覚的に理解するためのガイドです。

## 全体構造

```mermaid
graph TB
    subgraph "fleet.kdl"
        PROJECT[project "name"]
        STAGE[stage "name"]
        SERVICE[service "name"]
        PROVIDERS[providers]
        SERVER[server "name"]
    end

    PROJECT --> STAGE
    STAGE --> SERVICE
    STAGE --> SERVER
    PROVIDERS --> SERVER
```

## ノード階層図

```mermaid
graph LR
    subgraph "ルートノード"
        A[project]
        B[stage]
        C[service]
        D[providers]
        E[server]
    end

    subgraph "serviceの子ノード"
        C --> C1[image]
        C --> C2[version]
        C --> C3[command]
        C --> C4[ports]
        C --> C5[env/environment]
        C --> C6[volumes]
        C --> C7[depends_on]
        C --> C8[build]
        C --> C9[healthcheck]
        C --> C10[restart]
        C --> C11[wait_for]
    end

    subgraph "portsの子ノード"
        C4 --> C4a[port]
    end

    subgraph "volumesの子ノード"
        C6 --> C6a[volume]
    end
```

## サービス定義の詳細構造

```mermaid
classDiagram
    class Service {
        +Option~String~ image
        +Option~String~ version
        +Option~String~ command
        +Vec~Port~ ports
        +HashMap~String,String~ environment
        +Vec~Volume~ volumes
        +Vec~String~ depends_on
        +Option~BuildConfig~ build
        +Option~HealthCheck~ healthcheck
        +Option~RestartPolicy~ restart
        +Option~WaitConfig~ wait_for
    }

    class Port {
        +u16 host
        +u16 container
        +Protocol protocol
        +Option~String~ host_ip
    }

    class Volume {
        +PathBuf host
        +PathBuf container
        +bool read_only
    }

    class BuildConfig {
        +Option~PathBuf~ dockerfile
        +Option~PathBuf~ context
        +HashMap~String,String~ args
        +Option~String~ target
        +bool no_cache
        +Option~String~ image_tag
    }

    class HealthCheck {
        +Vec~String~ test
        +u64 interval
        +u64 timeout
        +u64 retries
        +u64 start_period
    }

    class RestartPolicy {
        <<enumeration>>
        No
        Always
        OnFailure
        UnlessStopped
    }

    class WaitConfig {
        +u32 max_retries
        +u64 initial_delay_ms
        +u64 max_delay_ms
        +f64 multiplier
    }

    Service --> Port
    Service --> Volume
    Service --> BuildConfig
    Service --> HealthCheck
    Service --> RestartPolicy
    Service --> WaitConfig
```

## ステージとサービスの関係

```mermaid
flowchart TB
    subgraph "プロジェクト"
        P[project "myapp"]
    end

    subgraph "ステージ"
        L[stage "local"]
        D[stage "dev"]
        LIVE[stage "live"]
    end

    subgraph "サービス定義"
        DB[service "db"]
        API[service "api"]
        REDIS[service "redis"]
    end

    P --> L
    P --> D
    P --> LIVE

    L --> |"service"| DB
    L --> |"service"| API
    L --> |"service"| REDIS

    D --> |"service"| DB
    D --> |"service"| API

    LIVE --> |"service"| DB
    LIVE --> |"service"| API
    LIVE --> |"service"| REDIS
```

## 依存関係とExponential Backoff

```mermaid
sequenceDiagram
    participant CLI as FleetFlow CLI
    participant API as api コンテナ
    participant DB as db コンテナ
    participant REDIS as redis コンテナ

    Note over CLI: fleet up local

    CLI->>DB: 1. db を起動
    CLI->>REDIS: 2. redis を起動 (並列)

    loop Exponential Backoff (wait_for)
        CLI->>DB: ヘルスチェック確認
        alt healthy
            Note over CLI,DB: db 準備完了
        else not ready
            Note over CLI: 待機 (1s → 2s → 4s → ...)
        end
    end

    loop Exponential Backoff (wait_for)
        CLI->>REDIS: ヘルスチェック確認
        alt healthy
            Note over CLI,REDIS: redis 準備完了
        else not ready
            Note over CLI: 待機 (exponential)
        end
    end

    CLI->>API: 3. api を起動 (depends_on: db, redis)
```

## Exponential Backoff の待機時間

```mermaid
xychart-beta
    title "Exponential Backoff 待機時間 (multiplier=2.0)"
    x-axis "リトライ回数" [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
    y-axis "待機時間 (秒)" 0 --> 35
    bar [1, 2, 4, 8, 16, 30, 30, 30, 30, 30]
```

**デフォルト設定**:
- `initial_delay`: 1000ms (1秒)
- `max_delay`: 30000ms (30秒)
- `multiplier`: 2.0
- `max_retries`: 23回

## クラウドインフラ構造

```mermaid
graph TB
    subgraph "providers"
        SAKURA[sakura-cloud]
        CF[cloudflare]
    end

    subgraph "stage 'live'"
        SRV[server "app-server"]
        R2[r2-bucket "assets"]
        DNS[dns "example.com"]
    end

    SAKURA --> SRV
    CF --> R2
    CF --> DNS

    subgraph "server設定"
        SRV --> PLAN[plan core=4 memory=4]
        SRV --> DISK[disk size=100]
        SRV --> SSH[ssh-key]
        SRV --> ALIASES[dns_aliases]
    end
```

## ファイル読み込み順序とマージ

```mermaid
flowchart LR
    subgraph "設定ファイル検索順序"
        F1["fleet.kdl"]
        F2["flow.local.kdl"]
        F3["flow.{stage}.kdl"]
        F4[".fleetflow/*.kdl"]
    end

    F1 --> |"1. ベース設定"| MERGE
    F2 --> |"2. ローカル上書き"| MERGE
    F3 --> |"3. ステージ固有"| MERGE
    F4 --> |"4. 分割ファイル"| MERGE

    MERGE[マージ処理] --> FINAL[最終設定]
```

## マージルール

```mermaid
flowchart TB
    subgraph "Option<T> フィールド"
        O1["image, version, command, build, healthcheck, restart, wait_for"]
        O2["後の定義がSomeなら上書き"]
    end

    subgraph "Vec<T> フィールド"
        V1["ports, volumes, depends_on"]
        V2["後の定義が空でなければ全体上書き"]
    end

    subgraph "HashMap<K,V> フィールド"
        H1["environment"]
        H2["キーごとにマージ（後の定義が優先）"]
    end

    O1 --> O2
    V1 --> V2
    H1 --> H2
```

## コンテナ命名規則

```mermaid
flowchart LR
    PROJECT[project: myapp] --> NAME
    STAGE[stage: local] --> NAME
    SERVICE[service: db] --> NAME

    NAME["myapp-local-db"]

    NAME --> LABEL1["com.docker.compose.project = myapp-local"]
    NAME --> LABEL2["com.docker.compose.service = db"]
    NAME --> LABEL3["fleetflow.project = myapp"]
    NAME --> LABEL4["fleetflow.stage = local"]
    NAME --> LABEL5["fleetflow.service = db"]
```

## KDL構文の例

### 最小構成

```kdl
project "myapp"

stage "local" {
    service "db"
}

service "db" {
    image "postgres:16"
}
```

### フル設定

```kdl
project "myapp"

stage "local" {
    service "db"
    service "api"
}

service "db" {
    image "postgres:16"
    restart "unless-stopped"
    ports {
        port 5432 5432
    }
    env {
        POSTGRES_PASSWORD "secret"
    }
    volumes {
        volume "./data" "/var/lib/postgresql/data"
    }
    healthcheck {
        test "pg_isready -U postgres"
        interval 10
        timeout 5
        retries 3
    }
}

service "api" {
    image "myapp/api:latest"
    restart "unless-stopped"
    depends_on "db"
    wait_for {
        max_retries 10
        initial_delay 1000
        max_delay 30000
        multiplier 2.0
    }
    ports {
        port 3000 3000
    }
    env {
        DATABASE_URL "postgres://postgres:secret@db:5432/postgres"
    }
    build {
        dockerfile "Dockerfile"
        context "."
        target "production"
    }
}
```

## 関連ドキュメント

- [KDL構文リファレンス](../spec/02-kdl-parser.md)
- [OrbStack連携ガイド](01-orbstack-integration.md)
- [Dockerビルドガイド](02-docker-build.md)
- [CI/CDデプロイガイド](03-ci-deployment.md)
