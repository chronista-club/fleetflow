//! ヘルスモニター — Docker コンテナ状態監視 + 異常検知 + CP アラート送信

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bollard::Docker;
use bollard::query_parameters::{InspectContainerOptions, ListContainersOptions};
use serde_json::json;
use tracing::{debug, error, info, warn};
use unison::network::client::ProtocolClient;

/// モニター設定
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    /// 監視間隔（秒）
    pub interval_secs: u64,
    /// リスタート閾値（この回数を超えたらアラート）
    pub restart_threshold: u32,
    /// アラートクールダウン（秒）— 同一コンテナ・同一アラートの重複抑制
    pub alert_cooldown_secs: u64,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            interval_secs: 30,
            restart_threshold: 3,
            alert_cooldown_secs: 300,
        }
    }
}

/// コンテナ状態（前回値との比較に使用）
#[derive(Debug, Clone)]
pub(crate) struct ContainerState {
    status: String,
    restart_count: i64,
    health: Option<String>,
}

/// 検知されたアラート
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedAlert {
    pub container_name: String,
    pub alert_type: AlertType,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AlertType {
    RestartLoop,
    UnexpectedStop,
    Unhealthy,
}

impl AlertType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RestartLoop => "restart_loop",
            Self::UnexpectedStop => "unexpected_stop",
            Self::Unhealthy => "unhealthy",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    Warning,
    Critical,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }
}

/// クールダウンキー: (container_name, alert_type)
type CooldownKey = (String, String);

/// モニターループを実行
pub async fn run_loop(
    client: &Arc<ProtocolClient>,
    server_slug: &str,
    config: &MonitorConfig,
) {
    let docker = match Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(e) => {
            error!(error = %e, "Docker 接続失敗 — モニター停止");
            return;
        }
    };

    info!(
        interval = config.interval_secs,
        threshold = config.restart_threshold,
        "ヘルスモニター開始"
    );

    let mut prev_states: HashMap<String, ContainerState> = HashMap::new();
    let mut cooldowns: HashMap<CooldownKey, Instant> = HashMap::new();

    loop {
        tokio::time::sleep(Duration::from_secs(config.interval_secs)).await;

        match check_containers(&docker, &prev_states, config).await {
            Ok((alerts, new_states)) => {
                for alert in &alerts {
                    let key = (
                        alert.container_name.clone(),
                        alert.alert_type.as_str().to_string(),
                    );

                    // クールダウンチェック
                    if let Some(last_sent) = cooldowns.get(&key) {
                        if last_sent.elapsed() < Duration::from_secs(config.alert_cooldown_secs) {
                            debug!(
                                container = %alert.container_name,
                                alert_type = alert.alert_type.as_str(),
                                "クールダウン中 — アラートスキップ"
                            );
                            continue;
                        }
                    }

                    // CP にアラート送信
                    if let Err(e) = send_alert(client, server_slug, alert).await {
                        warn!(error = %e, "アラート送信失敗");
                    } else {
                        cooldowns.insert(key, Instant::now());
                        info!(
                            container = %alert.container_name,
                            alert_type = alert.alert_type.as_str(),
                            severity = alert.severity.as_str(),
                            "アラート送信完了"
                        );
                    }
                }
                prev_states = new_states;
            }
            Err(e) => {
                warn!(error = %e, "コンテナチェック失敗");
            }
        }
    }
}

