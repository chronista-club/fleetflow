use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("Dockerfile not found: {0}")]
    DockerfileNotFound(PathBuf),

    #[error("Build context directory not found: {0}")]
    ContextNotFound(PathBuf),

    #[error("Docker connection error: {0}")]
    DockerConnection(#[from] bollard::errors::Error),

    #[error("Build failed: {0}")]
    BuildFailed(String),

    #[error("Invalid build configuration: {0}")]
    InvalidConfig(String),

    #[error("Variable not found: {0}")]
    VariableNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl BuildError {
    /// ユーザー向けの分かりやすいエラーメッセージ
    pub fn user_message(&self) -> String {
        match self {
            BuildError::DockerfileNotFound(path) => {
                format!(
                    "Dockerfileが見つかりません: {}\n\
                     \n\
                     解決方法:\n\
                     1. Dockerfileのパスを確認してください\n\
                     2. flow.kdlで明示的にパスを指定してください:\n\
                        dockerfile \"path/to/Dockerfile\"",
                    path.display()
                )
            }
            BuildError::BuildFailed(msg) => {
                format!(
                    "ビルドに失敗しました: {}\n\
                     \n\
                     Dockerfileの内容を確認してください。",
                    msg
                )
            }
            BuildError::ContextNotFound(path) => {
                format!(
                    "ビルドコンテキストが見つかりません: {}\n\
                     \n\
                     flow.kdlでcontextパスを確認してください。",
                    path.display()
                )
            }
            _ => format!("{}", self),
        }
    }
}

pub type Result<T> = std::result::Result<T, BuildError>;
