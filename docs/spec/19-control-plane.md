# Control Plane - 仕様書

## ステータス: Draft

## 概要

FleetFlow Control Plane は、複数プロジェクト・複数サーバーを横断管理する常駐プロセスである。CLI / MCP Server / WebUI の3つのインターフェースから統一的にアクセスでき、テナント単位でサービス群の状態を常時把握・制御する。

本仕様書は [spec/18-platform-vision.md](18-platform-vision.md) の Phase 1 を詳細化したものであり、以下の3ステップで段階的に実装する:

1. **データモデル** — SurrealDB 上のエンティティ定義とリレーション
2. **Core API** — Unison Protocol によるチャネル・メソッド定義
3. **常駐デーモン** — `fleetflowd` の起動・停止・ヘルスチェック

**ビジョン**: 「伝えれば、動く」を、1サービスから事業全体へ。

## データモデル

### 概念階層

```
Tenant (ANYCREATIVE Inc)
 └─ Project (creo-memories, vantage-point, ...)
     └─ Stage (各プロジェクトが自由定義: local/dev/prod/staging 等)
         └─ Service
             └─ Container
```

### CP-001: Tenant

**目的**: サービス群を所有する組織単位。マルチテナント対応の基盤。

**フィールド**:

| フィールド | 型 | 必須 | 説明 |
|-----------|-----|------|------|
| `id` | `record<tenant>` | 自動 | SurrealDB レコード ID |
| `slug` | `string` | ◯ | URL/CLI で使う短縮識別子（例: `anycreative`） |
| `name` | `string` | ◯ | 表示名（例: `ANYCREATIVE Inc`） |
| `auth0_org_id` | `option<string>` | - | Auth0 Organization ID（将来のマルチテナント用） |
| `plan` | `string` | ◯ | 契約プラン（初期は `"self-hosted"` のみ） |
| `created_at` | `datetime` | 自動 | 作成日時 |
| `updated_at` | `datetime` | 自動 | 更新日時 |

**制約**:
- `slug` はグローバルユニーク
- 初期テナント `anycreative` は DB マイグレーションで作成

### CP-002: Project

**目的**: 1つのサービス/プロダクトに対応するエンティティ。既存の FleetFlow プロジェクト（`fleet.kdl`）と1:1で対応する。

**フィールド**:

| フィールド | 型 | 必須 | 説明 |
|-----------|-----|------|------|
| `id` | `record<project>` | 自動 | SurrealDB レコード ID |
| `tenant` | `record<tenant>` | ◯ | 所属テナント |
| `slug` | `string` | ◯ | プロジェクト識別子（例: `creo-memories`） |
| `name` | `string` | ◯ | 表示名 |
| `description` | `option<string>` | - | プロジェクト説明 |
| `repository_url` | `option<string>` | - | Git リポジトリ URL |
| `created_at` | `datetime` | 自動 | 作成日時 |
| `updated_at` | `datetime` | 自動 | 更新日時 |

**制約**:
- `slug` はテナント内でユニーク
- `tenant` + `slug` の複合ユニーク制約

### CP-003: Stage

**目的**: プロジェクトのデプロイ環境。プロジェクトごとに自由に定義可能。

**フィールド**:

| フィールド | 型 | 必須 | 説明 |
|-----------|-----|------|------|
| `id` | `record<stage>` | 自動 | SurrealDB レコード ID |
| `project` | `record<project>` | ◯ | 所属プロジェクト |
| `slug` | `string` | ◯ | ステージ識別子（例: `local`, `dev`, `prod`） |
| `description` | `option<string>` | - | ステージ説明 |
| `server` | `option<record<server>>` | - | デプロイ先サーバー |
| `created_at` | `datetime` | 自動 | 作成日時 |
| `updated_at` | `datetime` | 自動 | 更新日時 |

**制約**:
- `slug` はプロジェクト内でユニーク
- Stage 名は全プロジェクトで統一する必要はない

### CP-004: Service

**目的**: ステージ内で稼働する個別のサービス定義。既存の `fleet.kdl` の `service` ノードに対応。

**フィールド**:

| フィールド | 型 | 必須 | 説明 |
|-----------|-----|------|------|
| `id` | `record<service>` | 自動 | SurrealDB レコード ID |
| `stage` | `record<stage>` | ◯ | 所属ステージ |
| `slug` | `string` | ◯ | サービス識別子（例: `web`, `db`） |
| `image` | `string` | ◯ | コンテナイメージ |
| `config` | `object` | - | ポート・環境変数・ボリューム等の設定（KDL から変換） |
| `desired_status` | `string` | ◯ | 期待状態: `running` / `stopped` |
| `created_at` | `datetime` | 自動 | 作成日時 |
| `updated_at` | `datetime` | 自動 | 更新日時 |

