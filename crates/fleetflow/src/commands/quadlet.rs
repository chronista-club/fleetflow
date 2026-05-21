//! Quadlet backend — `fleet up` / `fleet down` の `runtime "quadlet"` 経路
//!
//! Podman+Quadlet 追従 epic（creo-memories `mem_1CbD3b6j1s3pxQ1TGvaXtv`）WS2 Stage 2c。
//!
//! stage が `runtime "quadlet"` を宣言しているとき、`up.rs` / `down.rs` から
//! 本モジュールに分岐する。KDL から Quadlet ユニットを生成（`fleetflow-container`
//! の `quadlet` モジュール）し、ホストの `~/.config/containers/systemd/` に
//! 反映して `systemctl --user` で起動・停止する。
//!
//! 本経路は **`fleet up` をホスト上で実行する**ことを前提とする（CLI-local）。
//! クラウドの CP→agent 経由デプロイは WS3（agent の Quadlet 役割転換）。

use colored::Colorize;
use fleetflow_container::quadlet::{
    QuadletUnit, container_file_name, default_quadlet_dir, generate_container_unit,
    generate_network_unit, network_file_name, sync_quadlet_dir, systemctl_user_daemon_reload,
    systemctl_user_start, unit_base_name,
};
use fleetflow_core::{Flow, Stage};

/// stage の全 container サービスから Quadlet ユニット束を組み立てる（純粋関数）。
///
/// `.network` 1 つ + container サービスごとの `.container`。静的サイト
/// （`type "static"`）は Quadlet 対象外なのでスキップする。
pub fn build_stage_units(
    config: &Flow,
    stage_name: &str,
    stage: &Stage,
) -> anyhow::Result<Vec<QuadletUnit>> {
    let project = &config.name;
    let mut units = vec![QuadletUnit {
        file_name: network_file_name(project, stage_name),
        content: generate_network_unit(project, stage_name),
    }];

    for service_name in &stage.services {
        let service = config
            .services
            .get(service_name)
            .ok_or_else(|| anyhow::anyhow!("サービス '{}' の定義が見つかりません", service_name))?;

        // 静的サイトはコンテナではないため Quadlet 対象外
        if service.is_static() {
            continue;
        }
        if service.image.is_none() {
            anyhow::bail!("サービス '{}' に image が指定されていません", service_name);
        }

        units.push(QuadletUnit {
            file_name: container_file_name(project, stage_name, service_name),
            content: generate_container_unit(
                project,
                stage_name,
                service_name,
                service,
                &service.depends_on,
            ),
        });
    }

    Ok(units)
}

/// container サービスの systemd unit 名（`{project}-{stage}-{service}.service`）。
fn service_units(config: &Flow, stage_name: &str, stage: &Stage) -> Vec<String> {
    stage
        .services
        .iter()
        .filter(|name| {
            config
                .services
                .get(name.as_str())
                .is_some_and(|s| !s.is_static())
        })
        .map(|name| format!("{}.service", unit_base_name(&config.name, stage_name, name)))
        .collect()
}

/// `fleet up` の Quadlet 経路。
pub async fn up(
    config: &Flow,
    stage_name: &str,
    stage: &Stage,
    dry_run: bool,
) -> anyhow::Result<()> {
    println!("{}", format!("runtime: quadlet ({stage_name})").cyan());

    let units = build_stage_units(config, stage_name, stage)?;

    if dry_run {
        println!(
            "{}",
            format!("[dry-run] {} 個の Quadlet ユニットを生成:", units.len())
                .yellow()
                .bold()
        );
        for unit in &units {
            println!("  • {}", unit.file_name.cyan());
        }
        println!(
            "{}",
            "[dry-run] 実際の書き込み・systemctl は行われません。".yellow()
        );
        return Ok(());
    }

    let dir = default_quadlet_dir()
        .ok_or_else(|| anyhow::anyhow!("Quadlet ディレクトリを解決できません（HOME 未設定）"))?;

    println!(
        "  Quadlet ディレクトリ: {}",
        dir.display().to_string().cyan()
    );
    sync_quadlet_dir(&dir, &config.name, stage_name, &units)?;
    println!("  {} {} 個のユニットを反映", "✓".green(), units.len());

    // systemd に Quadlet generator を再走させる
    systemctl_user_daemon_reload()?;
    println!("  {} systemctl --user daemon-reload", "✓".green());

    // 各 container サービスを起動
    for unit in service_units(config, stage_name, stage) {
        systemctl_user_start(&unit)?;
        println!("  {} {} 起動", "✓".green(), unit.cyan());
    }

    println!();
    println!(
        "{}",
        "✓ すべてのサービスを起動しました（quadlet）！"
            .green()
            .bold()
    );
    Ok(())
}

