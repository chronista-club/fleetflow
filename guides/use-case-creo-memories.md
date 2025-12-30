# ユースケース: Creo Memories (Best Practice 構成)

## 概要

Creo Memoriesは、AIエージェント向けのメモリシステムです。SurrealDB（構造化データ）、Qdrant（ベクトル検索）、SeaweedFS（分散ストレージ）を組み合わせたハイブリッドアーキテクチャを採用しています。

本ドキュメントでは、FleetFlowの最新機能を最大限に活用した、**サービス分割 & サーバー定義ベース**の「おすすめ構成」を紹介します。

## プロジェクト構造

```
creo-memories/
├── .fleetflow/
│   ├── fleet.kdl         # メイン設定（ステージの宣言）
│   ├── flow.live.kdl    # ライブ環境用オーバーライド
│   └── services/        # 論理的な役割ごとのサービス定義
│       ├── storage.kdl  # DB群
│       ├── apps.kdl     # アプリ群
│       └── monitoring.kdl
├── apps/                # アプリケーションソース
└── ...
```

## おすすめ構成の 3 つの柱

### 1. サービスのファイル分割 (Service-based)
サービス定義を `services/*.kdl` に分割します。これにより、`fleet.kdl` が読みやすくなり、複数の環境で同じ構成を再利用しやすくなります。

### 2. 自分のマシンを「サーバー」として定義 (Local Machine as Server)
macOS上の OrbStack を `provider "orbstack"` として定義し、自分のマシンを `server "mito-mac.local"` のように具現化します。これにより、ローカル開発も「サーバーへのデプロイ」と同じメンタルモデルで扱えます。

### 3. ステージの統合 (Simplified Stages)
`local` と `dev` で迷う必要はありません。自分のマシン（サーバー）で動かす環境を `dev`、VPS で動かすライブ環境を `live` の 2 つに統合します。

---

## 実践例: fleet.kdl

```kdl
project "creo-memories"

// プロバイダーの定義
providers {
    sakura-cloud { zone "tk1a" } // 本番用
    orbstack                     // ローカル開発用 (macOS)
}

// サーバーの定義
server "mito-mac.local" {        // 自分のMac
    provider "orbstack"
}

server "creo-vps" {              // 本番サーバー
    provider "sakura-cloud"
    plan "4core-8gb"
    // ...
}

// ステージの定義
stage "dev" {
    server "mito-mac.local"
    service "surrealdb"
    service "qdrant"
    service "seaweedfs"
}

stage "live" {
    server "creo-vps"
    // 全サービスを起動
    service "surrealdb"
    service "qdrant"
    service "creo-app-server"
    // ...
}
```

## 運用メリット

1. **環境構築 IaC**: 新しい開発マシンに移行しても、`fleet up dev` 一発でミドルウェアが揃います。
2. **高い視認性**: OrbStack 上で `{project}-{stage}-{service}` の形式でグループ化され、管理が容易になります。
3. **本番との対称性**: 開発と本番で同じサービス定義を共有するため、環境差異によるバグを防げます。

## 関連リンク

- [KDL構造ビジュアルガイド](./04-kdl-structure-visual.md)
- [FleetFlow Cloud Infrastructure Spec](../spec/08-cloud-infrastructure.md)