**制約**:
- `slug` はステージ内でユニーク

### CP-005: Container

**目的**: 実際に稼働するコンテナインスタンスの状態追跡。

**フィールド**:

| フィールド | 型 | 必須 | 説明 |
|-----------|-----|------|------|
| `id` | `record<container>` | 自動 | SurrealDB レコード ID |
| `service` | `record<service>` | ◯ | 所属サービス |
| `container_id` | `string` | ◯ | Docker コンテナ ID |
| `container_name` | `string` | ◯ | コンテナ名（`{project}-{stage}-{service}`） |
| `status` | `string` | ◯ | 実際の状態: `created` / `running` / `stopped` / `exited` / `unknown` |
| `health` | `option<string>` | - | ヘルスチェック結果: `healthy` / `unhealthy` / `starting` |
| `server` | `option<record<server>>` | - | 稼働サーバー |
| `started_at` | `option<datetime>` | - | 起動日時 |
| `last_seen_at` | `datetime` | 自動 | 最終確認日時 |
| `created_at` | `datetime` | 自動 | 作成日時 |

### CP-006: Server

**目的**: コンテナが稼働する計算資源。既存の Fleet Registry の `server` 定義を継承。

**フィールド**:

| フィールド | 型 | 必須 | 説明 |
|-----------|-----|------|------|
| `id` | `record<server>` | 自動 | SurrealDB レコード ID |
| `tenant` | `record<tenant>` | ◯ | 所属テナント |
| `slug` | `string` | ◯ | サーバー識別子（例: `vps-01`） |
| `provider` | `string` | ◯ | プロバイダ（例: `sakura-cloud`） |
| `plan` | `option<string>` | - | プラン情報 |
| `ssh_host` | `string` | ◯ | SSH 接続先ホスト |
| `ssh_user` | `string` | ◯ | SSH ユーザー（デフォルト: `root`） |
| `deploy_path` | `string` | ◯ | デプロイ先パス |
| `status` | `string` | ◯ | `online` / `offline` / `maintenance` |
| `last_heartbeat_at` | `option<datetime>` | - | 最終ハートビート受信日時 |
| `created_at` | `datetime` | 自動 | 作成日時 |
| `updated_at` | `datetime` | 自動 | 更新日時 |

**制約**:
- `slug` はテナント内でユニーク

### 横断クエリ

Control Plane は以下の横断クエリをサポートする:

```bash
# 全プロジェクトの prod ステージを横断表示
fleet ps --stage prod

# 特定プロジェクトの全ステージを縦断表示
fleet ps --project creo-memories

# 全テナントの全サービス
fleet ps --all
```

**SurrealDB クエリ例**:

```sql
-- --stage prod: 全プロジェクト横断
SELECT * FROM stage WHERE slug = 'prod'
  FETCH project, project.tenant;

-- --project creo: 縦断
SELECT * FROM stage WHERE project.slug = 'creo-memories'
  FETCH project;
```

## 認証・認可

### CP-100: Auth0 統合

**目的**: Core API への全アクセスを Auth0 で保護する。CLI / MCP Server / WebUI すべてのクライアントが対象。

### Auth0 テナント構成

```
Auth0 テナント: ANYCREATIVE
 ├─ Application: FleetFlow Control Plane (M2M + SPA)
 ├─ Application: Creo Memories
 ├─ Application: Vantage Point
 └─ Application: CPLP Sound System

将来:
 ├─ Organization: ANYCREATIVE（自社）
 └─ Organization: 外部顧客（Auth0 Organizations で分離）
```

**設計判断**:
- Auth0 テナントは creo-memories と共有する（新規テナント作成しない）
- FleetFlow 用に専用 Application を作成
- 将来のマルチテナント対応は Auth0 Organizations で実現
- テナントの分離は Organizations 単位で行い、Tenant テーブルの `auth0_org_id` で紐づけ

### 認証フロー

#### CLI (`fleet`)

1. `fleet login` で Auth0 Device Authorization Flow を開始
2. ブラウザでログイン・認可
3. アクセストークンをローカルに保存（`~/.config/fleetflow/credentials.json`）
4. 以降のリクエストにトークンを付与
5. トークン期限切れ時はリフレッシュトークンで自動更新

