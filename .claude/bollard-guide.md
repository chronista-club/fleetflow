# Bollard 使い方ガイド（FleetFlowでの実装）

## 概要

**Bollard**は、Rust製の非同期Docker API クライアントライブラリです。
HyperとTokioを使用し、futuresとasync/awaitパラダイムで実装されています。

- **公式ドキュメント**: https://docs.rs/bollard/
- **GitHub**: https://github.com/fussybeaver/bollard
- **Docker API バージョン**: 1.49（最新）

## 基本的な接続

```rust
use bollard::Docker;

// OS固有のデフォルト設定で接続（推奨）
let docker = Docker::connect_with_local_defaults()?;

// 他の接続方法
// Unix socket
let docker = Docker::connect_with_socket_defaults()?;

// HTTP (DOCKER_HOST環境変数 or localhost:2375)
let docker = Docker::connect_with_http_defaults()?;

// SSL/TLS
let docker = Docker::connect_with_ssl_defaults()?;
```

## コンテナ操作

### 1. コンテナの作成

```rust
use bollard::container::{Config, CreateContainerOptions};
use bollard::models::HostConfig;
use std::collections::HashMap;

let config = Config {
    image: Some("postgres:16".to_string()),
    env: Some(vec![
        "POSTGRES_PASSWORD=postgres".to_string(),
    ]),
    exposed_ports: Some({
        let mut ports = HashMap::new();
        ports.insert("5432/tcp".to_string(), HashMap::new());
        ports
    }),
    host_config: Some(HostConfig {
        port_bindings: Some({
            let mut bindings = HashMap::new();
            bindings.insert(
                "5432/tcp".to_string(),
                Some(vec![PortBinding {
                    host_ip: Some("0.0.0.0".to_string()),
                    host_port: Some("11432".to_string()),
                }]),
            );
            bindings
        }),
        binds: Some(vec![
            "/path/to/data:/var/lib/postgresql/data:rw".to_string(),
        ]),
        ..Default::default()
    }),
    ..Default::default()
};

let options = CreateContainerOptions {
    name: "flow-postgres",
    platform: None,
};

let response = docker.create_container(Some(options), config).await?;
println!("Container ID: {}", response.id);
```

### 2. コンテナの起動

```rust
docker.start_container::<String>(&container_id, None).await?;
```

### 3. コンテナの停止

```rust
use bollard::container::StopContainerOptions;

docker.stop_container(
    &container_id,
    Some(StopContainerOptions { t: 10 }) // 10秒のタイムアウト
).await?;
```

### 4. コンテナの削除

```rust
use bollard::container::RemoveContainerOptions;

docker.remove_container(
    &container_id,
    Some(RemoveContainerOptions {
        force: true,    // 強制削除
        v: true,        // ボリュームも削除
        ..Default::default()
    })
).await?;
```

### 5. コンテナ一覧の取得

```rust
use bollard::container::ListContainersOptions;

let containers = docker.list_containers(
    Some(ListContainersOptions::<String> {
        all: true,  // 停止中も含む
        ..Default::default()
    })
).await?;

for container in containers {
    println!("Name: {:?}, Status: {}", container.names, container.status);
}
```

## イメージ操作

### イメージのPull

```rust
use bollard::image::CreateImageOptions;
use futures_util::stream::TryStreamExt;

let options = Some(CreateImageOptions {
    from_image: "postgres",
    tag: "16",
    ..Default::default()
});

let mut stream = docker.create_image(options, None, None);

while let Some(info) = stream.try_next().await? {
    println!("{:?}", info);
}
```

### イメージ一覧の取得

```rust
use bollard::image::ListImagesOptions;

let images = docker.list_images(
    Some(ListImagesOptions::<String> {
        all: true,
        ..Default::default()
    })
).await?;
```

## エラーハンドリング

### 主なエラータイプ

