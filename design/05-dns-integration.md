# DNS連携設計

## アーキテクチャ

```
┌─────────────────────────────────────────────────────────────────┐
│                        FleetFlow CLI                            │
│                                                                 │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────────────┐ │
│  │   cloud     │    │   cloud     │    │       cloud         │ │
│  │    up       │    │   down      │    │        dns          │ │
│  └──────┬──────┘    └──────┬──────┘    └──────────┬──────────┘ │
│         │                  │                      │            │
│         ▼                  ▼                      ▼            │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │                 CloudDnsManager                          │  │
│  │  - create_record(subdomain, ip)                          │  │
│  │  - delete_record(subdomain)                              │  │
│  │  - list_records()                                        │  │
│  │  - update_record(subdomain, ip)                          │  │
│  └──────────────────────────────────────────────────────────┘  │
│                              │                                  │
└──────────────────────────────┼──────────────────────────────────┘
                               │
                               ▼
                    ┌─────────────────────┐
                    │   Cloudflare API    │
                    │   (DNS Records)     │
                    └─────────────────────┘
```

## モジュール構成

### 新規クレート

```
crates/
└── fleetflow-cloud-dns/
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        ├── cloudflare.rs       # Cloudflare API実装
        ├── manager.rs          # DNSマネージャー
        └── error.rs            # エラー定義
```

### 既存クレートとの関係

```
fleetflow-cli
├── fleetflow-cloud-sakura     # さくらクラウド連携（既存）
└── fleetflow-cloud-dns        # DNS連携（新規）
```

## データ構造

### DnsRecord

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsRecord {
    pub id: String,           // Cloudflare record ID
    pub name: String,         // 完全なサブドメイン (mcp-prod.example.com)
    pub content: String,      // IPアドレス or CNAME target
    pub record_type: String,  // "A" or "CNAME"
    pub proxied: bool,        // false (DNS Only)
    pub ttl: u32,             // 1 = Auto
}
```

### ServerResource (DNS Aliases)

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerResource {
    pub provider: String,
    pub plan: Option<String>,
    pub disk_size: Option<u32>,
    pub ssh_keys: Vec<String>,
    pub os: Option<String>,
    pub startup_script: Option<String>,
    pub tags: Vec<String>,

    /// DNSエイリアス（CNAME）の一覧
    /// 例: ["app", "api"] -> app.{domain} と api.{domain} が {server-hostname}.{domain} を参照
    pub dns_aliases: Vec<String>,

    pub config: HashMap<String, String>,
}
```

### DnsConfig

```rust
#[derive(Debug, Clone)]
pub struct DnsConfig {
    pub api_token: String,
    pub zone_id: String,
    pub domain: String,
}

impl DnsConfig {
    pub fn from_env() -> Result<Self, DnsError> {
        Ok(Self {
            api_token: std::env::var("CLOUDFLARE_API_TOKEN")
                .map_err(|_| DnsError::MissingEnvVar("CLOUDFLARE_API_TOKEN".to_string()))?,
            zone_id: std::env::var("CLOUDFLARE_ZONE_ID")
                .map_err(|_| DnsError::MissingEnvVar("CLOUDFLARE_ZONE_ID".to_string()))?,
            domain: std::env::var("CLOUDFLARE_DOMAIN")
                .unwrap_or_else(|_| "example.com".to_string()),
        })
    }
}
```

## Cloudflare API

### エンドポイント

| 操作 | Method | Endpoint |
|------|--------|----------|
| レコード一覧 | GET | `/zones/{zone_id}/dns_records` |
| レコード作成 | POST | `/zones/{zone_id}/dns_records` |
| レコード更新 | PUT | `/zones/{zone_id}/dns_records/{record_id}` |
| レコード削除 | DELETE | `/zones/{zone_id}/dns_records/{record_id}` |

### 認証ヘッダー

```
Authorization: Bearer {CLOUDFLARE_API_TOKEN}
```