#### MCP Server

1. MCP Server 起動時に Service Account（M2M）トークンを取得
2. Core API へのリクエストに M2M トークンを付与

#### WebUI（Phase 3）

1. Auth0 SPA SDK による Authorization Code Flow + PKCE

### 認可モデル

Phase 1 ではシンプルなロールベース:

| ロール | 権限 |
|--------|------|
| `owner` | テナント管理を含む全操作 |
| `admin` | プロジェクト・ステージ・サービスの全操作 |
| `operator` | デプロイ・再起動・ログ閲覧 |
| `viewer` | 読み取りのみ |

**制約**:
- ロールはテナント単位で割り当て
- Phase 1 では ANYCREATIVE テナントの `owner` ロールのみ使用

## 通信プロトコル

### CP-200: Unison Protocol

**目的**: CLI / MCP Server / WebUI と Core API 間の通信に、ANYCREATIVE の次世代通信基盤である Unison Protocol を使用する。

**Unison の特徴**:
- QUIC + TLS 1.3 ベース
- チャネルベースの通信モデル（Request/Response + Event push + Raw bytes）
- KDL スキーマでプロトコル定義
- Rust ネイティブ実装
- リポジトリ: `~/repos/unison/`

### チャネル定義

Unison Protocol のチャネルを以下のように定義する:

```kdl
// fleetflow-unison-schema.kdl

protocol "fleetflow" version="1.0" {

    // --- テナント管理 ---
    channel "tenant" {
        method "get" {
            request { slug "string" }
            response { tenant "Tenant" }
        }
        method "list" {
            response { tenants "Vec<Tenant>" }
        }
        method "update" {
            request { slug "string"; patch "TenantPatch" }
            response { tenant "Tenant" }
        }
    }

    // --- プロジェクト管理 ---
    channel "project" {
        method "create" {
            request { tenant_slug "string"; slug "string"; name "string"; description "string?" }
            response { project "Project" }
        }
        method "get" {
            request { tenant_slug "string"; slug "string" }
            response { project "Project" }
        }
        method "list" {
            request { tenant_slug "string" }
            response { projects "Vec<Project>" }
        }
        method "update" {
            request { tenant_slug "string"; slug "string"; patch "ProjectPatch" }
            response { project "Project" }
        }
        method "delete" {
            request { tenant_slug "string"; slug "string" }
            response { deleted "bool" }
        }
    }

    // --- ステージ管理 ---
    channel "stage" {
        method "create" {
            request { project_id "string"; slug "string" }
            response { stage "Stage" }
        }
        method "list" {
            request { project_id "string" }
            response { stages "Vec<Stage>" }
        }
        method "list_across_projects" {
            request { tenant_slug "string"; stage_slug "string" }
            response { stages "Vec<StageWithProject>" }
        }
        method "delete" {
            request { project_id "string"; slug "string" }
            response { deleted "bool" }
        }
    }

    // --- サービス管理 ---
    channel "service" {
        method "list" {
            request { stage_id "string" }
            response { services "Vec<Service>" }
        }
        method "get" {
            request { stage_id "string"; slug "string" }
            response { service "ServiceWithContainers" }
        }
        method "sync" {
            request { stage_id "string"; kdl_config "string" }
            response { services "Vec<Service>" }
        }
    }

    // --- コンテナ操作 ---
    channel "container" {
        method "start" {
            request { service_id "string" }
            response { container "Container" }
        }
        method "stop" {
            request { service_id "string" }
            response { container "Container" }
        }
        method "restart" {
            request { service_id "string" }
            response { container "Container" }
        }
        method "logs" {
            request { service_id "string"; tail "u32?"; follow "bool?" }
            // Event push: ログ行をストリーミング
            event { line "string"; timestamp "datetime" }
        }
        method "exec" {
            request { service_id "string"; command "Vec<string>" }
            // Raw bytes: stdin/stdout/stderr
        }
    }

    // --- サーバー管理 ---
    channel "server" {
        method "list" {
            request { tenant_slug "string" }
            response { servers "Vec<Server>" }
        }
        method "register" {
            request { tenant_slug "string"; slug "string"; provider "string"; ssh_host "string"; ssh_user "string?"; deploy_path "string" }
            response { server "Server" }
        }
        method "heartbeat" {
            request { server_slug "string" }
            response { ack "bool" }
        }
        method "status" {
            request { server_slug "string" }
            response { server "ServerStatus" }
        }
    }

    // --- デプロイ ---
    channel "deploy" {
        method "execute" {
            request { project_slug "string"; stage_slug "string" }
            // Event push: デプロイ進捗をストリーミング
            event { step "string"; status "string"; message "string" }
        }
        method "status" {
            request { deploy_id "string" }
            response { deploy "DeployStatus" }
        }
        method "history" {
            request { project_slug "string"; stage_slug "string?"; limit "u32?" }
            response { deploys "Vec<DeployRecord>" }
        }
    }

    // --- ヘルスモニタリング ---
    channel "health" {
        method "overview" {
            request { tenant_slug "string" }
            response { health "TenantHealth" }
        }
        // Event push: 状態変化をリアルタイム通知
        event "status_change" {
            entity_type "string"  // "service" / "server"
            entity_id "string"
            old_status "string"
            new_status "string"
            timestamp "datetime"
        }
    }
}
```

