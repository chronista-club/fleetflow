# fleetflow-cloud

FleetFlowのクラウドインフラ管理のコア抽象化レイヤー。

## 概要

`fleetflow-cloud`は、複数のクラウドプロバイダーを統一的に扱うための抽象化レイヤーを提供します。

## 主要コンポーネント

### トレイト

- **CloudProvider**: プロバイダー共通インターフェース

### 構造体

- **Action**: 実行可能なアクション（Create, Update, Delete, NoOp）
- **Plan**: アクションのリスト
- **ApplyResult**: 適用結果
- **ResourceConfig**: リソース設定
- **ResourceState**: リソース状態
- **ResourceStatus**: リソースステータス（Running, Stopped, Unknown）

### 状態管理

- **StateManager**: ファイルロック付き状態永続化

## 使用例

```rust
use fleetflow_cloud::{CloudProvider, ResourceConfig, Plan};

// プロバイダーを使用
let provider = SomeCloudProvider::new()?;

// 認証確認
provider.check_auth().await?;

// 現在の状態取得
let state = provider.get_state(&["server:myserver"]).await?;

// プラン作成
let plan = provider.plan(&desired_configs, &state).await?;

// 適用
let results = provider.apply(&plan).await?;
```

## 設計原則

- **宣言的**: 期待状態を定義し、差分を計算
- **冪等性**: 同じ設定を何度適用しても同じ結果
- **プロバイダー非依存**: 統一インターフェースで複数クラウドをサポート

## 関連ドキュメント

- [仕様書](../../spec/08-cloud-infrastructure.md)
- [設計書](../../design/04-cloud-infrastructure.md)

## ライセンス

MIT OR Apache-2.0
