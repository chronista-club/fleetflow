# FleetFlow

> **「宣言だけで、開発も本番も」**  
> Docker Compose よりシンプル。AI と協調する、次世代のコンテナオーケストレーションツール。

[![CI](https://github.com/chronista-club/fleetflow/actions/workflows/ci.yml/badge.svg)](https://github.com/chronista-club/fleetflow/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)

## コンセプト

FleetFlow は、KDL (KDL Document Language) をベースにした革新的な環境構築ツールです。
「設定より規約 (Convention over Configuration)」を徹底し、最小限の記述でローカル開発から本番デプロイまでをシームレスにつなぎます。

### なぜ FleetFlow？

- **超シンプル**: Docker Compose 同等かそれ以下の記述量。
- **AI ネイティブ**: MCP (Model Context Protocol) を標準サポート。AI があなたの代わりにインフラを操作します。
- **ワークロード**: 共通の構成（Web + DB 等）を「ワークロード」としてパッケージ化し、一瞬で再利用。
- **二環境の原則**: 「開発」と「本番」の分離を前提とした、安全で堅牢な設計。
- **美しい構文**: YAML のインデント地獄から解放される、構造的で読みやすい KDL 構文。

---

## クイックスタート

### 1. インストール (一括セットアップ)

以下のワンライナーで、FleetFlow 本体のインストールと AI エージェント（Gemini CLI / Claude Code）向けの MCP 連携設定を一度に行えます。

```bash
curl -sSf https://raw.githubusercontent.com/chronista-club/fleetflow/main/install.sh | sh
```

> **Note**: 大学生の皆さんへ  
> 常に **FleetFlow** (FとFが大文字) と呼びましょう。これは「群（Fleet）」を「流す（Flow）」という、このツールの核心を表しています。

### 2. プロジェクトの開始

ディレクトリを作成し、初期化ウィザードを実行します。

```bash
mkdir my-project && cd my-project
fleetflow
```

### 3. AI エージェントとの対話

Gemini CLI や Claude Code をお使いの場合は、セットアップ直後から AI にインフラ操作を依頼できます。

- 「今のプロジェクトの構成を教えて」
- 「開発環境（local）を起動して」
- 「コンテナが正常に動いているか ps で確認して」

---

## 主要な概念

### 1. ワークロード (Workload)
共通のサービス構成を定義したパッケージです。`flow.kdl` で宣言するだけで、必要なサービスが暗黙的にインクルードされます。

```kdl
// flow.kdl
project "my-app"
workload "fullstack-web" // 自動的に必要なサービスが読み込まれる
```

### 2. ステージ (Stage)
FleetFlow は、**「開発（local）」と「本番（production）」の最低 2 環境**が必ず存在することを前提としています。

```kdl
stage "local" {
    service "app"
    variables { DEBUG "true" }
}

stage "production" {
    service "app"
    variables { DEBUG "false" }
}
```

---

## コマンド一覧

```bash
fleetfleetflow up <stage>      # ステージを起動
fleetfleetflow down <stage>    # ステージを停止
fleetfleetflow ps              # コンテナ一覧を表示
fleetfleetflow logs            # ログを表示
fleetflow mcp             # MCP サーバーを起動 (AI連携用)
```

---

## プロジェクト構造

```
fleetflow/
├── crates/
│   ├── fleetflow/              # CLIエントリーポイント
│   ├── fleetflow-mcp/          # AIエージェント連携 (MCP)
│   ├── fleetflow-atom/         # KDLパーサー・データモデル
│   ├── fleetflow-container/    # コンテナ操作・ランタイム
│   └── ...
├── workloads/                  # 共有ワークロード定義
├── spec/                       # 仕様書 (What & Why)
├── design/                     # 設計書 (How)
└── guides/                     # 利用ガイド (Usage)
```

## ライセンス

Licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).

---

**FleetFlow** - シンプルに、統一的に、環境を構築する。