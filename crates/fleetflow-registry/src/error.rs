//! Fleet Registry エラー型

/// Fleet Registry のエラー
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("Registry ファイルが見つかりません")]
    NotFound,

    #[error("KDL パースエラー: {0}")]
    KdlParse(#[from] kdl::KdlError),

    #[error("不正な Registry 定義: {0}")]
    InvalidConfig(String),

    #[error("Fleet '{0}' が見つかりません")]
    FleetNotFound(String),

    #[error("Server '{0}' が見つかりません")]
    ServerNotFound(String),

    #[error("Route 解決エラー: fleet '{fleet}' stage '{stage}'")]
    RouteNotFound { fleet: String, stage: String },

    #[error("IO エラー: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, RegistryError>;
