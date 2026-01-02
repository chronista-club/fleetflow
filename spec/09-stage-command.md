# FleetFlow Stage Command

## 概要

`fleet stage` コマンドは、**stage（環境）を中心概念**として、インフラからコンテナまで統一的に管理する。

## 背景と目的

### Why（なぜ必要か）

- 従来: `fleet up` と `fleet cloud up` が分離していた
- 「dev環境を起動」という1つの意図に対して、複数コマンドが必要だった
- stage という概念を中心に据え、操作を統一したい

### ゴール

- `fleet stage up dev` 一発で環境全体を起動
- stageの種類（local/dev/pre/prod）によって自動的に必要なリソースを判断
- 冪等性: 何度実行しても同じ状態に収束

## コマンド体系

### 基本構文

```bash
fleet stage <command> <stage-name> [options]
```

### サブコマンド一覧

| コマンド | 説明 |
|---------|------|
| `up` | ステージを起動（インフラ＋コンテナ） |
| `down` | ステージを停止 |
| `status` | ステージの状態を表示 |
| `logs` | ログを表示 |
| `ps` | コンテナ一覧 |

## `fleet stage up`

### 動作フロー

```
1. 設定ファイル読み込み
2. stageの種類を判定
   - local: Dockerコンテナのみ
   - remote (dev/pre/prod): インフラ + コンテナ
3. インフラ構築（remoteの場合）
   - サーバー存在確認 → なければ作成
   - 電源OFF → 電源ON
   - SSH接続待機
4. コンテナ起動
5. DNS設定（必要なら）
```

### オプション

| オプション | 説明 |
|-----------|------|
| `--yes`, `-y` | 確認プロンプトをスキップ |
| `--pull` | イメージを強制的にpull |

### 使用例

```bash
# ローカル環境を起動
fleet stage up local

# 開発環境を起動（VPS作成含む）
fleet stage up dev

# 確認なしで起動
fleet stage up dev --yes
```

## `fleet stage down`

### 動作モード

| モード | コンテナ | サーバー電源 | サーバー | 課金 |
|--------|---------|-------------|---------|------|
| デフォルト | 停止 | ON | 残す | 100% |
| `--suspend` | 停止 | OFF | 残す | 〜20%（ディスクのみ） |
| `--destroy` | 削除 | - | 削除 | 0% |

### オプション

| オプション | 説明 |
|-----------|------|
| `--suspend` | サーバー電源をOFFにする（さくらのクラウド） |
| `--destroy` | サーバーを削除する |
| `--yes`, `-y` | 確認プロンプトをスキップ |

### 使用例

```bash
# コンテナのみ停止（サーバーは稼働継続）
fleet stage down dev

# サーバー電源OFF（コスト削減、再起動は速い）
fleet stage down pre --suspend

# サーバー削除（完全停止）
fleet stage down dev --destroy --yes
```

### ユースケース

```bash
# 日常の開発終了
fleet stage down dev              # コンテナ停止、サーバー稼働継続

# 週末・長期離脱（pre環境）
fleet stage down pre --suspend    # 電源OFF、ディスク代のみ

# 月曜に再開
fleet stage up pre                # 電源ON → コンテナ起動

# 完全クリーンアップ
fleet stage down dev --destroy    # VPS削除、課金完全停止
```

## `fleet stage status`

### 表示内容

```
Stage: dev
Status: running

Infrastructure:
  Server: creo-dev-01
    Provider: sakura-cloud
    Status: running
    IP: 163.43.xxx.xxx
    CPU: 4 cores
    Memory: 4 GB

Services:
  ✓ surrealdb    running  (up 2 days)
  ✓ qdrant       running  (up 2 days)
  ✓ api          running  (up 1 hour)
```

## `fleet stage logs`

### オプション

| オプション | 説明 |
|-----------|------|
| `--service`, `-s` | 特定サービスのログのみ |
| `--follow`, `-f` | リアルタイム追従 |
| `--tail`, `-n` | 最新N行 |

### 使用例

```bash
# 全サービスのログ
fleet stage logs dev

# 特定サービス
fleet stage logs dev -s api

# リアルタイム追従
fleet stage logs dev -f
```

## `fleet stage ps`

### 表示例

```
STAGE  SERVICE    STATUS   PORTS              CREATED
dev    surrealdb  running  8000->8000/tcp     2 days ago
dev    qdrant     running  6333->6333/tcp     2 days ago
dev    api        running  3000->3000/tcp     1 hour ago
```

## stage種別による動作の違い

### local

- Dockerコンテナのみ管理
- インフラ操作なし
- `--suspend`, `--destroy` は `--destroy` のみ有効（コンテナ削除）

### dev / pre / prod

- インフラ（VPS, DNS）+ コンテナを管理
- stageにserver定義があれば、インフラ操作を実行
- `--suspend` でサーバー電源OFF

### 判定ロジック

```
if stage.servers.is_empty() {
    // ローカルモード: コンテナのみ
} else {
    // リモートモード: インフラ + コンテナ
}
```

## 移行計画

### Phase 1: 並行運用

- `fleet stage` コマンドを追加
- 既存の `fleet up/down` は維持（非推奨警告）

### Phase 2: 完全移行

- `fleet up/down/logs/ps` を `fleet stage` のエイリアスに
- `fleet cloud` を削除

## 設計原則

1. **stage中心**: 環境（stage）を操作の単位とする
2. **冪等性**: 何度実行しても同じ結果
3. **宣言的**: 設定ファイルで定義した状態に収束
4. **段階的停止**: down → suspend → destroy の3段階
