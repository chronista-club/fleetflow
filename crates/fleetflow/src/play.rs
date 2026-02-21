use crate::utils::{expand_variables, shell_escape};
use colored::Colorize;

/// Playbook用のサービス定義
struct PlaybookService {
    name: String,
    image: String,
    command: Option<String>,
    ports: Vec<PlaybookPort>,
    env: std::collections::HashMap<String, String>,
    volumes: Vec<PlaybookVolume>,
}

struct PlaybookPort {
    host: u16,
    container: u16,
}

struct PlaybookVolume {
    host: String,
    container: String,
    read_only: bool,
}

/// Playbookを実行（リモートサーバーでサービスを起動）
pub async fn handle_play_command(
    project_root: &std::path::Path,
    playbook_name: &str,
    yes: bool,
    pull: bool,
) -> anyhow::Result<()> {
    use std::process::Command;

    println!(
        "{}",
        format!("▶ Playbook '{}' を実行中...", playbook_name)
            .green()
            .bold()
    );

    // Playbook KDLファイルを探す
    let playbook_path = project_root
        .join("playbooks")
        .join(format!("{}.kdl", playbook_name));
    if !playbook_path.exists() {
        return Err(anyhow::anyhow!(
            "Playbook '{}' が見つかりません: {}",
            playbook_name,
            playbook_path.display()
        ));
    }

    println!("  Playbook: {}", playbook_path.display().to_string().cyan());

    // KDLをパース
    let kdl_content = std::fs::read_to_string(&playbook_path)?;
    let doc: kdl::KdlDocument = kdl_content
        .parse()
        .map_err(|e| anyhow::anyhow!("KDLパースエラー: {}", e))?;

    // Playbookのメタデータを取得
    let playbook_node = doc
        .get("playbook")
        .ok_or_else(|| anyhow::anyhow!("Playbook定義が見つかりません"))?;

    // targetは子ノード: target "creo-dev"
    let playbook_children = playbook_node
        .children()
        .ok_or_else(|| anyhow::anyhow!("Playbook定義にchildrenがありません"))?;
    let target_node = playbook_children
        .get("target")
        .ok_or_else(|| anyhow::anyhow!("target が指定されていません"))?;
    let target = target_node
        .entries()
        .first()
        .and_then(|e| e.value().as_string())
        .ok_or_else(|| anyhow::anyhow!("target の値が取得できません"))?;

    println!("  Target: {}", target.cyan());

    // 変数を取得
    let mut variables: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    // ビルトイン変数を追加（環境変数から取得）
    let builtin_vars = ["FLEET_STAGE", "FLEET_PROJECT_ROOT"];
    for var_name in builtin_vars {
        if let Ok(value) = std::env::var(var_name) {
            variables.insert(var_name.to_string(), value);
        }
    }

    // Playbook内の変数定義を追加（ビルトイン変数を上書き可能）
    if let Some(vars_node) = doc.get("variables")
        && let Some(children) = vars_node.children()
    {
        for node in children.nodes() {
            let var_name = node.name().value();
            if let Some(entry) = node.entries().first()
                && let Some(value) = entry.value().as_string()
            {
                variables.insert(var_name.to_string(), value.to_string());
            }
        }
    }

    // ステージを取得
    let mut stages: Vec<(String, Vec<PlaybookService>)> = Vec::new();
    for node in doc.nodes() {
        if node.name().value() == "stage" {
            let stage_name = node
                .entries()
                .first()
                .and_then(|e| e.value().as_string())
                .unwrap_or("default")
                .to_string();

            let mut services = Vec::new();
            if let Some(children) = node.children() {
                for child in children.nodes() {
                    if child.name().value() == "service"
                        && let Some(service) = parse_playbook_service(child)
                    {
                        services.push(service);
                    }
                }
            }
            stages.push((stage_name, services));
        }
    }

    if stages.is_empty() {
        return Err(anyhow::anyhow!("ステージが定義されていません"));
    }

    // 実行計画を表示
    println!();
    println!("{}", "実行計画:".bold());
    for (stage_name, services) in &stages {
        println!("  Stage: {}", stage_name.cyan());
        for service in services {
            println!("    • {} ({})", service.name.cyan(), service.image);
        }
    }

    // 確認
    if !yes {
        println!();
        println!(
            "{}",
            "リモートサーバーにサービスをデプロイします。続行するには --yes を指定してください。"
                .yellow()
        );
        return Ok(());
    }

    println!();
    println!("{}", format!("SSHで {} に接続中...", target).blue());

    // Dockerネットワークを作成（既存なら無視）
    let network_name = playbook_name;
    println!("  🔗 ネットワーク '{}' を作成中...", network_name.cyan());
    let create_network_cmd = format!(
        "docker network create {} 2>/dev/null || true",
        shell_escape(network_name)
    );
    let _ = Command::new("ssh")
        .arg(target)
        .arg(&create_network_cmd)
        .status();

    // 各ステージを実行
    for (stage_name, services) in &stages {
        println!();
        println!(
            "{}",
            format!("▶ Stage '{}' を実行中...", stage_name)
                .green()
                .bold()
        );

        for service in services {
            println!();
            println!(
                "{}",
                format!("  ▶ {} を起動中...", service.name).cyan().bold()
            );

            // 既存コンテナを停止・削除
            let escaped_name = shell_escape(&service.name);
            let stop_cmd = format!(
                "docker stop {} 2>/dev/null || true && docker rm {} 2>/dev/null || true",
                escaped_name, escaped_name
            );
            let ssh_stop = Command::new("ssh").arg(target).arg(&stop_cmd).status();

            if let Err(e) = ssh_stop {
                println!("    ⚠ 既存コンテナの停止でエラー: {}", e);
            }

            // pullが指定されている場合はイメージをpull
            if pull {
                println!("    ↓ イメージをpull中...");
                let pull_cmd = format!("docker pull {}", shell_escape(&service.image));
                let ssh_pull = Command::new("ssh").arg(target).arg(&pull_cmd).status()?;
                if !ssh_pull.success() {
                    println!("    ⚠ イメージpullでエラー（続行します）");
                }
            }

            // docker run コマンドを構築
            let mut docker_cmd = format!(
                "docker run -d --name {} --network {}",
                shell_escape(&service.name),
                shell_escape(network_name)
            );

            // ポートマッピング
            for port in &service.ports {
                docker_cmd.push_str(&format!(" -p {}:{}", port.host, port.container));
            }

            // 環境変数（変数展開付き）
            for (key, value) in &service.env {
                let expanded_value = expand_variables(value, &variables);
                docker_cmd.push_str(&format!(
                    " -e {}={}",
                    shell_escape(key),
                    shell_escape(&expanded_value)
                ));
            }

            // ボリューム
            for vol in &service.volumes {
                let vol_spec = if vol.read_only {
                    format!("{}:{}:ro", vol.host, vol.container)
                } else {
                    format!("{}:{}", vol.host, vol.container)
                };
                docker_cmd.push_str(&format!(" -v {}", shell_escape(&vol_spec)));
            }

            // イメージとコマンド
            docker_cmd.push_str(&format!(" {}", shell_escape(&service.image)));
            if let Some(cmd) = &service.command {
                docker_cmd.push_str(&format!(" {}", cmd));
            }

            println!("    $ {}", docker_cmd.dimmed());

            // SSH経由で実行
            let ssh_result = Command::new("ssh").arg(target).arg(&docker_cmd).output()?;

            if ssh_result.status.success() {
                println!("    ✓ 起動完了");
            } else {
                let stderr = String::from_utf8_lossy(&ssh_result.stderr);
                println!("    ✗ 起動エラー: {}", stderr.trim());
            }
        }
    }

    println!();
    println!(
        "{}",
        format!("✓ Playbook '{}' の実行が完了しました！", playbook_name)
            .green()
            .bold()
    );

    Ok(())
}