### 通信パターン

| パターン | Unison チャネル機能 | ユースケース |
|---------|-------------------|------------|
| Request/Response | `method` | CRUD 操作、ステータス取得 |
| Event Push | `event` | ログストリーミング、デプロイ進捗、ヘルス変化通知 |
| Raw Bytes | Raw channel | `exec` コマンドの stdin/stdout |

## Core API

### CP-300: API サーバー

**目的**: Unison Protocol サーバーとして稼働し、全チャネルのリクエストを処理する。

**コンポーネント構成**:

```
fleetflow-api (新規 crate)
 ├─ src/
 │   ├─ main.rs              # エントリーポイント
 │   ├─ server.rs             # Unison サーバー初期化
 │   ├─ auth.rs               # Auth0 トークン検証
 │   ├─ db.rs                 # SurrealDB 接続管理
 │   ├─ channels/
 │   │   ├─ tenant.rs
 │   │   ├─ project.rs
 │   │   ├─ stage.rs
 │   │   ├─ service.rs
 │   │   ├─ container.rs
 │   │   ├─ server.rs
 │   │   ├─ deploy.rs
 │   │   └─ health.rs
 │   └─ models/               # API 固有の型定義
 └─ Cargo.toml
```

### チャネル・メソッド一覧

| チャネル | メソッド | 説明 | 認可 |
|---------|---------|------|------|
| `tenant` | `get` | テナント情報取得 | viewer+ |
| `tenant` | `list` | テナント一覧 | viewer+ |
| `tenant` | `update` | テナント情報更新 | owner |
| `project` | `create` | プロジェクト作成 | admin+ |
| `project` | `get` | プロジェクト情報取得 | viewer+ |
| `project` | `list` | プロジェクト一覧 | viewer+ |
| `project` | `update` | プロジェクト情報更新 | admin+ |
| `project` | `delete` | プロジェクト削除 | admin+ |
| `stage` | `create` | ステージ作成 | admin+ |
| `stage` | `list` | ステージ一覧（プロジェクト内） | viewer+ |
| `stage` | `list_across_projects` | 横断クエリ（全プロジェクトの同名ステージ） | viewer+ |
| `stage` | `delete` | ステージ削除 | admin+ |
| `service` | `list` | サービス一覧 | viewer+ |
| `service` | `get` | サービス詳細（コンテナ情報含む） | viewer+ |
| `service` | `sync` | KDL からサービス定義を同期 | operator+ |
| `container` | `start` | コンテナ起動 | operator+ |
| `container` | `stop` | コンテナ停止 | operator+ |
| `container` | `restart` | コンテナ再起動 | operator+ |
| `container` | `logs` | ログ取得（ストリーミング対応） | operator+ |
| `container` | `exec` | コンテナ内コマンド実行 | admin+ |
| `server` | `list` | サーバー一覧 | viewer+ |
| `server` | `register` | サーバー登録 | admin+ |
| `server` | `heartbeat` | ハートビート受信 | system |
| `server` | `status` | サーバー状態取得 | viewer+ |
| `deploy` | `execute` | デプロイ実行（ストリーミング進捗） | operator+ |
| `deploy` | `status` | デプロイ状態取得 | viewer+ |
| `deploy` | `history` | デプロイ履歴 | viewer+ |
| `health` | `overview` | テナント全体のヘルス概要 | viewer+ |
| `health` | `status_change` | ヘルス変化イベント（push） | viewer+ |

### SurrealDB 接続

