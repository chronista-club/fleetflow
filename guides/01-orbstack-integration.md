# OrbStack連携ガイド

最終更新日: 2025-11-22

## 概要

このガイドでは、FleetFlowとOrbStackを連携してローカル開発環境でコンテナを管理する方法を説明します。

## 対象読者

- macOSでOrbStackを使用している開発者
- 複数のプロジェクトを並行して開発している方
- FleetFlowを使ってローカル環境を構築したい方

## 前提条件

### 必要な環境
- macOS
- [OrbStack](https://orbstack.dev/) インストール済み
- FleetFlow インストール済み

### 必要な知識
- 基本的なDocker/コンテナの知識
- KDL構文の基礎

## 基本的な使い方

### ステップ1: プロジェクト名の設定

`flow.kdl`ファイルにプロジェクト名を宣言します：

```kdl
project "my-project"
```

プロジェクト名は、コンテナ名とグループ名に使用されます。

### ステップ2: ステージとサービスの定義

ローカル開発用のステージとサービスを定義します：

```kdl
project "my-project"

stage "local" {
    service "postgres"
    service "redis"
}
```

### ステップ3: サービスの詳細設定

各サービスの詳細を定義します：

```kdl
service "postgres" {
    image "postgres:16"
    ports {
        port host=5432 container=5432
    }
    env {
        POSTGRES_PASSWORD "postgres"
        POSTGRES_DB "myapp"
    }
}

service "redis" {
    image "redis:7-alpine"
    ports {
        port host=6379 container=6379
    }
}
```

### ステップ4: コンテナの起動

```bash
fleetfleetflow up -s local
```

### ステップ5: OrbStackで確認

1. OrbStackアプリケーションを開く
2. 「Containers」セクションを確認
3. `my-project-local`というグループが作成され、その中に`postgres`と`redis`が表示されます

```
📁 my-project-local
  ├── postgres
  └── redis
```

## チーム開発での活用例

### 複数プロジェクトの並行開発

```bash
# プロジェクトA
cd ~/projects/project-a
fleetfleetflow up -s local

# プロジェクトB
cd ~/projects/project-b
fleetfleetflow up -s local

# OrbStackでの表示
# 📁 project-a-local
#   └── ...
# 📁 project-b-local
#   └── ...
```

各プロジェクトが独立したグループとして表示され、管理が容易になります。

### 朝のセットアップ

```bash
# プロジェクトディレクトリに移動して起動
cd ~/projects/main-app
fleetfleetflow up -s local

# 依存する別のプロジェクトも起動
cd ~/projects/backend-service
fleetfleetflow up -s local
```

### 終業時のクリーンアップ

OrbStackアプリから：
1. グループ名を右クリック
2. 「Stop All」を選択

または：

```bash
fleetfleetflow down -s local
```

## トラブルシューティング

### 問題1: グループ化されない

**症状**: OrbStackでグループとして表示されない

**原因**: ラベルが正しく設定されていない

**解決策**:

1. ラベルの確認
```bash
docker inspect <container-name> | jq '.[] | .Config.Labels'
```

`com.docker.compose.project`ラベルが存在することを確認します。

2. fleetflowの再ビルドと再実行
```bash
cd ~/path/to/fleetflow
cargo build --release

# プロジェクトディレクトリで
fleetfleetflow down -s local
fleetfleetflow up -s local
```

### 問題2: コンテナ名が競合する

**症状**: `Conflict. The container name "..." is already in use`

**原因**: 同じプロジェクト名・ステージ・サービス名の組み合わせが既に存在

**解決策**:

1. 既存のコンテナを確認
```bash
docker ps -a | grep my-project-local
```

2. 不要なコンテナを削除
```bash
docker rm my-project-local-postgres
```

または：

```bash
fleetfleetflow down -s local --remove
fleetfleetflow up -s local
```

### 問題3: ポートが競合する

**症状**: `Bind for 0.0.0.0:5432 failed: port is already allocated`

**原因**: 指定したポートが既に使用されている

**解決策**:

1. ポートを使用しているプロセスを確認
```bash
lsof -i :5432
```

2. ポート番号を変更
```kdl
service "postgres" {
    ports {
        port host=15432 container=5432  // ホストポートを変更
    }
}
```

3. コンテナを再作成
```bash
fleetfleetflow down -s local
fleetfleetflow up -s local
```

## ベストプラクティス

### 1. プロジェクト名は短く明確に

```kdl
// ✅ 良い例
project "myapp"
project "api"

// ❌ 悪い例
project "my-awesome-super-long-project-name"
```

### 2. ステージ名の統一

チーム内でステージ名を統一することで、混乱を避けます：

```kdl
// 推奨されるステージ名
stage "local"      // ローカル開発
stage "dev"        // 開発環境
stage "staging"    // ステージング
stage "prod"       // 本番環境
```

### 3. ポート範囲の割り当て

プロジェクトごとにポート範囲を決めておくと衝突を避けられます：

```kdl
// プロジェクトA: 10000-10099
service "postgres" {
    ports {
        port host=10001 container=5432
    }
}

// プロジェクトB: 20000-20099
service "postgres" {
    ports {
        port host=20001 container=5432
    }
}
```

### 4. 環境変数の管理

機密情報は`.env`ファイルで管理：

```kdl
service "postgres" {
    env {
        POSTGRES_PASSWORD from-env "DB_PASSWORD"
    }
}
```

```.env
DB_PASSWORD=secret-password
```

## よくある質問

**Q**: OrbStackが必須ですか？

**A**: いいえ。FleetFlowは標準的なDockerラベルを使用しているため、Docker Desktopや他のDocker環境でも動作します。ただし、グループ化機能はOrbStack特有です。

**Q**: 本番環境でも使えますか？

**A**: FleetFlowはローカル開発環境向けに設計されています。本番環境では、Kubernetes、Docker Swarm、AWS ECSなどの本格的なオーケストレーションツールの使用を推奨します。

**Q**: 既存のDocker Compose設定を移行できますか？

**A**: はい。Docker Composeの多くの機能はFleetFlowのKDL設定に変換できます。詳細は移行ガイド（作成予定）を参照してください。

**Q**: コンテナのログはどこで見れますか？

**A**: OrbStackアプリのコンテナ詳細画面、またはCLIから：

```bash
docker logs fleetflow-local-postgres
```

## 次のステップ

- [KDLパーサーガイド](02-kdl-parser.md)（作成予定）
- [CLIコマンドリファレンス](03-cli-commands.md)（作成予定）
- [テンプレート変数の使い方](04-template-variables.md)（作成予定）

## 参考資料

- [OrbStack公式サイト](https://orbstack.dev/)
- [Docker Compose ラベル仕様](https://docs.docker.com/compose/compose-file/#labels)
- [FleetFlow仕様書](../spec/06-orbstack-integration.md)
- [FleetFlow設計書](../design/02-orbstack-integration.md)
