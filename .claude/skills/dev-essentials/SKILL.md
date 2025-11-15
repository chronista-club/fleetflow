---
title: Dev Essentials Skills
description: 開発ツールの効果的な使用方法をまとめたスキルセット
version: 1.1.0
author: Vantage Hub Contributors
created_at: 2025-11-01
updated_at: 2025-11-16
tags:
  - dev-essentials
  - development-tools
  - mise
  - chrome-devtools
  - rust-cli-tools
  - mcp
  - automation
  - testing
  - e2e-testing
categories:
  - skill
  - development-tools
  - automation
---

# Dev Essentials スキル

開発ツールの効果的な使用方法とベストプラクティスをまとめたスキルセットです。

## 概要

このスキルは、Nexusプロジェクトで使用する主要な開発ツールについて、実践的な使い方とベストプラクティスを提供します。

## ディレクトリ構造

```
dev-essentials/
├── SKILL.md                          # このファイル
├── reference/                        # 詳細なリファレンスドキュメント
│   ├── mise-reference.md            # mise完全ガイド
│   ├── chrome-devtools-mcp-reference.md  # Chrome MCP完全ガイド
│   └── rust-cli-tools.md            # Rust製CLIツール完全ガイド
└── examples/                         # 実践例とサンプル
    ├── chrome-mcp-dashboard-test.md # E2Eテストの実例
    └── mise-config.toml            # mise設定ファイルの例
```

## 含まれるツール

### 1. mise - 開発環境管理ツール

プロジェクトごとに開発ツールのバージョンを管理し、自動的に切り替えを行います。

**主な機能:**

- プロジェクトごとのツールバージョン管理
- 自動的なバージョン切り替え
- タスクランナー機能
- 環境変数管理

**クイックスタート:**

```bash
# ツールをインストール
mise install

# 開発サーバーを起動
mise run dev

# テストを実行
mise run test
```

→ 詳細は [mise リファレンス](reference/mise-reference.md) を参照

### 2. Chrome DevTools MCP サーバー

ブラウザの自動操作とE2Eテストを可能にするMCPサーバーです。

**主なツール:**

- `new_page` - ページを開く
- `take_snapshot` - DOM構造を取得
- `click` - 要素をクリック
- `take_screenshot` - 画面キャプチャ
- `navigate_page` - ページ遷移

**基本的な使い方:**

```json
// ページを開く
mcp__chrome-devtools__new_page
{ "url": "http://localhost:8080" }

// 要素を確認
mcp__chrome-devtools__take_snapshot

// クリック
mcp__chrome-devtools__click
{ "uid": "element_uid" }
```

→ 詳細は [Chrome DevTools MCP リファレンス](reference/chrome-devtools-mcp-reference.md) を参照

### 3. Rust製CLIツール

従来のUnixツールの高速な代替ツール群。Rust実装による高パフォーマンスと使いやすさが特徴。

**主なツール:**

- `lsd` - `ls`の代替（カラフル表示、アイコン、ツリー表示）
- `bat` - `cat`の代替（シンタックスハイライト）
- `ripgrep (rg)` - `grep`の代替（高速検索）
- `fd` - `find`の代替（シンプルで高速）
- `zoxide` - `cd`の改善（スマートなディレクトリ移動）

**クイックスタート:**

```bash
# ファイル一覧（カラフル＋アイコン）
lsd -la

# ツリー表示
lsd --tree --depth 2

# コード表示（シンタックスハイライト）
bat src/main.rs

# 高速検索
rg "pattern" .

# ファイル検索
fd "*.rs"
```

→ 詳細は [Rust CLI Tools リファレンス](reference/rust-cli-tools.md) を参照

## 実践例

- [Webダッシュボードのテスト例](examples/chrome-mcp-dashboard-test.md)
- [mise設定ファイルの例](examples/mise-config.toml)

## いつ使うか

### mise

- ✅ 新しいプロジェクトをセットアップするとき
- ✅ チームで同じツールバージョンを使いたいとき
- ✅ CI/CD環境を構築するとき
- ✅ 複数プロジェクトで異なるバージョンを使うとき

### Chrome DevTools MCP

- ✅ WebUIの動作を確認したいとき
- ✅ E2Eテストを実施したいとき
- ✅ ブラウザでの問題を調査したいとき
- ✅ リリース前の手動テストを効率化したいとき

### Rust CLI Tools

- ✅ コードベースを素早く確認したいとき
- ✅ 大規模なファイル検索を高速に実行したいとき
- ✅ ターミナル作業の効率を上げたいとき
- ✅ より見やすい出力が欲しいとき

## トラブルシューティング

### よくある問題

**mise:** ツールバージョンが切り替わらない

```bash
mise doctor  # 診断を実行
eval "$(mise activate bash)"  # シェル統合を再実行
```

**Chrome MCP:** 要素が見つからない

- ページの読み込み完了を待つ
- 複数回スナップショットを取る
- 動的に生成される要素に注意

**lsd:** アイコンが表示されない

- Nerd Fontをインストール
- ターミナルのフォント設定を変更
- または `lsd --icon never` でアイコンを無効化

## 参考資料

- [mise公式ドキュメント](https://mise.jdx.dev/)
- [Chrome DevTools Protocol](https://chromedevtools.github.io/devtools-protocol/)
- [MCP仕様](https://modelcontextprotocol.io/)
- [lsd GitHub](https://github.com/lsd-rs/lsd)
- [Modern Unix Tools](https://github.com/ibraheemdev/modern-unix)
