# CLIコマンドリファレンス

FleetFlowのCLIコマンド一覧と詳細な使い方です。

## コマンド一覧

| コマンド | 説明 |
|---------|------|
| `up` | ステージを起動 |
| `down` | ステージを停止・削除 |
| `ps` | コンテナ一覧 |
| `logs` | ログ表示 |
| `start` | 停止中のサービスを起動 |
| `stop` | サービスを停止 |
| `restart` | サービスを再起動 |
| `build` | イメージをビルド |
| `rebuild` | イメージを再ビルドして再起動 |
| `validate` | 設定を検証 |
| `cloud` | クラウドインフラ管理 |
| `version` | バージョン表示 |

## 詳細

### `flow up`

指定したステージのコンテナを起動します。

```bash
flow up <stage>
flow up local
flow up --build local      # ビルドしてから起動
flow up --build --no-cache local  # キャッシュなしでビルド
```

**オプション**:

| オプション | 説明 |
|-----------|------|
| `--build` | 起動前にイメージをビルド |
| `--no-cache` | キャッシュを使わずにビルド（`--build`と併用） |

**動作**:
1. 設定ファイルを読み込み
2. `--build`指定時はイメージをビルド
3. イメージが無ければ自動pull
4. コンテナが存在しなければ作成
5. コンテナを起動
6. サービスごとに進捗を表示

### `flow down`

指定したステージのコンテナを停止・削除します。

```bash
flow down <stage>
flow down local
```

**動作**:
1. コンテナを停止
2. コンテナを削除
3. ボリュームは削除しない（データ保持）

### `flow ps`

コンテナの状態を表示します。

```bash
flow ps            # 実行中のコンテナのみ
flow ps --all      # 停止中も含む
```

**表示内容**:
- コンテナ名
- 状態（Running/Stopped）
- ポートマッピング

### `flow logs`

コンテナのログを表示します。

```bash
flow logs                    # 全サービス
flow logs [service]          # 特定サービス
flow logs --follow           # リアルタイム表示
flow logs --lines 100        # 行数指定
flow logs -f -n 50 web       # 組み合わせ
```

**オプション**:

| オプション | 短縮 | 説明 |
|-----------|------|------|
| `--follow` | `-f` | リアルタイムで追従 |
| `--lines` | `-n` | 表示する行数（デフォルト: 100） |

### `flow start`

停止中のサービスを起動します（コンテナは既に存在している場合）。

```bash
flow start <stage>           # ステージ内の全サービス
flow start <stage> [service] # 特定サービスのみ
flow start local db
```

**動作**:
- `docker start` 相当
- コンテナが存在しない場合はエラー

### `flow stop`

サービスを停止します（コンテナは保持）。

```bash
flow stop <stage>            # ステージ内の全サービス
flow stop <stage> [service]  # 特定サービスのみ
flow stop local db
```

**動作**:
- `docker stop` 相当
- コンテナは削除されない
- `start` で再起動可能

### `flow restart`

サービスを再起動します。

```bash
flow restart <stage>           # ステージ内の全サービス
flow restart <stage> [service] # 特定サービスのみ
flow restart local web
```

**動作**:
- `docker restart` 相当
- 停止 → 起動を実行

### `flow build`

イメージをビルドします（コンテナは起動しない）。

```bash
flow build <stage>                 # ステージ内の全サービス
flow build <stage> -n <service>    # 特定サービスのみ
flow build local -n api
flow build local --no-cache        # キャッシュなしでビルド

# レジストリにプッシュ
flow build local -n api --push
flow build local -n api --push --tag v1.0.0
```

**オプション**:

| オプション | 説明 |
|-----------|------|
| `--no-cache` | キャッシュを使わずにビルド |
| `--push` | ビルド後にレジストリへプッシュ |
| `--tag <tag>` | イメージタグを指定（`--push`と併用） |

**プッシュ時の認証**:

Docker標準の認証方式を使用：
- `~/.docker/config.json` から認証情報を取得
- credential helper（osxkeychain, desktop）も自動対応
- 環境変数 `DOCKER_CONFIG` でパスをカスタマイズ可能

**タグ解決の優先順位**:
1. `--tag` CLIオプション
2. KDL設定の `image` フィールドのタグ
3. デフォルト: `latest`

### `flow rebuild`

イメージを再ビルドしてコンテナを再起動します。

```bash
flow rebuild <service>           # サービスをリビルド
flow rebuild <service> [stage]   # ステージを指定
flow rebuild api local
flow rebuild api --no-cache      # キャッシュなしでリビルド
```

