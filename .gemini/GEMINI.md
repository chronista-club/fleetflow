# FleetFlow プロジェクトガイド (Gemini Edition)

最終更新日: 2025-12-27

## プロジェクト概要

FleetFlowは、KDL（KDL Document Language）をベースにした、革新的で超シンプルなコンテナオーケストレーション・環境構築ツールです。

### コンセプト
**「宣言だけで、開発も本番も」**

Docker Composeの手軽さはそのままに、より少ない記述で、より強力な設定管理を実現します。

### 主要な特徴
- **超シンプル**: Docker Composeと同等かそれ以下の記述量
- **可読性**: YAMLよりも読みやすいKDL構文
- **モジュール化**: include機能で設定を分割・再利用
- **統一管理**: 開発環境から本番環境まで同じツールで

## 技術スタック

### 言語とフレームワーク
- **言語**: Rust (edition 2024)
- **パーサー**: `kdl` crate
- **コンテナAPI**: `bollard` (Docker API client)
- **CLI**: `clap` (derive features)
- **非同期ランタイム**: `tokio`

### 主要な依存関係
- **設定管理**: `config`, `serde`, `serde_json`, `serde_yaml`
- **テンプレート**: `tera`
- **エラーハンドリング**: `anyhow`, `thiserror`
- **ロギング**: `tracing`, `tracing-subscriber`

## プロジェクト構造

```
fleetflow/
├── crates/
│   ├── fleetflow/              # CLIエントリーポイント
│   ├── fleetflow-core/             # KDLパーサー・データモデル
│   │   ├── src/model/              # データ構造（モジュール分割）
│   │   └── src/parser/             # パーサー（モジュール分割）
│   ├── fleetflow-config/           # 設定管理
│   ├── fleetflow-container/        # コンテナ操作
│   ├── fleetflow-build/            # Dockerビルド機能
│   ├── fleetflow-cloud/            # クラウドインフラ抽象化
│   ├── fleetflow-cloud-sakura/     # さくらクラウド連携
│   └── fleetflow-cloud-cloudflare/ # Cloudflare連携
├── spec/                           # 仕様書（What & Why）
│   ├── 01-core-concepts.md
│   ├── ...
├── design/                         # 設計書（How）
│   ├── 01-kdl-parser.md
│   ├── ...
├── guides/                         # 利用ガイド（Usage）
│   ├── ...
├── .claude/                        # Claude Code設定 (Geminiも参照)
│   ├── CLAUDE.md                   # 元のプロジェクトガイド
│   ├── ports.md                    # ポート設定ガイド
│   └── skills/                     # スキル定義 (Geminiも参照可能)
├── .gemini/                        # Gemini設定
│   └── GEMINI.md                   # プロジェクトガイド（このファイル）
├── docs/                           # 公式ドキュメント
└── README.md                       # プロジェクト説明
```

## 開発ワークフロー

### Code Flow - 5フェーズ開発プロセス

FleetFlowでは、`code-flow`スキルによる体系的な開発フローを採用しています。Geminiもこのフローに従って開発を進めます。

```
Phase 1: Brain相談 → Phase 2: ヒアリング → Phase 3: SDG → Phase 4: 実装 → Phase 5: 学習
```

#### Phase 1 & 2: 要件明確化
- ユーザーリクエストから開発パターンを推奨
- ヒアリングで仕様を詳細化

#### Phase 3: SDG（Spec-Design-Guide）
仕様・設計・ガイドのドキュメントをフラット構造で作成：

```
spec/              # What & Why（コンセプト・仕様・哲学）
design/            # How（モデル・手法・実装）
guides/            # Usage（使い方・ベストプラクティス）
```

**Living Documentation原則**：
- ドキュメントは「生きた写像」としてコードと常に同期
- コード変更時は必ず対応するドキュメントも更新
- 技術的負債を防ぎ、生きたメモリーとして機能

#### Phase 4: 実装
- チェックリスト駆動開発
- テストと共に実装
- 小さなコミットで進める

