# 変更履歴

このプロジェクトの注目すべき変更はすべてこのファイルに記録されます。

フォーマットは [Keep a Changelog](https://keepachangelog.com/ja/1.0.0/) に基づいており、
このプロジェクトは [セマンティック バージョニング](https://semver.org/lang/ja/spec/v2.0.0.html) に準拠しています。

## [Unreleased]

## [0.1.0] - 2025-01-09

### 追加

- FleetFlowの初回リリース
- KDLベースの設定構文
- 基本的なCLIコマンド:
  - `fleetflow up` - ステージ内のサービスを起動
  - `fleetflow down` - ステージ内のサービスを停止
  - `fleetflow ps` - 実行中のサービスを一覧表示
  - `fleetflow logs` - サービスのログを表示
- ステージベースの環境管理（local, dev, stg, prd）
- 自動イメージ推測機能付きサービス定義
- ポートマッピング設定
- 環境変数管理
- ボリュームマウントサポート
- サービス依存関係の解決
- モジュラー設定のための自動インポート機能
- テンプレート変数サポート
- カラー出力による美しいターミナルUI
- bollardによるDockerコンテナライフサイクル管理

### 機能

- **設定より規約**: 最小限のボイラープレート、最大限の生産性
- **段階的な開示**: シンプルなことはシンプルに、複雑なことも可能に
- **宣言的構文**: やり方ではなく、何をしたいかを記述
- **KDLベース**: 美しく、人間に優しい設定フォーマット
- **自動イメージ推測**: サービス名が自動的にDockerイメージにマッピング
- **ステージ管理**: 複数の環境（local/dev/stg/prd）を簡単に管理
- **ディレクトリベースの自動インポート**: 明示的な `include` 文が不要

### ドキュメント

- クイックスタートガイド付きの包括的なREADME
- `spec/` ディレクトリ内の詳細な仕様ドキュメント
- アーキテクチャと設計ドキュメント
- MIT OR Apache-2.0のデュアルライセンス

[Unreleased]: https://github.com/chronista-club/fleetflow/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/chronista-club/fleetflow/releases/tag/v0.1.0
