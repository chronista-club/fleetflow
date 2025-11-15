use crate::error::{ContainerError, Result};
use bollard::Docker;

/// Docker接続を初期化
pub async fn init_docker() -> Result<Docker> {
    let docker = Docker::connect_with_local_defaults()
        .map_err(|e| ContainerError::DockerConnectionFailed(e.to_string()))?;

    // 接続テスト
    docker
        .ping()
        .await
        .map_err(|e| ContainerError::DockerConnectionFailed(e.to_string()))?;

    Ok(docker)
}

/// Dockerバージョンを取得
pub async fn get_docker_version() -> Result<String> {
    let docker = init_docker().await?;
    let version = docker
        .version()
        .await
        .map_err(|e| ContainerError::DockerApiError(e.to_string()))?;
    Ok(version.version.unwrap_or_else(|| "unknown".to_string()))
}
