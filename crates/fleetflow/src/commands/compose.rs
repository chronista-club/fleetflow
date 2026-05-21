//! Compose backend — `fleet up` / `fleet down` の `backend "compose"` 経路
//!
//! Podman+Quadlet 追従 epic（creo-memories `mem_1CbD3b6j1s3pxQ1TGvaXtv`）WS4。
//!
//! cloud は Quadlet、demo 環境は Compose という分担。stage が
//! `backend "compose"` を宣言しているとき、`up.rs` / `down.rs` から本モジュール
//! に分岐する。実体化ロジックは `fleetflow-container::compose` に集約されており、
//! 本モジュールはその薄い presenter。

use std::path::Path;

use colored::Colorize;
use fleetflow_container::compose::{compose_down, compose_up, generate_compose_yaml};
use fleetflow_core::{Flow, Stage};

/// `fleet up` の Compose 経路。
pub async fn up(
    config: &Flow,
    project_root: &Path,
    stage_name: &str,
    stage: &Stage,
    dry_run: bool,
) -> anyhow::Result<()> {
    println!("{}", format!("backend: compose ({stage_name})").cyan());

    if dry_run {
        let yaml = generate_compose_yaml(project_root, config, stage_name, stage)?;
        println!("{}", "[dry-run] 生成される compose.yaml:".yellow().bold());
        for line in yaml.lines() {
            println!("  {line}");
        }
        println!(
            "{}",
            "[dry-run] 実際の書き込み・podman compose は行われません。".yellow()
        );
        return Ok(());
    }

    let file = compose_up(project_root, config, stage_name, stage)?;
    println!(
        "  {} {} → podman compose up -d",
        "✓".green(),
        file.display().to_string().cyan()
    );

    println!();
    println!(
        "{}",
        "✓ すべてのサービスを起動しました（compose）！"
            .green()
            .bold()
    );
    Ok(())
}

/// `fleet down` の Compose 経路。
pub async fn down(
    config: &Flow,
    project_root: &Path,
    stage_name: &str,
    stage: &Stage,
    remove: bool,
) -> anyhow::Result<()> {
    println!("{}", format!("backend: compose ({stage_name})").cyan());

    compose_down(project_root, config, stage_name, stage, remove)?;
    println!(
        "  {} podman compose down{}",
        "✓".green(),
        if remove { " --volumes" } else { "" }
    );

    println!();
    println!("{}", "✓ ステージを停止しました（compose）".green().bold());
    Ok(())
}
