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

/// プロジェクトのflow.kdlファイルを探す
///
/// 以下の優先順位で設定ファイルを検索:
/// 1. 環境変数 FLOW_CONFIG_PATH (直接パス指定)
/// 2. カレントディレクトリ: flow.local.kdl, .flow.local.kdl, flow.kdl, .flow.kdl
/// 3. ./.flow/ ディレクトリ内: 同様の順序
/// 4. ~/.config/flow/flow.kdl (グローバル設定)
pub fn find_flow_file() -> Result<PathBuf> {
    // 1. 環境変数で直接指定
    if let Ok(config_path) = std::env::var("FLOW_CONFIG_PATH") {
        let path = PathBuf::from(config_path);
        if path.exists() {
            return Ok(path);
        }
    }

    let current_dir = std::env::current_dir()?;
    let candidates = ["flow.local.kdl", ".flow.local.kdl", "flow.kdl", ".flow.kdl"];

    // 2. カレントディレクトリで検索
    for filename in &candidates {
        let path = current_dir.join(filename);
        if path.exists() {
            return Ok(path);
        }
    }

    // 3. ./.flow/ ディレクトリで検索
    let flow_dir = current_dir.join(".flow");
    if flow_dir.is_dir() {
        for filename in &candidates {
            let path = flow_dir.join(filename);
            if path.exists() {
                return Ok(path);
            }
        }
    }

    // 4. グローバル設定ファイル (~/.config/flow/flow.kdl)
    if let Some(config_dir) = dirs::config_dir() {
        let global_config = config_dir.join("flow").join("flow.kdl");
        if global_config.exists() {
            return Ok(global_config);
        }
    }

    // どの設定ファイルも見つからなかった
    Err(ConfigError::FlowFileNotFound)
}
