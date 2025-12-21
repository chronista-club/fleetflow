# Core Concept - 設計書

## データモデル

### 構造定義

```rust
/// Flow設定のルート
pub struct FlowConfig {
    pub environments: HashMap<String, Environment>,
    pub services: HashMap<String, Service>,
}

/// 環境定義
pub struct Environment {
    pub services: Vec<String>,           // サービス名のリスト
    pub variables: HashMap<String, String>,  // 環境変数
}

/// サービス定義
pub struct Service {
    pub image: Option<String>,           // Dockerイメージ
    pub version: Option<String>,         // バージョン（イメージ推測用）
    pub ports: Vec<Port>,                // ポートマッピング
    pub environment: HashMap<String, String>,  // 環境変数
    pub volumes: Vec<Volume>,            // ボリュームマウント
    pub depends_on: Vec<String>,         // 依存サービス
}

/// ポート定義
pub struct Port {
    pub host: u16,                       // ホスト側ポート
    pub container: u16,                  // コンテナ側ポート
    pub protocol: Protocol,              // TCP/UDP
    pub host_ip: Option<String>,         // バインドIP
}

/// プロトコル種別
pub enum Protocol {
    Tcp,
    Udp,
}

/// ボリューム定義
pub struct Volume {
    pub host: PathBuf,                   // ホスト側パス
    pub container: PathBuf,              // コンテナ側パス
    pub read_only: bool,                 // 読み取り専用フラグ
}
```

### モデルの関係性

```
FlowConfig
├── environments: HashMap<String, Environment>
│   └── Environment
│       ├── services: Vec<String> ───┐
│       └── variables: HashMap       │
│                                     │ (参照)
└── services: HashMap<String, Service> ◄─┘
    └── Service
        ├── ports: Vec<Port>
        ├── volumes: Vec<Volume>
        └── depends_on: Vec<String> ─┐
                                      │ (参照)
                                      └─► 他のService
```

### 設計判断

#### Q: なぜHashMapのキーと構造体内のnameフィールドを分離？

**A: 冗長性を排除**

```rust
// Bad: 重複
HashMap<String, Service> where Service { name: String }
// "api" と service.name が重複

// Good: キーを名前として使用
HashMap<String, Service>
// キー "api" がサービス名
```

**利点**:

- データの重複なし
- 不整合の可能性ゼロ
- メモリ効率的

#### Q: なぜOption<String>とVec<T>を混在？

**A: セマンティクスの違い**

```rust
pub image: Option<String>,     // "あるかないか"
pub ports: Vec<Port>,          // "0個以上"
```

- `Option`: 概念的に単一の値、存在しないことに意味がある
- `Vec`: 複数要素、空リストと未定義は同じ

#### Q: なぜPathBufを使用？

**A: 型安全性とクロスプラットフォーム対応**

```rust
pub host: PathBuf,  // String ではなく PathBuf
```

- パス操作の型安全性
- プラットフォーム固有の区切り文字を自動処理
- パス結合などの操作が安全

## アーキテクチャ

### レイヤー構成

```
┌─────────────────────────────────┐
│   CLI Layer (flow-cli)          │
│   - コマンド解析                 │
│   - ユーザーインターフェース      │
└────────────┬────────────────────┘
             │
┌────────────▼────────────────────┐
│   Config Layer (flow-config)    │
│   - 設定ファイル検索             │
│   - パス解決                     │
└────────────┬────────────────────┘
             │
┌────────────▼────────────────────┐
│   Core Layer (flow-atom)        │
│   - KDLパース                    │
│   - データモデル                 │
│   - バリデーション               │
└────────────┬────────────────────┘
             │
┌────────────▼────────────────────┐
│   Runtime Layer (flow-container)│
│   - Docker API呼び出し           │
│   - コンテナ管理                 │
└─────────────────────────────────┘
```

### データフロー

```
fleetflow.kdl (ファイル)
    ↓ [flow-config]
設定ファイルパス解決
    ↓ [flow-atom::parser]
KdlDocument
    ↓ [flow-atom::parser]
FlowConfig (内部モデル)
    ↓ [flow-atom::validator]
検証済みFlowConfig
    ↓ [flow-container]
Docker API呼び出し
    ↓
コンテナ起動
```

## 実装手法

### Default トレイトの活用

```rust
#[derive(Default)]
pub struct Service {
    pub image: Option<String>,
    #[serde(default)]
    pub ports: Vec<Port>,
    // ...
}
```

**理由**:

- デフォルト値の明示化
- `Service::default()` で空の構造体生成
- Serdeの `#[serde(default)]` と連携

