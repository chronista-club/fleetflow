//! デプロイ実行 — Docker Compose 操作（ログストリーミング対応）

use std::process::Stdio;

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
use tracing::info;

/// 許可される docker compose サブコマンド
const ALLOWED_SUBCOMMANDS: &[&str] = &["up", "down", "pull", "restart", "ps", "stop"];

/// デプロイ実行結果
#[derive(Debug)]
pub struct DeployResult {
    pub status: String,
    pub log: Vec<String>,
}

/// docker compose サブコマンドの検証
///
/// 許可リストに含まれるサブコマンドのみ通す。
/// `--env-file` 等の危険なフラグ注入を防止する。
pub fn validate_compose_command(command: &str) -> Result<Vec<String>> {
    let args: Vec<String> = command.split_whitespace().map(String::from).collect();
    let subcommand = args.first().context("command is empty")?;

    if !ALLOWED_SUBCOMMANDS.contains(&subcommand.as_str()) {
        anyhow::bail!("disallowed compose subcommand: {subcommand}");
    }

    // 危険なフラグの拒否
    for arg in &args[1..] {
        let a = arg.as_str();
        if a.starts_with("--env-file")
            || a.starts_with("--project-directory")
            || matches!(a, "--follow" | "-f" | "--volumes" | "-v")
        {
            anyhow::bail!("disallowed flag: {arg}");
        }
    }

    Ok(args)
}

/// compose_path がベースディレクトリ配下にあることを検証
///
/// パストラバーサル (`../`) を防止し、`deploy_base` 外のファイルへのアクセスを遮断する。
pub fn validate_compose_path(path: &str, deploy_base: &str) -> Result<()> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("compose_path が存在しない: {path}"))?;
    let base = std::fs::canonicalize(deploy_base)
        .with_context(|| format!("deploy_base が存在しない: {deploy_base}"))?;

    if !canonical.starts_with(&base) {
        anyhow::bail!(
            "compose_path がベースディレクトリ外: {} (base: {})",
            canonical.display(),
            base.display()
        );
    }

    Ok(())
}

/// デプロイコマンド実行（ログストリーミング対応）
///
/// `log_tx` が指定されると、stdout/stderr の各行を非同期でストリーム送信する。
/// チャネルが閉じられても（受信側 drop）デプロイ自体は続行する。
///
/// payload 例:
/// ```json
/// {
///   "project_slug": "creo-memories",
///   "stage": "live",
///   "compose_path": "/opt/apps/creo-memories/live/docker-compose.yml",
///   "command": "up -d"
/// }
/// ```
pub async fn execute(
    payload: &Value,
    deploy_base: &str,
    log_tx: Option<mpsc::Sender<String>>,
) -> Result<DeployResult> {
    let compose_path = payload["compose_path"]
        .as_str()
        .context("compose_path is required")?;
    let command = payload["command"].as_str().unwrap_or("up -d");
    let project_slug = payload["project_slug"].as_str().unwrap_or("unknown");
    let stage = payload["stage"].as_str().unwrap_or("unknown");

    // セキュリティ検証
    let args = validate_compose_command(command)?;
    validate_compose_path(compose_path, deploy_base)?;

    info!(
        project = project_slug,
        stage, compose_path, command, "デプロイ実行開始"
    );

    // docker compose -f <path> <args...> を実行
    let mut cmd = tokio::process::Command::new("docker");
    cmd.arg("compose")
        .arg("-f")
        .arg(compose_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for arg in &args {
        cmd.arg(arg);
    }

    let mut child = cmd.spawn().context("docker compose spawn 失敗")?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // stdout と stderr を並行で読み取り
    // log_tx が Some なら mpsc に送信（呼び出し側が収集を担当、Vec 不使用）
    // log_tx が None なら Vec にバッファリング（DeployResult.log で返す）
    let tx1 = log_tx.clone();
    let stdout_task = tokio::spawn(async move {
        let mut lines = Vec::new();
        let mut count: usize = 0;
        if let Some(out) = stdout {
            let mut reader = BufReader::new(out).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                count += 1;
                if let Some(ref tx) = tx1 {
                    let _ = tx.send(line).await;
                } else {
                    lines.push(line);
                }
            }
        }
        (lines, count)
    });

    let tx2 = log_tx;
    let stderr_task = tokio::spawn(async move {
        let mut lines = Vec::new();
        let mut count: usize = 0;
        if let Some(err) = stderr {
            let mut reader = BufReader::new(err).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                count += 1;
                let tagged = format!("[stderr] {line}");
                if let Some(ref tx) = tx2 {
                    let _ = tx.send(tagged).await;
                } else {
                    lines.push(tagged);
                }
            }
        }
        (lines, count)
    });

    let ((stdout_lines, stdout_count), (stderr_lines, stderr_count)) =
        tokio::try_join!(stdout_task, stderr_task).context("ログ読み取りタスク失敗")?;

    let exit_status = child.wait().await.context("docker compose wait 失敗")?;

    let status = if exit_status.success() {
        "success"
    } else {
        "failed"
    };

    let line_count = stdout_count + stderr_count;
    let mut log = stdout_lines;
    log.extend(stderr_lines);

    info!(
        project = project_slug,
        stage, status, line_count, "デプロイ実行完了"
    );

    Ok(DeployResult {
        status: status.into(),
        log,
    })
}

