//! Quadlet backend — `fleet up` / `fleet down` の `backend "quadlet"` 経路
//!
//! Podman+Quadlet 追従 epic（creo-memories `mem_1CbD3b6j1s3pxQ1TGvaXtv`）WS2 Stage 2c。
//!
//! stage が `backend "quadlet"` を宣言しているとき、`up.rs` / `down.rs` から
//! 本モジュールに分岐する。実体化のロジックは `fleetflow-container` の
//! `quadlet` モジュールに集約されており（CLI・agent 共用）、本モジュールは
//! その薄い presenter（結果を stdout に整形するだけ）。
//!
//! 本経路は **`fleet up` をホスト上で実行する**ことを前提とする（CLI-local）。
//! クラウドの CP→agent 経由デプロイは WS3（agent の Quadlet 役割転換）。

use colored::Colorize;
use fleetflow_container::quadlet::{
    apply_stage, build_stage_units, default_quadlet_dir, service_units, sync_quadlet_dir,
    systemctl_user_daemon_reload, systemctl_user_stop,
};
use fleetflow_core::{Flow, Stage};

/// `fleet up` の Quadlet 経路。
pub async fn up(
    config: &Flow,
    stage_name: &str,
    stage: &Stage,
    dry_run: bool,
) -> anyhow::Result<()> {
    println!("{}", format!("backend: quadlet ({stage_name})").cyan());

    if dry_run {
        let units = build_stage_units(config, stage_name, stage)?;
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

    let outcome = apply_stage(config, stage_name, stage)?;
    println!(
        "  {} {} 個のユニットを反映 + daemon-reload",
        "✓".green(),
        outcome.units_written
    );
    for unit in &outcome.services_started {
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
    println!("{}", format!("backend: quadlet ({stage_name})").cyan());

    // 各 container サービスを停止
    for unit in service_units(config, stage_name, stage) {
        match systemctl_user_stop(&unit) {
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
