# FleetFlow

[![Crates.io](https://img.shields.io/crates/v/fleetflow.svg)](https://crates.io/crates/fleetflow)
[![Documentation](https://docs.rs/fleetflow/badge.svg)](https://docs.rs/fleetflow)
[![License](https://img.shields.io/crates/l/fleetflow.svg)](https://github.com/chronista-club/fleetflow#license)

> **環境構築は、対話になった。伝えれば、動く。**

FleetFlowは、KDL（KDL Document Language）をベースにしたコンテナオーケストレーションツールです。シンプルな宣言で、ローカル開発から本番デプロイまでをシームレスにつなぎます。

## ✨ 特徴

- **超シンプル**: Docker Composeと同等かそれ以下の記述量
- **可読性**: YAMLよりも読みやすいKDL構文
- **ステージ管理**: 開発環境から本番環境まで統一管理
- **自動推測**: サービス名から自動的にDockerイメージを推測

## 📦 インストール

```bash
cargo install --git https://github.com/chronista-club/fleetflow
```

## 🚀 クイックスタート

### 1. 設定ファイルを作成

`flow.kdl`:
```kdl
service "postgres" {
    version "16"
    ports {
        port host=5432 container=5432
    }
    environment {
        POSTGRES_USER "myuser"
        POSTGRES_PASSWORD "mypass"
        POSTGRES_DB "mydb"
    }
}

service "redis" {
    version "7"
    ports {
        port host=6379 container=6379
    }
}

service "app" {
    image "myapp"
    version "latest"
    ports {
        port host=8080 container=8080
    }
    environment {
        DATABASE_URL "postgresql://myuser:mypass@postgres:5432/mydb"
        REDIS_URL "redis://redis:6379"
    }
    depends_on "postgres" "redis"
}

stage "local" {
    service "postgres"
    service "redis"
    service "app"
}
```

### 2. サービスを起動

```bash
flow up
```

### 3. サービスを確認

```bash
flow ps
```

### 4. サービスを停止

```bash
flow down
```

## 📚 主なコマンド

| コマンド | 説明 |
|---------|------|
| `flow up [stage]` | ステージ内のサービスを起動 |
| `flow down [stage]` | ステージ内のサービスを停止 |
| `flow ps` | 実行中のサービスを一覧表示 |
| `flow logs <service>` | サービスのログを表示 |

## 🎯 主な機能

### KDLベースの直感的な記述

YAMLの冗長さから解放され、読みやすく書きやすい設定ファイルを実現。

```kdl
service "api" {
    image "myapp:latest"
    port 8080
    env {
        DATABASE_URL "postgresql://localhost/mydb"
    }
}
```

### ステージベースの環境管理

開発環境から本番環境まで、ステージで管理。

```kdl
stage "local" {
    service "postgres"
    service "redis"
    service "app"
}

stage "production" {
    service "postgres"
    service "redis"
}
```

### 自動イメージ推測

サービス名から自動的にDockerイメージを推測。設定の記述量を削減。

```kdl
service "postgres" {
    version "16"  // postgres:16 として自動推測
}
```

### テンプレート変数

環境変数や変数定義を使って、設定を動的に生成。

```kdl
variables {
    app_version "1.0.0"
    registry "ghcr.io/myorg"
}

service "api" {
    image "{{ registry }}/api:{{ app_version }}"
}
```

## 📖 ドキュメント

- [GitHubリポジトリ](https://github.com/chronista-club/fleetflow)
- [仕様書](https://github.com/chronista-club/fleetflow/tree/main/spec)
- [CHANGELOG](https://github.com/chronista-club/fleetflow/blob/main/CHANGELOG.md)

## 🏗️ アーキテクチャ

FleetFlowは以下のクレートで構成されています：

- **fleetflow** - メインCLI
- **fleetflow-core** - コア機能（パーサー、モデル、ローダー）
- **fleetflow-config** - 設定ファイル検索と管理
- **fleetflow-container** - Dockerコンテナランタイム統合

## 📄 ライセンス

MIT OR Apache-2.0

## 🙏 コントリビューション

Issue、Pull Requestを歓迎します！

## 🔗 関連クレート

- [`fleetflow-core`](https://crates.io/crates/fleetflow-core) - コア機能
- [`fleetflow-config`](https://crates.io/crates/fleetflow-config) - 設定管理
- [`fleetflow-container`](https://crates.io/crates/fleetflow-container) - コンテナランタイム統合
