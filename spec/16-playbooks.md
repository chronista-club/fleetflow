# Spec: Playbooks

## 1. 概念 (Concept)

「Playbook」は、複数のサービスに対する運用操作を宣言的に定義し、一括実行するための仕組みです。

### 背景

本番環境の運用では、以下のような複合的な操作が必要になります：

- **デプロイ**: 複数サービスの順次更新（ローリングアップデート）
- **メンテナンス開始**: アプリを停止しつつDBは維持
- **メンテナンス終了**: 依存関係を考慮した起動順序

これらを毎回手動でコマンド実行するのは煩雑でミスが起きやすいため、
Playbookとして宣言的に定義し、`flow play <env>/<name>` で一括実行できるようにします。

### Workloadとの違い

| 概念 | 目的 | 定義するもの |
|------|------|-------------|
| **Workload** | サービスのグルーピング | 何を動かすか（サービス群） |
| **Playbook** | 運用操作の定義 | どう操作するか（アクション） |

## 2. ディレクトリ構造

**環境別にplaybooksを配置**:

```
playbooks/
├── prod/                     # 本番環境用
│   ├── update-apps.kdl       # 慎重にローリング更新
│   ├── maintenance-in.kdl
│   ├── maintenance-out.kdl
│   └── deploy.kdl
├── stg/                      # ステージング環境用
│   ├── update-apps.kdl       # prodと同じ手順でテスト
│   └── deploy.kdl
└── dev/                      # 開発環境用
    ├── reset-all.kdl         # 全部消して作り直し
    └── seed-data.kdl
```

### ルール: ディレクトリ名 = ステージ名

- `playbooks/prod/` 内のplaybookは `stage "prod"` に対して実行される
- `playbooks/stg/` 内のplaybookは `stage "stg"` に対して実行される
- ディレクトリ名と `flow.kdl` のステージ定義が一致している必要がある
- 環境をまたぐ共通playbookは設けない（シンプルさ優先）

## 3. CLIコマンド

### play コマンド

```bash
# 環境/Playbook名 で実行
flow play <env>/<playbook-name> [OPTIONS]

# 例
flow play prod/update-apps
flow play stg/deploy
flow play dev/reset-all
flow play common/healthcheck
```

### オプション

| オプション | 説明 |
|-----------|------|
| `--dry-run` | 実行せずに計画を表示 |
| `--yes` / `-y` | 確認プロンプトをスキップ |
| `--verbose` / `-v` | 詳細ログ出力 |

### Playbook一覧表示

```bash
flow play --list
```

出力例:
```
Available playbooks:

prod/
  update-apps       アプリケーションを順次更新
  maintenance-in    メンテナンスモード開始
  maintenance-out   メンテナンスモード終了
  deploy            全サービスをデプロイ

stg/
  update-apps       アプリケーションを順次更新
  deploy            全サービスをデプロイ

dev/
  reset-all         全コンテナを削除して再作成
```

## 4. KDL構文

### 基本構造

```kdl
// playbooks/prod/update-apps.kdl
playbook "update-apps" {
    description "アプリケーションサービスを順次更新"

    services "creo-mcp-server" "creo-app-server"

    strategy "rolling" {
        delay 5  // 各サービス間の待機秒数
    }

    action "pull"
    action "restart"
}
```

### アクション種別

| アクション | 説明 |
|-----------|------|
| `pull` | 最新イメージをpull |
| `start` | サービスを起動 |
| `stop` | サービスを停止 |
| `restart` | サービスを再起動 |
| `remove` | コンテナを削除 |
| `up` | pull + start |
| `down` | stop + remove |

### 実行戦略

```kdl
// 順次実行（デフォルト）
strategy "rolling" {
    delay 5
}

// 並列実行
strategy "parallel"
```

### 環境別の違いの例

```kdl
// playbooks/prod/update-apps.kdl - 本番は慎重に
playbook "update-apps" {
    services "creo-mcp-server" "creo-app-server"

    strategy "rolling" {
        delay 10           // 長めの待機
        healthcheck true   // ヘルスチェック必須
    }

    action "pull"
    action "restart"
}
```

```kdl
// playbooks/dev/update-apps.kdl - 開発は速く
playbook "update-apps" {
    services "creo-mcp-server" "creo-app-server"

    strategy "parallel"  // 全部同時

    action "pull"
    action "restart"
}
```

## 5. 実行フロー

```
flow play prod/update-apps
    │
    ├─ 1. playbooks/prod/update-apps.kdl をロード
    │
    ├─ 2. 対象ステージを "prod" に自動設定
    │
    ├─ 3. 実行計画を表示
    │      > Environment: prod
    │      > Playbook: update-apps
    │      > Services: creo-mcp-server, creo-app-server
    │      > Strategy: rolling (delay: 10s, healthcheck: true)
    │      > Actions: pull, restart
    │      > Continue? [y/N]
    │
    ├─ 4. アクション実行
    │      > [1/2] creo-mcp-server: pulling...
    │      > [1/2] creo-mcp-server: restarting...
    │      > [1/2] creo-mcp-server: healthy ✓
    │      > (waiting 10s)
    │      > [2/2] creo-app-server: pulling...
    │      > [2/2] creo-app-server: restarting...
    │      > [2/2] creo-app-server: healthy ✓
    │
    └─ 5. 完了
           > Playbook 'prod/update-apps' completed successfully.
```

## 6. 実装フェーズ

### Phase 1（MVP）
- [ ] 基本的なplaybook定義とパース
- [ ] services + action の組み合わせ
- [ ] rolling / parallel 戦略
- [ ] `flow play <env>/<name>` コマンド
- [ ] `flow play --list`

### Phase 2
- [ ] ヘルスチェック統合
- [ ] on_failure ハンドリング
- [ ] step による複合Playbook

### Phase 3
- [ ] rollback 機能
- [ ] 通知連携（Slack, Discord）
- [ ] 実行履歴の記録

## 7. 関連仕様

- [12-workloads.md](12-workloads.md) - サービスのグルーピング
- [03-cli-commands.md](03-cli-commands.md) - CLIコマンド一覧
