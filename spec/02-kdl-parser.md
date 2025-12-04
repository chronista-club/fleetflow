# KDL Parser - 仕様書

## 概要

KDL（KDL Document Language）形式の設定ファイルをパースし、Fleetflowの内部データモデル（`FlowConfig`）に変換する機能を提供します。

## 目標

- [x] KDLファイルとKDL文字列をパース可能にする
- [x] `service` ノードをパースして `Service` 構造体に変換
- [x] `environment` ノードをパースして `Environment` 構造体に変換
- [x] `image` フィールドを必須化（明示的な指定が必要）
- [x] サービスマージロジック（複数ファイルからの定義をマージ）
- [ ] `include` ディレクティブのサポート
- [ ] 変数定義と展開のサポート
- [ ] 詳細なエラーメッセージの提供

## 要件

### 機能要件

#### FR-001: KDLファイルのパース

KDLファイルを読み込み、`FlowConfig` に変換できること。

```rust
let config = parse_kdl_file("fleetflow.kdl")?;
```

#### FR-002: KDL文字列のパース

KDL形式の文字列を直接パースできること。

```rust
let kdl = r#"
service "postgres" {
    image "postgres:16"
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

#### FR-005: イメージフィールドの必須化

`image` フィールドは必須。省略するとエラーになる：

```kdl
// ✅ 正しい定義
service "postgres" {
    image "postgres:16"
}

// ❌ エラー: imageが必須
service "postgres" {
    version "16"
}
// Error: サービス 'postgres' に image が指定されていません
```

**設計理由**:
- 明示的な指定により、使用するイメージを明確化
- 自動推測による予期しない動作を防止
- 設定の可読性と保守性を向上

#### FR-006: サービスマージロジック

複数のファイルで同じサービスを定義した場合、後のファイルの定義が前の定義とマージされる：

```kdl
// flow.kdl（ベース設定）
service "api" {
    image "myapp:latest"
    ports { port 8080 3000 }
    env { NODE_ENV "production" }
}

// flow.local.kdl（ローカルオーバーライド）
service "api" {
    env { DATABASE_URL "localhost:5432" }
}

// 結果:
// - image: "myapp:latest" (保持)
// - ports: [8080:3000] (保持)
// - env: { NODE_ENV: "production", DATABASE_URL: "localhost:5432" } (マージ)
```

**マージルール**:
- `Option<T>`: 後の定義が`Some`なら上書き、`None`なら元を保持
- `Vec<T>`: 後の定義が空でなければ上書き、空なら元を保持
- `HashMap<K, V>`: 両方をマージ（後の定義が優先）

#### FR-007: デフォルト値の適用

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
- 有効なfleetflow.kdlファイルが存在

**フロー**:
1. 開発者が `parse_kdl_file("fleetflow.kdl")` を呼び出す
2. パーサーがファイルを読み込み
3. KDL構文を解析
4. `FlowConfig` 構造体を生成
5. 結果を返す

**期待結果**:
- `FlowConfig` が正しく生成される
- すべてのサービス定義が含まれる

### UC-002: サービス定義のマージ

**アクター**: 開発者

**前提条件**:
- 複数のファイルで同じサービスが定義されている

**フロー**:
1. パーサーが各ファイルのservice定義を順番に処理
2. 同名のサービスが既に存在する場合、マージを実行
3. フィールドタイプに応じたマージルールを適用
4. 最終的なサービス定義を生成

**期待結果**:
- ベース設定の値は保持される
- オーバーライドで指定された値は上書きまたはマージされる
- 環境変数は両方がマージされる

### UC-003: エラー検出

**アクター**: 開発者

**前提条件**:
- 不正なKDLファイル、または必須フィールドが欠落

**フロー**:
1. パーサーが問題を検出（構文エラー、バリデーションエラー）
2. `FlowError` を生成
3. エラーメッセージに詳細を含める

**期待結果**:
- わかりやすいエラーメッセージ
- 問題箇所の特定が容易
- 例: `Error: サービス 'api' に image が指定されていません`

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
