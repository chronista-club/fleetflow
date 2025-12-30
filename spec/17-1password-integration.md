# 17. 1Password 統合

## 概要

FleetFlowのKDL設定ファイル内で1Password Secret Reference（`op://`形式）を使用し、機密情報を安全に管理する機能。

## 背景と動機

### 課題

- 各サービスの環境変数（DB接続文字列、APIキーなど）が分散管理されている
- `.env`ファイルをGitにコミットできない
- チーム間での秘密情報共有が煩雑
- ローカル/CI/CD環境での設定統一が困難

### 解決策

1Password CLIの`op://`参照をKDL内で使用可能にし、実行時に自動解決する。

```kdl
service "api" {
    environment {
        DATABASE_URL "op://Development/postgres/connection-string"
        API_KEY "op://Development/external-api/key"
    }
}
```

## Secret Reference 形式

### 基本構文

```
op://vault/item/[section/]field
```

| 部分 | 説明 | 例 |
|------|------|-----|
| `vault` | Vault名またはID | `Development`, `Production` |
| `item` | アイテム名またはID | `postgres`, `api-keys` |
| `section` | セクション名（省略可） | `credentials` |
| `field` | フィールド名 | `password`, `connection-string` |

### 例

```
op://Development/postgres/password
op://Production/api-keys/stripe/secret-key
op://Shared/database/credentials/connection-string
```

### 制約

- 特殊文字（`@`, `(`, `)` 等）を含む名前はアイテムIDを使用
- 環境変数展開も可能: `op://${VAULT:-dev}/item/field`

## 機能仕様

### 1. 環境変数解決

`environment`ブロック内の値が`op://`で始まる場合、`fleet up`実行時に1Password CLIで解決する。

**対象スコープ**: `environment`ブロックのみ

```kdl
service "web" {
    environment {
        // op://参照 → 解決される
        DB_PASSWORD "op://Dev/postgres/password"

        // 通常の値 → そのまま
        NODE_ENV "production"
    }
}
```

### 2. エラーハンドリング

`op` CLIが利用不可または参照解決に失敗した場合、**エラーで停止**する（厳格モード）。

| 状況 | 動作 |
|------|------|
| `op`コマンドが見つからない | エラー終了 |
| 1Passwordがロック状態 | エラー終了（認証を促す） |
| 参照が無効（Vault/Item不在） | エラー終了 |
| ネットワークエラー | キャッシュフォールバック |

### 3. キャッシュ機能

#### 目的
ネットワーク障害時のフォールバックとして、解決済みの値をローカルにキャッシュする。

#### キャッシュファイル
```
.fleetflow/secrets-cache.json
```

#### 動作
1. `op read`成功時 → キャッシュに保存
2. `op read`失敗時（ネットワークエラー等） → キャッシュから読み込み
3. キャッシュも存在しない → エラー終了

#### キャッシュ形式
```json
{
  "version": 1,
  "updated_at": "2025-01-15T10:30:00Z",
  "secrets": {
    "op://Development/postgres/password": {
      "value_hash": "sha256:...",
      "cached_at": "2025-01-15T10:30:00Z"
    }
  }
}
```

**注意**: キャッシュには値のハッシュのみ保存し、平文は保存しない設計も検討。

### 4. CLIコマンド

#### `fleet env <stage>`

指定ステージの環境変数を一覧表示（マスク付き）。

```bash
$ fleet env local

Service: api
  DATABASE_URL: postgres://user:****@localhost/db
  API_KEY: sk-live-****

Service: worker
  REDIS_URL: redis://****@localhost:6379
```

#### `fleet env <stage> --reveal`

値を完全に表示（機密情報注意）。

```bash
$ fleet env local --reveal

Service: api
  DATABASE_URL: postgres://user:actualpassword@localhost/db
  API_KEY: sk-live-abcd1234efgh5678
```

#### `fleet validate --secrets`

`op://`参照の有効性をチェック（値は取得しない）。

```bash
$ fleet validate --secrets

Checking secret references...
✓ op://Development/postgres/password
✓ op://Development/api-keys/stripe
✗ op://Development/missing-item/key
  Error: item "missing-item" not found in vault "Development"

1 error(s) found
```

## セキュリティ考慮事項

### 平文の扱い

- 解決済みの値はメモリ上でのみ保持
- ログに平文を出力しない
- キャッシュへの平文保存は避ける（ハッシュのみ or 暗号化）

### `.gitignore`

以下をデフォルトで無視:
```
.fleetflow/secrets-cache.json
```

### 1Password認証

- ローカル: 1Passwordアプリ連携（Touch ID等）
- CI/CD: Service Account（`OP_SERVICE_ACCOUNT_TOKEN`環境変数）

## 依存関係

### 必須
- 1Password CLI (`op`) バージョン 2.x 以上
- 1Passwordアカウント（Personal, Family, Team, Business）

### CI/CD向け
- 1Password Service Account（有料プラン）

## 非対応スコープ（将来検討）

- `image`フィールドでのレジストリ認証
- `build.args`での参照
- 1Password Connect API対応
