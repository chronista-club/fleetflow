# KDL構文リファレンス

最終更新日: 2025-11-23

## 概要

FleetFlowで使用するKDL（KDL Document Language）の構文リファレンスです。各ノードの書式、属性、使用例を説明します。

## 基本構文

KDLは階層的な構造を持つドキュメント言語です。FleetFlowでは以下のトップレベルノードをサポートしています。

## ノード一覧

### 1. `project` ノード

プロジェクト名を定義します。

**書式**:
```kdl
project "プロジェクト名"
```

**説明**:
- プロジェクト名は必須ではありませんが、OrbStack連携で使用されます
- 省略した場合、ディレクトリ名またはデフォルト名が使用されます

**例**:
```kdl
project "fleetflow"
```

**パーサー実装**: `parser.rs` - `"project"` ケース

---

### 2. `variables` ノード

テンプレート変数を定義します（Phase 2機能）。

**書式**:
```kdl
variables {
    変数名 "値"
    ...
}
```

**説明**:
- 変数は文字列、数値、真偽値をサポート
- Teraテンプレートエンジンで展開されます
- `{{ 変数名 }}` の形式でKDL内で参照可能

**例**:
```kdl
variables {
    registry "ghcr.io/myorg"
    version "1.0.0"
    port 8080
    debug #true
}

service "api" {
    image "{{ registry }}/api:{{ version }}"
}
```

**パーサー実装**: `template.rs` - `extract_variables()`

---

### 3. `service` ノード

コンテナサービスを定義します。

**書式**:
```kdl
service "サービス名" {
    // 基本設定
    image "イメージ名:タグ"
    version "バージョン"
    command "コマンド文字列"

    // ポート設定
    ports { ... }

    // 環境変数
    environment { ... }

    // ボリューム
    volumes { ... }

    // 依存関係
    depends_on "サービス1" "サービス2" ...
}
```

**属性**:
| 属性 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `image` | 文字列 | いいえ | Dockerイメージ名。省略時はサービス名から推測 |
| `version` | 文字列 | いいえ | バージョン。imageが省略時に使用 |
| `command` | 文字列 | いいえ | コンテナ起動コマンド |
| `ports` | ブロック | いいえ | ポートマッピング定義 |
| `environment` | ブロック | いいえ | 環境変数定義 |
| `volumes` | ブロック | いいえ | ボリュームマウント定義 |
| `depends_on` | 文字列リスト | いいえ | 依存サービスのリスト |

**例**:
```kdl
service "postgres" {
    version "16"
    ports {
        port 5432 5432
    }
    environment {
        POSTGRES_PASSWORD "postgres"
        POSTGRES_DB "myapp"
    }
    volumes {
        volume "./pgdata" "/var/lib/postgresql/data"
    }
}

service "api" {
    image "myapp:latest"
    command "npm start"
    depends_on "postgres" "redis"
}
```

**パーサー実装**: `parser.rs` - `parse_service()`

---

### 4. `ports` ブロック（serviceの子ノード）

ポートマッピングを定義します。

**書式**:
```kdl
ports {
    port ホストポート コンテナポート [protocol="tcp|udp"] [host_ip="IPアドレス"]
    ...
}
```

**属性**:
| 属性 | 型 | 必須 | デフォルト | 説明 |
|------|-----|------|-----------|------|
| 第1引数 | 数値 | はい | - | ホストポート番号 |
| 第2引数 | 数値 | はい | - | コンテナポート番号 |
| `protocol` | 文字列 | いいえ | `"tcp"` | プロトコル（tcp/udp） |
| `host_ip` | 文字列 | いいえ | `"0.0.0.0"` | バインドするIPアドレス |

**例**:
```kdl
ports {
    port 8080 3000
    port 8443 3443 protocol="tcp"
    port 5432 5432 host_ip="127.0.0.1"
}
```

**パーサー実装**: `parser.rs` - `parse_port()`

---

### 5. `environment` ブロック（serviceの子ノード）

環境変数を定義します。

**書式**:
```kdl
environment {
    変数名 "値"
    ...
}
```

**例**:
```kdl
environment {
    NODE_ENV "production"
    DATABASE_URL "postgresql://db:5432/mydb"
    API_KEY "secret-key-12345"
    DEBUG #false
}
```

**パーサー実装**: `parser.rs` - `parse_service()` 内の environment ブロック処理

---

### 6. `volumes` ブロック（serviceの子ノード）

ボリュームマウントを定義します。

**書式**:
```kdl
volumes {
    volume "ホストパス" "コンテナパス" [read_only=#true|#false]
    ...
}
```

**属性**:
| 属性 | 型 | 必須 | デフォルト | 説明 |
|------|-----|------|-----------|------|
| 第1引数 | 文字列 | はい | - | ホストパス（相対または絶対） |
| 第2引数 | 文字列 | はい | - | コンテナ内のマウント先パス |
| `read_only` | 真偽値 | いいえ | `#false` | 読み取り専用フラグ |

**例**:
```kdl
volumes {
    volume "./data" "/var/lib/postgresql/data"
    volume "./config" "/etc/config" read_only=#true
    volume "/tmp/logs" "/app/logs"
}
```

**注意**: KDL 2.0では真偽値は `#true`/`#false` の形式で記述します。

**パーサー実装**: `parser.rs` - `parse_volume()`

