# Spec: ワークロード (Workload)

## 1. 概念 (Concept)

「ワークロード」は、特定のアプリケーション実行形態に必要な一連のサービス群や設定をパッケージ化したものです。

### 背景
これまで FleetFlow では `flow.kdl` に全てのサービスを列挙するか、手動でファイルを分割・インクルードする必要がありました。
ワークロードを導入することで、「何を動かしたいか」を宣言するだけで、必要な構成要素が自動的に組み込まれるようになります。

## 2. 挙動 (Behavior)

### 暗黙的インクルード (Implicit Include)
`.fleetflow/flow.kdl` 内で `workload "name"` が宣言されると、FleetFlow は以下の場所から自動的に設定ファイルを検索し、ロードします。

1. `.fleetflow/workloads/{name}.kdl`
2. `.fleetflow/workloads/{name}/*.kdl`
3. (将来的に) 共有ライブラリやリモートのワークロード定義

### 構成要素のオーバーライド
メインの `flow.kdl` 内で、ワークロードが提供するサービスの一部を上書き（オーバーライド）することも可能です。

```kdl
workload "fullstack"

service "api" {
    // ワークロードで定義された api サービスのイメージだけ書き換える
    image "my-custom-api:latest"
}
```

## 3. 記述例

### 最小限の `flow.kdl`
```kdl
project "my-app"
workload "spa-with-backend"

stage "local" {
    // ワークロードに含まれる全てのサービスを起動
}
```

### ワークロード定義側 (`workloads/spa-with-backend.kdl`)
```kdl
service "frontend" {
    build "./frontend"
}
service "backend" {
    build "./backend"
}
service "db" {
    image "postgres:16"
}
```