| 環境 | 接続先 | 用途 |
|------|--------|------|
| local | `ws://127.0.0.1:12000` | 開発 |
| dev | SSH tunnel → `creo-dev:12000` | ステージング |
| prod | SSH tunnel → `creo-prod:12000` | 本番 |

**データベース設計**:
- Namespace: `fleetflow`
- Database: `control_plane`
- creo-memories とは Namespace レベルで分離

## 常駐デーモン

### CP-400: `fleetflowd`

**目的**: Control Plane の常駐デーモンプロセス。Core API サーバーの起動・停止・ヘルスモニタリングを管理する。

### 起動

```bash
# フォアグラウンド起動（開発用）
fleetflowd

# デーモン起動
fleetflowd --daemon

# 設定ファイル指定
fleetflowd --config /etc/fleetflow/fleetflowd.kdl
```

**設定ファイル**:

```kdl
// fleetflowd.kdl

daemon {
    pid-file "/var/run/fleetflowd.pid"
    log-file "/var/log/fleetflow/fleetflowd.log"
    log-level "info"
}

api {
    // Unison Protocol リスナー
    listen "0.0.0.0:4510"
    tls {
        cert "/etc/fleetflow/tls/cert.pem"
        key "/etc/fleetflow/tls/key.pem"
    }
}

database {
    endpoint "ws://127.0.0.1:12000"
    namespace "fleetflow"
    database "control_plane"
    username "fleetflow-api"
    password "${FLEETFLOW_DB_PASSWORD}"
}

auth {
    provider "auth0"
    domain "anycreative.auth0.com"
    audience "https://api.fleetflow.dev"
    // M2M 用クライアント
    client-id "${FLEETFLOW_AUTH0_CLIENT_ID}"
    client-secret "${FLEETFLOW_AUTH0_CLIENT_SECRET}"
}

health {
    // サーバーヘルスチェック間隔
    check-interval "30s"
    // ハートビートタイムアウト（これを超えたら offline 判定）
    heartbeat-timeout "90s"
}
```

### 停止

```bash
# 正常停止
fleetflowd --stop

# PID ファイルを使った停止
kill $(cat /var/run/fleetflowd.pid)
```

**シャットダウンシーケンス**:
1. 新規接続の受付停止
2. 既存接続のドレイン（30秒タイムアウト）
3. SurrealDB コネクション切断
4. PID ファイル削除

### ヘルスチェック

#### 自己ヘルスチェック

`fleetflowd` 自体のヘルス確認:

```bash
fleet daemon status
```

出力例:

```
FleetFlow Daemon: running (PID 12345)
  Uptime:     3d 14h 22m
  API:        listening on 0.0.0.0:4510 (TLS)
  Database:   connected (ws://127.0.0.1:12000)
  Auth:       Auth0 (anycreative.auth0.com)
  Clients:    3 active connections
```

#### サーバーヘルスモニタリング

登録されたサーバーの死活監視:

1. `health.check-interval`（デフォルト30秒）ごとにハートビート確認
2. `health.heartbeat-timeout`（デフォルト90秒）を超えてハートビートがなければ `offline` に変更
3. 状態変化時は `health.status_change` イベントを接続中の全クライアントに push

### コンポーネント構成

```
crates/fleetflowd/ (新規 crate)
 ├─ src/
 │   ├─ main.rs        # CLI パース、デーモン化
 │   ├─ daemon.rs       # PID 管理、シグナルハンドリング
 │   ├─ config.rs       # fleetflowd.kdl パーサー
 │   └─ monitor.rs      # ヘルスモニタリングループ
 └─ Cargo.toml
```

**依存関係**:
- `fleetflow-api` — Core API サーバー本体
- `fleetflow-core` — KDL パーサー（設定ファイル用）
- `tokio` — 非同期ランタイム
- `daemonize` — デーモン化

## CLI 拡張

### CP-500: 認証コマンド

```bash
# ログイン（Auth0 Device Authorization Flow）
fleet login

# ログアウト（トークン破棄）
fleet logout

# 認証状態確認
fleet auth status
```

**出力例** (`fleet auth status`):

```
Authenticated as: makoto@anycreative.co.jp
  Tenant:  ANYCREATIVE Inc (anycreative)
  Role:    owner
  Token:   valid (expires in 23h 45m)
  API:     https://api.fleetflow.dev:4510
```

**トークン保存先**: `~/.config/fleetflow/credentials.json`

