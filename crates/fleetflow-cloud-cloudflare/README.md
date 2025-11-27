# fleetflow-cloud-cloudflare

Cloudflareプロバイダーの実装（スケルトン）。

## 概要

`fleetflow-cloud-cloudflare`は、[wrangler](https://developers.cloudflare.com/workers/wrangler/) CLIをラップしてCloudflareのリソースを管理します。

> **注意**: このクレートは現在スケルトン実装です。

## 前提条件

- wrangler CLIのインストール
- Cloudflareの認証設定（`wrangler login`）

## サポート予定リソース

- **R2バケット**: オブジェクトストレージ
- **Workers**: エッジコンピューティング
- **DNS**: ドメイン管理

## 使用例

```rust
use fleetflow_cloud_cloudflare::CloudflareProvider;
use fleetflow_cloud::CloudProvider;

let provider = CloudflareProvider::new("my-account-id")?;

// 認証確認
provider.check_auth().await?;
```

## KDL設定例（予定）

```kdl
providers {
    cloudflare account_id="xxx"
}

r2-bucket "assets" {
    location "APAC"
}

worker "api" {
    script "workers/api/index.ts"
    routes {
        route "api.example.com/*"
    }
}
```

## 関連ドキュメント

- [fleetflow-cloud](../fleetflow-cloud/README.md)
- [仕様書](../../spec/08-cloud-infrastructure.md)
- [設計書](../../design/04-cloud-infrastructure.md)

## ライセンス

MIT OR Apache-2.0
