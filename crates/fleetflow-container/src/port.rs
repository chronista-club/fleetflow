use anyhow::Result;
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use std::process::Command;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// 指定されたポートを使用しているプロセスの PID を取得する
pub fn find_pids_by_port(port: u16) -> Result<Vec<i32>> {
    // lsof -ti:{port} を実行
    let output = Command::new("lsof")
        .arg("-t")
        .arg(format!("-i:{}", port))
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let pids_str = String::from_utf8_lossy(&out.stdout);
            let pids: Vec<i32> = pids_str
                .lines()
                .filter_map(|line| line.trim().parse::<i32>().ok())
                .collect();
            Ok(pids)
        }
        _ => Ok(vec![]), // エラーまたは空の場合は占有プロセスなしとみなす
    }
}

/// プロセスをグレースフルにシャットダウンする
pub async fn kill_process_gracefully(pid: i32) -> Result<()> {
    let nix_pid = Pid::from_raw(pid);

    // 1. SIGTERM を送信
    info!("Sending SIGTERM to process {}", pid);
    if let Err(e) = signal::kill(nix_pid, Signal::SIGTERM) {
        debug!("Failed to send SIGTERM to {}: {}", pid, e);
        return Ok(()); // 既に死んでいる場合は正常終了
    }

    // 2. 5秒間ポーリング
    let start = Instant::now();
    let timeout = Duration::from_secs(5);

    while start.elapsed() < timeout {
        // プロセスがまだ存在するか確認
        if !is_process_alive(pid) {
            info!("Process {} exited gracefully", pid);
            return Ok(()); // 即時キャッチ
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    // 3. 5秒経っても生きていれば SIGKILL
    warn!("Timeout reached. Sending SIGKILL to process {}", pid);
    let _ = signal::kill(nix_pid, Signal::SIGKILL);

    Ok(())
}

/// ポートが使用可能であることを保証する
pub async fn ensure_port_available(port: u16) -> Result<()> {
    let pids = find_pids_by_port(port)?;
    if pids.is_empty() {
        return Ok(());
    }

    for pid in pids {
        warn!(
            "Port {} is occupied by process {}. Attempting cleanup...",
            port, pid
        );
        kill_process_gracefully(pid).await?;
    }

    // ポートが解放されるのを最終確認（OSのリサイクル待ちなど）
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(1) {
        if find_pids_by_port(port)?.is_empty() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Ok(())
}

fn is_process_alive(pid: i32) -> bool {
    // signal 0 を送ることで存在確認が可能
    signal::kill(Pid::from_raw(pid), None).is_ok()
}
