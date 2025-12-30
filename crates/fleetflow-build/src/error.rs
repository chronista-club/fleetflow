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

    #[error("Authentication failed for registry {registry}: {message}")]
    AuthFailed { registry: String, message: String },

    #[error("Image name does not contain registry: {image}")]
    NoRegistry { image: String },

    #[error("Push failed: {message}")]
    PushFailed { message: String },

    #[error("Invalid tag: {tag}")]
    InvalidTag { tag: String },
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
                     2. fleet.kdlで明示的にパスを指定してください:\n\
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
                     fleet.kdlでcontextパスを確認してください。",
                    path.display()
                )
            }
            BuildError::AuthFailed { registry, message } => {
                format!(
                    "レジストリへの認証に失敗しました: {}\n\
                     \n\
                     エラー: {}\n\
                     \n\
                     解決方法:\n\
                     • docker login {} を実行してください\n\
                     • CIの場合は適切な認証設定を確認してください",
                    registry, message, registry
                )
            }
            BuildError::NoRegistry { image } => {
                format!(
                    "イメージ名にレジストリが含まれていません: {}\n\
                     \n\
                     解決方法:\n\
                     fleet.kdlで完全なイメージ名を指定してください:\n\
                     image \"ghcr.io/org/{}\"",
                    image, image
                )
            }
            BuildError::PushFailed { message } => {
                format!(
                    "イメージのプッシュに失敗しました: {}\n\
                     \n\
                     解決方法:\n\
                     • ネットワーク接続を確認してください\n\
                     • レジストリへの権限を確認してください\n\
                     • docker login を実行してください",
                    message
                )
            }
            BuildError::InvalidTag { tag } => {
                format!(
                    "タグに使用できない文字が含まれています: {}\n\
                     \n\
                     タグには英数字、ピリオド、ハイフン、アンダースコアのみ使用できます。",
                    tag
                )
            }
            _ => format!("{}", self),
        }
    }
}

/// Result型のエイリアス
pub type BuildResult<T> = std::result::Result<T, BuildError>;
