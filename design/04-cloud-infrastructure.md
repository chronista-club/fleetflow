# FleetFlow Cloud Infrastructure - 設計

## アーキテクチャ

```
┌─────────────────────────────────────────────────┐
│                  FleetFlow CLI                   │
│                  (fleetflow up/down)                  │
└─────────────────┬───────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────┐
│               fleetflow-cloud                    │
│  ┌──────────────────────────────────────────┐   │
│  │          Provider Abstraction             │   │
│  │  trait CloudProvider { ... }              │   │
│  └──────────────────────────────────────────┘   │
│  ┌──────────────┐  ┌──────────────┐            │
│  │  KDL Parser  │  │  State Mgmt  │            │
│  └──────────────┘  └──────────────┘            │
└───────┬─────────────────┬───────────────────────┘
        │                 │
┌───────▼───────┐ ┌───────▼───────┐
│ sakura-cloud  │ │  cloudflare   │
│   provider    │ │   provider    │
├───────────────┤ ├───────────────┤
│ usacloud CLI  │ │ wrangler CLI  │
│               │ │ cf API        │
└───────────────┘ └───────────────┘
```

## クレート構成

| クレート | 責務 |
|---------|------|
| `fleetflow-cloud` | プロバイダー抽象化、状態管理、KDLパース |
| `fleetflow-cloud-sakura` | さくらのクラウドプロバイダー（usacloud） |
| `fleetflow-cloud-cloudflare` | Cloudflareプロバイダー（R2, Workers, DNS） |

## Provider Trait

```rust
#[async_trait]
pub trait CloudProvider {
    /// プロバイダー名
    fn name(&self) -> &str;

    /// 現在の状態を取得
    async fn get_state(&self) -> Result<ProviderState>;

    /// 差分を計算
    async fn plan(&self, desired: &ResourceConfig) -> Result<Vec<Action>>;

    /// 変更を適用
    async fn apply(&self, actions: Vec<Action>) -> Result<ApplyResult>;

    /// リソースを削除
    async fn destroy(&self, resource_id: &str) -> Result<()>;
}
```

## 状態管理

### 宣言的な状態収束

```
desired state (KDL) ─┐
                     ├─→ diff → actions → apply
current state ───────┘
```

### 状態ファイル

```
.fleetflow/
├── state.json          # 現在の状態
├── state.json.backup   # バックアップ
└── lock.json           # 同時実行防止
```

### 状態スキーマ

```json
{
  "version": 1,
  "resources": {
    "sakura-cloud:server:creo-dev-01": {
      "id": "123456789",
      "ip": "xxx.xxx.xxx.xxx",
      "status": "running",
      "created_at": "2024-01-01T00:00:00Z"
    },
    "cloudflare:r2:creo-attachments": {
      "name": "creo-attachments",
      "location": "APAC",
      "created_at": "2024-01-01T00:00:00Z"
    }
  }
}
```

## さくらのクラウド連携

### usacloudコマンドマッピング

| FleetFlow操作 | usacloudコマンド |
|--------------|------------------|
| server作成 | `usacloud server create` |
| server削除 | `usacloud server delete` |
| server状態確認 | `usacloud server read` |
| ディスク作成 | `usacloud disk create` |
| SSH鍵登録 | `usacloud ssh-key create` |

### サーバー作成フロー

```
1. KDLパース → ServerConfig
2. 既存サーバー確認 (usacloud server list)
3. 差分計算
4. ディスク作成 (usacloud disk create)
5. サーバー作成 (usacloud server create)
6. SSH鍵設定
7. 起動待機
8. 状態ファイル更新
```

## Cloudflare連携

### CLIツール/API

| リソース | ツール |
|---------|--------|
| R2 Bucket | `wrangler r2 bucket` |
| Workers | `wrangler deploy` |
| DNS | Cloudflare API (REST) |
| Pages | `wrangler pages` |

### R2バケット作成フロー

```
1. KDLパース → R2Config
2. 既存バケット確認 (wrangler r2 bucket list)
3. 差分計算
4. バケット作成 (wrangler r2 bucket create)
5. CORS設定
6. カスタムドメイン設定
7. 状態ファイル更新
```

## エラーハンドリング

### リトライ戦略

```rust
pub struct RetryConfig {
    max_attempts: u32,      // デフォルト: 3
    initial_delay: Duration, // デフォルト: 1s
    max_delay: Duration,     // デフォルト: 30s
    backoff_multiplier: f64, // デフォルト: 2.0
}
```

### ロールバック

部分的な適用失敗時：
1. 成功したアクションを記録
2. 失敗を報告
3. ユーザーに選択肢を提示（続行/ロールバック/中止）

## セキュリティ考慮

### 認証情報

- さくらのクラウド: `usacloud config` から取得（ファイルに保存しない）
- Cloudflare: 環境変数 `CF_API_TOKEN`, `CF_ACCOUNT_ID`

### 状態ファイル

- `.gitignore` に追加を推奨
- センシティブ情報（IPアドレス等）を含む可能性

## 実装優先順位

### Phase 0（MVP）

1. `fleetflow-cloud` クレート作成
2. CloudProvider trait定義
3. `fleetflow-cloud-sakura` 実装
   - server create/delete
   - 状態管理

### Phase 1

4. `fleetflow-cloud-cloudflare` 実装
   - R2 bucket管理
   - DNS管理

### Phase 2

5. Cloudflare Workers対応
6. scale ノード実装
7. 複数サーバー対応
