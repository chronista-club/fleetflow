# KDL Parser - 設計書

## アーキテクチャ概要

KDL Parserは、`kdl` crateを使用してKDL形式のテキストをパースし、FleetFlow内部のデータモデルに変換する役割を担います。

```
Input (KDL File/String)
    ↓
kdl crate (KdlDocument)
    ↓
parse_kdl_string()
    ↓
parse_environment() / parse_service()
    ↓
FlowConfig (Internal Model)
```

## コンポーネント設計

### Parser Module (`flow-atom/src/parser.rs`)

#### 公開API

```rust
/// KDLファイルをパース
pub fn parse_kdl_file<P: AsRef<Path>>(path: P) -> Result<FlowConfig>

/// KDL文字列をパース
pub fn parse_kdl_string(content: &str) -> Result<FlowConfig>
```

#### 内部関数

```rust
/// environmentノードをパース
fn parse_environment(node: &KdlNode) -> Result<(String, Environment)>

/// serviceノードをパース
fn parse_service(node: &KdlNode) -> Result<(String, Service)>

/// portノードをパース
fn parse_port(node: &KdlNode) -> Option<Port>

/// volumeノードをパース
fn parse_volume(node: &KdlNode) -> Option<Volume>

/// サービス名からイメージ名を推測
fn infer_image_name(service_name: &str, version: Option<&str>) -> String
```

## データフロー

### 1. ファイル読み込み

```rust
parse_kdl_file("fleetflow.kdl")
    ↓
fs::read_to_string()  // ファイル → String
    ↓
parse_kdl_string()
```

### 2. KDL → KdlDocument

```rust
let doc: KdlDocument = content.parse()?;
```

`kdl` crateが以下を処理：
- 字句解析
- 構文解析
- エラー検出

### 3. KdlDocument → FlowConfig

```rust
for node in doc.nodes() {
    match node.name().value() {
        "service" => parse_service(node),
        "environment" => parse_environment(node),
        _ => warning
    }
}
```

### 4. ノード個別パース

#### Service ノード

```kdl
service "api" {          // ← entries[0] = name
    image "..."          // ← child node
    ports { ... }        // ← child node with children
}
```

```rust
fn parse_service(node: &KdlNode) -> Result<(String, Service)> {
    // 1. サービス名を取得
    let name = node.entries().first()...;

    // 2. 子ノードを走査
    for child in node.children().nodes() {
        match child.name().value() {
            "image" => ...,
            "ports" => ...,
            ...
        }
    }

    // 3. イメージ名の自動推測
    if service.image.is_none() {
        service.image = Some(infer_image_name(&name, ...));
    }
}
```

#### Port ノード

```kdl
port 8080 3000 protocol="tcp" host_ip="127.0.0.1"
```

```rust
fn parse_port(node: &KdlNode) -> Option<Port> {
    // entries[0] = host port (8080)
    // entries[1] = container port (3000)
    // properties["protocol"] = "tcp"
    // properties["host_ip"] = "127.0.0.1"
}
```

## エラーハンドリング

### エラー型

```rust
#[derive(Error, Debug)]
pub enum FlowError {
    #[error("KDLパースエラー: {0}")]
    KdlParse(#[from] kdl::KdlError),

    #[error("無効な設定: {0}")]
    InvalidConfig(String),

    #[error("ファイル読み込みエラー: {0}")]
    Io(#[from] std::io::Error),
}
```

### エラーハンドリング戦略

#### 1. 致命的エラー (即座に中断)

- KDL構文エラー
- 必須フィールドの欠落 (service名, environment名)
- ファイルI/Oエラー

```rust
let name = node.entries().first()
    .ok_or_else(|| FlowError::InvalidConfig("service requires a name"))?;
```

#### 2. 警告 (処理継続)

- 未知のノードタイプ
- 未知のプロパティ

```rust
_ => {
    eprintln!("Warning: Unknown node '{}'", node.name().value());
}
```

#### 3. デフォルト値で回復

- オプショナルフィールドの欠落

```rust
let protocol = node.get("protocol")
    .and_then(|e| ...)
    .unwrap_or_default();  // Protocol::Tcp
```

## イメージ名推測ロジック

### アルゴリズム

```rust
fn infer_image_name(service_name: &str, version: Option<&str>) -> String {
    let tag = version.unwrap_or("latest");
    format!("{}:{}", service_name, tag)
}
```

