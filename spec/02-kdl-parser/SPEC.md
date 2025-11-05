# KDL Parser - 仕様書

## 概要

KDL（KDL Document Language）形式の設定ファイルをパースし、Unison Flowの内部データモデル（`FlowConfig`）に変換する機能を提供します。

## 目標

- [x] KDLファイルとKDL文字列をパース可能にする
- [x] `service` ノードをパースして `Service` 構造体に変換
- [x] `environment` ノードをパースして `Environment` 構造体に変換
- [x] サービス名からイメージ名を自動推測
- [ ] `include` ディレクティブのサポート
- [ ] 変数定義と展開のサポート
- [ ] 詳細なエラーメッセージの提供

## 要件

### 機能要件

#### FR-001: KDLファイルのパース

KDLファイルを読み込み、`FlowConfig` に変換できること。

```rust
let config = parse_kdl_file("unison.kdl")?;
```

#### FR-002: KDL文字列のパース

KDL形式の文字列を直接パースできること。

```rust
let kdl = r#"
service "postgres" {
    version "16"
}
"#;
let config = parse_kdl_string(kdl)?;
```

#### FR-003: Service定義のパース

以下の要素を含むservice定義をパース可能：

```kdl
service "api" {
    image "myapp:1.0.0"
    version "1.0.0"

    ports {
        port 8080 3000
        port 8443 3443 protocol="tcp" host_ip="127.0.0.1"
    }

    environment {
        DATABASE_URL "postgresql://db:5432/mydb"
        NODE_ENV "production"
    }

    volumes {
        volume "./data" "/app/data"
        volume "./config" "/app/config" read_only=true
    }

    depends_on "db" "redis"
}
```

#### FR-004: Environment定義のパース

以下の要素を含むenvironment定義をパース可能：

```kdl
environment "production" {
    services "api" "worker" "db" "redis"

    variables {
        DEBUG "false"
        LOG_LEVEL "info"
    }
}
```

#### FR-005: イメージ名の自動推測

`image` が省略された場合、サービス名とバージョンからイメージ名を推測：

```kdl
service "postgres" {
    // image "postgres:latest" が自動的に設定される
}

service "postgres" {
    version "16"
    // image "postgres:16" が設定される
}
```

#### FR-006: デフォルト値の適用

省略可能なフィールドにはデフォルト値を設定：

- `protocol`: `tcp`
- `host_ip`: `None` (0.0.0.0を意味)
- `read_only`: `false`
- `ports`: `[]`
- `volumes`: `[]`
- `depends_on`: `[]`
- `environment`: `{}`

### 非機能要件

#### NFR-001: パフォーマンス

- 100サービス定義を含むファイルを1秒以内にパース

#### NFR-002: エラーハンドリング

- 不正なKDL構文の場合、行番号を含むエラーメッセージ
- 必須フィールドが欠落している場合、明確なエラー
- 未知のノードは警告を出してスキップ

#### NFR-003: 拡張性

- 新しいノードタイプを容易に追加可能な設計

## ユースケース

### UC-001: 基本的なサービス定義のパース

**アクター**: 開発者

**前提条件**:
- 有効なunison.kdlファイルが存在

**フロー**:
1. 開発者が `parse_kdl_file("unison.kdl")` を呼び出す
2. パーサーがファイルを読み込み
3. KDL構文を解析
4. `FlowConfig` 構造体を生成
5. 結果を返す

**期待結果**:
- `FlowConfig` が正しく生成される
- すべてのサービス定義が含まれる

### UC-002: イメージ名の自動推測

**アクター**: 開発者

**前提条件**:
- `image` フィールドが省略されたサービス定義

**フロー**:
1. パーサーがservice定義を処理
2. `image` フィールドが `None` であることを検出
3. サービス名とバージョンから推測
4. `service.image` に設定

**期待結果**:
- `service "postgres"` → `image: Some("postgres:latest")`
- `service "postgres" { version "16" }` → `image: Some("postgres:16")`

### UC-003: エラー検出

**アクター**: 開発者

**前提条件**:
- 不正なKDLファイル

**フロー**:
1. パーサーが不正な構文を検出
2. `FlowError` を生成
3. エラーメッセージに行番号と詳細を含める

**期待結果**:
- わかりやすいエラーメッセージ
- 問題箇所の特定が容易

## データモデル

### 入力: KDL形式

```kdl
service "api" {
    image "myapp:1.0.0"
    ports {
        port 8080 3000
    }
}

environment "production" {
    services "api" "db"
}
```

### 出力: FlowConfig

```rust
FlowConfig {
    services: HashMap::from([
        ("api", Service {
            image: Some("myapp:1.0.0"),
            ports: vec![Port { host: 8080, container: 3000, ... }],
            ...
        })
    ]),
    environments: HashMap::from([
        ("production", Environment {
            services: vec!["api", "db"],
            ...
        })
    ])
}
```

## 制約事項

- KDL v4.7.1 の仕様に準拠
- UTF-8エンコーディングのファイルのみサポート
- ファイルサイズの上限: 10MB

## 依存関係

### 外部ライブラリ
- `kdl` crate (v4.7.1)
- `std::fs` (ファイル読み込み)
- `std::collections::HashMap`

### 内部モジュール
- `flow-atom::model` (データモデル)
- `flow-atom::error` (エラー型)

## パース対象ノード

| ノード名 | 状態 | 説明 |
|---------|------|------|
| `service` | ✅ 実装済 | サービス定義 |
| `environment` | ✅ 実装済 | 環境定義 |
| `include` | 🚧 TODO | 外部ファイルのインクルード |
| `variables` | 🚧 TODO | グローバル変数定義 |

## 参考資料

- [KDL仕様](https://kdl.dev/)
- [kdl-rs ドキュメント](https://docs.rs/kdl/)
- [Core Concept仕様](../01-core-concept/spec.md)
