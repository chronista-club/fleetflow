use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("設定ディレクトリが見つかりません")]
    ConfigDirNotFound,

    #[error("unison.kdl が見つかりません: {0}")]
    UnisonFileNotFound(PathBuf),

    #[error("IO エラー: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ConfigError>;