/// KDLノードからPlaybookServiceをパース
fn parse_playbook_service(node: &kdl::KdlNode) -> Option<PlaybookService> {
    let name = node.entries().first()?.value().as_string()?.to_string();

    let children = node.children()?;

    // image
    let image_node = children.get("image")?;
    let image = image_node
        .entries()
        .first()?
        .value()
        .as_string()?
        .to_string();

    // command
    let command = children
        .get("command")
        .and_then(|n| n.entries().first())
        .and_then(|e| e.value().as_string())
        .map(|s| s.to_string());

    // ports
    let mut ports = Vec::new();
    if let Some(ports_node) = children.get("ports")
        && let Some(ports_children) = ports_node.children()
    {
        for port_node in ports_children.nodes() {
            if port_node.name().value() == "port" {
                let host = port_node
                    .get("host")
                    .and_then(|v| v.as_integer())
                    .map(|v| v as u16);
                let container = port_node
                    .get("container")
                    .and_then(|v| v.as_integer())
                    .map(|v| v as u16);
                if let (Some(h), Some(c)) = (host, container) {
                    ports.push(PlaybookPort {
                        host: h,
                        container: c,
                    });
                }
            }
        }
    }

    // env
    let mut env = std::collections::HashMap::new();
    if let Some(env_node) = children.get("env")
        && let Some(env_children) = env_node.children()
    {
        for env_entry in env_children.nodes() {
            let key = env_entry.name().value().to_string();
            if let Some(value) = env_entry
                .entries()
                .first()
                .and_then(|e| e.value().as_string())
            {
                env.insert(key, value.to_string());
            }
        }
    }

    // volumes
    let mut volumes = Vec::new();
    if let Some(vols_node) = children.get("volumes")
        && let Some(vols_children) = vols_node.children()
    {
        for vol_node in vols_children.nodes() {
            if vol_node.name().value() == "volume" {
                let host = vol_node
                    .get("host")
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string());
                let container = vol_node
                    .get("container")
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string());
                let read_only = vol_node
                    .get("read_only")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if let (Some(h), Some(c)) = (host, container) {
                    volumes.push(PlaybookVolume {
                        host: h,
                        container: c,
                        read_only,
                    });
                }
            }
        }
    }

    Some(PlaybookService {
        name,
        image,
        command,
        ports,
        env,
        volumes,
    })
}
