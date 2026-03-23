# CLIコマンドリファレンス

FleetFlowのCLIコマンド一覧と詳細な使い方です。

## コマンド一覧

### Daily（日常操作）

| コマンド | 説明 |
|---------|------|
| `up` | ステージを起動 |
| `down` | ステージを停止 |
| `restart` | サービスまたはステージ全体を再起動 |
| `ps` | コンテナの一覧・状態を表示 |
| `logs` | ログ表示 |
| `exec` | コンテナ内でコマンド実行 |

### Ship（ビルド・デプロイ）

| コマンド | 説明 |
|---------|------|
| `build` | イメージをビルド |
| `deploy` | デプロイ（pull→停止→再起動） |

### Admin（Control Plane）

| コマンド | 説明 |
|---------|------|
| `cp` | Control Plane 管理（サブコマンド群） |

### Util

| コマンド | 説明 |
|---------|------|
| `mcp` | MCPサーバーを起動 |
| `self-update` | FleetFlow自体を更新 |
| `--version` | バージョン表示（フラグ） |

## 環境変数

| 変数 | 説明 |
|------|------|
| `FLEET_STAGE` | ステージ名を指定（local, dev, pre, live） |
| `FLEETFLOW_CONFIG_PATH` | 設定ファイルの直接パス指定 |
| `CLOUDFLARE_API_TOKEN` | Cloudflare APIトークン（DNS自動管理用） |
| `CLOUDFLARE_ZONE_ID` | Cloudflare Zone ID（DNS自動管理用） |
| `CLOUDFLARE_DOMAIN` | 管理対象ドメイン |

## 詳細

### `fleet up`

指定したステージのコンテナを起動します。

```bash
fleet up [stage]
fleet up local
fleet up local --pull           # イメージを事前にpull
fleet up local --dry-run        # 実行せず計画のみ表示（設定検証にも使える）
```

**オプション**:

| オプション | 短縮 | 説明 |
|-----------|------|------|
| `[stage]` | | 位置引数でステージ名を指定 |
| `--stage` | `-s` | フラグでステージ名を指定（`FLEET_STAGE`でも可） |
| `--pull` | `-p` | 起動前にイメージをpull |
| `--dry-run` | | 実行せず計画のみ表示 |

**動作**:
1. 設定ファイルを読み込み
2. イメージが無ければ自動pull
3. 依存関係順にコンテナを作成・起動
4. `wait_for`設定がある場合は依存サービスの準備を待機
5. サービスごとに進捗を表示

### `fleet down`

指定したステージのコンテナを停止・削除します。

```bash
fleet down [stage]
fleet down local
fleet down local --remove       # ボリュームも削除
```

**オプション**:

| オプション | 短縮 | 説明 |
|-----------|------|------|
| `[stage]` | | 位置引数でステージ名を指定 |
| `--stage` | `-s` | フラグでステージ名を指定 |
| `--remove` | `-r` | ボリュームも削除 |

### `fleet restart`

サービスまたはステージ全体を再起動します。

```bash
fleet restart [stage]                # ステージ全体
fleet restart -s local -n web        # 特定サービスのみ
```

**オプション**:

| オプション | 短縮 | 説明 |
|-----------|------|------|
| `[stage]` | | 位置引数でステージ名を指定 |
| `--stage` | `-s` | フラグでステージ名を指定 |
| `--service` | `-n` | サービス名（省略時はステージ全体） |

### `fleet ps`

コンテナの一覧・状態を表示します。

```bash
fleet ps                 # 実行中のコンテナのみ
fleet ps -s local        # 特定ステージのみ
fleet ps --all           # 停止中も含む
fleet ps --global        # CP 横断: 全プロジェクト・全ステージ
fleet ps --project myapp # CP 横断: 特定プロジェクト
```

**オプション**:

| オプション | 短縮 | 説明 |
|-----------|------|------|
| `[stage]` | | 位置引数でステージ名を指定 |
| `--stage` | `-s` | フラグでステージ名を指定 |
| `--all` | `-a` | 停止中のコンテナも表示 |
| `--global` | | CP 横断: 全プロジェクト・全ステージ |
| `--project` | | CP 横断: プロジェクト名で絞り込み |

### `fleet logs`

コンテナのログを表示します。

```bash
fleet logs [stage]                # 全サービス
fleet logs -s local -n app        # 特定サービス
fleet logs -s local --follow      # リアルタイム追跡
fleet logs -s local --lines 200   # 行数指定
fleet logs -s local --since 5m    # 直近5分のログ
```

**オプション**:

| オプション | 短縮 | 説明 |
|-----------|------|------|
| `[stage]` | | 位置引数でステージ名を指定 |
| `--stage` | `-s` | フラグでステージ名を指定 |
| `--service` | `-n` | サービス名（複数指定可） |
| `--follow` | `-f` | リアルタイムで追従 |
| `--lines` | `-l` | 表示する行数（デフォルト: 100） |
| `--since` | | 指定時間以降のログ（例: 5m, 1h, 30s） |

