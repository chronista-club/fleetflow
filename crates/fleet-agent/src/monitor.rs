//! ヘルスモニター — Docker コンテナ状態監視 + 異常検知 + CP アラート送信

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bollard::Docker;
use bollard::query_parameters::{InspectContainerOptions, ListContainersOptions};
use serde::Serialize;
use serde_json::json;
use tracing::{debug, info, warn};
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

/// 観測対象の container runtime endpoint。
struct RuntimeEndpoint {
    /// runtime 識別子（"docker" | "podman-rootless-<uid>"）
    id: String,
    docker: Docker,
}

/// Podman socket path から runtime_id を導出する（純粋関数）。
///
/// - `/run/podman/podman.sock` → `podman-rootful`
/// - `/run/user/1000/podman/podman.sock` → `podman-rootless-1000`
fn podman_runtime_id(socket_path: &str) -> String {
    if socket_path == "/run/podman/podman.sock" {
        return "podman-rootful".to_string();
    }
    let uid = socket_path
        .strip_prefix("/run/user/")
        .and_then(|rest| rest.split('/').next())
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown");
    format!("podman-rootless-{uid}")
}

/// Podman socket を列挙する。
///
/// - rootful: `/run/podman/podman.sock`（uid 0、system service）
/// - rootless: `/run/user/*/podman/podman.sock`（per-user）
fn discover_podman_sockets() -> Vec<String> {
    let mut sockets = Vec::new();

    let rootful = "/run/podman/podman.sock";
    if std::path::Path::new(rootful).exists() {
        sockets.push(rootful.to_string());
    }

    if let Ok(entries) = std::fs::read_dir("/run/user") {
        for entry in entries.flatten() {
            let sock = entry.path().join("podman/podman.sock");
            if sock.exists()
                && let Some(s) = sock.to_str()
            {
                sockets.push(s.to_string());
            }
        }
    }

    sockets
}

/// container runtime を auto-discovery する。
///
/// - root Docker: `connect_with_local_defaults`（常に試行、socket 無ければ除外）
/// - rootful Podman: `/run/podman/podman.sock`、存在すれば adopt
/// - rootless Podman: `/run/user/*/podman/podman.sock` を glob、存在分のみ
///
/// socket の存在 = opt-in。接続失敗は warn してスキップ（fail-soft）。
fn discover_runtimes() -> Vec<RuntimeEndpoint> {
    let mut runtimes = Vec::new();

    match Docker::connect_with_local_defaults() {
        Ok(docker) => runtimes.push(RuntimeEndpoint {
            id: "docker".to_string(),
            docker,
        }),
        Err(e) => debug!(error = %e, "root Docker 未接続 — スキップ"),
    }

    for path in discover_podman_sockets() {
        let unix_url = format!("unix://{path}");
        match Docker::connect_with_unix(&unix_url, 120, bollard::API_DEFAULT_VERSION) {
            Ok(docker) => runtimes.push(RuntimeEndpoint {
                id: podman_runtime_id(&path),
                docker,
            }),
            Err(e) => warn!(socket = %path, error = %e, "Podman socket 接続失敗 — スキップ"),
        }
    }

    runtimes
}

/// container の labels から fleetflow attribution（project/stage/service）を
/// 抽出する（純粋関数）。Quadlet `.container` units にも同 label を付与する。
fn extract_attribution(
    labels: &HashMap<String, String>,
) -> (Option<String>, Option<String>, Option<String>) {
    (
        labels.get("fleetflow.project").cloned(),
        labels.get("fleetflow.stage").cloned(),
        labels.get("fleetflow.service").cloned(),
    )
}

/// inventory entry — `inventory_report` の wire 形式に対応。
#[derive(Debug, Clone, Serialize)]
struct ContainerInventoryEntry {
    runtime: String,
    container_id: String,
    container_name: String,
    status: String,
    health: Option<String>,
    image: Option<String>,
    project: Option<String>,
    stage: Option<String>,
    service: Option<String>,
}