/// 全コンテナを取得し、前回状態と比較して異常を検知
pub(crate) async fn check_containers(
    docker: &Docker,
    prev_states: &HashMap<String, ContainerState>,
    config: &MonitorConfig,
) -> anyhow::Result<(Vec<DetectedAlert>, HashMap<String, ContainerState>)> {
    let containers = docker
        .list_containers(Some(ListContainersOptions {
            all: true,
            ..Default::default()
        }))
        .await?;

    let mut alerts = Vec::new();
    let mut new_states = HashMap::new();

    for container in &containers {
        let name = match container.names.as_ref().and_then(|n| n.first()) {
            Some(n) => n.trim_start_matches('/').to_string(),
            None => continue,
        };

        let id = match &container.id {
            Some(id) => id,
            None => continue,
        };

        // inspect で詳細取得
        let info = match docker
            .inspect_container(id, None::<InspectContainerOptions>)
            .await
        {
            Ok(info) => info,
            Err(e) => {
                debug!(container = %name, error = %e, "inspect 失敗 — スキップ");
                continue;
            }
        };

        let state = match &info.state {
            Some(s) => s,
            None => continue,
        };

        let status = state.status.as_ref().map(|s| s.to_string()).unwrap_or_default();
        let restart_count = info
            .restart_count
            .unwrap_or(0);
        let health = state
            .health
            .as_ref()
            .and_then(|h| h.status.as_ref())
            .map(|s| s.to_string());

        let current = ContainerState {
            status: status.clone(),
            restart_count,
            health: health.clone(),
        };

        // 異常検知
        let detected = detect_anomalies(&name, &current, prev_states.get(&name), config);
        alerts.extend(detected);

        new_states.insert(name, current);
    }

    Ok((alerts, new_states))
}

/// 異常検知ロジック
pub(crate) fn detect_anomalies(
    container_name: &str,
    current: &ContainerState,
    previous: Option<&ContainerState>,
    config: &MonitorConfig,
) -> Vec<DetectedAlert> {
    let mut alerts = Vec::new();

    // 1. restart_loop: リスタート回数が閾値超え & 前回から増加
    if current.restart_count > config.restart_threshold as i64 {
        let prev_count = previous.map(|p| p.restart_count).unwrap_or(0);
        if current.restart_count > prev_count {
            alerts.push(DetectedAlert {
                container_name: container_name.to_string(),
                alert_type: AlertType::RestartLoop,
                severity: Severity::Critical,
                message: format!(
                    "コンテナ {} がリスタートループ: {} 回（閾値: {}）",
                    container_name, current.restart_count, config.restart_threshold
                ),
            });
        }
    }

    // 2. unexpected_stop: running → exited/dead
    if let Some(prev) = previous
        && prev.status == "running"
        && (current.status == "exited" || current.status == "dead")
    {
        alerts.push(DetectedAlert {
            container_name: container_name.to_string(),
            alert_type: AlertType::UnexpectedStop,
            severity: Severity::Critical,
            message: format!(
                "コンテナ {} が予期しない停止: {} → {}",
                container_name, prev.status, current.status
            ),
        });
    }

    // 3. unhealthy: Docker ヘルスチェック
    if current.health.as_deref() == Some("unhealthy") {
        alerts.push(DetectedAlert {
            container_name: container_name.to_string(),
            alert_type: AlertType::Unhealthy,
            severity: Severity::Warning,
            message: format!(
                "コンテナ {} が unhealthy",
                container_name
            ),
        });
    }

    alerts
}