**動作**:
1. 既存のコンテナを停止（実行中の場合）
2. イメージをリビルド
3. コンテナを再作成・起動

### `flow validate`

設定ファイルの構文チェックを行います。

```bash
flow validate
```

**チェック内容**:
- KDL構文エラー
- 必須フィールドの欠落
- 論理的な矛盾

### `flow cloud`

クラウドインフラを管理します。

```bash
# クラウド環境を構築
flow cloud up --stage <stage>
flow cloud up --stage dev --yes  # 確認をスキップ

# クラウド環境を削除
flow cloud down --stage <stage>
flow cloud down --stage dev --yes

# 差分を確認（dry-run）
flow cloud plan --stage <stage>

# DNS管理（オプション）
flow cloud dns list
flow cloud dns add --subdomain api-prod --ip 203.0.113.1
flow cloud dns remove --subdomain api-prod
```

**サブコマンド**:

| サブコマンド | 説明 |
|-------------|------|
| `up` | クラウド環境を構築（サーバー作成 + DNS設定） |
| `down` | クラウド環境を削除（サーバー削除 + DNS削除） |
| `plan` | 差分を確認（dry-run） |
| `dns list` | DNSレコード一覧 |
| `dns add` | DNSレコード追加 |
| `dns remove` | DNSレコード削除 |

**オプション**:

| オプション | 説明 |
|-----------|------|
| `--stage` | 対象のステージ名（必須） |
| `--yes` | 確認をスキップ |

**DNS自動管理**:

環境変数が設定されている場合、`cloud up`/`cloud down`時にDNSレコードを自動管理：

| 環境変数 | 説明 |
|---------|------|
| `CLOUDFLARE_API_TOKEN` | Cloudflare APIトークン |
| `CLOUDFLARE_ZONE_ID` | ドメインのZone ID |

### `flow version`

バージョン情報を表示します。

```bash
flow version
# 出力: flow 0.4.2
```

## 環境変数

| 変数 | 説明 |
|------|------|
| `FLEETFLOW_CONFIG_PATH` | 設定ファイルの直接パス指定 |
| `CLOUDFLARE_API_TOKEN` | Cloudflare APIトークン（DNS自動管理用） |
| `CLOUDFLARE_ZONE_ID` | Cloudflare Zone ID（DNS自動管理用） |

## 終了コード

| コード | 説明 |
|--------|------|
| 0 | 成功 |
| 1 | 一般エラー |
| 2 | 設定エラー |

## トラブルシューティング

### 設定ファイルが見つからない

```
エラー: Flow設定ファイルが見つかりません
```

**解決方法**:
1. カレントディレクトリに`flow.kdl`があるか確認
2. 環境変数`FLEETFLOW_CONFIG_PATH`を確認
3. `flow validate`で検証

### イメージが見つからない

```
エラー: イメージが見つかりません: xxx:yyy
```

**解決方法**:
1. イメージ名とタグが正しいか確認
2. 手動でpull: `docker pull image:tag`
3. プライベートレジストリの認証を確認

### ポートが使用中

```
エラー: ポート xxxx は既に使用されています
```

**解決方法**:
1. 他のコンテナを確認: `docker ps`
2. ホストのプロセスを確認: `lsof -i :xxxx`
3. flow.kdlで別のポート番号を指定

### コンテナが起動しない

**解決方法**:
1. ログを確認: `flow logs` または `docker logs {container}`
2. 環境変数が正しいか確認
3. ボリュームマウントのパスを確認
4. コマンドが正しいか確認

### ビルドが失敗する

```
エラー: ビルドに失敗しました
```

**解決方法**:
1. Dockerfileのパスが正しいか確認
2. ビルドコンテキストが正しいか確認
3. `.dockerignore`で必要なファイルが除外されていないか確認
4. `--no-cache`でキャッシュをクリアしてリビルド

### プッシュが失敗する

```
エラー: プッシュに失敗しました
```

**解決方法**:
1. レジストリへのログインを確認: `docker login <registry>`
2. `~/.docker/config.json` に認証情報があるか確認
3. 認証情報の有効期限を確認（特にGHCR、ECRなど）
4. イメージ名がレジストリの形式に合っているか確認:
   - GHCR: `ghcr.io/owner/image:tag`
   - Docker Hub: `username/image:tag`
   - ECR: `123456789.dkr.ecr.region.amazonaws.com/image:tag`
