use anyhow::{Context, Result};
use std::path::Path;

/// PID ファイルを書き込み
pub fn write_pid_file(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("PID ディレクトリ作成失敗: {}", parent.display()))?;
    }

    let pid = std::process::id();
    std::fs::write(path, pid.to_string())
        .with_context(|| format!("PID ファイル書き込み失敗: {}", path.display()))?;

    Ok(())
}

/// PID ファイルを削除
pub fn remove_pid_file(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// PID ファイルからプロセスの生存確認
pub fn read_pid_file(path: &Path) -> Result<Option<u32>> {
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("PID ファイル読み込み失敗: {}", path.display()))?;

    let pid: u32 = content
        .trim()
        .parse()
        .with_context(|| format!("PID パース失敗: {}", content.trim()))?;

    Ok(Some(pid))
}

/// プロセスが生存しているか確認 (kill -0)
pub fn is_process_alive(pid: u32) -> bool {
    // kill(pid, 0) — シグナルは送らず、プロセスの存在確認のみ
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// デーモンの状態
pub enum DaemonStatus {
    Running(u32),
    Stale(u32),
    Stopped,
}

/// PID ファイルからデーモンの状態を判定
pub fn check_status(pid_path: &Path) -> Result<DaemonStatus> {
    match read_pid_file(pid_path)? {
        Some(pid) => {
            if is_process_alive(pid) {
                Ok(DaemonStatus::Running(pid))
            } else {
                Ok(DaemonStatus::Stale(pid))
            }
        }
        None => Ok(DaemonStatus::Stopped),
    }
}