---

### 7. `stage` ノード

ステージ（環境）を定義します。

**書式**:
```kdl
stage "ステージ名" {
    service "サービス1" "サービス2" ...

    variables {
        変数名 "値"
        ...
    }
}
```

**属性**:
| 属性 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `service` | 文字列リスト | はい | このステージで起動するサービスのリスト |
| `variables` | ブロック | いいえ | ステージ固有の変数定義 |

**例**:
```kdl
stage "local" {
    service "postgres" "redis" "api"
    variables {
        DEBUG "#true"
        LOG_LEVEL "debug"
    }
}

stage "production" {
    service "postgres" "redis" "api" "worker"
    variables {
        DEBUG "#false"
        LOG_LEVEL "info"
    }
}
```

**パーサー実装**: `parser.rs` - `parse_stage()`

---

### 8. `include` ノード（Phase 2機能 - 未実装）

外部KDLファイルをインクルードします。

**書式**:
```kdl
include "ファイルパス"
include "グロブパターン"
```

**説明**:
- 相対パスで指定されたKDLファイルを読み込みます
- グロブパターン（`*.kdl`, `services/*.kdl` など）をサポート予定
- 循環参照チェックあり

**例**:
```kdl
include "common/database.kdl"
include "services/*.kdl"
```

**パーサー実装**: 未実装（Issue #7）

---

## データ型

KDL 2.0でサポートされるデータ型：

| 型 | 書式例 | 説明 |
|------|--------|------|
| 文字列 | `"hello"` | ダブルクォートで囲む |
| 数値 | `8080`, `3.14` | 整数または浮動小数点数 |
| 真偽値 | `#true`, `#false` | `#` プレフィックスが必要 |
| null | `#null` | null値 |

## テンプレート変数展開

Teraテンプレートエンジンを使用した変数展開をサポートしています（Phase 2機能）。

### 基本的な変数展開

```kdl
variables {
    registry "ghcr.io/myorg"
    version "1.0.0"
}

service "api" {
    image "{{ registry }}/api:{{ version }}"
}
```

### フィルター

```kdl
variables {
    name "HELLO"
}

service "{{ name | lower }}" {
    image "myapp:latest"
}
// → service "hello" が生成される
```

### 条件分岐（検討中）

```kdl
{% if is_prod %}
service "api" {
    replicas 3
}
{% else %}
service "api" {
    replicas 1
}
{% endif %}
```

### ループ（検討中）

```kdl
{% for service in services %}
service "{{ service }}" {
    image "myapp:latest"
}
{% endfor %}
```

## 完全な設定例

```kdl
// プロジェクト名
project "myapp"

// 変数定義
variables {
    registry "ghcr.io/myorg"
    db_version "16"
    redis_version "7-alpine"
}

// データベースサービス
service "postgres" {
    image "postgres:{{ db_version }}"
    ports {
        port 5432 5432 host_ip="127.0.0.1"
    }
    environment {
        POSTGRES_PASSWORD "postgres"
        POSTGRES_DB "myapp"
    }
    volumes {
        volume "./pgdata" "/var/lib/postgresql/data"
    }
}

// キャッシュサービス
service "redis" {
    image "redis:{{ redis_version }}"
    ports {
        port 6379 6379
    }
}

// APIサービス
service "api" {
    image "{{ registry }}/api:latest"
    command "npm start"
    ports {
        port 8080 3000
    }
    environment {
        NODE_ENV "development"
        DATABASE_URL "postgresql://postgres:postgres@localhost:5432/myapp"
        REDIS_URL "redis://localhost:6379"
    }
    depends_on "postgres" "redis"
}

// ローカル開発環境
stage "local" {
    service "postgres" "redis" "api"
    variables {
        DEBUG "#true"
    }
}

// 本番環境
stage "production" {
    service "postgres" "redis" "api"
    variables {
        DEBUG "#false"
    }
}
```

## エラーハンドリング

パーサーは以下のエラーを検出します：

| エラー | 説明 | 例 |
|--------|------|-----|
| `InvalidConfig` | KDL構文エラー | 不正なノード、閉じ括弧なし |
| `MissingServiceName` | サービス名が未指定 | `service { ... }` |
| `MissingStageName` | ステージ名が未指定 | `stage { ... }` |
| `TemplateRenderError` | テンプレート展開エラー | 未定義変数の参照 |
| `IoError` | ファイル読み込みエラー | ファイルが存在しない |

## パーサー実装との対応

| ノード | 実装ファイル | 関数 |
|--------|-------------|------|
| `project` | `parser.rs` | `parse_kdl_string()` - projectケース |
| `variables` | `template.rs` | `extract_variables()` |
| `service` | `parser.rs` | `parse_service()` |
| `stage` | `parser.rs` | `parse_stage()` |
| `ports` | `parser.rs` | `parse_port()` |
| `volumes` | `parser.rs` | `parse_volume()` |
| テンプレート展開 | `template.rs` | `TemplateProcessor` |
| include | 未実装 | - |

## 関連ドキュメント

- [設計書: KDL Parser](../design/01-kdl-parser.md) - アーキテクチャと実装詳細
- [仕様書: KDL Parser](./02-kdl-parser.md) - 機能要件
- [仕様書: Template Variables](./05-template-variables.md) - テンプレート変数の詳細
