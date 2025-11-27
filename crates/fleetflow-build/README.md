# fleetflow-build

FleetFlowのDockerビルド機能を提供するクレート。

## 概要

`fleetflow-build`は、FleetFlowの設定ファイルからDockerイメージをビルドする機能を提供します。

## 主要コンポーネント

### モジュール

- **resolver**: Dockerfile検出と変数展開
- **context**: ビルドコンテキスト作成
- **builder**: Bollard APIでのビルド実行
- **progress**: 進捗表示

## 使用例

```rust
use fleetflow_build::{DockerfileResolver, BuildContext, ImageBuilder};

// Dockerfile検出
let resolver = DockerfileResolver::new(project_root);
let dockerfile = resolver.resolve(&service)?;

// ビルドコンテキスト作成
let context = BuildContext::new(project_root, &service)?;
let tar = context.create_tar()?;

// イメージビルド
let builder = ImageBuilder::new(docker_client);
builder.build(&dockerfile, tar, &build_config).await?;
```

## 機能

- **規約ベースのDockerfile検出**: `services/{name}/Dockerfile`を自動検出
- **明示的なパス指定**: KDL設定での`dockerfile`フィールド
- **変数展開**: ビルド引数での変数置換
- **プロジェクトルートベースのコンテキスト**: 相対パスでの柔軟な設定

## 関連ドキュメント

- [仕様書](../../spec/07-docker-build.md)
- [設計書](../../design/03-docker-build.md)
- [利用ガイド](../../guides/02-docker-build.md)

## ライセンス

MIT OR Apache-2.0
