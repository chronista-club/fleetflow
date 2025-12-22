# FleetFlow おすすめ構成ガイド (Best Practices)

FleetFlowを最大限に活用し、クリーンで保守性の高いインフラ定義を実現するための推奨構成ガイドです。

## 1. ワークロードへの分割 (Workload-based Architecture)

全ての定義を `flow.kdl` に詰め込むのではなく、論理的な役割（Workload）ごとにファイルを分割します。

### 推奨ディレクトリ構造
```
.fleetflow/
├── flow.kdl           # メイン（WorkloadとStageの宣言）
├── flow.prod.kdl      # 本番用オーバーライド（任意）
└── workloads/         # 自動読み込みディレクトリ
    ├── storage.kdl    # データベース関連
    ├── apps.kdl       # アプリケーション関連
    └── monitoring.kdl # 監視関連
```

## 2. 開発マシンを「サーバー」として定義する

ローカル開発環境も「特定のリソース（Macなど）へのデプロイ」として捉えます。

### flow.kdl での定義例
```kdl
providers {
    orbstack  // macOS + OrbStack の場合
}

server "mito-mac.local" {
    provider "orbstack"
}

stage "dev" {
    server "mito-mac.local"
    service "db"
    service "redis"
}
```

### メリット
* **一貫性**: ローカル構築も本番デプロイも `fleetflow up <stage>` という同一のコマンド体系で行えます。
* **可搬性**: 新しい開発マシンをセットアップする際も、設定ファイルを共有するだけで環境が再現されます。

## 3. ステージの最小化 (Minimal Stages)

ステージ名を増やしすぎると管理が煩雑になります。以下の 2 つ（または 3 つ）に統合することを推奨します。

1. **dev**: 開発者の手元（ローカルマシンサーバー）で動かす環境。
2. **prod**: 実際のサービスを稼働させる本番環境。
3. (任意) **stg**: 本番直前の確認用環境。

## 4. プロバイダーの明示的指定 (Multi-Provider)

`providers` ブロックを使用して、どの技術スタックを使用するかを明示します。

```kdl
providers {
    sakura-cloud { zone "tk1a" }
    orbstack
}
```

## 5. 命名規則の統一

FleetFlowはデフォルトで `{project}-{stage}-{service}` の形式でコンテナを命名します。OrbStack などのプロバイダーはこのラベルを認識し、プロジェクトごとに自動的にグループ化して表示します。

---

**FleetFlow** - シンプルに、統一的に、環境を構築する。