### 推測例

| サービス名 | バージョン | 推測されるイメージ |
|-----------|----------|------------------|
| `postgres` | `None` | `postgres:latest` |
| `postgres` | `Some("16")` | `postgres:16` |
| `redis` | `None` | `redis:latest` |
| `node` | `Some("20-alpine")` | `node:20-alpine` |

### 将来の拡張

カスタムマッピングのサポート（TODO）：

```kdl
image-aliases {
    db "postgres:16"
    cache "redis:7-alpine"
}

service "db" {
    // postgres:16 が使われる
}
```

## テスト戦略

### ユニットテスト

#### 1. parse_kdl_string のテスト

```rust
#[test]
fn test_parse_simple_service() {
    let kdl = r#"
        service "postgres" {
            version "16"
        }
    "#;
    let config = parse_kdl_string(kdl).unwrap();
    assert_eq!(config.services.len(), 1);
    assert_eq!(config.services["postgres"].image, Some("postgres:16".to_string()));
}
```

#### 2. parse_service のテスト

- 完全なサービス定義
- 最小限のサービス定義
- イメージ名の自動推測

#### 3. parse_port のテスト

- 基本的なポート定義
- プロトコル指定
- host_ip指定

#### 4. parse_volume のテスト

- 基本的なボリューム定義
- read_only指定

#### 5. infer_image_name のテスト

- バージョンなし → `:latest`
- バージョンあり → `:version`

### 統合テスト

#### シナリオ1: 実際のfleetflow.kdlファイル

```rust
#[test]
fn test_parse_real_config() {
    let config = parse_kdl_file("tests/fixtures/fleetflow.kdl").unwrap();
    // 期待される構造を検証
}
```

#### シナリオ2: エラーケース

```rust
#[test]
fn test_invalid_syntax() {
    let kdl = r#"
        service "invalid {
            broken syntax
        }
    "#;
    assert!(parse_kdl_string(kdl).is_err());
}
```

## パフォーマンス考慮事項

### 最適化ポイント

1. **String allocation**: 必要な場合のみクローン
2. **HashMap pre-allocation**: サービス数が事前にわかる場合
3. **Lazy parsing**: 不要なフィールドはパースしない

### ベンチマーク目標

- 10サービス: < 10ms
- 100サービス: < 100ms
- 1000サービス: < 1s

## 実装チェックリスト

### Phase 1: MVP機能
- [x] parse_kdl_file 実装
- [x] parse_kdl_string 実装
- [x] parse_service 実装
- [x] parse_stage 実装 (旧 parse_environment)
- [x] parse_port 実装
- [x] parse_volume 実装
- [x] parse_command 実装
- [x] parse_project 実装
- [x] infer_image_name 実装
- [x] ユニットテスト (41件)
  - [x] サービスパース（基本・バージョン・イメージ・ポート・環境変数・ボリューム・依存関係・コマンド）
  - [x] ステージパース
  - [x] プロジェクト名パース
  - [x] エラーケース（名前なしサービス/ステージ）

### Phase 2: 拡張機能（Issue #7）
- [ ] include ディレクティブ対応
- [ ] 変数展開対応

### Phase 3: 品質向上
- [ ] 統合テスト（実際のKDLファイルを使用）
- [ ] エラーメッセージ改善
- [ ] パフォーマンステスト

## 将来の拡張

### 1. Include機能

```kdl
include "common/database.kdl"
```

- 相対パス解決
- 循環参照チェック
- キャッシュ機構

### 2. 変数展開

```kdl
variables {
    registry "ghcr.io/myorg"
    version "1.0.0"
}

service "api" {
    image "{registry}/api:{version}"
}
```

- 変数定義のパース
- テンプレート展開
- ネストした変数参照

### 3. 条件分岐（検討中）

```kdl
service "api" {
    if env "production" {
        replicas 3
    } else {
        replicas 1
    }
}
```

## 変更履歴

### 2025-11-23: チェックリスト更新
- **理由**: MVP完成状況の記録
- **影響**: 実装チェックリストをPhase別に再構成
  - Phase 1: MVP機能（完了）
  - Phase 2: 拡張機能（Issue #7で計画中）
  - Phase 3: 品質向上（今後の課題）
- **コミット**: (未定)
