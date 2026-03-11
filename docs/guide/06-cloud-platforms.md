# 外部プラットフォーム対応状況

FleetFlowが現在対応している外部プラットフォームと、その機能一覧です。

## 対応プラットフォーム一覧

| プラットフォーム | 対応状況 | 必要なCLI/認証 |
|-----------------|---------|---------------|
| さくらのクラウド | ✅ 対応 | `usacloud` CLI |
| Cloudflare DNS | ✅ 対応 | API Token (環境変数) |
| Cloudflare R2 | 🚧 実装中 | `wrangler` CLI |
| Cloudflare Workers | 📋 予定 | `wrangler` CLI |

---

## さくらのクラウド

### 対応機能

| 機能 | CLI コマンド | 状態 |
|------|-------------|------|
| サーバー作成 | `flow cloud server create` | ✅ |
| サーバー削除 | `flow cloud server delete` | ✅ |
| サーバー起動 | `flow cloud server start` | ✅ |
| サーバー停止 | `flow cloud server stop` | ✅ |
| サーバー一覧 | `flow cloud server list` | ✅ |
| 認証確認 | `flow cloud auth` | ✅ |
| SSH鍵管理 | - | ✅ (内部) |
| ディスク管理 | - | ✅ (内部) |
| スタートアップスクリプト | - | ✅ (内部) |

### セットアップ

```bash
# 1. usacloud CLIのインストール
brew install sacloud/usacloud/usacloud

# 2. 認証設定
usacloud config

# 3. 認証確認
flow cloud auth
```

### 使用例

```bash
# サーバー一覧
flow cloud server list

# サーバー作成（KDL設定に基づく）
flow cloud up -s dev

# サーバー停止
flow cloud server stop --name my-server

# サーバー削除
flow cloud server delete --name my-server --yes
```

### KDL設定例

```kdl
providers {
    sakura-cloud { zone "tk1a" }
}

stage "dev" {
    server "app-server" {
        provider "sakura-cloud"
        plan core=2 memory=4
        disk size=40 os="ubuntu-24.04"
        ssh-key "~/.ssh/id_ed25519.pub"
    }
}
```

---

## Cloudflare DNS

### 対応機能

| 機能 | 状態 |
|------|------|
| Aレコード一覧取得 | ✅ |
| Aレコード作成 | ✅ |
| Aレコード更新 | ✅ |
| Aレコード削除 | ✅ |
| サーバー作成時の自動DNS登録 | ✅ |
| サーバー削除時の自動DNS削除 | ✅ |

### セットアップ

```bash
# 環境変数を設定
export CLOUDFLARE_API_TOKEN="your-api-token"
export CLOUDFLARE_ZONE_ID="your-zone-id"
export CLOUDFLARE_DOMAIN="example.com"
```

### 動作

`flow cloud up` / `flow cloud down` 実行時に自動的にDNSレコードを管理：

- **サーバー作成時**: `{service}-{stage}.{domain}` のAレコードを自動追加
- **サーバー削除時**: 対応するDNSレコードを自動削除

### DNS命名規則

```
{service}-{stage}.{domain}

例: app-dev.example.com
```

---

## Cloudflare R2 (実装中)

### 予定機能

| 機能 | 状態 |
|------|------|
| バケット作成 | 🚧 |
| バケット削除 | 🚧 |
| バケット一覧 | 🚧 |

### セットアップ（予定）

```bash
# wrangler CLIのインストール
npm install -g wrangler

# 認証
wrangler login
```

---

## 環境変数まとめ

| 変数名 | 用途 | 必須 |
|--------|------|------|
| `CLOUDFLARE_API_TOKEN` | Cloudflare API認証 | DNS使用時 |
| `CLOUDFLARE_ZONE_ID` | DNSゾーンID | DNS使用時 |
| `CLOUDFLARE_DOMAIN` | 管理対象ドメイン | DNS使用時 |

---

## アーキテクチャ

```
fleetflow-cloud/           # 抽象化レイヤー（CloudProviderトレイト）
├── fleetflow-cloud-sakura/    # さくらのクラウド実装
│   └── usacloud CLI wrapper
└── fleetflow-cloud-cloudflare/ # Cloudflare実装
    ├── DNS API (直接呼び出し)
    └── wrangler CLI wrapper (R2/Workers)
```

---

## 開発者向け情報

### 新規プロバイダーの追加

`CloudProvider` トレイトを実装することで、新しいプラットフォームを追加できます：

```rust
// fleetflow-cloud/src/provider.rs
#[async_trait]
pub trait CloudProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn check_auth(&self) -> Result<AuthStatus>;
    async fn get_state(&self) -> Result<CloudState>;
    // ...
}
```

### クレート構成

| クレート | 役割 |
|---------|------|
| `fleetflow-cloud` | 共通トレイト・型定義 |
| `fleetflow-cloud-sakura` | さくらのクラウド実装 |
| `fleetflow-cloud-cloudflare` | Cloudflare実装 |
