//! Tailscale SSH executor
//!
//! `tailscale ssh` を経由してリモートサーバーでコマンドを実行する。
//! 公開鍵/パスワード認証は不要 — Tailscale の WireGuard 認証に委譲。

use std::time::Duration;

use serde::Serialize;
use tokio::process::Command;

use crate::error::CloudError;

/// SSH 実行結果
#[derive(Debug, Clone, Serialize)]
pub struct SshResult {
    pub host: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

/// リモートホストでコマンドを実行
///
/// `tailscale ssh <user>@<host> <command>` を使用。
/// Tailscale ACL で SSH アクセスが許可されている必要がある。
pub async fn exec(host: &str, user: &str, command: &str) -> Result<SshResult, CloudError> {
    exec_with_timeout(host, user, command, Duration::from_secs(30)).await
}

/// タイムアウト付きでリモートコマンドを実行
pub async fn exec_with_timeout(
    host: &str,
    user: &str,
    command: &str,
    timeout: Duration,
) -> Result<SshResult, CloudError> {
    let target = format!("{user}@{host}");

    let output = tokio::time::timeout(
        timeout,
        Command::new("tailscale")
            .args(["ssh", &target, "--", command])
            .output(),
    )
    .await
    .map_err(|_| {
        CloudError::Timeout(format!(
            "SSH タイムアウト ({:.0}s): {target}",
            timeout.as_secs_f64()
        ))
    })?
    .map_err(|e| CloudError::CommandFailed(format!("tailscale ssh 実行失敗: {e}")))?;

    let exit_code = output.status.code().unwrap_or(-1);

    Ok(SshResult {
        host: host.to_string(),
        exit_code,
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        success: output.status.success(),
    })
}

/// 複数コマンドを順次実行（1つでも失敗したら停止）
pub async fn exec_commands(
    host: &str,
    user: &str,
    commands: &[&str],
) -> Result<Vec<SshResult>, CloudError> {
    let mut results = Vec::new();
    for cmd in commands {
        let result = exec(host, user, cmd).await?;
        let success = result.success;
        results.push(result);
        if !success {
            break;
        }
    }
    Ok(results)
}

/// シェルエスケープ（シングルクォート内に安全に配置）
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// ファイルをリモートにコピー（tailscale ssh 経由で安全に転送）
///
/// ローカルファイルを base64 エンコードし、リモート側で decode して書き込む。
/// コマンドインジェクション対策として、リモートパスはシェルエスケープされる。
pub async fn copy_file(local_path: &str, host: &str, remote_path: &str) -> Result<(), CloudError> {
    let content = tokio::fs::read(local_path)
        .await
        .map_err(|e| CloudError::CommandFailed(format!("ローカルファイル読み込み失敗: {e}")))?;

    let encoded = base64_encode(&content);

    // base64 でエンコードしてリモートで decode → ファイルに書き込み
    // remote_path はシェルエスケープ済み
    let escaped_path = shell_escape(remote_path);
    let decode_cmd = format!("echo '{encoded}' | base64 -d > {escaped_path}");

    let result = exec_with_timeout(host, "root", &decode_cmd, Duration::from_secs(60)).await?;
    if !result.success {
        return Err(CloudError::CommandFailed(format!(
            "ファイル転送失敗: {}",
            result.stderr
        )));
    }

    Ok(())
}

fn base64_encode(data: &[u8]) -> String {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut encoder = Base64Encoder::new(&mut buf);
        encoder.write_all(data).ok();
    }
    String::from_utf8(buf).unwrap_or_default()
}

/// 簡易 base64 エンコーダー（外部依存なし）
struct Base64Encoder<'a> {
    output: &'a mut Vec<u8>,
}

impl<'a> Base64Encoder<'a> {
    fn new(output: &'a mut Vec<u8>) -> Self {
        Self { output }
    }
}

impl std::io::Write for Base64Encoder<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        for chunk in buf.chunks(3) {
            match chunk.len() {
                3 => {
                    self.output.push(TABLE[(chunk[0] >> 2) as usize]);
                    self.output
                        .push(TABLE[((chunk[0] & 0x03) << 4 | chunk[1] >> 4) as usize]);
                    self.output
                        .push(TABLE[((chunk[1] & 0x0f) << 2 | chunk[2] >> 6) as usize]);
                    self.output.push(TABLE[(chunk[2] & 0x3f) as usize]);
                }
                2 => {
                    self.output.push(TABLE[(chunk[0] >> 2) as usize]);
                    self.output
                        .push(TABLE[((chunk[0] & 0x03) << 4 | chunk[1] >> 4) as usize]);
                    self.output.push(TABLE[((chunk[1] & 0x0f) << 2) as usize]);
                    self.output.push(b'=');
                }
                1 => {
                    self.output.push(TABLE[(chunk[0] >> 2) as usize]);
                    self.output.push(TABLE[((chunk[0] & 0x03) << 4) as usize]);
                    self.output.push(b'=');
                    self.output.push(b'=');
                }
                _ => {}
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_escape_simple() {
        assert_eq!(shell_escape("/tmp/file.txt"), "'/tmp/file.txt'");
    }

    #[test]
    fn test_shell_escape_with_quote() {
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
    }

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b"Hello"), "SGVsbG8=");
        assert_eq!(base64_encode(b"Hi"), "SGk=");
        assert_eq!(base64_encode(b"A"), "QQ==");
        assert_eq!(base64_encode(b"abc"), "YWJj");
    }

    #[test]
    fn test_ssh_result_serialize() {
        let result = SshResult {
            host: "creo-prod".into(),
            exit_code: 0,
            stdout: "ok\n".into(),
            stderr: String::new(),
            success: true,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["host"], "creo-prod");
        assert!(json["success"].as_bool().unwrap());
    }
}