/// `fleet down` の Quadlet 経路。
///
/// `remove` 指定時は Quadlet ファイル自体も削除する（`systemctl` 停止に加えて
/// snapshot を空にし、daemon-reload で `.service` ユニットを消す）。
pub async fn down(
    config: &Flow,
    stage_name: &str,
    stage: &Stage,
    remove: bool,
) -> anyhow::Result<()> {
    println!("{}", format!("runtime: quadlet ({stage_name})").cyan());

    // 各 container サービスを停止
    for unit in service_units(config, stage_name, stage) {
        match fleetflow_container::quadlet::systemctl_user_stop(&unit) {
            Ok(()) => println!("  {} {} 停止", "✓".green(), unit.cyan()),
            Err(e) => println!("  {} {} 停止失敗: {}", "⚠".yellow(), unit.cyan(), e),
        }
    }

    if remove {
        let dir = default_quadlet_dir().ok_or_else(|| {
            anyhow::anyhow!("Quadlet ディレクトリを解決できません（HOME 未設定）")
        })?;
        // 空の束で sync → 同 project/stage の fleetflow ユニットを全削除
        sync_quadlet_dir(&dir, &config.name, stage_name, &[])?;
        systemctl_user_daemon_reload()?;
        println!("  {} Quadlet ファイルを削除", "✓".green());
    }

    println!();
    println!("{}", "✓ ステージを停止しました（quadlet）".green().bold());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleetflow_core::{Service, Stage};
    use std::collections::HashMap;

    fn flow_with(services: Vec<(&str, Service)>, stage_services: Vec<&str>) -> (Flow, Stage) {
        let mut svc_map = HashMap::new();
        for (name, svc) in services {
            svc_map.insert(name.to_string(), svc);
        }
        let stage = Stage {
            services: stage_services.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        let mut stages = HashMap::new();
        stages.insert("live".to_string(), stage.clone());
        let flow = Flow {
            name: "myapp".to_string(),
            services: svc_map,
            stages,
            providers: HashMap::new(),
            servers: HashMap::new(),
            registry: None,
            variables: HashMap::new(),
            tenant: None,
        };
        (flow, stage)
    }

    fn container_service() -> Service {
        Service {
            image: Some("postgres:16".to_string()),
            ..Service::default()
        }
    }

    #[test]
    fn build_stage_units_emits_network_plus_containers() {
        let (flow, stage) = flow_with(
            vec![("db", container_service()), ("web", container_service())],
            vec!["db", "web"],
        );
        let units = build_stage_units(&flow, "live", &stage).unwrap();
        // network 1 + container 2
        assert_eq!(units.len(), 3);
        let names: Vec<&str> = units.iter().map(|u| u.file_name.as_str()).collect();
        assert!(names.contains(&"myapp-live.network"));
        assert!(names.contains(&"myapp-live-db.container"));
        assert!(names.contains(&"myapp-live-web.container"));
    }

    #[test]
    fn build_stage_units_skips_static_services() {
        let static_svc = Service {
            service_type: Some(fleetflow_core::ServiceType::Static),
            ..Service::default()
        };
        let (flow, stage) = flow_with(
            vec![("db", container_service()), ("site", static_svc)],
            vec!["db", "site"],
        );
        let units = build_stage_units(&flow, "live", &stage).unwrap();
        // network 1 + container 1（static は除外）
        assert_eq!(units.len(), 2);
        let names: Vec<&str> = units.iter().map(|u| u.file_name.as_str()).collect();
        assert!(!names.contains(&"myapp-live-site.container"));
    }

    #[test]
    fn build_stage_units_errors_on_missing_image() {
        let (flow, stage) = flow_with(vec![("db", Service::default())], vec!["db"]);
        assert!(build_stage_units(&flow, "live", &stage).is_err());
    }

    #[test]
    fn service_units_returns_systemd_service_names() {
        let (flow, stage) = flow_with(vec![("db", container_service())], vec!["db"]);
        assert_eq!(
            service_units(&flow, "live", &stage),
            vec!["myapp-live-db.service"]
        );
    }
}
