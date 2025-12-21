# Spec: FleetFlow MCP サーバー

## 1. 目的 (What & Why)

FleetFlow を MCP (Model Context Protocol) サーバーとして提供することで、AI エージェント（Gemini CLI, Claude Code, Cursor 等）が自然言語を通じてインフラを直接操作できるようにします。

### なぜ必要か
- **インフラ操作の民主化**: コマンドのオプションを覚える必要がなくなり、「今の環境を本番にデプロイして」といった意図を伝えるだけで完結します。
- **自律的な問題解決**: AI が `ps` や `logs` の結果を直接取得し、エラーが発生していれば自動的に修正案を提示、実行できるようになります。
- **環境の壁の撤廃**: AI を介することで、ローカル、クラウド、CI/CD といった環境の差異を意識せずに FleetFlow を操作できます。

## 2. 提供するツール定義

MCP サーバーは、AI に対して以下の「ツール」を提供します。

### `fleetflow_inspect_project`
- **概要**: カレントディレクトリの FleetFlow プロジェクト構造を解析し、AI に教える。
- **出力**: 定義されているサービス一覧、ステージ一覧、変数定義。

### `fleetflow_up`
- **概要**: 指定したステージを起動する。
- **引数**: `stage` (string, optional)
- **効果**: `fleetfleetflow up` を実行し、コンテナを起動する。

### `fleetflow_down`
- **概要**: 指定したステージを停止・削除する。
- **引数**: `stage` (string, optional), `remove` (boolean, optional)

### `fleetflow_status`
- **概要**: 現在のコンテナの稼働状況を取得する。
- **引数**: `stage` (string, optional)
- **出力**: 実行中のコンテナ名、イメージ、ステータス、ポート情報。

### `fleetflow_get_logs`
- **概要**: コンテナのログを取得し、AI に解析させる。
- **引数**: `service` (string), `lines` (number, default: 50)

## 3. ユースケース例

1. **プロジェクト初期化**:
   - ユーザー: 「このプロジェクトを FleetFlow で動かせるようにして」
   - AI: `fleetflow_inspect_project` で現状を確認し、`flow.kdl` を生成する。

2. **デバッグと修復**:
   - AI: 「コンテナが起動に失敗しています」
   - AI: `fleetflow_get_logs` で原因を特定し、設定を修正して `fleetflow_up` を再試行する。

3. **マルチ環境管理**:
   - ユーザー: 「本番環境の状態を教えて」
   - AI: `fleetflow_status(stage="production")` を実行して報告する。
