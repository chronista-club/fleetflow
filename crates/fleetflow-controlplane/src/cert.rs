//! Control Plane の TLS trust マテリアル — club-unison `MeshCa`（private-CA）。
//!
//! CP↔agent / CLI↔CP / MCP↔CP の QUIC trust を private-CA で構成する。CP は CA を
//! 初回に生成して永続化し、起動ごとに自身の server cert を CA から発行する。
//! CA cert（公開部分）はクライアントに配布され、クライアントは
//! `TrustAnchors::Custom([ca_cert])` で CA を信頼 → rustls が leaf を chain 検証する。
//!
//! trust モデル決定: creo-memories `mem_1CbHWGhygnjy5D8bMa1efe`。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::info;
use unison::network::cert::CertSource;
use unison::network::mesh::MeshCa;

/// CP の設定ディレクトリ（`~/.config/fleetflow` 相当、OS 依存）。
fn config_dir() -> Result<PathBuf> {
    Ok(dirs::config_dir()
        .context("設定ディレクトリが見つかりません")?
        .join("fleetflow"))
}

/// CA cert / key の永続化パス（`cp-ca-cert.pem` / `cp-ca-key.pem`）。
///
/// `cp-ca-cert.pem` は公開部分 — クライアント（agent/CLI/MCP）への配布対象。
/// `cp-ca-key.pem` は mesh 全体の cert を発行できる最重要 secret。
pub fn mesh_ca_paths() -> Result<(PathBuf, PathBuf)> {
    let dir = config_dir()?;
    Ok((dir.join("cp-ca-cert.pem"), dir.join("cp-ca-key.pem")))
}

/// MeshCa を取得する — 永続化済みならロード、無ければ生成して永続化する。
///
/// CA key ファイルは 0600（Unix）で書き出す。
pub fn ensure_mesh_ca() -> Result<MeshCa> {
    let (cert_path, key_path) = mesh_ca_paths()?;
    load_or_generate_ca(&cert_path, &key_path)
}

/// 指定パスから MeshCa をロード、両ファイルが揃っていなければ生成して永続化する。
fn load_or_generate_ca(cert_path: &Path, key_path: &Path) -> Result<MeshCa> {
    if cert_path.exists() && key_path.exists() {
        let cert_pem = std::fs::read_to_string(cert_path)
            .with_context(|| format!("CA cert 読み込み失敗: {}", cert_path.display()))?;
        let key_pem = std::fs::read_to_string(key_path)
            .with_context(|| format!("CA key 読み込み失敗: {}", key_path.display()))?;
        let ca = MeshCa::from_pem(&cert_pem, &key_pem).context("MeshCa の復元失敗")?;
        info!(cert = %cert_path.display(), "既存の MeshCa をロード");
        return Ok(ca);
    }

    // 初回 — 生成して永続化
    let ca = MeshCa::generate().context("MeshCa の生成失敗")?;
    let (cert_pem, key_pem) = ca.to_pem();

    if let Some(parent) = cert_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("設定ディレクトリ作成失敗: {}", parent.display()))?;
    }
    std::fs::write(cert_path, cert_pem.as_bytes())
        .with_context(|| format!("CA cert 書き込み失敗: {}", cert_path.display()))?;
    write_secret(key_path, &key_pem)?;

    info!(
        cert = %cert_path.display(),
        key = %key_path.display(),
        "MeshCa を新規生成・永続化（CA key は最重要 secret）"
    );
    Ok(ca)
}

/// CP 自身の QUIC server cert を MeshCa から発行する。
///
/// `sans` はクライアントが接続に使うホスト名・IP（`cp.fleetstage.cloud` /
/// Tailscale IP / `localhost` 等）。rustls の SAN 検証に一致する必要がある。
pub fn issue_server_cert(ca: &MeshCa, sans: &[String]) -> Result<CertSource> {
    ca.issue(sans.iter().cloned())
        .context("CP server cert の発行失敗")
}

/// secret ファイルを 0600（Unix）で書き出す。
fn write_secret(path: &Path, content: &str) -> Result<()> {
    std::fs::write(path, content.as_bytes())
        .with_context(|| format!("secret 書き込み失敗: {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("secret パーミッション設定失敗: {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_or_generate_ca_generates_then_reuses() {
        let dir = tempfile::tempdir().unwrap();
        let cert = dir.path().join("ca-cert.pem");
        let key = dir.path().join("ca-key.pem");

        // 初回 — 生成して永続化
        load_or_generate_ca(&cert, &key).unwrap();
        assert!(cert.exists() && key.exists());
        let cert_pem_1 = std::fs::read_to_string(&cert).unwrap();

        // 2 回目 — 既存をロード（再生成しない）
        load_or_generate_ca(&cert, &key).unwrap();
        let cert_pem_2 = std::fs::read_to_string(&cert).unwrap();
        assert_eq!(cert_pem_1, cert_pem_2, "既存 CA が再生成された");
    }

    #[test]
    fn issue_server_cert_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let ca = load_or_generate_ca(&dir.path().join("c.pem"), &dir.path().join("k.pem")).unwrap();
        issue_server_cert(&ca, &["localhost".into(), "cp.example.com".into()])
            .expect("server cert 発行");
    }

    #[cfg(unix)]
    #[test]
    fn ca_key_file_is_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("k.pem");
        load_or_generate_ca(&dir.path().join("c.pem"), &key).unwrap();
        let mode = std::fs::metadata(&key).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
