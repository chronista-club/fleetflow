use colored::Colorize;
use futures_util::stream::StreamExt;

/// イメージ名とタグを分離
/// 例: "redis:7-alpine" -> ("redis", "7-alpine")
///     "postgres" -> ("postgres", "latest")
pub fn parse_image_tag(image: &str) -> (&str, &str) {
    if let Some((name, tag)) = image.split_once(':') {
        (name, tag)
    } else {
        (image, "latest")
    }
}

/// イメージpullの内部ヘルパー（ストリーム処理を共通化）
async fn pull_image_inner(
    docker: &bollard::Docker,
    image: &str,
    pre_msg: &str,
    done_msg: &str,
) -> anyhow::Result<()> {
    let (image_name, tag) = parse_image_tag(image);

    println!("{}", pre_msg);

    let auth = fleetflow_build::RegistryAuth::new();
    let credentials = auth
        .get_credentials(image)
        .map_err(|e| anyhow::anyhow!("認証情報の取得に失敗: {}", e))?;

    #[allow(deprecated)]
    let options = bollard::image::CreateImageOptions {
        from_image: image_name,
        tag,
        ..Default::default()
    };

    #[allow(deprecated)]
    let mut stream = docker.create_image(Some(options), None, credentials);

    while let Some(info) = stream.next().await {
        match info {
            Ok(bollard::models::CreateImageInfo {
                status: Some(status),
                progress: Some(progress),
                ..
            }) => {
                print!("\r  ↓ {}: {}", status, progress);
                use std::io::Write;
                std::io::stdout().flush()?;
            }
            Ok(bollard::models::CreateImageInfo {
                status: Some(status),
                ..
            }) => {
                print!("\r  ↓ {}                    ", status);
                use std::io::Write;
                std::io::stdout().flush()?;
            }
            Err(e) => {
                println!();
                return Err(anyhow::anyhow!(
                    "イメージのダウンロードに失敗しました: {}",
                    e
                ));
            }
            _ => {}
        }
    }

    println!();
    println!("{}", done_msg);

    Ok(())
}

/// Dockerイメージを自動的にpull
pub async fn pull_image(docker: &bollard::Docker, image: &str) -> anyhow::Result<()> {
    pull_image_inner(
        docker,
        image,
        &format!(
            "  ℹ イメージが見つかりません: {}\n  ↓ イメージをダウンロード中...",
            image.cyan()
        ),
        "  ✓ イメージのダウンロード完了",
    )
    .await
}

/// 最新イメージを強制的にpull（--pull フラグ用）
pub async fn pull_image_always(docker: &bollard::Docker, image: &str) -> anyhow::Result<()> {
    pull_image_inner(
        docker,
        image,
        &format!("  ↓ 最新イメージをプル中: {}", image.cyan()),
        "  ✓ プル完了",
    )
    .await
}

/// ネットワークを作成（既に存在する場合はスキップ）
pub async fn ensure_network(docker: &bollard::Docker, network_name: &str) -> anyhow::Result<()> {
    let network_config = bollard::models::NetworkCreateRequest {
        name: network_name.to_string(),
        driver: Some("bridge".to_string()),
        ..Default::default()
    };

    match docker.create_network(network_config).await {
        Ok(_) => {
            println!("  ✓ ネットワーク作成完了");
        }
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 409, ..
        }) => {
            println!("  ✓ ネットワークは既に存在します");
        }
        Err(e) => {
            return Err(anyhow::anyhow!("ネットワーク作成エラー: {}", e));
        }
    }

    Ok(())
}

/// コンテナが存在しない場合にイメージ確認→pull→作成→起動する
#[allow(deprecated)]
pub async fn ensure_container_running(
    docker: &bollard::Docker,
    container_name: &str,
    container_config: bollard::container::Config<String>,
    create_options: bollard::container::CreateContainerOptions<String>,
) -> anyhow::Result<()> {
    let image = container_config
        .image
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("イメージ設定が見つかりません"))?;

    // イメージの存在確認とpull
    match docker.inspect_image(image).await {
        Ok(_) => {}
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => {
            pull_image(docker, image).await?;
        }
        Err(e) => return Err(e.into()),
    }

    // コンテナ作成
    docker
        .create_container(Some(create_options), container_config)
        .await?;

    // コンテナ起動
    docker
        .start_container(
            container_name,
            None::<bollard::query_parameters::StartContainerOptions>,
        )
        .await?;

    println!("  ✓ コンテナを作成・起動しました");

    Ok(())
}

/// Docker接続を初期化（エラーハンドリング付き）
pub async fn init_docker_with_error_handling() -> anyhow::Result<bollard::Docker> {
    match bollard::Docker::connect_with_local_defaults() {
        Ok(docker) => {
            // 接続テスト
            match docker.ping().await {
                Ok(_) => Ok(docker),
                Err(e) => {
                    eprintln!();
                    eprintln!("{}", "✗ Docker接続エラー".red().bold());
                    eprintln!();
                    eprintln!("{}", "原因:".yellow());
                    eprintln!("  {}", e);
                    eprintln!();
                    eprintln!("{}", "解決方法:".yellow());
                    eprintln!("  • Dockerが起動しているか確認してください");
                    eprintln!(
                        "  • OrbStackまたはDocker Desktopがインストールされているか確認してください"
                    );
                    eprintln!("  • docker ps コマンドが正常に動作するか確認してください");
                    Err(anyhow::anyhow!("Docker接続に失敗しました"))
                }
            }
        }
        Err(e) => {
            eprintln!();
            eprintln!("{}", "✗ Docker接続エラー".red().bold());
            eprintln!();
            eprintln!("{}", "原因:".yellow());
            eprintln!("  {}", e);
            eprintln!();
            eprintln!("{}", "解決方法:".yellow());
            eprintln!("  • Dockerが起動しているか確認してください");
            eprintln!("  • OrbStackまたはDocker Desktopがインストールされているか確認してください");
            eprintln!("  • docker ps コマンドが正常に動作するか確認してください");
            Err(anyhow::anyhow!("Docker接続に失敗しました"))
        }
    }
}
