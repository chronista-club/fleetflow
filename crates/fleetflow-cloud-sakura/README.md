# fleetflow-cloud-sakura

さくらクラウドプロバイダーの実装。

## 概要

`fleetflow-cloud-sakura`は、[usacloud](https://github.com/sacloud/usacloud) CLIをラップしてさくらクラウドのリソースを管理します。

## 前提条件

- usacloud CLIのインストール
- さくらクラウドの認証設定（`usacloud config`）

## サポートリソース

- **サーバー**: 作成、起動、停止、削除
- **ディスク**: 作成、削除
- **スイッチ**: 作成、削除

## 使用例

```rust
use fleetflow_cloud_sakura::SakuraProvider;
use fleetflow_cloud::CloudProvider;

let provider = SakuraProvider::new("is1a")?;

// 認証確認
provider.check_auth().await?;

// サーバー一覧取得
let state = provider.get_state(&["server:*"]).await?;
```

## KDL設定例

```kdl
providers {
    sakura zone="is1a"
}

server "web" {
    core 2
    memory 4
    disk_size 100
    os "ubuntu2204"
}
```

## 関連ドキュメント

- [fleetflow-cloud](../fleetflow-cloud/README.md)
- [仕様書](../../spec/08-cloud-infrastructure.md)
- [設計書](../../design/04-cloud-infrastructure.md)

## ライセンス

MIT OR Apache-2.0