/// 1 runtime の全 container を inventory entry に変換する。
async fn collect_runtime_inventory(
    rt: &RuntimeEndpoint,
) -> anyhow::Result<Vec<ContainerInventoryEntry>> {
    let containers = rt
        .docker
        .list_containers(Some(ListContainersOptions {
            all: true,
            ..Default::default()
        }))
        .await?;

    let mut entries = Vec::new();
    for c in &containers {
        let Some(container_id) = c.id.clone() else {
            continue;
        };
        let container_name = c
            .names
            .as_ref()
            .and_then(|n| n.first())
            .map(|n| n.trim_start_matches('/').to_string())
            .unwrap_or_default();
        let labels = c.labels.clone().unwrap_or_default();
        let (project, stage, service) = extract_attribution(&labels);
        let status = c.state.as_ref().map(|s| s.to_string()).unwrap_or_default();
        let health = c
            .health
            .as_ref()
            .and_then(|h| h.status.as_ref())
            .map(|s| s.to_string());

        entries.push(ContainerInventoryEntry {
            runtime: rt.id.clone(),
            container_id,
            container_name,
            status,
            health,
            image: c.image.clone(),
            project,
            stage,
            service,
        });
    }
    Ok(entries)
}

/// CP に inventory snapshot を送信する（#185）。`server` チャネルの
/// `inventory_report` method。
async fn send_inventory_report(
    client: &ProtocolClient,
    server_slug: &str,
    entries: &[ContainerInventoryEntry],
) -> anyhow::Result<()> {
    let channel = client.open_channel("server").await?;
    let _: serde_json::Value = channel
        .request(
            "inventory_report",
            &json!({
                "server_slug": server_slug,
                "containers": entries,
            }),
        )
        .await?;
    channel.close().await.ok();
    Ok(())
}

/// モニターループを実行
pub async fn run_loop(client: &Arc<ProtocolClient>, server_slug: &str, config: &MonitorConfig) {
    info!(
        interval = config.interval_secs,
        threshold = config.restart_threshold,
        "ヘルスモニター開始"
    );

    let mut prev_states: HashMap<String, ContainerState> = HashMap::new();
    let mut cooldowns: HashMap<CooldownKey, Instant> = HashMap::new();

    loop {
        tokio::time::sleep(Duration::from_secs(config.interval_secs)).await;

        // runtime を毎 interval 再 discovery（rootless socket は agent 起動後に
        // 現れうるため）。
        let runtimes = discover_runtimes();
        if runtimes.is_empty() {
            warn!("観測可能な container runtime が無い — このサイクルをスキップ");
            continue;
        }

        // ── 異常検知: root Docker のみ（既存挙動を維持）──
        // Quadlet (rootless Podman) は healthcheck 未定義 + systemd 管理で
        // restart_count が死角のため anomaly alert の対象外（#185 既知の限界）。
        if let Some(docker_rt) = runtimes.iter().find(|r| r.id == "docker") {
            match check_containers(&docker_rt.docker, &prev_states, config).await {
                Ok(result) => {
                    for alert in &result.alerts {
                        let key = (
                            alert.container_name.clone(),
                            alert.alert_type.as_str().to_string(),
                        );

                        // クールダウンチェック
                        if let Some(last_sent) = cooldowns.get(&key)
                            && last_sent.elapsed() < Duration::from_secs(config.alert_cooldown_secs)
                        {
                            debug!(
                                container = %alert.container_name,
                                alert_type = alert.alert_type.as_str(),
                                "クールダウン中 — アラートスキップ"
                            );
                            continue;
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

                    // 正常復帰 → CP に resolve 送信
                    for container_name in &result.recovered {
                        if let Err(e) = send_resolve(client, server_slug, container_name).await {
                            warn!(error = %e, container = %container_name, "resolve 送信失敗");
                        } else {
                            info!(container = %container_name, "コンテナ正常復帰 — アラート解決送信");
                        }
                    }

                    prev_states = result.new_states;
                }
                Err(e) => {
                    warn!(error = %e, "コンテナチェック失敗");
                }
            }
        }

        // ── inventory: 全 runtime（#185）──
        // dead socket 対策で各 runtime 列挙を timeout で囲む（fail-soft）。
        let mut inventory = Vec::new();
        let mut any_runtime_ok = false;
        for rt in &runtimes {
            match tokio::time::timeout(Duration::from_secs(10), collect_runtime_inventory(rt)).await
            {
                Ok(Ok(entries)) => {
                    any_runtime_ok = true;
                    inventory.extend(entries);
                }
                Ok(Err(e)) => {
                    warn!(runtime = %rt.id, error = %e, "inventory 収集失敗 — スキップ")
                }
                Err(_) => warn!(runtime = %rt.id, "inventory 収集 timeout — スキップ"),
            }
        }
        // 全 runtime の列挙が失敗したら空 snapshot を送らない。送ると CP 側で
        // observed_container が false-clear される（次 interval で復元）。
        // runtime が成功して container 0 件のケースは正当な空なので送信する。
        if any_runtime_ok {
            match send_inventory_report(client, server_slug, &inventory).await {
                Ok(()) => debug!(count = inventory.len(), "inventory_report 送信完了"),
                Err(e) => {
                    warn!(error = %e, count = inventory.len(), "inventory_report 送信失敗")
                }
            }
        } else {
            warn!("全 runtime の inventory 収集に失敗 — inventory_report 送信スキップ");
        }
    }
}

/// チェック結果
pub(crate) struct CheckResult {
    pub alerts: Vec<DetectedAlert>,
    pub recovered: Vec<String>,
    pub new_states: HashMap<String, ContainerState>,
}

/// 全コンテナを取得し、前回状態と比較して異常を検知
pub(crate) async fn check_containers(
    docker: &Docker,
    prev_states: &HashMap<String, ContainerState>,
    config: &MonitorConfig,
) -> anyhow::Result<CheckResult> {
    let containers = docker
        .list_containers(Some(ListContainersOptions {
            all: true,
            ..Default::default()
        }))
        .await?;

    let mut alerts = Vec::new();
    let mut recovered = Vec::new();
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

        let status = state
            .status
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let restart_count = info.restart_count.unwrap_or(0);
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

        // 正常復帰検知: 前回異常 → 今回正常
        if detected.is_empty()
            && let Some(prev) = prev_states.get(&name)
        {
            let was_abnormal = prev.status == "exited"
                || prev.status == "dead"
                || prev.health.as_deref() == Some("unhealthy")
                || prev.restart_count > config.restart_threshold as i64;
            let is_healthy_now =
                current.status == "running" && current.health.as_deref() != Some("unhealthy");
            if was_abnormal && is_healthy_now {
                recovered.push(name.clone());
            }
        }

        alerts.extend(detected);
        new_states.insert(name, current);
    }

    Ok(CheckResult {
        alerts,
        recovered,
        new_states,
    })
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
            message: format!("コンテナ {} が unhealthy", container_name),
        });
    }

    alerts
}