/// コンテナ名の文字種検証
///
/// Docker コンテナ名に使用可能な文字: `[a-zA-Z0-9_.-/]`
/// `--` 始まりのオプションインジェクションを防止する。
pub fn validate_container_name(name: &str) -> Result<()> {
    if name.is_empty() {
        anyhow::bail!("container name is required");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | '/'))
    {
        anyhow::bail!("invalid container name: {name}");
    }
    if name.starts_with('-') {
        anyhow::bail!("container name must not start with '-': {name}");
    }
    Ok(())
}

/// サービス再起動
pub async fn restart_service(container_name: &str) -> Result<()> {
    validate_container_name(container_name)?;

    info!(container = container_name, "サービス再起動");

    let output = tokio::process::Command::new("docker")
        .args(["restart", container_name])
        .output()
        .await
        .context("docker restart 実行失敗")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("docker restart failed: {}", stderr);
    }

    Ok(())
}

/// コンテナ状態一覧を取得
pub async fn container_status() -> Result<Value> {
    let output = tokio::process::Command::new("docker")
        .args(["ps", "--format", "{{json .}}", "--no-trunc"])
        .output()
        .await
        .context("docker ps 実行失敗")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let containers: Vec<Value> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    Ok(serde_json::json!({ "containers": containers }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_allowed_subcommands() {
        assert!(validate_compose_command("up -d").is_ok());
        assert!(validate_compose_command("down").is_ok());
        assert!(validate_compose_command("pull").is_ok());
        assert!(validate_compose_command("restart").is_ok());
        assert!(validate_compose_command("ps").is_ok());
        assert!(validate_compose_command("stop").is_ok());
    }

    #[test]
    fn reject_disallowed_subcommands() {
        assert!(validate_compose_command("exec bash").is_err());
        assert!(validate_compose_command("run --rm app sh").is_err());
        assert!(validate_compose_command("config").is_err());
        assert!(validate_compose_command("logs").is_err()); // Phase 3 で専用設計
        assert!(validate_compose_command("").is_err());
    }

    #[test]
    fn reject_dangerous_flags() {
        assert!(validate_compose_command("up -d --env-file /etc/passwd").is_err());
        assert!(validate_compose_command("up --project-directory /tmp").is_err());
        assert!(validate_compose_command("logs --follow").is_err());
        assert!(validate_compose_command("logs -f").is_err());
        assert!(validate_compose_command("down --volumes").is_err());
        assert!(validate_compose_command("down -v").is_err());
    }

    #[test]
    fn validate_compose_path_rejects_nonexistent() {
        let result = validate_compose_path("/nonexistent/path/compose.yml", "/opt/apps");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn execute_rejects_missing_compose_path() {
        let payload = serde_json::json!({ "command": "up -d" });
        let result = execute(&payload, "/opt/apps", None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("compose_path"));
    }

    #[tokio::test]
    async fn execute_rejects_disallowed_command() {
        let payload = serde_json::json!({
            "compose_path": "/opt/apps/test/docker-compose.yml",
            "command": "exec bash"
        });
        let result = execute(&payload, "/opt/apps", None).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("disallowed compose subcommand")
        );
    }

    #[tokio::test]
    async fn restart_service_rejects_empty_name() {
        let result = restart_service("").await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("container name is required")
        );
    }

    #[test]
    fn validate_container_name_valid() {
        assert!(validate_container_name("my-app").is_ok());
        assert!(validate_container_name("creo_web.1").is_ok());
        assert!(validate_container_name("project/service").is_ok());
    }

    #[test]
    fn validate_container_name_rejects_option_injection() {
        assert!(validate_container_name("--all").is_err());
        assert!(validate_container_name("-t").is_err());
    }

    #[test]
    fn validate_container_name_rejects_invalid_chars() {
        assert!(validate_container_name("app;rm -rf /").is_err());
        assert!(validate_container_name("app$(whoami)").is_err());
        assert!(validate_container_name("").is_err());
    }
}
