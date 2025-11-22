# FleetFlow プロジェクトガイド

最終更新日: 2025-11-22

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
│   ├── fleetflow-cli/       # CLIエントリーポイント
│   ├── fleetflow-atom/      # 基本的なデータ構造
│   ├── fleetflow-config/    # 設定パーサーと管理
│   └── fleetflow-container/ # コンテナ操作
├── spec/                    # 仕様・設計ドキュメント（Living Documentation）
│   └── {feature}/           # 機能ごとにSPEC.md, DESIGN.md, GUIDE.md
├── .claude/                 # Claude Code設定
│   ├── CLAUDE.md            # プロジェクトガイド（このファイル）
│   ├── bollard-guide.md     # bollard使用ガイド
│   ├── ports.md             # ポート設定ガイド
│   └── skills/              # インストール済みスキル
│       ├── code-flow/           # 開発フロー管理
│       ├── spec-design-guide/   # 仕様・設計ドキュメント作成
│       ├── document-skills/     # ドキュメント管理
│       ├── dev-essentials/      # 開発ツール
│       ├── github/              # GitHub操作
│       ├── mcp-builder/         # MCP開発
│       ├── physical-event/      # 物理デバイスイベント
│       ├── qdrant/              # ベクトルDB
│       ├── surrealdb/           # SurrealDB
│       └── sycamore/            # Sycamoreフレームワーク
├── docs/                    # 公式ドキュメント
│   └── design/              # 設計文書（OrbStack連携など）
└── README.md                # プロジェクト説明
```

## 開発ワークフロー

### Code Flow - 5フェーズ開発プロセス

FleetFlowでは、`code-flow`スキルによる体系的な開発フローを採用しています：

```
Phase 1: Brain相談 → Phase 2: ヒアリング → Phase 3: SDG → Phase 4: 実装 → Phase 5: 学習
```

#### Phase 1 & 2: 要件明確化
- ユーザーリクエストから開発パターンを推奨
- ヒアリングで仕様を詳細化

#### Phase 3: SDG（Spec-Design-Guide）
仕様・設計ドキュメントを`spec/`ディレクトリに作成：

```
spec/{feature-name}/
├── SPEC.md      # What & Why（コンセプト・仕様・哲学）
├── DESIGN.md    # How（モデル・手法・実装）
└── GUIDE.md     # Usage（使い方・ベストプラクティス）
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
- 成功/失敗からの学習

### スキル活用ガイド

#### いつ使うか

**spec-design-guide（SDG）**：
- ✅ 新機能の設計・実装
- ✅ 既存機能のリファクタリング
- ✅ バグ修正で設計に影響がある場合
- ✅ `spec/`ディレクトリ操作時

**document-skills**：
- ✅ README、ガイドなどの公式文書作成
- ✅ APIドキュメント、アーキテクチャ図
- ✅ ユーザー向けドキュメント

**code-flow**：
- ✅ 複雑な機能開発の開始時
- ✅ 要件が不明確な場合

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
- ライブラリ内: `Result<T, E>`を使用し、エラーを適切に伝播
- `thiserror`を使用してカスタムエラー型を定義
- CLIレベル: `anyhow`で包括的なエラー処理

### 2. 設計原則

#### シンプルさを最優先
- 過度な抽象化を避ける
- YAGNIの原則に従う（You Ain't Gonna Need It）
- 明確な責任分離（Separation of Concerns）

#### モジュール設計
- 各クレートは単一の責任を持つ
- 依存関係は一方向に保つ
- 公開APIは最小限に

#### パフォーマンス
- 起動時間を最小化
- メモリ使用量を抑える
- 非同期処理を適切に活用

### 3. テスト戦略

#### 単体テスト
- 各モジュールに対応するテストを作成
- `#[cfg(test)]`モジュールを活用
- エッジケースを網羅

#### 統合テスト
- `tests/`ディレクトリに配置
- 実際のDocker操作を含むテスト
- `tempfile`を使用した一時ファイル管理

#### テストコマンド
```bash
cargo test              # 全テスト実行
cargo test --lib        # ライブラリテストのみ
cargo test --doc        # docテストのみ
cargo clippy            # リント
cargo fmt               # フォーマット
```

## KDL設定ファイル仕様

### 基本構造

```kdl
// プロジェクト名宣言（必須）
project "my-project"

// ステージ定義
stage "local" {
    service "web"
    service "db"
}

// サービス詳細定義
service "web" {
    image "node:20-alpine"
    ports {
        port host=3000 container=3000
    }
    env {
        NODE_ENV "development"
    }
}
```

### 重要な概念

#### 1. プロジェクト名
- すべての設定ファイルで`project`ノードを最初に宣言
- コンテナ命名規則: `{project}-{stage}-{service}`

#### 2. ステージ（環境）
- `local`, `dev`, `staging`, `prod`など
- OrbStackグループ化のキー: `{project}-{stage}`

#### 3. サービス
- Docker Composeの`service`に相当
- 各サービスはステージ間で共通定義可能

## OrbStack連携

### 推奨利用環境
このツールは**主にローカル開発環境（macOS）**での利用を想定しています。

### コンテナ命名とラベル

