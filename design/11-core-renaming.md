# Design: fleetflow-core から fleetflow-core へのリネーム

## 1. 目的
`atom` という抽象的な名前から、プロジェクトの中核であることを示す `core` へ変更し、構造を明確にします。

## 2. 変更チェックリスト
- [ ] `crates/fleetflow-core` ディレクトリを `crates/fleetflow-core` にリネーム。
- [ ] `crates/fleetflow-core/Cargo.toml` の `name` を `fleetflow-core` に変更。
- [ ] ワークスペースのルート `Cargo.toml` の `members` を更新。
- [ ] 他の全てのクレート（`fleetflow`, `fleetflow-container` 等）の依存関係を `fleetflow-core` に更新。
- [ ] コード内の `use fleetflow_core` を `use fleetflow_core` に置換。
- [ ] ドキュメント（`README.md`, `spec/`, `design/`）内の言及を更新。
