use thiserror::Error;

#[derive(Error, Debug)]
pub enum FlowError {
    #[error("KDLパースエラー: {0}")]
    KdlParse(#[from] kdl::KdlError),

    #[error("ファイル読み込みエラー: {0}")]
    Io(#[from] std::io::Error),

    #[error("無効な設定: {0}")]
    InvalidConfig(String),

    #[error("サービスが見つかりません: {0}")]
    ServiceNotFound(String),

    #[error("環境が見つかりません: {0}")]
    EnvironmentNotFound(String),

    #[error("循環依存が検出されました: {0}")]
    CircularDependency(String),
}

pub type Result<T> = std::result::Result<T, FlowError>;