/// CP にアラート解決を送信
async fn send_resolve(
    client: &ProtocolClient,
    server_slug: &str,
    container_name: &str,
) -> anyhow::Result<()> {
    let channel = client.open_channel("server").await?;

    let _: serde_json::Value = channel
        .request(
            "alert_resolve",
            &json!({
                "server_slug": server_slug,
                "container_name": container_name,
            }),
        )
        .await?;

    channel.close().await.ok();
    Ok(())
}

/// CP にアラートを送信
async fn send_alert(
    client: &ProtocolClient,
    server_slug: &str,
    alert: &DetectedAlert,
) -> anyhow::Result<()> {
    let channel = client.open_channel("server").await?;

    let _: serde_json::Value = channel
        .request(
            "alert",
            &json!({
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

    // ── #185 multi-runtime observability ──

    #[test]
    fn podman_runtime_id_extracts_uid() {
        assert_eq!(
            podman_runtime_id("/run/user/1000/podman/podman.sock"),
            "podman-rootless-1000"
        );
        assert_eq!(
            podman_runtime_id("/run/user/0/podman/podman.sock"),
            "podman-rootless-0"
        );
    }

    #[test]
    fn podman_runtime_id_handles_unexpected_path() {
        // 想定外 path でも panic せず "unknown" にフォールバック
        assert_eq!(
            podman_runtime_id("/tmp/podman.sock"),
            "podman-rootless-unknown"
        );
    }

    #[test]
    fn podman_runtime_id_recognizes_rootful() {
        // rootful Podman は system service として 1 つしか存在し得ない
        // → uid suffix なしの "podman-rootful"
        assert_eq!(
            podman_runtime_id("/run/podman/podman.sock"),
            "podman-rootful"
        );
    }

    #[test]
    fn extract_attribution_reads_fleetflow_labels() {
        let mut labels = HashMap::new();
        labels.insert("fleetflow.project".to_string(), "fleetstage".to_string());
        labels.insert("fleetflow.stage".to_string(), "live".to_string());
        labels.insert("fleetflow.service".to_string(), "hq-api".to_string());
        labels.insert("other.label".to_string(), "ignored".to_string());

        let (project, stage, service) = extract_attribution(&labels);
        assert_eq!(project.as_deref(), Some("fleetstage"));
        assert_eq!(stage.as_deref(), Some("live"));
        assert_eq!(service.as_deref(), Some("hq-api"));
    }

    #[test]
    fn extract_attribution_missing_labels_are_none() {
        // fleetflow.* label を持たない container（attribution 不明）
        let labels = HashMap::new();
        let (project, stage, service) = extract_attribution(&labels);
        assert!(project.is_none());
        assert!(stage.is_none());
        assert!(service.is_none());
    }

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