### `fleet exec`

コンテナ内でコマンドを実行します。

```bash
fleet exec -n app -- npm run migrate
fleet exec -s local -n app -it -- /bin/bash
```

**オプション**:

| オプション | 短縮 | 説明 |
|-----------|------|------|
| `[stage]` | | 位置引数でステージ名を指定 |
| `--stage` | `-s` | フラグでステージ名を指定 |
| `--service` | `-n` | サービス名（必須） |
| `--interactive` | `-i` | stdin を接続 |
| `--tty` | `-t` | 擬似 TTY を割り当て |

### `fleet build`

イメージをビルドします（コンテナは起動しない）。

```bash
fleet build [stage]                              # ステージ内の全サービス
fleet build -s local -n api                      # 特定サービスのみ
fleet build -s local -n api --push               # ビルド + レジストリにプッシュ
fleet build -s local -n api --push --tag v1.0.0  # タグ指定
fleet build -s local --platform linux/amd64      # クロスビルド
```

**オプション**:

| オプション | 短縮 | 説明 |
|-----------|------|------|
| `[stage]` | | 位置引数でステージ名を指定 |
| `--stage` | `-s` | フラグでステージ名を指定 |
| `--service` | `-n` | サービス名（複数指定可） |
| `--no-cache` | | キャッシュを使わずにビルド |
| `--push` | | ビルド後にレジストリへプッシュ |
| `--tag` | | イメージタグを指定（`--push`と併用） |
| `--registry` | | レジストリURL（例: ghcr.io/owner） |
| `--platform` | | ターゲットプラットフォーム（クロスビルド用） |

### `fleet deploy`

CI/CDパイプラインからの自動デプロイに最適化されたコマンドです。

```bash
fleet deploy [stage] --yes
fleet deploy -s live --yes              # 確認なしでデプロイ
fleet deploy -s live --no-pull --yes    # pullをスキップ
fleet deploy -s live --dry-run          # 実行せず計画のみ
```

**オプション**:

| オプション | 短縮 | 説明 |
|-----------|------|------|
| `[stage]` | | 位置引数でステージ名を指定 |
| `--stage` | `-s` | フラグでステージ名を指定 |
| `--service` | `-n` | サービス名（複数指定可） |
| `--yes` | `-y` | 確認なしで実行（CI向け） |
| `--no-pull` | | イメージのpullをスキップ |
| `--no-prune` | | 不要イメージの削除をスキップ |
| `--dry-run` | | 実行せず計画のみ表示 |

### `fleet cp`

Control Plane 管理のサブコマンド群。

```bash
fleet cp login [--endpoint URL]    # Auth0 Device Flow でログイン
fleet cp logout                    # ログアウト
fleet cp auth                      # 認証状態を表示

fleet cp daemon start/stop/status  # デーモン管理

fleet cp tenant list/create/status # テナント管理
fleet cp project list/create/show  # プロジェクト管理
fleet cp server list/register/status/check/ping  # サーバー管理

fleet cp cost list --month 2026-03         # 月次コスト一覧
fleet cp cost summary --month 2026-03      # コスト集計

fleet cp dns list/create/delete/sync       # DNS 管理

fleet cp remote deploy --project X --stage live --server Y --command "..."
fleet cp remote history                    # デプロイ履歴

fleet cp registry list/status/sync/deploy  # Fleet Registry 管理
```

### `fleet mcp`

Model Context Protocol (MCP) サーバーを起動します。

```bash
fleet mcp
```

AI/LLMアシスタントとの連携に使用します。

### `fleet self-update`

FleetFlow自体を最新バージョンに更新します。

```bash
fleet self-update
```

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
1. カレントディレクトリに`fleet.kdl`があるか確認
2. 環境変数`FLEETFLOW_CONFIG_PATH`を確認
3. `fleet up --dry-run`で設定を検証

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
3. fleet.kdlで別のポート番号を指定

### コンテナが起動しない

**解決方法**:
1. ログを確認: `fleet logs -s <stage>` または `docker logs {container}`
2. 環境変数が正しいか確認
3. ボリュームマウントのパスを確認
4. コマンドが正しいか確認

### ビルドが失敗する

**解決方法**:
1. Dockerfileのパスが正しいか確認
2. ビルドコンテキストが正しいか確認
3. `.dockerignore`で必要なファイルが除外されていないか確認
4. `--no-cache`でキャッシュをクリアしてリビルド

### プッシュが失敗する

**解決方法**:
1. レジストリへのログインを確認: `docker login <registry>`
2. `~/.docker/config.json` に認証情報があるか確認
3. 認証情報の有効期限を確認（特にGHCR、ECRなど）