/// CP にアラートを送信
async fn send_alert(
    client: &ProtocolClient,
    server_slug: &str,
    alert: &DetectedAlert,
) -> anyhow::Result<()> {
    let channel = client.open_channel("server").await?;

    channel
        .request(
            "alert",
            json!({
                "server_slug": server_slug,
                "container_name": alert.container_name,
                "alert_type": alert.alert_type.as_str(),
                "severity": alert.severity.as_str(),
                "message": alert.message,
            }),
        )
        .await?;

    channel.close().await.ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> MonitorConfig {
        MonitorConfig {
            interval_secs: 30,
            restart_threshold: 3,
            alert_cooldown_secs: 300,
        }
    }

    #[test]
    fn test_detect_restart_loop() {
        let config = default_config();
        let current = ContainerState {
            status: "running".into(),
            restart_count: 5,
            health: None,
        };
        let previous = ContainerState {
            status: "running".into(),
            restart_count: 3,
            health: None,
        };

        let alerts = detect_anomalies("web", &current, Some(&previous), &config);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, AlertType::RestartLoop);
        assert_eq!(alerts[0].severity, Severity::Critical);
    }

    #[test]
    fn test_no_restart_loop_below_threshold() {
        let config = default_config();
        let current = ContainerState {
            status: "running".into(),
            restart_count: 2,
            health: None,
        };

        let alerts = detect_anomalies("web", &current, None, &config);
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_no_restart_loop_same_count() {
        let config = default_config();
        let current = ContainerState {
            status: "running".into(),
            restart_count: 5,
            health: None,
        };
        let previous = ContainerState {
            status: "running".into(),
            restart_count: 5,
            health: None,
        };

        let alerts = detect_anomalies("web", &current, Some(&previous), &config);
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_detect_unexpected_stop() {
        let config = default_config();
        let current = ContainerState {
            status: "exited".into(),
            restart_count: 0,
            health: None,
        };
        let previous = ContainerState {
            status: "running".into(),
            restart_count: 0,
            health: None,
        };

        let alerts = detect_anomalies("db", &current, Some(&previous), &config);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, AlertType::UnexpectedStop);
    }

    #[test]
    fn test_detect_unexpected_stop_dead() {
        let config = default_config();
        let current = ContainerState {
            status: "dead".into(),
            restart_count: 0,
            health: None,
        };
        let previous = ContainerState {
            status: "running".into(),
            restart_count: 0,
            health: None,
        };

        let alerts = detect_anomalies("db", &current, Some(&previous), &config);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, AlertType::UnexpectedStop);
    }

    #[test]
    fn test_no_unexpected_stop_without_previous() {
        let config = default_config();
        let current = ContainerState {
            status: "exited".into(),
            restart_count: 0,
            health: None,
        };

        let alerts = detect_anomalies("db", &current, None, &config);
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_detect_unhealthy() {
        let config = default_config();
        let current = ContainerState {
            status: "running".into(),
            restart_count: 0,
            health: Some("unhealthy".into()),
        };

        let alerts = detect_anomalies("api", &current, None, &config);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, AlertType::Unhealthy);
        assert_eq!(alerts[0].severity, Severity::Warning);
    }

    #[test]
    fn test_healthy_container_no_alerts() {
        let config = default_config();
        let current = ContainerState {
            status: "running".into(),
            restart_count: 0,
            health: Some("healthy".into()),
        };
        let previous = ContainerState {
            status: "running".into(),
            restart_count: 0,
            health: Some("healthy".into()),
        };

        let alerts = detect_anomalies("api", &current, Some(&previous), &config);
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_multiple_anomalies() {
        let config = default_config();
        // restart_loop + unhealthy 同時検知
        let current = ContainerState {
            status: "running".into(),
            restart_count: 10,
            health: Some("unhealthy".into()),
        };
        let previous = ContainerState {
            status: "running".into(),
            restart_count: 5,
            health: Some("healthy".into()),
        };

        let alerts = detect_anomalies("api", &current, Some(&previous), &config);
        assert_eq!(alerts.len(), 2);

        let types: Vec<_> = alerts.iter().map(|a| &a.alert_type).collect();
        assert!(types.contains(&&AlertType::RestartLoop));
        assert!(types.contains(&&AlertType::Unhealthy));
    }

    #[test]
    fn test_cooldown_key_uniqueness() {
        // CooldownKey が (container_name, alert_type) で区別されることを確認
        let key1: CooldownKey = ("web".into(), "restart_loop".into());
        let key2: CooldownKey = ("web".into(), "unhealthy".into());
        let key3: CooldownKey = ("db".into(), "restart_loop".into());

        let mut map: HashMap<CooldownKey, Instant> = HashMap::new();
        map.insert(key1.clone(), Instant::now());
        map.insert(key2.clone(), Instant::now());
        map.insert(key3.clone(), Instant::now());

        assert_eq!(map.len(), 3);
        assert!(map.contains_key(&key1));
    }
}