#### Phase 5: 学習
- セッション記録とパターン更新

### スキル活用ガイド (Gemini版)

Geminiは`.claude/skills/`以下のドキュメントを参照し、必要に応じてその知識を活用します。

**spec-design-guide（SDG）**：
- ✅ 新機能の設計・実装、リファクタリング時
- ✅ `spec/`, `design/`, `guides/`ディレクトリ操作時

**document-skills**：
- ✅ README、ガイドなどの公式文書作成時

**code-flow**：
- ✅ 複雑な機能開発の開始時

**bollard**：
- ✅ Docker操作の実装時（`bollard`クレートの使用方法参照）

## 開発方針

### 1. コーディング規約

#### Rust スタイル
- `rustfmt`の標準設定に従う
- `clippy`の推奨事項を遵守
- エディション: 2024を使用

#### 命名規則
- **クレート名**: `fleetflow-*` (kebab-case)
- **構造体**: PascalCase
- **関数/変数**: snake_case
- **定数**: SCREAMING_SNAKE_CASE

#### エラーハンドリング
- ライブラリ内: `Result<T, E>`を使用
- `thiserror`でカスタムエラー型定義
- CLIレベル: `anyhow`

### 2. 設計原則
- **シンプルさを最優先**: YAGNI
- **モジュール設計**: 単一責任、一方向依存
- **パフォーマンス**: 起動時間最小化

### 3. テスト戦略
- **単体テスト**: `#[cfg(test)]`
- **統合テスト**: `tests/`ディレクトリ
- **コマンド**:
    ```bash
    cargo test              # 全テスト
    cargo clippy            # リント
    cargo fmt               # フォーマット
    ```

## KDL設定ファイル仕様 & OrbStack連携

詳細は`spec/`, `design/`および`.claude/CLAUDE.md`を参照してください。
基本的な構造（`project`, `stage`, `service`）や命名規則（`{project}-{stage}-{service}`）は厳守します。

## Geminiでの開発推奨ワークフロー

1.  **コミュニケーションと言語**:
    - ユーザーとの対話、および思考プロセス（Thought）は原則として**日本語**で行います。
    - 思考プロセスをユーザーと共有し、意図を明確にしながら開発を進めます。

2.  **名称・環境変数の規約 (確定)**:
    - **正式名称**: `FleetFlow` (表記ゆれを防ぐため、FとFを大文字にする)
    - **CLIコマンド名**: `flow` (シンプルで入力しやすい)
    - **プロジェクト名（コード内）**: `fleetflow` (crate名、ディレクトリ名)
    - **環境変数プレフィックス**: `FLEETFLOW_`
    - 例: `FLEET_STAGE`, `FLEETFLOW_PROJECT_ROOT`, `FLEETFLOW_CONFIG_PATH`

3.  **CLIコマンド例**:
    ```bash
    flow up <stage>       # ステージを起動
    flow down <stage>     # ステージを停止
    flow setup <stage>    # インフラを構築（冪等）
    flow build <stage>    # Dockerイメージをビルド
    flow deploy <stage>   # デプロイ
    flow ps               # コンテナ一覧
    flow logs             # ログ表示
    ```

4.  **コンテキスト把握**:
    - `GEMINI.md` (本ファイル) および `.claude/CLAUDE.md` を確認。
    - `spec/`, `design/` で仕様・設計を確認。

5.  **新機能・修正**:
    - ユーザーとの対話で要件を明確化 (Code Flow Phase 1-2)。
    - 必要に応じて `spec/`, `design/` を更新 (Code Flow Phase 3)。
    - 実装とテスト (Code Flow Phase 4)。
    - **Living Documentation**: コード変更に合わせてドキュメントも更新。

6.  **コミットメッセージ**:
    - Conventional Commits (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `spec:`) を使用。

## 開発環境セットアップ

```bash
git clone https://github.com/chronista-club/fleetflow.git
cd fleetflow
cargo build
cargo test
```