#### 命名規則
```
{project}-{stage}-{service}
```

例: `vantage-local-surrealdb`

#### Dockerラベル
自動的に以下のラベルが付与されます：

| ラベル名 | 値 | 用途 |
|---------|-----|------|
| `com.docker.compose.project` | `{project}-{stage}` | OrbStackグループ化 |
| `com.docker.compose.service` | `{service}` | サービス識別 |
| `fleetflow.project` | プロジェクト名 | メタデータ |
| `fleetflow.stage` | ステージ名 | メタデータ |
| `fleetflow.service` | サービス名 | メタデータ |

詳細: [OrbStack連携設計書](../docs/design/orbstack-integration.md)

## 開発フェーズとロードマップ

### Phase 1: MVP (現在)
- [x] プロジェクト初期化
- [x] 基本的なクレート構造
- [x] OrbStack連携機能
- [x] プロジェクト名とステージ名を含む命名規則
- [ ] KDLパーサーの完全実装
- [ ] 基本的なCLIコマンド（up/down/validate）

### Phase 2: 拡張機能（次のステップ）
- [ ] 環境変数の参照
- [ ] 変数定義と展開
- [ ] 環境継承（include-from）
- [ ] グロブパターンによるinclude

### Phase 3: 独自実行エンジン
- [ ] Docker API直接利用の最適化
- [ ] リアルタイムログストリーミング
- [ ] ヘルスチェック機能

## bollardガイド

bollard（Rust Docker API client）の使用方法については、[bollard-guide.md](bollard-guide.md)を参照してください。

### 重要なポイント
- 非同期処理を前提とした設計
- エラーハンドリングの適切な実装
- リソースのクリーンアップを忘れない

## Claude Codeでの開発

### 推奨ワークフロー

1. **実装前の準備**
   - **既存コードの確認**: 関連するコードを必ず読む
   - **仕様の確認**: `spec/`ディレクトリ内のSPEC.md/DESIGN.mdを確認
   - **関連ドキュメント**: `docs/`や`.claude/`のガイドを参照

2. **新機能開発時（Code Flowの活用）**
   - **Phase 1-2**: 要件をヒアリングで明確化
   - **Phase 3**: `spec/{feature}/`にSPEC.md, DESIGN.mdを作成
   - **Phase 4**: チェックリスト駆動で実装
   - **Phase 5**: 学習とパターン記録

3. **実装時のベストプラクティス**
   - TodoWriteツールでタスク管理
   - 小さなコミットで進める
   - テストと共に実装
   - **Living Documentation**: コード変更時は必ず対応ドキュメントも更新

4. **コミットメッセージ規約**
   ```
   feat: 新機能追加
   fix: バグ修正
   refactor: リファクタリング
   docs: ドキュメント更新
   spec: 仕様・設計ドキュメント更新
   test: テスト追加・修正
   ```

### 権限設定
`.claude/settings.local.json`で`bypassPermissions`モードを使用しています。
- GitHub PR関連コマンドは自動許可
- その他のツール実行も権限確認なしで実行可能

## 参考資料

### 内部ドキュメント
- [README.md](../README.md) - プロジェクト概要
- [bollard-guide.md](bollard-guide.md) - Docker API使用ガイド
- [ports.md](ports.md) - ポート設定ガイド
- [OrbStack連携設計書](../docs/design/orbstack-integration.md)

### 外部リソース
- [KDL Document Language](https://kdl.dev/)
- [kdl-rs](https://github.com/kdl-org/kdl-rs) - KDL parser for Rust
- [bollard](https://docs.rs/bollard/) - Docker API client
- [clap](https://docs.rs/clap/) - CLI framework

### Rust関連
- [The Rust Programming Language](https://doc.rust-lang.org/book/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)

## 開発環境セットアップ

### 必須ツール
- Rust toolchain (edition 2024対応)
- Docker または OrbStack
- cargo

### 推奨ツール
- rust-analyzer (LSP)
- clippy (lint)
- rustfmt (formatter)

### セットアップコマンド
```bash
# リポジトリクローン
git clone https://github.com/chronista-club/fleetflow.git
cd fleetflow

# ビルド
cargo build

# テスト実行
cargo test

# 開発用実行
cargo run -- --help
```

## トラブルシューティング

### ビルドエラー
1. Rustのバージョン確認: `rustc --version`
2. 依存関係の更新: `cargo update`
3. クリーンビルド: `cargo clean && cargo build`

### Docker接続エラー
1. Docker/OrbStackが起動しているか確認
2. Docker socketのパーミッション確認
3. bollard-guide.mdの接続方法を参照

### テスト失敗
1. Dockerコンテナが残っていないか確認
2. ポートが使用されていないか確認
3. 一時ファイルのクリーンアップ

## コントリビューション

Issue、Pull Requestは大歓迎です！

### 貢献前のチェックリスト
- [ ] コードがフォーマットされている（`cargo fmt`）
- [ ] Lintが通る（`cargo clippy`）
- [ ] テストが通る（`cargo test`）
- [ ] 必要に応じてドキュメントを更新
- [ ] コミットメッセージが規約に従っている

---

FleetFlow - シンプルに、統一的に、環境を構築する。
