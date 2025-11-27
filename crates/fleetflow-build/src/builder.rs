use crate::error::{BuildError, Result};
use bollard::image::BuildImageOptions;
use bollard::Docker;
use colored::Colorize;
use futures_util::stream::StreamExt;
use std::collections::HashMap;

pub struct ImageBuilder {
    docker: Docker,
}

impl ImageBuilder {
    pub fn new(docker: Docker) -> Self {
        Self { docker }
    }

    /// イメージをビルド
    pub async fn build_image(
        &self,
        context_data: Vec<u8>,
        tag: &str,
        build_args: HashMap<String, String>,
        target: Option<&str>,
        no_cache: bool,
    ) -> Result<()> {
        tracing::info!("Building image: {}", tag);

        // ビルドオプションの設定
        // build_argsを&str型に変換
        let build_args_refs: std::collections::HashMap<&str, &str> = build_args
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let options = BuildImageOptions {
            dockerfile: "Dockerfile",
            t: tag,
            buildargs: build_args_refs,
            target: target.unwrap_or(""),
            nocache: no_cache,
            rm: true,     // 中間コンテナを削除
            forcerm: true, // ビルド失敗時も中間コンテナを削除
            pull: true,   // ベースイメージを常にpull
            ..Default::default()
        };

        tracing::debug!("Build options: {:?}", options);
        if !build_args.is_empty() {
            tracing::debug!("Build args: {:?}", build_args);
        }

        // ビルドストリームの開始
        use bytes::Bytes;
        use http_body_util::{Either, Full};
        let context_bytes = Bytes::from(context_data);
        let body = Full::new(context_bytes);
        let mut stream = self
            .docker
            .build_image(options, None, Some(Either::Left(body)));

        // ビルド進捗の表示
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(output) => {
                    self.handle_build_output(output)?;
                }
                Err(e) => {
                    return Err(BuildError::DockerConnection(e));
                }
            }
        }

        tracing::info!("Successfully built: {}", tag);
        Ok(())
    }

    /// ビルド出力の処理
    fn handle_build_output(&self, output: bollard::models::BuildInfo) -> Result<()> {
        if let Some(stream) = output.stream {
            // ビルドステップの出力
            print!("{}", stream);
        }

        if let Some(error) = output.error {
            // エラーが発生した場合
            return Err(BuildError::BuildFailed(error));
        }

        if let Some(error_detail) = output.error_detail {
            // 詳細なエラー情報
            let error_msg = error_detail
                .message
                .unwrap_or_else(|| "Unknown build error".to_string());
            return Err(BuildError::BuildFailed(error_msg));
        }

        if let Some(status) = output.status {
            // ステータスメッセージ（pull, push等）
            println!("{}", status.cyan());
        }

        Ok(())
    }

    /// イメージの存在確認
    pub async fn image_exists(&self, image_tag: &str) -> Result<bool> {
        match self.docker.inspect_image(image_tag).await {
            Ok(_) => Ok(true),
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404,
                ..
            }) => Ok(false),
            Err(e) => Err(BuildError::DockerConnection(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Docker接続が必要なため、通常のテストではスキップ
    async fn test_build_simple_image() {
        let docker = Docker::connect_with_local_defaults().unwrap();
        let builder = ImageBuilder::new(docker);

        // シンプルなDockerfileを含むコンテキストを作成
        use crate::context::ContextBuilder;
        use std::fs;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let dockerfile = temp_dir.path().join("Dockerfile");
        fs::write(&dockerfile, "FROM alpine:latest\nCMD echo 'test'").unwrap();

        let context_data = ContextBuilder::create_context(temp_dir.path(), &dockerfile).unwrap();

        let result = builder
            .build_image(
                context_data,
                "fleetflow-test:latest",
                HashMap::new(),
                None,
                false,
            )
            .await;

        assert!(result.is_ok());

        // クリーンアップ
        builder
            .docker
            .remove_image("fleetflow-test:latest", None, None)
            .await
            .ok();
    }
}
