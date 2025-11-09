# FleetFlow

> Docker Composeよりシンプル。KDLで書く、次世代の環境構築ツール。

## コンセプト

**「宣言だけで、開発も本番も」**

FleetFlowは、KDL（KDL Document Language）をベースにした、革新的で超シンプルなデプロイ・環境構築ツールです。
Docker Composeの手軽さはそのままに、より少ない記述で、より強力な設定管理を実現します。

### なぜFleetFlow？

- **超シンプル**: Docker Composeと同等かそれ以下の記述量
- **可読性**: YAMLよりも読みやすいKDL構文
- **モジュール化**: include機能で設定を分割・再利用
- **統一管理**: 開発環境から本番環境まで同じツールで

## クイックスタート

### インストール

```bash
cargo install fleetflow
```

### 基本的な使い方

```kdl
// unison.kdl
environment "development" {
  service "web" {
    image "node:20-alpine"
    port 3000
    env {
      NODE_ENV "development"
    }
  }

  service "db" {
    image "postgres:16"
    port 5432
    volume "./data:/var/lib/postgresql/data"
  }
}
```

```bash
# 環境を起動
fleetflow up

# 環境を停止
fleetflow down

# 設定を検証
fleetflow validate
```

## 特徴

### 1. KDLベースの直感的な記述

YAMLの冗長さから解放され、読みやすく書きやすい設定ファイルを実現。

```kdl
service "api" {
  image "myapp:latest"
  port 8080
  env {
    DATABASE_URL "postgresql://localhost/mydb"
    REDIS_URL "redis://localhost:6379"
  }
}
```

### 2. 強力なinclude機能

設定を分割して管理。共通設定を再利用できます。

```kdl
// flow.kdl
include "common/database.kdl"
include "common/redis.kdl"
include "services/api.kdl"
include "services/worker.kdl"

stage "development" {
  // includeした設定が自動的に利用される
}
```

プロジェクト構造例：

```
project/
├── flow.kdl                # メイン設定
├── common/
│   ├── database.kdl       # DB共通設定
│   └── redis.kdl          # Redis共通設定
├── environments/
│   ├── dev.kdl            # 開発環境
│   ├── staging.kdl        # ステージング
│   └── prod.kdl           # 本番環境
└── services/
    ├── api.kdl
    ├── worker.kdl
    └── frontend.kdl
```

### 3. 環境間の継承

開発環境と本番環境の差分だけを記述。

```kdl
environment "development" {
  service "api" {
    image "node:20-alpine"
    port 3000
    env {
      NODE_ENV "development"
    }
  }
}

environment "production" {
  include-from "development"

  service "api" {
    replicas 3  // 本番環境では3台に
    env {
      NODE_ENV "production"
    }
  }
}
```

### 4. 変数とテンプレート

繰り返しを減らし、保守性を向上。

```kdl
vars {
  app-version "1.0.0"
  registry "ghcr.io/myorg"
  node-image "node:20-alpine"
}

service "api" {
  image "{registry}/api:{app-version}"
  base-image "{node-image}"
}
```

## 拡張機能

### include

ファイル全体をインクルード。

```kdl
include "path/to/config.kdl"
include "services/*.kdl"  // グロブパターン対応（予定）
```

### 環境変数参照

```kdl
service "api" {
  env {
    DATABASE_URL from-env "DATABASE_URL"
    API_KEY from-secret "api-key"
  }
}
```

### 条件分岐（予定）

```kdl
service "api" {
  if env "production" {
    replicas 3
  } else {
    replicas 1
  }
}
```

## コマンド

```bash
# 環境を起動
fleetflow up [--stage <stage>]

# 環境を停止
fleetflow down [--stage <stage>]

# 環境を再起動
fleetflow restart [--stage <stage>]

# 設定を検証
fleetflow validate

# ステージ間の差分を表示
fleetflow diff <stage1> <stage2>

# 設定をDocker Composeに変換
fleetflow export docker-compose

# ログを表示
fleetflow logs [service-name]

# サービス一覧を表示
fleetflow ps
```

## ロードマップ

### Phase 1: MVP (現在の目標)

- [x] プロジェクト初期化
- [ ] KDLパーサーの実装
- [ ] 基本的なservice定義のパース
- [ ] include機能の実装
- [ ] Docker Compose形式への変換
- [ ] 基本的なCLIコマンド（up/down/validate）

### Phase 2: 拡張機能

- [ ] 環境変数の参照
- [ ] 変数定義と展開
- [ ] 環境継承（include-from）
- [ ] グロブパターンによるinclude
- [ ] 設定の検証とエラーメッセージ改善

### Phase 3: 独自実行エンジン

- [ ] Docker API直接利用
- [ ] パフォーマンス最適化
- [ ] リアルタイムログストリーミング
- [ ] ヘルスチェック機能

### Phase 4: エコシステム拡張

- [ ] Kubernetes manifestへの変換
- [ ] Terraform/Pulumiとの統合
- [ ] Web UI
- [ ] プラグインシステム
- [ ] CI/CDパイプライン統合

## 技術スタック

- **言語**: Rust
- **パーサー**: `kdl` crate
- **コンテナ**: Docker API / bollard
- **CLI**: clap
- **設定検証**: serde + custom validation

## 開発に参加する

Issue、Pull Requestは大歓迎です！

### 開発環境のセットアップ

```bash
git clone https://github.com/chronista-club/fleetflow.git
cd fleetflow
cargo build
cargo test
```

### テスト

```bash
cargo test
cargo clippy
cargo fmt
```

## ライセンス

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

## 関連リンク

- [KDL - The KDL Document Language](https://kdl.dev/)
- [kdl-rs](https://github.com/kdl-org/kdl-rs)

---

**FleetFlow** - シンプルに、統一的に、環境を構築する。
