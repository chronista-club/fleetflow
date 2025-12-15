//! 依存サービス待機モジュール（Exponential Backoff）
//!
//! K8sのReadiness Probeのコンセプトを取り入れた、
//! 依存サービスの準備完了を待機する機能を提供します。

// Bollard 0.19.4 の非推奨APIを一時的に使用
#![allow(deprecated)]

use crate::error::{ContainerError, Result};
use bollard::Docker;
use bollard::container::InspectContainerOptions;
use bollard::models::HealthStatusEnum;
use fleetflow_atom::WaitConfig;
use std::time::Duration;
use tokio::time::sleep;

/// 依存サービスの準備完了を待機
///
/// # Arguments
/// * `docker` - Docker接続
/// * `container_name` - 待機対象のコンテナ名
/// * `config` - 待機設定（exponential backoff）
///
/// # Returns
/// * `Ok(())` - サービスが準備完了
/// * `Err(ContainerError)` - タイムアウトまたはエラー
pub async fn wait_for_service(
    docker: &Docker,
    container_name: &str,
    config: &WaitConfig,
) -> Result<()> {
    for attempt in 0..config.max_retries {
        match check_container_health(docker, container_name).await {
            Ok(true) => {
                return Ok(());
            }
            Ok(false) => {
                // コンテナは存在するが、まだ準備完了していない
            }
            Err(_) => {
                // コンテナが見つからない、または他のエラー
            }
        }

        // 最後の試行でなければ待機
        if attempt + 1 < config.max_retries {
            let delay_ms = config.delay_for_attempt(attempt);
            sleep(Duration::from_millis(delay_ms)).await;
        }
    }

    Err(ContainerError::ServiceWaitTimeout {
        service: container_name.to_string(),
        max_retries: config.max_retries,
    })
}

/// 複数の依存サービスの準備完了を待機
pub async fn wait_for_services(
    docker: &Docker,
    container_names: &[String],
    config: &WaitConfig,
) -> Result<()> {
    for container_name in container_names {
        wait_for_service(docker, container_name, config).await?;
    }
    Ok(())
}

/// コンテナのヘルス状態を確認
async fn check_container_health(docker: &Docker, container_name: &str) -> Result<bool> {
    let inspect_result = docker
        .inspect_container(container_name, None::<InspectContainerOptions>)
        .await
        .map_err(|e| ContainerError::DockerApiError(e.to_string()))?;

    // コンテナの状態を確認
    let state = inspect_result
        .state
        .ok_or_else(|| ContainerError::ContainerNotFound {
            container: container_name.to_string(),
        })?;

    // Running状態かどうか
    let is_running = state.running.unwrap_or(false);

    if !is_running {
        return Ok(false);
    }

    // ヘルスチェックが設定されている場合、そのステータスを確認
    if let Some(health) = state.health {
        if let Some(status) = health.status {
            return Ok(status == HealthStatusEnum::HEALTHY);
        }
    }

    // ヘルスチェックがない場合はRunning状態で準備完了とみなす
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delay_calculation() {
        let config = WaitConfig {
            max_retries: 5,
            initial_delay_ms: 1000,
            max_delay_ms: 10000,
            multiplier: 2.0,
        };

        assert_eq!(config.delay_for_attempt(0), 1000);
        assert_eq!(config.delay_for_attempt(1), 2000);
        assert_eq!(config.delay_for_attempt(2), 4000);
        assert_eq!(config.delay_for_attempt(3), 8000);
        assert_eq!(config.delay_for_attempt(4), 10000); // capped at max
    }
}
