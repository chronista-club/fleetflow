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
| `validate` | 設定を検証 |
| `version` | バージョン表示 |

## 詳細

### `fleetflow up`

指定したステージのコンテナを起動します。

```bash
fleetflow up <stage>
fleetflow up local
```

**動作**:
1. 設定ファイルを読み込み
2. イメージが無ければ自動pull
3. コンテナが存在しなければ作成
4. コンテナを起動
5. サービスごとに進捗を表示

### `fleetflow down`

指定したステージのコンテナを停止・削除します。

```bash
fleetflow down <stage>
fleetflow down local
```

**動作**:
1. コンテナを停止
2. コンテナを削除
3. ボリュームは削除しない（データ保持）

### `fleetflow ps`

コンテナの状態を表示します。

```bash
fleetflow ps            # 実行中のコンテナのみ
fleetflow ps --all      # 停止中も含む
```

**表示内容**:
- コンテナ名
- 状態（Running/Stopped）
- ポートマッピング

### `fleetflow logs`

コンテナのログを表示します。

```bash
fleetflow logs                    # 全サービス
fleetflow logs [service]          # 特定サービス
fleetflow logs --follow           # リアルタイム表示
fleetflow logs --lines 100        # 行数指定
fleetflow logs -f -n 50 web       # 組み合わせ
```

**オプション**:

| オプション | 短縮 | 説明 |
|-----------|------|------|
| `--follow` | `-f` | リアルタイムで追従 |
| `--lines` | `-n` | 表示する行数（デフォルト: 100） |

### `fleetflow start`

停止中のサービスを起動します（コンテナは既に存在している場合）。

```bash
fleetflow start <stage>           # ステージ内の全サービス
fleetflow start <stage> [service] # 特定サービスのみ
fleetflow start local db
```

**動作**:
- `docker start` 相当
- コンテナが存在しない場合はエラー

### `fleetflow stop`

サービスを停止します（コンテナは保持）。

```bash
fleetflow stop <stage>            # ステージ内の全サービス
fleetflow stop <stage> [service]  # 特定サービスのみ
fleetflow stop local db
```

**動作**:
- `docker stop` 相当
- コンテナは削除されない
- `start` で再起動可能

### `fleetflow restart`

サービスを再起動します。

```bash
fleetflow restart <stage>           # ステージ内の全サービス
fleetflow restart <stage> [service] # 特定サービスのみ
fleetflow restart local web
```

**動作**:
- `docker restart` 相当
- 停止 → 起動を実行

### `fleetflow validate`

設定ファイルの構文チェックを行います。

```bash
fleetflow validate
```

**チェック内容**:
- KDL構文エラー
- 必須フィールドの欠落
- 論理的な矛盾

### `fleetflow version`

バージョン情報を表示します。

```bash
fleetflow version
# 出力: fleetflow 0.2.0
```

## 環境変数

| 変数 | 説明 |
|------|------|
| `FLOW_CONFIG_PATH` | 設定ファイルの直接パス指定 |

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
2. 環境変数`FLOW_CONFIG_PATH`を確認
3. `fleetflow validate`で検証

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
1. ログを確認: `fleetflow logs` または `docker logs {container}`
2. 環境変数が正しいか確認
3. ボリュームマウントのパスを確認
4. コマンドが正しいか確認
