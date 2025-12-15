//! レジストリ認証処理
//!
//! Docker config.json から認証情報を取得し、Bollard の DockerCredentials に変換します。

use crate::error::{BuildError, BuildResult};
use base64::Engine;
use bollard::auth::DockerCredentials;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Docker config.json の構造
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DockerConfig {
    /// 認証情報 (レジストリ -> AuthEntry)
    #[serde(default)]
    auths: HashMap<String, AuthEntry>,
    /// credential helper 名 (例: "osxkeychain", "desktop")
    #[serde(default)]
    creds_store: Option<String>,
}

/// 認証エントリ
#[derive(Debug, Deserialize)]
struct AuthEntry {
    /// Base64エンコードされた "username:password"
    auth: Option<String>,
}

/// credential helper からのレスポンス
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CredentialResponse {
    username: String,
    secret: String,
}

/// レジストリ認証を管理
#[derive(Debug)]
pub struct RegistryAuth {
    config_path: PathBuf,
}

impl Default for RegistryAuth {
    fn default() -> Self {
        Self::new()
    }
}

impl RegistryAuth {
    /// 新しい RegistryAuth を作成
    ///
    /// デフォルトで ~/.docker/config.json を使用
    pub fn new() -> Self {
        let config_path = std::env::var("DOCKER_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .map(|h| h.join(".docker"))
                    .unwrap_or_else(|| PathBuf::from(".docker"))
            })
            .join("config.json");

