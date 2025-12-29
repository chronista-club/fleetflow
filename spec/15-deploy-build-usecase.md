# FleetFlow 仕様設計 v2（ユースケースベース）

作成日: 2024-12-24

## 操作元

| 操作元 | 用途 |
|--------|------|
| Mac（ターミナル） | 手動ビルド・デプロイ |
| GitHub CI | 自動デプロイ（将来） |

## ステージ構成

| ステージ | 場所 | 稼働 | イメージ |
|----------|------|------|----------|
| `local` | Mac | 手元開発 | ローカルビルド（pushしない） |
| `dev` | クラウド | 常時 | `myapp-dev` |
| `pre` | クラウド | オンデマンド | `myapp-pre` |
| `live` | クラウド | 常時 | `myapp-live` |

### local vs dev の区別

- `local`: Mac上で起動する手元の開発環境
- `dev`: クラウド上の開発サーバー（常時稼働）

混同防止のため命名を明確に分離。

## ビルド要件

- **場所**: Mac（ローカル）
- **クロスコンパイル**: arm64 → linux/amd64（local以外）
- **キャッシュ**: buildx活用で高速化
- **タグ**: `latest`（CI移行時に再検討）
- **push先**: ghcr.io（クラウド向けのみ）
- **イメージ分離**: リポジトリ名で環境を区別
  - `ghcr.io/{owner}/{project}-live`
  - `ghcr.io/{owner}/{project}-pre`
  - `ghcr.io/{owner}/{project}-dev`

### ビルドコマンドオプション

| オプション | 説明 |
|-----------|------|
| `-n, --service <SERVICE>` | ビルド対象のサービスを指定（省略時は全サービス） |
| `--push` | ビルド後にレジストリにプッシュ |
| `--tag <TAG>` | イメージタグを指定（--pushと併用） |
| `--registry <REGISTRY>` | レジストリURL（例: `ghcr.io/owner`） |
| `--platform <PLATFORM>` | ターゲットプラットフォーム（例: `linux/amd64`） |
| `--no-cache` | キャッシュを使用しない |

### ステージ別の挙動

| ステージ | プラットフォーム | buildx | push |
|----------|-----------------|--------|------|
| `local` | ネイティブ（arm64） | 使用しない | しない |
| `dev`, `pre`, `live` | linux/amd64 | 使用する | `--push`で指定 |

### ビルド使用例

```bash
# ローカル開発用ビルド（ネイティブ、pushなし）
fleet build local

# dev環境用: ghcr.ioにpush（linux/amd64）
fleet build dev --registry ghcr.io/myorg --push

# live環境用: 特定サービスのみビルド＆push
fleet build live --registry ghcr.io/myorg --push --service api

# プラットフォーム明示（複数アーキテクチャ対応時）
fleet build live --registry ghcr.io/myorg --platform linux/arm64 --push
```

## デプロイ要件

### 動作順序

1. **stop** - 既存コンテナを停止
2. **rm** - コンテナを削除（ポート解放）
3. **up** - 新しいコンテナを起動

ポート競合を回避するため、この順序を厳守。

### オプション

- `-n, --service <SERVICE>`: デプロイ対象のサービスを指定（省略時は全サービス）
- `--no-pull`: イメージのpullをスキップ（デフォルトは常にpull）
- `-y, --yes`: 確認なしで実行

### デフォルト動作

**イメージは常に最新をpull**する。リポジトリに更新があれば差分のみダウンロードされる。
ローカルキャッシュを使いたい場合は `--no-pull` を指定。

### 使用例

```bash
# 全サービスをデプロイ（最新イメージを自動pull）
fleet deploy live --yes

# 特定サービスのみデプロイ
fleet deploy live --service db --yes

# ローカルイメージを使用（pullスキップ）
fleet deploy live --yes --no-pull
```

## 未実装・検討事項

1. **クラウドへの接続方法** (SSH / Docker context)
2. **GitHub CI対応**

## 関連ドキュメント

- [spec/03-cli-commands.md](03-cli-commands.md) - CLIコマンド仕様
- [spec/07-docker-build.md](07-docker-build.md) - Dockerビルド仕様
