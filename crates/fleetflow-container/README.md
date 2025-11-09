# fleetflow-container

[![Crates.io](https://img.shields.io/crates/v/fleetflow-container.svg)](https://crates.io/crates/fleetflow-container)
[![Documentation](https://docs.rs/fleetflow-container/badge.svg)](https://docs.rs/fleetflow-container)
[![License](https://img.shields.io/crates/l/fleetflow-container.svg)](https://github.com/chronista-club/fleetflow#license)

FleetFlowのDockerコンテナランタイム統合を提供するライブラリクレート。

## 概要

`fleetflow-container`は、FleetFlowの設定をDocker APIパラメータに変換し、コンテナライフサイクルを管理する機能を提供します：

- **設定変換** - FleetFlowの設定をDocker APIパラメータに変換
- **ランタイムトレイト** - コンテナランタイムの抽象化
- **コンテナ管理** - 起動、停止、ステータス確認

## 使用例

### FlowConfigからDockerコンテナ設定に変換

```rust
use fleetflow_container::converter::service_to_container_config;
use fleetflow_atom::Service;

let service = Service {
    image: Some("postgres:16".to_string()),
    version: Some("16".to_string()),
    ..Default::default()
};

let (config, options) = service_to_container_config("postgres", &service);

// configとoptionsをBollardに渡してコンテナを作成
```

### ステージからサービスリストを取得

```rust
use fleetflow_container::converter::get_stage_services;
use fleetflow_atom::Flow;

let flow = /* ... */;
let services = get_stage_services(&flow, "local")?;

for service_name in services {
    println!("Service: {}", service_name);
}
```

### コンテナランタイムトレイト

```rust
use fleetflow_container::runtime::ContainerRuntime;
use fleetflow_atom::Flow;

#[async_trait]
pub trait ContainerRuntime {
    async fn start(&self, flow: &Flow) -> Result<()>;
    async fn stop(&self, flow: &Flow) -> Result<()>;
    async fn status(&self) -> Result<Vec<ContainerStatus>>;
}
```

## 機能

### サービス設定の変換

FleetFlowのServiceをDocker APIのパラメータに変換：

- **イメージとバージョン** - Dockerイメージとタグ
- **ポートマッピング** - ホストとコンテナのポートバインディング
- **環境変数** - コンテナ内の環境変数
- **ボリューム** - ホストとコンテナのボリュームマウント
- **コマンド** - コンテナ起動時のコマンド
- **依存関係** - サービス間の依存関係

### 対応プロトコル

- TCP (デフォルト)
- UDP

### ボリュームマウント

- 読み取り専用 (`read_only`)
- 読み書き可能 (デフォルト)
- 相対パスの自動解決

## 依存関係

- [bollard](https://crates.io/crates/bollard) - Docker APIクライアント
- [fleetflow-atom](https://crates.io/crates/fleetflow-atom) - FleetFlowコア機能

## ドキュメント

- [FleetFlow メインプロジェクト](https://github.com/chronista-club/fleetflow)
- [API ドキュメント](https://docs.rs/fleetflow-container)

## ライセンス

MIT OR Apache-2.0