### 型駆動設計

```rust
// 不正な状態を型で排除
pub enum Protocol {
    Tcp,
    Udp,
}

// String ではなく enum を使用
// → "tcp", "udp" 以外の値を型レベルで排除
```

### イミュータブルな設計

```rust
// FlowConfig は一度パースしたら変更しない
pub struct FlowConfig {
    pub environments: HashMap<String, Environment>,
    // pub ではあるが、変更は想定しない
}
```

**理由**:

- 予測可能な振る舞い
- デバッグが容易
- 並行処理で安全

## エラーハンドリング

### エラー戦略

#### 1. 早期失敗（Fail Fast）

```rust
// 必須フィールドの欠落は即座にエラー
let name = node.entries().first()
    .ok_or_else(|| FlowError::InvalidConfig("service requires name"))?;
```

#### 2. デフォルト値で回復

```rust
// オプショナルフィールドはデフォルト値
let protocol = node.get("protocol")
    .unwrap_or(Protocol::Tcp);
```

#### 3. 警告で継続

```rust
// 未知のノードは警告
_ => eprintln!("Warning: Unknown node '{}'", node.name()),
```

### エラー型の階層

```rust
FlowError (flow-atom)
├── KdlParse(kdl::KdlError)      // KDL構文エラー
├── InvalidConfig(String)         // 設定エラー
├── ServiceNotFound(String)       // サービス参照エラー
└── CircularDependency(String)    // 循環依存

ConfigError (flow-config)
├── ConfigDirNotFound
├── FleetFlowFileNotFound(PathBuf)
└── Io(std::io::Error)
```

## バリデーション戦略（TODO）

### レベル1: 構文バリデーション

KDLパーサーが自動実行:

- 括弧の対応
- 引用符の閉じ忘れ

### レベル2: 意味的バリデーション

```rust
impl FlowConfig {
    pub fn validate(&self) -> Result<()> {
        // 1. サービス参照の検証
        for env in self.environments.values() {
            for svc_name in &env.services {
                if !self.services.contains_key(svc_name) {
                    return Err(FlowError::ServiceNotFound(svc_name.clone()));
                }
            }
        }

        // 2. 依存関係の検証
        self.validate_dependencies()?;

        // 3. ポート番号の検証
        self.validate_ports()?;

        Ok(())
    }
}
```

### レベル3: 実行時バリデーション

```rust
// Docker API呼び出し前
// - イメージの存在確認
// - ポートの競合チェック
```

## パフォーマンス最適化

### メモリ効率

```rust
// String のクローンを最小化
pub fn parse_service(node: &KdlNode) -> Result<(String, Service)> {
    // String::from() ではなく as_string().to_string()
    // 必要な場合のみクローン
}
```

### パース高速化

```rust
// HashMap の事前割り当て
let mut services = HashMap::with_capacity(estimated_size);
```

## テスト戦略

### ユニットテスト

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_service() {
        let svc = Service::default();
        assert_eq!(svc.ports.len(), 0);
        assert_eq!(svc.image, None);
    }

    #[test]
    fn test_protocol_default() {
        let proto = Protocol::default();
        assert_eq!(proto, Protocol::Tcp);
    }
}
```

### プロパティベーステスト（TODO）

```rust
#[test]
fn test_service_name_uniqueness() {
    // FlowConfig内のサービス名は一意であることを検証
}
```

## 実装チェックリスト

- [x] FlowConfig 構造体定義
- [x] Environment 構造体定義
- [x] Service 構造体定義
- [x] Port 構造体定義
- [x] Volume 構造体定義
- [x] Protocol enum定義
- [x] Default トレイト実装
- [x] Serialize/Deserialize 実装
- [ ] Validation実装
- [ ] ユニットテスト
- [ ] ドキュメントコメント
- [ ] 使用例

## 将来の拡張

### 1. スマートデフォルト

```rust
// サービスタイプを検出して適切なデフォルトを適用
match service_name {
    "postgres" => apply_postgres_defaults(),
    "redis" => apply_redis_defaults(),
    _ => apply_generic_defaults(),
}
```

### 2. 型付き環境変数

```rust
pub struct TypedEnvVar {
    pub key: String,
    pub value: EnvValue,
}

pub enum EnvValue {
    String(String),
    Secret(String),    // 機密情報
    FromFile(PathBuf), // ファイルから読み込み
}
```

### 3. ヘルスチェック

```rust
pub struct HealthCheck {
    pub test: Vec<String>,
    pub interval: Duration,
    pub timeout: Duration,
    pub retries: u32,
}
```
