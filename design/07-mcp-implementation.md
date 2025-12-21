# Design: FleetFlow MCP サーバー実装

## 1. 構成案

FleetFlow 本体のバイナリに MCP サーバー機能を統合します。

- **実行コマンド**: `fleetflow mcp`
- **通信プロトコル**: JSON-RPC over Standard I/O (MCP 標準)
- **ライブラリ**: `serde_json`, `tokio` (既存の依存関係を活用)

## 2. 内部構造

`crates/fleetflow-mcp` クレートを新設し、以下の責務を持たせます。

### メッセージループ
Stdio から JSON-RPC リクエストを読み取り、適切なツールハンドラに振り分けます。

### ツールハンドラ
各ツール（`up`, `status` 等）の実行には、既存のクレートを再利用します。
- `fleetflow-atom`: 設定ファイルの解析・バリデーション。
- `fleetflow-container`: Docker 操作の実装。
- `fleetflow-build`: イメージビルド。

## 3. 実装のポイント

### セキュリティ
AI が予期せぬコマンドを実行しないよう、MCP サーバー経由での操作は FleetFlow が提供するツールセット（`fleetflow_*`）に限定します。任意のシェルコマンド実行は許可しません。

### コンテキストの共有
`fleetflow_inspect_project` は、単に `flow.kdl` を返すだけでなく、AI が理解しやすい構造化されたサマリー（YAML または JSON）を返します。

### エラーハンドリング
Docker 接続エラーやパースエラーが発生した場合、AI が「何が起きたか」を正しく理解し、ユーザーに報告または自ら修正できるように、詳細なエラーメッセージを JSON-RPC レスポンスに含めます。

## 4. 拡張手順

1. `crates/fleetflow-mcp` クレートを作成。
2. `Cargo.toml` の workspace members に追加。
3. `fleetflow` CLI に `mcp` サブコマンドを追加。
4. Stdio 経由の JSON-RPC 通信の基本骨格を実装。
5. 各ツールを順次実装。