### リクエスト例（レコード作成）

```json
POST /zones/{zone_id}/dns_records
{
  "type": "A",
  "name": "mcp-prod",
  "content": "203.0.113.1",
  "ttl": 1,
  "proxied": false
}
```

### レスポンス例

```json
{
  "success": true,
  "result": {
    "id": "372e67954025e0ba6aaa6d586b9e0b59",
    "type": "A",
    "name": "mcp-prod.example.com",
    "content": "203.0.113.1",
    "proxied": false,
    "ttl": 1
  }
}
```

## 実装詳細

### CloudDnsManager

```rust
pub struct CloudDnsManager {
    client: reqwest::Client,
    config: DnsConfig,
}

impl CloudDnsManager {
    pub fn new(config: DnsConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }

    /// サービス名からサブドメインを生成
    pub fn generate_subdomain(&self, service: &str, stage: &str) -> String {
        let short_name = service
            .trim_start_matches("creo-")
            .trim_end_matches("-server")
            .trim_end_matches("-viewer");
        format!("{}-{}", short_name, stage)
    }

    /// DNSレコード(A)を作成または更新
    pub async fn ensure_record(&self, subdomain: &str, ip: &str) -> Result<DnsRecord, DnsError> {
        // 既存レコードを検索
        if let Some(existing) = self.find_record(subdomain).await? {
            // IPが同じなら何もしない
            if existing.content == ip {
                return Ok(existing);
            }
            // IPが違えば更新
            return self.update_record(&existing.id, ip).await;
        }
        // 新規作成
        self.create_record(subdomain, ip).await
    }

    /// DNSレコード(A)を削除
    pub async fn remove_record(&self, subdomain: &str) -> Result<(), DnsError> {
        if let Some(record) = self.find_record(subdomain).await? {
            self.delete_record(&record.id).await?;
        }
        Ok(())
    }

    /// CNAMEレコードを作成または更新
    pub async fn ensure_cname_record(&self, subdomain: &str, target: &str) -> Result<DnsRecord, DnsError> {
        // 既存CNAMEレコードを検索
        if let Some(existing) = self.find_cname_record(subdomain).await? {
            // ターゲットが同じなら何もしない
            if existing.content == target {
                return Ok(existing);
            }
            // ターゲットが違えば更新
            return self.update_cname_record(&existing.id, target).await;
        }
        // 新規作成
        self.create_cname_record(subdomain, target).await
    }

    /// CNAMEレコードを削除
    pub async fn remove_cname_record(&self, subdomain: &str) -> Result<(), DnsError> {
        if let Some(record) = self.find_cname_record(subdomain).await? {
            self.delete_record(&record.id).await?;
        }
        Ok(())
    }

    /// すべてのDNSレコードを取得
    pub async fn list_records(&self) -> Result<Vec<DnsRecord>, DnsError> {
        // Cloudflare API呼び出し
    }
}
```

### cloud upへの統合

```rust
// fleetflow-cli/src/commands/cloud_up.rs

pub async fn execute(args: CloudUpArgs) -> Result<()> {
    // 1. クラウドプロバイダーでサーバー作成
    let server = cloud_manager.create_server(&args.stage).await?;
    let server_ip = server.ip_address;

    // 2. DNS設定（オプショナル）
    if let Ok(dns_config) = DnsConfig::from_env() {
        let dns_manager = CloudDnsManager::new(dns_config);

        for service in &config.services {
            if service.is_public {
                let subdomain = dns_manager.generate_subdomain(&service.name, &args.stage);
                match dns_manager.ensure_record(&subdomain, &server_ip).await {
                    Ok(record) => {
                        println!("✓ DNS: {}", record.name);
                    }
                    Err(e) => {
                        eprintln!("⚠ DNS設定失敗: {}", e);
                        // 続行（サーバーは作成済み）
                    }
                }
            }
        }
    } else {
        println!("ℹ DNS設定をスキップ（環境変数未設定）");
    }

    // 3. SSH接続してコンテナ起動
    // ...
}
```

