use anyhow::Result;
use colored::Colorize;

use crate::DaemonCommands;

pub async fn handle(cmd: &DaemonCommands) -> Result<()> {
    match cmd {
        DaemonCommands::Start => handle_start().await,
        DaemonCommands::Stop => handle_stop().await,
        DaemonCommands::Status => handle_status().await,
    }
}

async fn handle_start() -> Result<()> {
    println!("{}", "Control Plane デーモンを起動します".bold());
    println!();

    // fleetflowd バイナリを探索
    let fleetflowd = which_fleetflowd();

    match fleetflowd {
        Some(path) => {
            println!("  fleetflowd: {}", path.display().to_string().cyan());
            println!();

            // フォアグラウンドで起動（Ctrl+C で停止）
            let status = tokio::process::Command::new(&path)
                .status()
                .await?;

            if !status.success() {
                eprintln!(
                    "{} fleetflowd が異常終了しました (exit: {})",
                    "Error:".red().bold(),
                    status.code().unwrap_or(-1)
                );
                std::process::exit(1);
            }
        }
        None => {
            eprintln!("{}", "fleetflowd が見つかりません。".red().bold());
            eprintln!();
            eprintln!("  cargo install fleetflowd");
            eprintln!("  または");
            eprintln!("  cargo build -p fleetflowd --release");
            std::process::exit(1);
        }
    }

    Ok(())
}

async fn handle_stop() -> Result<()> {
    let fleetflowd = which_fleetflowd();

    match fleetflowd {
        Some(path) => {
            let status = tokio::process::Command::new(&path)
                .arg("stop")
                .status()
                .await?;

            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        None => {
            // PID ファイルから直接停止を試みる
            let pid_file = dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                .join("fleetflow/fleetflowd.pid");

            if pid_file.exists() {
                let pid_str = std::fs::read_to_string(&pid_file)?;
                let pid: i32 = pid_str.trim().parse()?;
                println!("fleetflowd (PID: {}) を停止中...", pid);
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                }
                println!("{}", "停止シグナルを送信しました。".green());
            } else {
                println!("{}", "fleetflowd は起動していません。".yellow());
            }
        }
    }

    Ok(())
}

async fn handle_status() -> Result<()> {
    let fleetflowd = which_fleetflowd();

    match fleetflowd {
        Some(path) => {
            let status = tokio::process::Command::new(&path)
                .arg("status")
                .status()
                .await?;

            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        None => {
            // PID ファイルから直接状態確認
            let pid_file = dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                .join("fleetflow/fleetflowd.pid");

            if pid_file.exists() {
                let pid_str = std::fs::read_to_string(&pid_file)?;
                let pid: i32 = pid_str.trim().parse()?;
                let alive = unsafe { libc::kill(pid, 0) == 0 };
                if alive {
                    println!("{}", "fleetflowd: running".green().bold());
                    println!("  PID: {}", pid);
                } else {
                    println!("{}", "fleetflowd: stale".yellow().bold());
                    println!("  PID ファイルが存在しますが、プロセスは停止しています。");
                }
            } else {
                println!("{}", "fleetflowd: stopped".red().bold());
            }
        }
    }

    Ok(())
}

/// fleetflowd バイナリの探索
fn which_fleetflowd() -> Option<std::path::PathBuf> {
    // 1. 同じディレクトリ (cargo target, インストール先)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("fleetflowd");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    // 2. PATH から探索
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(':') {
            let candidate = std::path::PathBuf::from(dir).join("fleetflowd");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    None
}
