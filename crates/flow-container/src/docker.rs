use anyhow::Result;
use bollard::Docker;

/// Docker接続を初期化
pub async fn init_docker() -> Result<Docker> {
    let docker = Docker::connect_with_local_defaults()?;
    Ok(docker)
}

/// Dockerバージョンを取得
pub async fn get_docker_version() -> Result<String> {
    let docker = init_docker().await?;
    let version = docker.version().await?;
    Ok(version.version.unwrap_or_else(|| "unknown".to_string()))
}