### cloud downへの統合

```rust
// fleetflow-cli/src/commands/cloud_down.rs

pub async fn execute(args: CloudDownArgs) -> Result<()> {
    // 1. SSH接続してコンテナ停止
    // ...

    // 2. DNS削除（オプショナル）
    if let Ok(dns_config) = DnsConfig::from_env() {
        let dns_manager = CloudDnsManager::new(dns_config);

        for service in &config.services {
            if service.is_public {
                let subdomain = dns_manager.generate_subdomain(&service.name, &args.stage);
                match dns_manager.remove_record(&subdomain).await {
                    Ok(_) => {
                        println!("✓ DNS削除: {}.{}", subdomain, dns_config.domain);
                    }
                    Err(e) => {
                        eprintln!("⚠ DNS削除失敗: {}", e);
                        // 続行
                    }
                }
            }
        }
    }

    // 3. クラウドプロバイダーでサーバー削除
    cloud_manager.delete_server(&args.stage).await?;
}
```

## エラー型

```rust
#[derive(Debug, thiserror::Error)]
pub enum DnsError {
    #[error("環境変数が設定されていません: {0}")]
    MissingEnvVar(String),

    #[error("Cloudflare APIエラー: {0}")]
    ApiError(String),

    #[error("認証エラー: トークンが無効です")]
    AuthenticationError,

    #[error("Zone ID '{0}' が見つかりません")]
    ZoneNotFound(String),

    #[error("ネットワークエラー: {0}")]
    NetworkError(#[from] reqwest::Error),
}
```

## テスト戦略

### ユニットテスト

```rust
#[test]
fn test_generate_subdomain() {
    let config = DnsConfig {
        api_token: "test".to_string(),
        zone_id: "test".to_string(),
        domain: "example.com".to_string(),
    };
    let manager = CloudDnsManager::new(config);

    assert_eq!(manager.generate_subdomain("creo-mcp-server", "prod"), "mcp-prod");
    assert_eq!(manager.generate_subdomain("creo-api-server", "dev"), "api-dev");
    assert_eq!(manager.generate_subdomain("creo-memory-viewer", "prod"), "memory-prod");
}
```

### 統合テスト

```rust
#[tokio::test]
#[ignore] // 実際のCloudflare APIを使用
async fn test_dns_lifecycle() {
    let config = DnsConfig::from_env().unwrap();
    let manager = CloudDnsManager::new(config);

    // 作成
    let record = manager.ensure_record("test-integration", "192.0.2.1").await.unwrap();
    assert_eq!(record.content, "192.0.2.1");

    // 更新
    let updated = manager.ensure_record("test-integration", "192.0.2.2").await.unwrap();
    assert_eq!(updated.content, "192.0.2.2");

    // 削除
    manager.remove_record("test-integration").await.unwrap();
}
```

## 依存クレート

```toml
[dependencies]
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "2.0"
tokio = { version = "1", features = ["full"] }
```

## セットアップ手順

### 1. Cloudflare APIトークン作成

1. https://dash.cloudflare.com/profile/api-tokens にアクセス
2. "Create Token" をクリック
3. "Custom token" を選択
4. 権限: `Zone - DNS - Edit`
5. Zone Resources: `Include - Specific zone - your-domain.com`
6. トークンをコピー

### 2. Zone ID取得

1. Cloudflareダッシュボードでドメインを選択
2. 右サイドバーの "API" セクションで "Zone ID" をコピー

### 3. 環境変数設定

```bash
export CLOUDFLARE_API_TOKEN="your-api-token"
export CLOUDFLARE_ZONE_ID="your-zone-id"
export CLOUDFLARE_DOMAIN="your-domain.com"
```

## 関連ドキュメント

- [DNS連携仕様](../spec/09-dns-integration.md)
- [クラウドインフラ設計](./04-cloud-infrastructure.md)
