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
            // 自身の PID と一致する場合は Stale 扱い。
            //
            // docker container 内では PID 1 が init プロセスとして常に生存しているため、
            // 前回異常終了で pid_file に PID 1 が残ると、再起動した自分自身を
            // 「既存プロセス」と誤検知して crash loop する問題を回避する。
            if pid == std::process::id() {
                return Ok(DaemonStatus::Stale(pid));
            }
            if is_process_alive(pid) {
                Ok(DaemonStatus::Running(pid))
            } else {
                Ok(DaemonStatus::Stale(pid))
            }
        }
        None => Ok(DaemonStatus::Stopped),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_pid_path(suffix: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "fleetflowd-test-{}-{}.pid",
            std::process::id(),
            suffix
        ));
        p
    }

    #[test]
    fn check_status_returns_stopped_when_file_absent() {
        let path = temp_pid_path("absent");
        let _ = std::fs::remove_file(&path);

        let status = check_status(&path).unwrap();
        assert!(matches!(status, DaemonStatus::Stopped));
    }

    #[test]
    fn check_status_returns_stale_for_self_pid() {
        // 再現: 前回異常終了で pid_file に自身の PID が残った状況
        // (docker container で PID 1 が残って再起動した場合に相当)
        let path = temp_pid_path("self");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(f, "{}", std::process::id()).unwrap();
        drop(f);

        let status = check_status(&path).unwrap();
        assert!(
            matches!(status, DaemonStatus::Stale(_)),
            "self-PID が書かれた pid_file は Stale 扱いになるべき"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn check_status_returns_stale_for_dead_pid() {
        // 絶対に存在しない巨大 PID (Linux の pid_max は通常 2^22 程度)
        let path = temp_pid_path("dead");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(f, "{}", u32::MAX - 1).unwrap();
        drop(f);

        let status = check_status(&path).unwrap();
        assert!(
            matches!(status, DaemonStatus::Stale(_)),
            "死んでいる PID は Stale 扱いになるべき"
        );

        let _ = std::fs::remove_file(&path);
    }
}
