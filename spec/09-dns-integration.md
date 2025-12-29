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
| creo-mcp-server | live | example.com | `mcp-live.example.com` |
| creo-mcp-server | dev | example.com | `mcp-dev.example.com` |
| creo-api-server | live | example.com | `api-live.example.com` |

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
flow cloud up --stage live --yes
```

1. クラウドプロバイダーでサーバー作成
2. サーバーIPを取得
3. **DNSレコード作成** (環境変数設定時)
   - `{service}-{stage}.{domain}` → サーバーIP
4. SSH接続してコンテナ起動

### cloud down

```bash
flow cloud down --stage live --yes
```

1. SSH接続してコンテナ停止
2. **DNSレコード削除** (環境変数設定時)
3. クラウドプロバイダーでサーバー削除

### cloud dns (新規サブコマンド)

DNS操作のみを行う補助コマンド:

```bash
# 現在のDNSレコード一覧
flow cloud dns list

# 手動でレコード追加
flow cloud dns add --subdomain mcp-prod --ip 203.0.113.1

# 手動でレコード削除
flow cloud dns remove --subdomain mcp-prod
```

## DNS設定

### 概要

サーバー設定に`dns`ブロックを追加することで、Cloudflare DNSレコードを宣言的に管理できます。
サーバー作成時にIPアドレスを動的取得し、A/AAAAレコードとCNAMEエイリアスを自動作成します。

### KDL構文

```kdl
server "creo-dev" {
    provider "sakura-cloud"
    plan "4core-8gb"

    dns {
        hostname "dev"       // A + AAAAレコード（IPは動的取得）
        aliases "forge"      // CNAME: forge.domain → dev.domain
    }
}
```

### 構文要素

| 要素 | 説明 | 例 |
|------|------|-----|
| `hostname` | A/AAAAレコードのサブドメイン名。IPはサーバーから動的取得 | `hostname "dev"` |
| `aliases` | CNAMEレコード。hostnameへの参照を作成 | `aliases "forge" "build"` |

### 動作

1. **cloud up時**:
   - さくらクラウドでサーバー作成
   - IPv4/IPv6アドレスを動的取得
   - Aレコード作成: `dev.example.com` → IPv4
   - AAAAレコード作成: `dev.example.com` → IPv6
   - CNAMEエイリアス作成: `forge.example.com` → `dev.example.com`

2. **cloud down時**:
   - CNAMEエイリアスを削除
   - A/AAAAレコードを削除
   - サーバーを削除

### 使用例

```kdl
// Cloudflareプロバイダー設定
provider "cloudflare" {
    zone "creo-memories.in"  // Zone IDは環境変数 CLOUDFLARE_ZONE_ID
}

provider "sakura-cloud" {
    zone "tk1a"
}

// ライブサーバー
server "creo-vps" {
    provider "sakura-cloud"
    plan "4core-8gb"

    dns {
        hostname "vps-live"
        aliases "app"
    }
}

// 開発サーバー（ビルド拠点兼用）
server "creo-dev" {
    provider "sakura-cloud"
    plan "4core-8gb"

    dns {
        hostname "dev"
        aliases "forge"
    }
}

stage "live" {
    server "creo-vps"
}

stage "dev" {
    server "creo-dev"
}
```

結果（live）:
- `vps-live.creo-memories.in` (A/AAAA) → サーバーIP
- `app.creo-memories.in` (CNAME) → `vps-live.creo-memories.in`

結果（dev）:
- `dev.creo-memories.in` (A/AAAA) → サーバーIP
- `forge.creo-memories.in` (CNAME) → `dev.creo-memories.in`

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