        Self { config_path }
    }

    /// 指定したパスの config.json を使用
    pub fn with_config_path(config_path: PathBuf) -> Self {
        Self { config_path }
    }

    /// イメージ名からレジストリの認証情報を取得
    ///
    /// # Arguments
    /// * `image` - イメージ名（例: "ghcr.io/org/myapp:v1.0"）
    ///
    /// # Returns
    /// * `Ok(Some(credentials))` - 認証情報が見つかった場合
    /// * `Ok(None)` - 認証情報が不要または見つからない場合
    /// * `Err(e)` - 認証情報の取得に失敗した場合
    pub fn get_credentials(&self, image: &str) -> BuildResult<Option<DockerCredentials>> {
        let registry = self.extract_registry(image);

        // config.json が存在しない場合は認証なしで続行
        if !self.config_path.exists() {
            tracing::debug!("Docker config.json not found at {:?}", self.config_path);
            return Ok(None);
        }

        let config = self.load_docker_config()?;

        // 1. auths セクションを確認
        if let Some(auth_entry) = config.auths.get(&registry)
            && let Some(auth_b64) = &auth_entry.auth
            && let Some(creds) = self.decode_auth(auth_b64, &registry)?
        {
            tracing::debug!("Found credentials in auths for {}", registry);
            return Ok(Some(creds));
        }

        // 2. credential helper を確認
        if let Some(helper) = &config.creds_store {
            tracing::debug!("Trying credential helper: {}", helper);
            if let Ok(Some(creds)) = self.get_from_helper(helper, &registry) {
                return Ok(Some(creds));
            }
        }

        tracing::debug!("No credentials found for {}", registry);
        Ok(None)
    }

    /// イメージ名からレジストリを抽出
    ///
    /// # Examples
    /// - `ghcr.io/org/app:tag` -> `ghcr.io`
    /// - `myuser/app:tag` -> `docker.io`
    /// - `123456.dkr.ecr.region.amazonaws.com/app` -> `123456.dkr.ecr.region.amazonaws.com`
    /// - `localhost:5000/app` -> `localhost:5000`
    pub fn extract_registry(&self, image: &str) -> String {
        // まず / で分割してレジストリ候補を取得
        let parts: Vec<&str> = image.split('/').collect();

        if parts.len() >= 2 {
            let first = parts[0];

            // レジストリの判定:
            // - `.` を含む（例: ghcr.io, gcr.io, *.amazonaws.com）
            // - `:` を含む（例: localhost:5000）
            // - ただし、タグの `:` は除外する必要があるため、
            //   first にはタグが含まれないことを確認
            if first.contains('.') || first.contains(':') {
                return first.to_string();
            }
        }

        // デフォルトは Docker Hub
        "docker.io".to_string()
    }

    /// Docker config.json を読み込み
    fn load_docker_config(&self) -> BuildResult<DockerConfig> {
        let content =
            std::fs::read_to_string(&self.config_path).map_err(|e| BuildError::AuthFailed {
                registry: self.config_path.display().to_string(),
                message: format!("Failed to read config.json: {}", e),
            })?;

        serde_json::from_str(&content).map_err(|e| BuildError::AuthFailed {
            registry: self.config_path.display().to_string(),
            message: format!("Failed to parse config.json: {}", e),
        })
    }

    /// Base64エンコードされた認証情報をデコード
    fn decode_auth(
        &self,
        auth_b64: &str,
        registry: &str,
    ) -> BuildResult<Option<DockerCredentials>> {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(auth_b64)
            .map_err(|e| BuildError::AuthFailed {
                registry: registry.to_string(),
                message: format!("Failed to decode auth: {}", e),
            })?;

        let auth_str = String::from_utf8(decoded).map_err(|e| BuildError::AuthFailed {
            registry: registry.to_string(),
            message: format!("Invalid UTF-8 in auth: {}", e),
        })?;

        if let Some((username, password)) = auth_str.split_once(':') {
            Ok(Some(DockerCredentials {
                username: Some(username.to_string()),
                password: Some(password.to_string()),
                serveraddress: Some(registry.to_string()),
                ..Default::default()
            }))
        } else {
            Ok(None)
        }
    }

    /// credential helper から認証情報を取得
    fn get_from_helper(
        &self,
        helper: &str,
        registry: &str,
    ) -> BuildResult<Option<DockerCredentials>> {
        let helper_cmd = format!("docker-credential-{}", helper);

        let mut child = Command::new(&helper_cmd)
            .arg("get")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| BuildError::AuthFailed {
                registry: registry.to_string(),
                message: format!("Failed to run {}: {}", helper_cmd, e),
            })?;

        // レジストリ名を stdin に渡す
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(registry.as_bytes()).ok();
        }

        let output = child
            .wait_with_output()
            .map_err(|e| BuildError::AuthFailed {
                registry: registry.to_string(),
                message: format!("Credential helper failed: {}", e),
            })?;

        if !output.status.success() {
            // credential helper が認証情報を持っていない場合は None を返す
            tracing::debug!(
                "Credential helper returned error for {}: {}",
                registry,
                String::from_utf8_lossy(&output.stderr)
            );
            return Ok(None);
        }

        let response: CredentialResponse =
            serde_json::from_slice(&output.stdout).map_err(|e| BuildError::AuthFailed {
                registry: registry.to_string(),
                message: format!("Failed to parse credential helper response: {}", e),
            })?;

        Ok(Some(DockerCredentials {
            username: Some(response.username),
            password: Some(response.secret),
            serveraddress: Some(registry.to_string()),
            ..Default::default()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_registry_ghcr() {
        let auth = RegistryAuth::new();
        assert_eq!(auth.extract_registry("ghcr.io/org/app"), "ghcr.io");
        assert_eq!(auth.extract_registry("ghcr.io/org/app:v1.0"), "ghcr.io");
    }

    #[test]
    fn test_extract_registry_docker_hub() {
        let auth = RegistryAuth::new();
        assert_eq!(auth.extract_registry("myuser/app"), "docker.io");
        assert_eq!(auth.extract_registry("myuser/app:latest"), "docker.io");
        assert_eq!(auth.extract_registry("nginx"), "docker.io");
        assert_eq!(auth.extract_registry("nginx:alpine"), "docker.io");
    }

    #[test]
    fn test_extract_registry_ecr() {
        let auth = RegistryAuth::new();
        assert_eq!(
            auth.extract_registry("123456789.dkr.ecr.ap-northeast-1.amazonaws.com/app"),
            "123456789.dkr.ecr.ap-northeast-1.amazonaws.com"
        );
    }

    #[test]
    fn test_extract_registry_localhost() {
        let auth = RegistryAuth::new();
        assert_eq!(
            auth.extract_registry("localhost:5000/myapp"),
            "localhost:5000"
        );
    }

    #[test]
    fn test_extract_registry_gcr() {
        let auth = RegistryAuth::new();
        assert_eq!(auth.extract_registry("gcr.io/project/app"), "gcr.io");
        assert_eq!(
            auth.extract_registry("asia.gcr.io/project/app"),
            "asia.gcr.io"
        );
    }
}
