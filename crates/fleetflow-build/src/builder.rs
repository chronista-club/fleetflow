use crate::error::{BuildError, BuildResult};
use bollard::Docker;
use colored::Colorize;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

/// ImageBuilder - docker buildxを使用してBuildKitでイメージをビルド
pub struct ImageBuilder {
    // Docker接続（後方互換性のため保持、実際はCLI経由でビルド）
    #[allow(dead_code)]
    docker: Docker,
}

impl ImageBuilder {
    pub fn new(docker: Docker) -> Self {
        Self { docker }
    }

    /// イメージをビルド（docker buildx使用でBuildKit有効）
    pub async fn build_image_from_path(
        &self,
        context_path: &Path,
        dockerfile_path: &Path,
        tag: &str,
        build_args: HashMap<String, String>,
        target: Option<&str>,
        no_cache: bool,
        platform: Option<&str>,
    ) -> BuildResult<()> {
        tracing::info!("Building image: {}", tag);

        let mut cmd = Command::new("docker");
        cmd.arg("buildx")
            .arg("build")
            .arg("-t")
            .arg(tag)
            .arg("-f")
            .arg(dockerfile_path);

        // ビルド引数
        for (key, value) in &build_args {
            cmd.arg("--build-arg").arg(format!("{}={}", key, value));
        }

        // ターゲットステージ
        if let Some(t) = target {
            cmd.arg("--target").arg(t);
        }

        // キャッシュ無効化
        if no_cache {
            cmd.arg("--no-cache");
        }

        // プラットフォーム指定
        if let Some(p) = platform {
            cmd.arg("--platform").arg(p);
        }

        // ローカルにロード（デフォルト）
        cmd.arg("--load");

        // コンテキストパス
        cmd.arg(context_path);

        // 出力をリアルタイムで表示
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        tracing::debug!("Build command: {:?}", cmd);
        if !build_args.is_empty() {
            tracing::debug!("Build args: {:?}", build_args);
        }

        let mut child = cmd.spawn().map_err(|e| {
            BuildError::BuildFailed(format!("Failed to spawn docker buildx: {}", e))
        })?;

        // stdoutを読み取り
        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if let Ok(line) = line {
                    println!("{}", line);
                }
            }
        }

        // stderrを読み取り
        if let Some(stderr) = child.stderr.take() {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(line) = line {
                    eprintln!("{}", line);
                }
            }
        }

        let status = child.wait().map_err(|e| {
            BuildError::BuildFailed(format!("Failed to wait for docker buildx: {}", e))
        })?;

        if !status.success() {
            return Err(BuildError::BuildFailed(format!(
                "docker buildx build failed with exit code: {:?}",
                status.code()
            )));
        }

        tracing::info!("{}", format!("Successfully built: {}", tag).green());
        Ok(())
    }

    /// イメージをビルド（tarコンテキスト用 - 互換性のため残す）
    pub async fn build_image(
        &self,
        context_data: Vec<u8>,
        tag: &str,
        build_args: HashMap<String, String>,
        target: Option<&str>,
        no_cache: bool,
    ) -> BuildResult<()> {
        // tarファイルを一時ディレクトリに展開してビルド
        let temp_dir = tempfile::tempdir().map_err(|e| {
            BuildError::BuildFailed(format!("Failed to create temp dir: {}", e))
        })?;

        // tarを展開
        use std::io::Cursor;
        let cursor = Cursor::new(context_data);
        let mut archive = tar::Archive::new(cursor);
        archive.unpack(temp_dir.path()).map_err(|e| {
            BuildError::BuildFailed(format!("Failed to unpack context: {}", e))
        })?;

        let dockerfile_path = temp_dir.path().join("Dockerfile");

        self.build_image_from_path(
            temp_dir.path(),
            &dockerfile_path,
            tag,
            build_args,
            target,
            no_cache,
            None,
        )
        .await
    }

    /// イメージの存在確認
    pub async fn image_exists(&self, image_tag: &str) -> BuildResult<bool> {
        let output = Command::new("docker")
            .args(["image", "inspect", image_tag])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| BuildError::BuildFailed(format!("Failed to check image: {}", e)))?;

        Ok(output.success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    #[ignore] // Docker接続が必要なため、通常のテストではスキップ
    async fn test_build_simple_image() {
        let docker = Docker::connect_with_local_defaults().unwrap();
        let builder = ImageBuilder::new(docker);

        let temp_dir = tempdir().unwrap();
        let dockerfile = temp_dir.path().join("Dockerfile");
        fs::write(&dockerfile, "FROM alpine:latest\nCMD echo 'test'").unwrap();

        let result = builder
            .build_image_from_path(
                temp_dir.path(),
                &dockerfile,
                "fleetflow-test:latest",
                HashMap::new(),
                None,
                false,
                None,
            )
            .await;

        assert!(result.is_ok());

        // クリーンアップ
        Command::new("docker")
            .args(["rmi", "fleetflow-test:latest"])
            .status()
            .ok();
    }
}
