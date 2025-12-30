use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FlowError {
    #[error("KDLパースエラー: {0}")]
    KdlParse(#[from] kdl::KdlError),

    #[error("ファイル読み込みエラー: {0}")]
    Io(#[from] std::io::Error),

    #[error("IO エラー: {path}\n理由: {message}")]
    IoError { path: PathBuf, message: String },

    #[error("無効な設定: {0}")]
    InvalidConfig(String),

    #[error("テンプレートエラー: {file}\n理由: {message}")]
    TemplateError {
        file: PathBuf,
        line: Option<usize>,
        message: String,
    },

    #[error("テンプレート展開エラー: {0}")]
    TemplateRenderError(String),

    #[error("ファイル発見エラー: {path}\n理由: {message}")]
    DiscoveryError { path: PathBuf, message: String },

    #[error(
        "プロジェクトルートが見つかりません\n探索開始位置: {0}\nヒント: fleet.kdl ファイルを含むディレクトリで実行してください"
    )]
    ProjectRootNotFound(PathBuf),

    #[error("サービスが見つかりません: {0}")]
    ServiceNotFound(String),

    #[error("環境が見つかりません: {0}")]
    EnvironmentNotFound(String),

    #[error("循環依存が検出されました: {0}")]
    CircularDependency(String),

    #[error("サービス '{0}' に image が指定されていません")]
    MissingImage(String),
}

pub type Result<T> = std::result::Result<T, FlowError>;
