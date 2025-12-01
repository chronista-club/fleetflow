# DNS連携仕様

## 概要

FleetFlowの`cloud up`/`cloud down`コマンドにCloudflare DNS自動管理機能を追加する。
サーバー作成時にDNSレコードを自動追加し、削除時に自動削除する。

## 目的

- **自動化**: 手動でのDNS設定を排除し、デプロイ時間を短縮
- **一貫性**: サーバーIPとDNSレコードの同期を保証
- **簡素化**: `cloud up`一発で完全なデプロイ完了

## サブドメイン命名規則

### フォーマット

```
{service}-{stage}.{domain}
```

### 例

| サービス | ステージ | ドメイン | サブドメイン |
|----------|---------|---------|-------------|
| creo-mcp-server | prod | example.com | `mcp-prod.example.com` |
| creo-mcp-server | dev | example.com | `mcp-dev.example.com` |
| creo-api-server | prod | example.com | `api-prod.example.com` |

### サービス名変換ルール

デフォルトの変換ルール:
- `creo-`プレフィックスを除去
- `-server`/`-viewer`サフィックスを除去

例:
- `creo-mcp-server` → `mcp`
- `creo-api-server` → `api`
- `creo-memory-viewer` → `memory`

## API認証

### 環境変数

| 変数名 | 説明 | 必須 |
|--------|------|------|
| `CLOUDFLARE_API_TOKEN` | Cloudflare APIトークン | Yes |
| `CLOUDFLARE_ZONE_ID` | ドメインのZone ID | Yes |

### トークン権限

```
Zone:DNS:Edit
```

APIトークンは https://dash.cloudflare.com/profile/api-tokens で作成。

## コマンド動作

### cloud up

```bash
fleetflow cloud up --stage prod --yes
```

1. クラウドプロバイダーでサーバー作成
2. サーバーIPを取得
3. **DNSレコード作成** (環境変数設定時)
   - `{service}-{stage}.{domain}` → サーバーIP
4. SSH接続してコンテナ起動

### cloud down

```bash
fleetflow cloud down --stage prod --yes
```

1. SSH接続してコンテナ停止
2. **DNSレコード削除** (環境変数設定時)
3. クラウドプロバイダーでサーバー削除

### cloud dns (新規サブコマンド)

DNS操作のみを行う補助コマンド:

```bash
# 現在のDNSレコード一覧
fleetflow cloud dns list

# 手動でレコード追加
fleetflow cloud dns add --subdomain mcp-prod --ip 203.0.113.1

# 手動でレコード削除
fleetflow cloud dns remove --subdomain mcp-prod
```

## DNSエイリアス機能

### 概要

サーバー設定に`dns_aliases`を追加することで、メインのDNSレコード(`{service}-{stage}.{domain}`)に加えて、任意のCNAMEエイリアスを自動作成できます。

### KDL設定

```kdl
server "creo-vps" {
    provider "sakura-cloud"
    plan "4core-8gb"

    // DNSエイリアスを指定
    dns_aliases "app" "api" "www"

    // ...
}
```

### 動作

1. **cloud up時**:
   - メインのAレコード作成: `vps-prod.example.com` → `203.0.113.1`
   - CNAMEエイリアス作成:
     - `app.example.com` → `vps-prod.example.com`
     - `api.example.com` → `vps-prod.example.com`
     - `www.example.com` → `vps-prod.example.com`

2. **cloud down時**:
   - CNAMEエイリアスを削除
   - メインのAレコードを削除

### 使用例

```kdl
provider "sakura-cloud" {
    zone "tk1a"
}

server "creo-vps" {
    provider "sakura-cloud"
    plan "4core-8gb"
    disk_size 100
    os "ubuntu-24.04"

    // アプリケーションへのアクセスを複数のサブドメインで可能に
    dns_aliases "app" "api" "www"
}

stage "production" {
    servers "creo-vps"
}
```

結果:
- `vps-production.example.com` (Aレコード) → サーバーIP
- `app.example.com` (CNAME) → `vps-production.example.com`
- `api.example.com` (CNAME) → `vps-production.example.com`
- `www.example.com` (CNAME) → `vps-production.example.com`

## KDL設定 (将来拡張)

```kdl
cloud "sakura" {
    // ... 既存設定 ...

    dns "cloudflare" {
        domain "example.com"
        // サービスごとのカスタムサブドメイン（オプション）
        mapping {
            "creo-mcp-server" "mcp"
            "creo-api-server" "api"
        }
    }
}
```

## DNSレコード設定

| 項目 | 値 |
|------|-----|
| レコードタイプ | A（IPv4） |
| Proxy | 無効（DNS Only） |
| TTL | Auto（300秒） |

## エラーハンドリング

### cloud up時のエラー

| シナリオ | 対応 |
|----------|------|
| 環境変数未設定 | 警告を出してDNS設定をスキップ |
| Zone ID不正 | エラー表示してDNS設定をスキップ |
| DNS作成失敗 | 警告を出して続行（サーバーは作成済み） |
| 既存レコードあり | 更新（IPを上書き） |

### cloud down時のエラー

| シナリオ | 対応 |
|----------|------|
| 環境変数未設定 | 警告を出して続行 |
| DNS削除失敗 | 警告を出して続行 |
| レコード存在しない | 正常終了 |

## セキュリティ考慮事項

1. **トークン管理**: 環境変数で管理、設定ファイルには記載しない
2. **最小権限**: `Zone:DNS:Edit`のみ、他の権限は不要
3. **監査ログ**: Cloudflareダッシュボードで操作履歴を確認可能

## 関連ドキュメント

- [クラウドインフラ仕様](./08-cloud-infrastructure.md)
- [DNS連携設計](../design/05-dns-integration.md)