```rust
use bollard::errors::Error;

match docker.create_container(options, config).await {
    Ok(response) => {
        println!("Created: {}", response.id);
    }
    Err(Error::DockerResponseServerError {
        status_code: 409, // Conflict
        message,
    }) => {
        println!("Container already exists: {}", message);
        // 既存コンテナを起動
    }
    Err(Error::DockerResponseServerError {
        status_code: 404, // Not Found
        message,
    }) => {
        println!("Image not found: {}", message);
        // イメージをpull
    }
    Err(e) => {
        eprintln!("Error: {}", e);
    }
}
```

### FleetFlowでの実装例

```rust
match docker.create_container(Some(create_options.clone()), container_config.clone()).await {
    Ok(response) => {
        println!("  ✓ コンテナ作成: {}", response.id);

        // コンテナ起動
        docker.start_container::<String>(&response.id, None).await?;
        println!("  ✓ 起動完了");
    }
    Err(bollard::errors::Error::DockerResponseServerError { status_code: 409, .. }) => {
        // コンテナが既に存在する場合
        println!("  ℹ コンテナは既に存在します");
        let container_name = &create_options.name;

        // 既存コンテナを起動
        match docker.start_container::<String>(container_name, None).await {
            Ok(_) => println!("  ✓ 既存コンテナを起動"),
            Err(e) => println!("  ⚠ 起動エラー: {}", e),
        }
    }
    Err(e) => {
        return Err(anyhow::anyhow!("コンテナ作成エラー: {}", e));
    }
}
```

## ログ取得

```rust
use bollard::container::LogsOptions;
use futures_util::stream::TryStreamExt;

let options = Some(LogsOptions::<String> {
    stdout: true,
    stderr: true,
    follow: true,  // ストリーミング
    tail: "100",   // 最後の100行
    ..Default::default()
});

let mut stream = docker.logs(&container_id, options);

while let Some(log) = stream.try_next().await? {
    print!("{}", log);
}
```

## ボリューム操作

```rust
use bollard::volume::{CreateVolumeOptions, RemoveVolumeOptions};

// ボリューム作成
let options = CreateVolumeOptions {
    name: "flow_postgres_data",
    driver: "local",
    ..Default::default()
};

let volume = docker.create_volume(options).await?;

// ボリューム削除
docker.remove_volume("flow_postgres_data", None).await?;
```

## ネットワーク操作

```rust
use bollard::network::CreateNetworkOptions;

let options = CreateNetworkOptions {
    name: "flow_network",
    driver: "bridge",
    ..Default::default()
};

let network = docker.create_network(options).await?;
```

## ベストプラクティス

### 1. 接続の再利用

```rust
// Docker接続は再利用可能
let docker = Docker::connect_with_local_defaults()?;

// 複数の操作で同じインスタンスを使用
docker.create_container(...).await?;
docker.start_container(...).await?;
docker.list_containers(...).await?;
```

### 2. エラーの詳細な分類

```rust
match result {
    Err(Error::DockerResponseServerError { status_code: 404, .. }) => {
        // イメージが見つからない
    }
    Err(Error::DockerResponseServerError { status_code: 409, .. }) => {
        // 既に存在
    }
    Err(Error::DockerResponseServerError { status_code: 500, .. }) => {
        // サーバーエラー（ポート競合など）
    }
    Err(e) => {
        // その他のエラー
    }
    Ok(result) => {
        // 成功
    }
}
```

### 3. 非同期処理

```rust
// Tokioランタイムが必要
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_local_defaults()?;

    // 非同期操作
    let containers = docker.list_containers(None).await?;

    Ok(())
}
```

## 参考リンク

- [Bollard公式ドキュメント](https://docs.rs/bollard/)
- [Docker API Reference](https://docs.docker.com/engine/api/)
- [FleetFlow実装例](../crates/flow-cli/src/main.rs)
- [converter実装](../crates/flow-container/src/converter.rs)

## 未実装機能（今後追加予定）

- [ ] イメージの自動pull
- [ ] コンテナログのストリーミング表示
- [ ] ヘルスチェック
- [ ] ネットワーク管理
- [ ] ボリューム管理
- [ ] Docker Composeのように依存関係順に起動
