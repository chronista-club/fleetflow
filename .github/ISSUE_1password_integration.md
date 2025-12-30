# feat: 1Password統合 - op://参照による環境変数管理

## 概要

KDL設定ファイル内で1Password Secret Reference（`op://`形式）を使用し、機密情報を安全に管理する機能を実装する。

## 背景

- 各サービスの環境変数（DB接続文字列、APIキーなど）が分散管理されている
- `.env`ファイルをGitにコミットできない
- チーム間での秘密情報共有が煩雑

## 仕様

### Secret Reference形式

```
op://vault/item/[section/]field
```

### 使用例

```kdl
service "api" {
    environment {
        DATABASE_URL "op://Development/postgres/connection-string"
        API_KEY "op://Development/external-api/key"
    }
}
```

### 対応スコープ

- `environment`ブロックのみ

### エラーハンドリング

- `op` CLIが見つからない → エラー終了
- 1Passwordがロック状態 → エラー終了
- 参照が無効 → エラー終了
- **キャッシュは行わない**（値の不整合を防ぐため）

## 新規CLIコマンド

```bash
fleet env <stage>           # 環境変数一覧（マスク付き）
fleet env <stage> --reveal  # 値を表示
fleet validate --secrets    # op://参照の有効性チェック
```

## 実装タスク

- [ ] `fleetflow-secrets`クレート作成
- [ ] `OnePasswordCli`実装（`op read`連携）
- [ ] `SecretResolver`実装
- [ ] `fleet env`コマンド実装
- [ ] `fleet validate --secrets`実装
- [ ] `fleet up`でのコンテナ起動時統合

## 関連ドキュメント

- 仕様書: `spec/17-1password-integration.md`
- 設計書: `design/12-1password-integration.md`

## 依存関係

- 1Password CLI (`op`) バージョン 2.x 以上
