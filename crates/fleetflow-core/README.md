# fleetflow-core

[![Crates.io](https://img.shields.io/crates/v/fleetflow-core.svg)](https://crates.io/crates/fleetflow-core)
[![Documentation](https://docs.rs/fleetflow-core/badge.svg)](https://docs.rs/fleetflow-core)
[![License](https://img.shields.io/crates/l/fleetflow-core.svg)](https://github.com/chronista-club/fleetflow#license)

FleetFlowのコア機能を提供するライブラリクレート。

## 概要

`fleetflow-core`は、FleetFlowの中核となる機能を提供します：

- **KDLパーサー** - KDL設定ファイルの解析
- **データモデル** - Flow、Service、Stage、Processなどの構造体
- **ローダー** - プロジェクト全体の設定読み込み
- **テンプレートエンジン** - 変数展開とテンプレート処理
- **ファイル検出** - 自動的な設定ファイルの発見

## モジュール構造

```
src/
├── model/           # データ構造
│   ├── flow.rs      # Flow
│   ├── stage.rs     # Stage
│   ├── service.rs   # Service, BuildConfig
│   ├── port.rs      # Port, Protocol
│   ├── volume.rs    # Volume
│   └── process.rs   # Process, ProcessState
└── parser/          # KDLパーサー
    ├── mod.rs       # メインパース関数
    ├── stage.rs     # stageノードパース
    ├── service.rs   # serviceノードパース
    ├── port.rs      # portノードパース
    └── volume.rs    # volumeノードパース
```

## 使用例

```rust
use fleetflow_core::{Flow, Service, Stage, parser};

// KDL文字列をパース
let kdl_content = r#"
service "postgres" {
    version "16"
}

stage "local" {
    service "postgres"
}
"#;

let flow = parser::parse_kdl_string(kdl_content, "example".to_string())?;

// Flowからサービスにアクセス
if let Some(postgres) = flow.services.get("postgres") {
    println!("PostgreSQL version: {:?}", postgres.version);
}
```

## 主な型

### Flow

プロセスの設計図。データベースに格納可能。

```rust
pub struct Flow {
    pub name: String,
    pub services: HashMap<String, Service>,
    pub stages: HashMap<String, Stage>,
}
```

### Service

コンテナサービスの定義。

```rust
pub struct Service {
    pub image: Option<String>,
    pub version: Option<String>,
    pub command: Option<String>,
    pub ports: Vec<Port>,
    pub environment: HashMap<String, String>,
    pub volumes: Vec<Volume>,
    pub depends_on: Vec<String>,
    pub build: Option<BuildConfig>,  // Dockerビルド設定
}

pub struct BuildConfig {
    pub dockerfile: Option<PathBuf>,
    pub context: Option<PathBuf>,
    pub args: HashMap<String, String>,
    pub target: Option<String>,
    pub no_cache: bool,
    pub image_tag: Option<String>,
}
```

### Stage

環境（local、dev、stg、prdなど）の定義。

```rust
pub struct Stage {
    pub services: Vec<String>,
    pub variables: HashMap<String, String>,
}
```

### Process

実行中のプロセス情報。データベースに格納可能。

```rust
pub struct Process {
    pub id: String,
    pub flow_name: String,
    pub stage_name: String,
    pub service_name: String,
    pub container_id: Option<String>,
    pub state: ProcessState,
    pub started_at: i64,
    // ... その他のフィールド
}
```

## 機能

### KDLパーサー

KDL形式の設定ファイルを解析してFlowオブジェクトに変換。

```rust
use fleetflow_core::parser;

let flow = parser::parse_kdl_file("flow.kdl")?;
```

### プロジェクトローダー

プロジェクト全体（複数ファイル）を自動的に読み込み。

```rust
use fleetflow_core::loader;

let flow = loader::load_project()?;
```

### テンプレート処理

変数展開とテンプレート機能。

```rust
use fleetflow_core::template::TemplateProcessor;

let mut processor = TemplateProcessor::new();
processor.add_variable("version", "1.0.0");
let result = processor.render("{{ version }}")?;
```

## ドキュメント

- [FleetFlow メインプロジェクト](https://github.com/chronista-club/fleetflow)
- [API ドキュメント](https://docs.rs/fleetflow-core)
- [仕様書](https://github.com/chronista-club/fleetflow/tree/main/spec)

## ライセンス

MIT OR Apache-2.0
