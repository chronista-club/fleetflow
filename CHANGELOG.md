# Changelog

FleetFlow の変更履歴。[Conventional Commits](https://www.conventionalcommits.org) に準拠。

## [0.8.0] - 2026-03-07

### Bug Fixes

- Stage logs の -s/--service を -n に統一 (#62)

### Documentation

- CONTRIBUTING.md 作成 (#82)
- Spec/design の旧コマンド名 flow → fleet 一括置換 (#75)

### Features

- -v/--verbose と -q/--quiet グローバルフラグの追加 (#93)
- Logs --since <duration> の追加 (#92)
- Service 指定の統一 — logs を Vec<String> に、stop/start にステージ全体操作を追加 (#64)
- Restart <stage> でステージ全体再起動 (#94)

### Miscellaneous

- Cargo-deny 導入（ライセンス・脆弱性チェック） (#79)
- CHANGELOG 自動生成 + git-cliff 導入 (#73)
- Dependabot 導入 (#76)
- Issue/PR テンプレート作成 (#77)

### Refactor

- Bollard deprecated API を新しい query_parameters/models API に移行 (#97)
- コード重複解消 — コンテナ・ネットワーク・サービスフィルタ・エラー処理を共通化
## [0.7.11] - 2026-03-06

### Bug Fixes

- Phase 1 セキュリティ修正 + バグ修正（10件） (#96)

### Features

- Deploy/build の -n オプションで複数サービスを同時指定できるようにした (#53)

### Miscellaneous

- バージョンを v0.7.11 にバンプ + workspace.dependencies 整合性修正
## [0.7.10] - 2026-02-23

### Miscellaneous

- バージョンを v0.7.10 にバンプ（プラグインと同期）
## [0.7.9] - 2026-02-23

### Bug Fixes

- リリースCIをsoftprops/action-gh-releaseに移行

### Documentation

- README全面リライト - 動作フロー図追加・未実装機能を削除
- README大幅改訂 - fleet setup削除・stage/play/registry追加・KDL例修正

### Features

- GHCR環境変数認証 + Pull認証をRegistryAuthに統合 (#52)
## [0.7.8] - 2026-02-21

### Bug Fixes

- プロダクト品質改善 — セキュリティ・安全性・コード品質を向上
- ステージ未指定時のテンプレートエラーをユーザーフレンドリーなメッセージに改善

### Features

- Fleet registry deploy でSSHリモートデプロイを実行
- KDLパーサーにinclude機能と変数展開を追加 (#46)
- Fleet Registry - コンピュートとサービスの疎結合化 (#44)
- Fleet deploy 後にDockerクリーンアップを自動実行 (#20) (#40)

### Miscellaneous

- バージョンを v0.7.8 にバンプ

### Refactor

- Main.rs(4054行)を17モジュールに分割 (#30) (#39)

### Style

- Cargo fmtでフォーマット修正
## [0.7.7] - 2026-02-14

### Bug Fixes

- Clippy collapsible_if警告を修正（let-chain使用）

### Features

- `fleet exec` コマンドを追加 — コンテナ内コマンド実行 (#34)
- 全コマンドに位置引数でのステージ指定をサポート (#33)

### Miscellaneous

- Bump version to 0.7.7 + cargo fmt修正
## [0.7.6] - 2026-01-21

### Bug Fixes

- Deployコマンドにネットワーク作成処理を追加
## [0.7.5] - 2026-01-21

### Bug Fixes

- アセット名をDocker/Go主流の命名規則に変更
## [0.7.4] - 2026-01-21

### Bug Fixes

- Clippy collapsible_if警告を修正（let-chain使用）
- Self-updateで削除→コピー方式に変更（Linux対応改善）

### Miscellaneous

- Bump version to 0.7.4
## [0.7.3] - 2026-01-21

### Miscellaneous

- Bump version to 0.7.3
## [0.7.2] - 2026-01-21

### Bug Fixes

- Clippy警告を修正（未使用import/変数、collapsible_if）

### Documentation

- Fleet version コマンドを修正

### Features

- 環境変数フォールバックを追加

### Miscellaneous

- Bump version to 0.7.2

### Style

- Cargo fmtによるフォーマット修正
## [0.7.1] - 2026-01-09

### Bug Fixes

- Clippy collapsible_if警告を修正（let-chain使用）
- Self-update権限エラー時に一時ファイルを保持

### Features

- Self-update時に/usr/local/bin/fleetへも自動コピー

### Miscellaneous

- V0.7.1 リリース準備

### Refactor

- Self-updateでシンボリックリンク方式を採用

### Style

- Cargo fmtによるフォーマット修正
## [0.7.0] - 2026-01-09

### Bug Fixes

- BuildKit対応のためdocker buildx CLIを使用

### Features

- KDL変数のop://参照を1Passwordから自動解決

### Miscellaneous

- V0.7.0 リリース
## [0.6.0] - 2026-01-02

### Bug Fixes

- ステージ固有の変数が正しく解決されるよう修正 (#29)

### Features

- Fleet stageコマンドを追加
- 1Password統合モジュールを追加
- プロジェクトレベルの変数定義をサポート

### Miscellaneous

- V0.6.0 リリース

### Style

- フォーマット修正
## [0.5.1] - 2025-12-30

### Bug Fixes

- PROJECT_ROOT変数を自動設定する

### Documentation

- READMEを実際の実装内容に合わせて修正

### Refactor

- フォールバックを削除、.fleetflow/fleet.kdlのみを設定ファイルとして認識
- 設定ファイル名をfleet.kdlに変更、.fleetflow/を優先に
## [0.5.0] - 2025-12-29

### Miscellaneous

- V0.5.0 リリース (**BREAKING**)

### Refactor

- Unison-kdlをGitHub依存に変更
- バイナリ名を flow から fleet に変更 (**BREAKING**)
## [0.4.7] - 2025-12-29

### Bug Fixes

- Self-updateのバイナリ名を修正
## [0.4.6] - 2025-12-29

### Bug Fixes

- テンプレートエラーで未定義変数名を表示

### Miscellaneous

- V0.4.6 リリース
## [0.4.5] - 2025-12-29

### Bug Fixes

- 未使用のinfoインポートを削除
- Self-updateのアセット名をリリースワークフローと統一

### Style

- フォーマット修正
## [0.4.4] - 2025-12-28

### Bug Fixes

- リリースワークフローのバイナリ名を修正
## [0.4.3] - 2025-12-28

### Bug Fixes

- CIにRustツールセットアップステップを追加
- Clippyの未使用コード警告と非推奨警告を修正

### CI

- Docker統合テストを一旦無効化
- 通常CIからビルドステップを削除

### Documentation

- FleetFlowスキルをv0.4.2に対応・タグライン更新
- Workload参照を削除（残りのドキュメント）
- CLI名称を flow に統一

### Features

- CLI引数を位置引数から--stageオプションに変更
- FLEET_STAGE環境変数と.env.external対応

### Miscellaneous

- V0.4.3 リリース

### Refactor

- 未使用のworkload機能を削除

### Testing

- テストインフラを3段階構成に整理
- 軽量CLIテスト追加と既存テストのCLI形式更新

### Style

- Cargo fmtでコードフォーマット修正
## [0.4.2] - 2025-12-25

### Bug Fixes

- テンプレート展開でFLEETFLOW_STAGEが展開されない問題を修正

### Miscellaneous

- バージョンを 0.4.2 にバンプ
## [0.4.1] - 2025-12-25

### Features

- クラウドサーバー管理とplayコマンドの機能拡張
## [0.4.0] - 2025-12-24

### Features

- Playコマンドを追加 - リモートサーバーへのPlaybook実行

### Miscellaneous

- Data/ディレクトリをgitignoreに追加
- V0.4.0 - playコマンドとclippy修正
## [0.3.2] - 2025-12-23

### Bug Fixes

- ステージ内サービス定義の解析とステージ固有オーバーライドの適用

### Features

- KDLでのレジストリ設定とclippy警告の修正

### Testing

- Config_priority_testを一時的にignore
- Config_priority_complex テストを一時的にスキップ
## [0.3.1] - 2025-12-21

### Miscellaneous

- バージョンを 0.3.1 にバンプ
## [0.3.0] - 2025-12-21

### Bug Fixes

- メッセージやドキュメント内の古いコマンド表記を fleetflow に統一
- Install.sh をより堅牢な形式 (printf) に修正
- MCPサーバーの起動順序修正とinstall.shのバグ修正
- 依存関係のバージョン不整合を修正

### Features

- FleetFlow 0.3.0 - AI連携とワークロードの導入
## [0.2.14] - 2025-12-16

### Features

- Upコマンドに--pullオプションを追加
- プロジェクト自動追加機能を追加
- Issue/PR作成者を自動で担当者に設定するワークフロー追加

### Miscellaneous

- ビルドターゲットをLinux x86_64とmacOS ARM64に絞る

### Style

- Cargo fmt
## [0.2.13] - 2025-12-16

### Bug Fixes

- Reqwestをrustls-tlsに変更（クロスコンパイル対応）
- Clippy warnings (collapsible_if, format in println, etc.)

### CI

- MacOS-13をmacOS-15に変更（廃止対応）
- リリース前にCIチェックを必須化

### Documentation

- スキルをv0.2.12に更新

### Features

- Build設定がある場合はローカルビルドを優先
- Serverブロックにdeploy_path属性を追加 (#19)

### Miscellaneous

- Bump version to 0.2.13

### Style

- Cargo fmt
- Cargo fmt
## [0.2.12] - 2025-12-15

### Features

- Deployコマンドを追加（CI/CD向け）

### Miscellaneous

- Bump version to 0.2.12
## [0.2.11] - 2025-12-15

### Features

- Upコマンド実行時のセルフアップデート機能を追加
- Discord通知ワークフローを追加

### Miscellaneous

- Bump version to 0.2.11
## [0.2.10] - 2025-12-14

### Bug Fixes

- Bollard 0.19 API変更に対応、バージョン0.2.10に更新

### Documentation

- Cargo install fleetflow を削除（未公開のため）

### Features

- 組み込みスタートアップスクリプトとself-updateコマンドを追加
## [0.2.9] - 2025-12-14

### Miscellaneous

- Bump version to 0.2.9
## [0.2.8] - 2025-12-13

### Features

- Fleetflow up で起動中コンテナを自動再起動
## [0.2.7] - 2025-12-13

### Features

- Stageフィルタリング後のimageバリデーションとauto-build
- 依存サービス待機機能を追加（Exponential Backoff）
## [0.2.6] - 2025-12-13

### Features

- イメージプッシュ機能を追加

### Miscellaneous

- Bump version to 0.2.6
## [0.2.5] - 2025-12-04

### Miscellaneous

- Bump version to 0.2.5
## [0.2.4] - 2025-12-02

### Features

- Docker config.jsonからの認証情報読み取りを追加
## [0.2.3] - 2025-12-02

### Bug Fixes

- Rust-toolchain アクション名を修正

### Features

- マルチプラットフォームリリースビルドを追加
- マルチステージデプロイメント機能を追加
## [0.2.2] - 2025-12-01

### Bug Fixes

- Bollard非推奨警告とコンパイル警告を完全に解消 (#16)
- 既存サーバーでもDNSレコードを設定
- SSH鍵とネットワーク設定を修正

### Features

- Cloudflare DNS統合を実装
- Cloud up/downで実際のサーバー作成・削除を実装
- クラウドリソース管理機能を追加 (#15)
- ステージ別ネットワーク自動設定 (#14)
- Env/environment両対応とブール値エラー改善 (#12, #13)

### Miscellaneous

- バージョン 0.2.2 リリース準備
## [0.2.1] - 2025-11-28

### Documentation

- FleetFlowスキルをリファクタ・モジュール化
- V0.2.0に合わせてドキュメントを更新

### Features

- .fleetflow/ディレクトリのサポートを追加

### Miscellaneous

- バージョン 0.2.1 リリース準備
## [0.2.0] - 2025-11-27

### Bug Fixes

- CLIコマンドのコンテナ命名規則をOrbStack連携に統一
- バージョン表示をunisonからfleetflowに修正
- クレート間のバージョン依存関係を0.2.0-develに統一

### Documentation

- FleetFlow使用ガイドスキルを追加
- 設計書のチェックリストをMVP完成状況に更新
- SDGスキルに基づくドキュメント構造のフラット化
- CLAUDE.mdにbollardスキルの説明を追加
- Claude Code開発環境の統合セットアップ
- OrbStack連携をローカル開発環境向けに最適化
- 命名規則ドキュメントとdev-essentials統合 (#5)

### Features

- Dockerビルド機能とクラウドインフラ管理の実装 (Issue #10) (#11)
- Dockerイメージの自動pull機能を実装
- OrbStack連携機能を実装
- プロジェクト名とステージ名を含むコンテナ命名規則を実装

### Miscellaneous

- バージョン 0.2.0 リリース準備

### Refactor

- Bollard-guide.mdをスキル化
- 全ドキュメントの旧名称unisonをfleetflowに統一

### Testing

- OrbStack連携機能のユニットテスト追加

### V0.1.1

- 全クレートにREADMEを追加
## [0.1.0] - 2025-11-09

### Features

- Add Flow and Process structs to fleetflow-atom

### Makoフロースキルの用語を修正

- Implementation → Coding

### Miscellaneous

- Prepare for crates.io release v0.1.0

### Refactor

- Rename all crate directories with fleetflow- prefix
- Rename all crates with fleetflow- prefix
