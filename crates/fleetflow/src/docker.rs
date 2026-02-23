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

/// Dockerイメージを自動的にpull
pub async fn pull_image(docker: &bollard::Docker, image: &str) -> anyhow::Result<()> {
    let (image_name, tag) = parse_image_tag(image);

    println!("  ℹ イメージが見つかりません: {}", image.cyan());
    println!("  ↓ イメージをダウンロード中...");

    // RegistryAuth で認証情報を取得（config.json → 環境変数のフォールバック）
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
                // 進捗を表示（同じ行に上書き）
                print!("\r  ↓ {}: {}", status, progress);
                use std::io::Write;
                std::io::stdout().flush()?;
            }
            Ok(bollard::models::CreateImageInfo {
                status: Some(status),
                ..
            }) => {
                // 進捗なしの場合
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
    println!("  ✓ イメージのダウンロード完了");

    Ok(())
}

/// 最新イメージを強制的にpull（--pull フラグ用）
pub async fn pull_image_always(docker: &bollard::Docker, image: &str) -> anyhow::Result<()> {
    let (image_name, tag) = parse_image_tag(image);

    println!("  ↓ 最新イメージをプル中: {}", image.cyan());

    // RegistryAuth で認証情報を取得（config.json → 環境変数のフォールバック）
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
                return Err(anyhow::anyhow!("イメージのプルに失敗しました: {}", e));
            }
            _ => {}
        }
    }

    println!();
    println!("  ✓ プル完了");

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
