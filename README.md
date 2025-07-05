# unison-flow

Claude code/Linear統合を中心とした、モダンな開発ワークフロー管理ツール
イシュー駆動開発をベースとし、シンプルで効率的なワークフローを実現します。

## 特徴

- **Linear First**: すべての作業はLinear issueから始まる
- **自動化**: ブランチ作成からPRまで、繰り返し作業を自動化
- **シンプル**: 必要最小限のコマンドで完結
- **追跡可能**: すべての変更がissueに紐付けられる

## コンセプト

### Single Source of Truth
- Linear Issue IDが全ての起点
- ブランチ名、PR、コミットメッセージが自動的に整合

### Workflow as Code
- 設定ファイルでチームのワークフローを定義
- プロジェクトごとにカスタマイズ可能

### Developer Experience
- 直感的なCLI
- エラーメッセージが親切
- 状態の可視化

## インストール

```bash
cargo install unison-flow
```

## 基本的な使い方

```bash
# 新しいタスクを開始
unison-flow start UNI-123

# 現在の状態を確認
unison-flow status

# PRを作成
unison-flow pr

# タスクを完了
unison-flow finish
```

## アーキテクチャ

unison-flowは以下のコンポーネントで構成されています：

1. **CLI**: ユーザーインターフェース
2. **Linear Client**: Linear APIとの通信
3. **Git Manager**: Git操作の抽象化
4. **Config Manager**: 設定とカスタマイズ
5. **State Machine**: ワークフローの状態管理

## 開発

```bash
# 開発環境のセットアップ
cargo build

# テストの実行
cargo test

# CLIの実行
cargo run -- <command>
```

## ライセンス

Proprietary - All Rights Reserved
