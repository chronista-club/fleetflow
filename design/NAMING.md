# FleetFlow - 命名規則

## プロジェクト名

**正式名称**: FleetFlow

**決定日**: 2025-11-16

## 命名の由来

- **Fleet（艦隊）**: クラスター管理、複数コンテナの協調動作を表現
- **Flow（流れ）**: オーケストレーション、ワークフローの概念を想起

## ケーシング規則

### ブランド名・ドキュメント
```
FleetFlow
```

ドキュメント、UI、マーケティング資料で使用。先頭を大文字に。

### パッケージ名・コマンド名
```
fleetflow
```

すべて小文字。スペースなし。

### 設定ファイル
```yaml
# YAML形式
fleetflow.yaml

# TOML形式
fleetflow.toml

# プロジェクト設定ディレクトリ
.fleetflow/
```

### CLIコマンド例
```bash
fleet deploy
flow status
flow cluster list
flow flow run
```

### Dockerイメージ
```
vantage/fleetflow:latest
vantage/fleetflow:v0.2.0
```

組織名/プロジェクト名の形式。

### GitHubリポジトリ
```
github.com/chronista-club/fleetflow
```

### Rustクレート

#### ワークスペース全体
```toml
[workspace.package]
name = "fleetflow"
```

#### 個別クレート
```toml
[package]
name = "fleetflow-core"
name = "fleetflow"
name = "fleetflow-config"
name = "fleetflow-container"
```

すべてのクレート名は `fleetflow-` プレフィックスを使用。

## ディレクトリ構造

```
fleetflow/
├── crates/
│   ├── fleetflow-core/
│   ├── fleetflow/
│   ├── fleetflow-config/
│   └── fleetflow-container/
├── design/
├── docs/
└── .fleetflow/
```

## サブプロジェクト・モジュール命名

### 将来的なサブプロジェクト例
- `fleetflow-operator`: Kubernetes Operator
- `fleetflow-agent`: エージェント
- `fleetflow-api`: API サーバー
- `fleetflow-web`: Web UI

### モジュール・パッケージの命名ルール
1. すべて小文字
2. ハイフン区切り（kebab-case）
3. `fleetflow-` プレフィックスを付ける

## 用語の統一

### 推奨用語
- **Flow**: ワークフロー定義
- **Process**: 実行プロセス
- **Stage**: フロー内のステージ
- **Container**: コンテナ
- **Cluster**: クラスター

### 避けるべき用語
- 旧名称（Unison Flow など）の使用
- 混在した大文字小文字（FleetFlow, FLEETFLOW など）

## 変更履歴

| 日付 | 変更内容 |
|------|---------|
| 2025-11-16 | 初版作成。FleetFlow の正式な命名規則を定義 |
