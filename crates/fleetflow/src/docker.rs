use colored::Colorize;
use futures_util::stream::StreamExt;

/// Docker config.json からレジストリの認証情報を取得
pub fn get_docker_credentials(registry: &str) -> Option<bollard::auth::DockerCredentials> {
    // ~/.docker/config.json を読み込み
    let home = std::env::var("HOME").ok()?;
    let config_path = format!("{}/.docker/config.json", home);
    let config_content = std::fs::read_to_string(&config_path).ok()?;
    let config: serde_json::Value = serde_json::from_str(&config_content).ok()?;

    // auths セクションからレジストリの認証情報を取得
    let auths = config.get("auths")?.as_object()?;
    let auth_entry = auths.get(registry)?;
    let auth_b64 = auth_entry.get("auth")?.as_str()?;

    // Base64 デコード (username:password 形式)
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(auth_b64)
        .ok()?;
    let auth_str = String::from_utf8(decoded).ok()?;
    let (username, password) = auth_str.split_once(':')?;

    Some(bollard::auth::DockerCredentials {
        username: Some(username.to_string()),
        password: Some(password.to_string()),
        serveraddress: Some(registry.to_string()),
        ..Default::default()
    })
}

/// イメージ名からレジストリを抽出
pub fn extract_registry(image: &str) -> Option<&str> {
    // ghcr.io/owner/repo:tag のような形式
    // docker.io/library/nginx:latest のような形式
    // 最初の / の前がレジストリ
    if image.contains('/') {
        let parts: Vec<&str> = image.split('/').collect();
        let first = parts[0];
        // レジストリは . または : を含む（例: ghcr.io, localhost:5000）
        if first.contains('.') || first.contains(':') {
            return Some(first);
        }
    }
    None
}

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

    // レジストリから認証情報を取得（あれば）
    let credentials = extract_registry(image).and_then(get_docker_credentials);

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

    // レジストリから認証情報を取得（あれば）
    let credentials = extract_registry(image).and_then(get_docker_credentials);

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