```json
{
  "access_token": "...",
  "refresh_token": "...",
  "expires_at": "2026-03-10T12:00:00Z",
  "api_endpoint": "https://api.fleetflow.dev:4510"
}
```

### CP-501: デーモン管理コマンド

```bash
# デーモン状態確認
fleet daemon status

# デーモン起動（ローカル開発用）
fleet daemon start

# デーモン停止
fleet daemon stop
```

### CP-502: テナント・プロジェクト管理コマンド

```bash
# テナント状態
fleet tenant status

# プロジェクト一覧
fleet project list

# プロジェクト作成
fleet project create --slug creo-memories --name "Creo Memories"

# プロジェクト詳細
fleet project show creo-memories
```

### CP-503: 横断クエリコマンド

既存の `fleet ps` を拡張し、Control Plane 経由で横断クエリを実行:

```bash
# 全プロジェクトの prod を一覧
fleet ps --stage prod

# 特定プロジェクトの全 stage を一覧
fleet ps --project creo-memories

# 全部
fleet ps --all
```

**出力例** (`fleet ps --stage prod`):

```
Project           Stage  Service       Status    Health
creo-memories     prod   web           running   healthy
creo-memories     prod   surrealdb     running   healthy
creo-memories     prod   caddy         running   healthy
vantage-point     prod   app           running   healthy
vantage-point     prod   redis         running   healthy
```

**振る舞い**:
- `--stage` / `--project` / `--all` が指定された場合、Control Plane API に接続
- いずれも指定されない場合、従来通りローカルの `fleet.kdl` を使用（後方互換）
- Control Plane への認証が必要（未ログインの場合はエラーメッセージで `fleet login` を案内）

### CP-504: サーバー管理コマンド

```bash
# サーバー一覧
fleet server list

# サーバー登録
fleet server register --slug vps-01 --provider sakura-cloud \
    --ssh-host 153.xxx.xxx.xxx --deploy-path /opt/apps

# サーバー状態
fleet server status vps-01
```

### CLI コマンド一覧（Phase 1 追加分）

| コマンド | 説明 |
|---------|------|
| `fleet login` | Auth0 ログイン |
| `fleet logout` | ログアウト |
| `fleet auth status` | 認証状態確認 |
| `fleet daemon start` | デーモン起動 |
| `fleet daemon stop` | デーモン停止 |
| `fleet daemon status` | デーモン状態確認 |
| `fleet tenant status` | テナント状態表示 |
| `fleet project list` | プロジェクト一覧 |
| `fleet project create` | プロジェクト作成 |
| `fleet project show <slug>` | プロジェクト詳細 |
| `fleet project delete <slug>` | プロジェクト削除 |
| `fleet server list` | サーバー一覧 |
| `fleet server register` | サーバー登録 |
| `fleet server status <slug>` | サーバー状態 |
| `fleet ps --stage <name>` | ステージ横断表示 |
| `fleet ps --project <name>` | プロジェクト縦断表示 |
| `fleet ps --all` | 全表示 |

## 未決事項

1. **Unison Protocol の成熟度** — Unison 自体がまだ開発中。Control Plane の実装と Unison の開発を並行する必要がある。Unison が遅延した場合のフォールバック戦略は未定
2. **SurrealDB のマイグレーション管理** — スキーマ変更時のマイグレーション手法。SurrealDB 自体にマイグレーションツールが組み込まれていないため、自前で管理する方法を決める必要がある
3. **Control Plane の配置** — 専用サーバー vs 既存 VPS 同居。規模に応じて後決め
4. **WebUI のフレームワーク選定** — Phase 3 で決定
5. **コスト管理の詳細設計** — さくらクラウド / Cloudflare / Auth0 の API 統合は Phase 4 で詳細化
6. **Fleet Registry との統合** — 既存の `fleet-registry.kdl`（ファイルベース）から Control Plane（DB ベース）への移行パス
7. **ローカル開発体験** — `fleetflowd` をローカルで起動する場合の SurrealDB / Auth0 のセットアップ簡素化
8. **サーバーエージェント** — リモートサーバー上でハートビートを送信するエージェントプロセスの仕様は別途策定

## 変更履歴

### 2026-03-09: 初版作成

- **理由**: Platform Vision (spec/18) の Phase 1 を詳細仕様として具体化
- **影響**: 新規 crate `fleetflow-api`, `fleetflowd` の追加。既存 CLI (`fleetflow`) への認証・横断クエリコマンド追加
