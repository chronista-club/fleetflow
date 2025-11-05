pub mod error;

pub use error::*;

use std::path::PathBuf;

/// Unison Flowの設定ファイルパスを取得
pub fn get_config_dir() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or(ConfigError::ConfigDirNotFound)?
        .join("unison-flow");

    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir)?;
    }

    Ok(config_dir)
}

/// プロジェクトのunison.kdlファイルを探す
pub fn find_unison_file() -> Result<PathBuf> {
    let current_dir = std::env::current_dir()?;
    let unison_file = current_dir.join("unison.kdl");

    if !unison_file.exists() {
        return Err(ConfigError::UnisonFileNotFound(unison_file));
    }

    Ok(unison_file)
}
