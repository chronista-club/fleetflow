pub mod error;

pub use error::*;

use std::path::PathBuf;

/// FleetFlowの設定ファイルパスを取得
pub fn get_config_dir() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or(ConfigError::ConfigDirNotFound)?
        .join("fleetflow");

    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir)?;
    }

    Ok(config_dir)
}

/// プロジェクトのfleet.kdlファイルを探す
///
/// 以下の優先順位で設定ファイルを検索:
/// 1. 環境変数 FLEETFLOW_CONFIG_PATH (直接パス指定)
/// 2. カレントディレクトリ: flow.local.kdl, .flow.local.kdl, fleet.kdl, .fleet.kdl
/// 3. ./.fleetflow/ ディレクトリ内: 同様の順序
/// 4. ~/.config/fleetflow/fleet.kdl (グローバル設定)
pub fn find_flow_file() -> Result<PathBuf> {
    // 1. 環境変数で直接指定
    if let Ok(config_path) = std::env::var("FLEETFLOW_CONFIG_PATH") {
        let path = PathBuf::from(config_path);
        if path.exists() {
            return Ok(path);
        }
    }

    let current_dir = std::env::current_dir()?;
    let candidates = ["flow.local.kdl", ".flow.local.kdl", "fleet.kdl", ".fleet.kdl"];

    // 2. カレントディレクトリで検索
    for filename in &candidates {
        let path = current_dir.join(filename);
        if path.exists() {
            return Ok(path);
        }
    }

    // 3. ./.fleetflow/ ディレクトリで検索
    let flow_dir = current_dir.join(".fleetflow");
    if flow_dir.is_dir() {
        for filename in &candidates {
            let path = flow_dir.join(filename);
            if path.exists() {
                return Ok(path);
            }
        }
    }

    // 4. グローバル設定ファイル (~/.config/fleetflow/fleet.kdl)
    if let Some(config_dir) = dirs::config_dir() {
        let global_config = config_dir.join("fleetflow").join("fleet.kdl");
        if global_config.exists() {
            return Ok(global_config);
        }
    }

    // どの設定ファイルも見つからなかった
    Err(ConfigError::FlowFileNotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;

    #[test]
    fn test_get_config_dir() {
        let result = get_config_dir();
        assert!(result.is_ok());

        let config_dir = result.unwrap();
        assert!(config_dir.ends_with("fleetflow"));
        assert!(config_dir.exists());
    }

    #[test]
    #[serial]
    fn test_find_flow_file_in_current_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().ok();

        // fleet.kdlを作成
        fs::write(temp_dir.path().join("fleet.kdl"), "// test").unwrap();

        // テンポラリディレクトリに移動
        std::env::set_current_dir(&temp_dir).unwrap();

        let result = find_flow_file();
        assert!(result.is_ok());

        let flow_file = result.unwrap();
        assert!(flow_file.ends_with("fleet.kdl"));

        // 元のディレクトリに戻る（エラーは無視）
        if let Some(dir) = original_dir {
            let _ = std::env::set_current_dir(dir);
        }
    }

    #[test]
    #[serial]
    fn test_find_flow_file_local_priority() {
        let temp_dir = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().ok();

        // fleet.kdl と flow.local.kdl の両方を作成
        fs::write(temp_dir.path().join("fleet.kdl"), "// global").unwrap();
        fs::write(temp_dir.path().join("flow.local.kdl"), "// local").unwrap();

        // ディレクトリを移動して検索（環境変数ではなくcurrent_dir方式）
        std::env::set_current_dir(&temp_dir).unwrap();

        let result = find_flow_file().unwrap();

        // flow.local.kdl が優先される
        assert!(result.ends_with("flow.local.kdl"));

        if let Some(dir) = original_dir {
            let _ = std::env::set_current_dir(dir);
        }
    }

    #[test]
    #[serial]
    fn test_find_flow_file_in_flow_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().ok();

        // .fleetflow/ ディレクトリを作成
        let flow_dir = temp_dir.path().join(".fleetflow");
        fs::create_dir(&flow_dir).unwrap();
        fs::write(flow_dir.join("fleet.kdl"), "// in flow dir").unwrap();

        std::env::set_current_dir(&temp_dir).unwrap();

        let result = find_flow_file().unwrap();
        assert!(result.ends_with(".fleetflow/fleet.kdl"));

        if let Some(dir) = original_dir {
            let _ = std::env::set_current_dir(dir);
        }
    }

    #[test]
    #[serial]
    fn test_find_flow_file_env_var() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("custom.kdl");
        fs::write(&config_path, "// custom").unwrap();

        // 環境変数を設定
        unsafe {
            std::env::set_var("FLEETFLOW_CONFIG_PATH", config_path.to_str().unwrap());
        }

        let result = find_flow_file().unwrap();

        // パス全体ではなくファイル名で比較（一時ディレクトリのパスは環境依存）
        assert!(result.ends_with("custom.kdl"));

        // クリーンアップ
        unsafe {
            std::env::remove_var("FLEETFLOW_CONFIG_PATH");
        }
    }

    #[test]
    #[serial]
    fn test_find_flow_file_not_found() {
        let temp_dir = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().ok();

        // 空のディレクトリに移動
        std::env::set_current_dir(&temp_dir).unwrap();

        let result = find_flow_file();
        assert!(result.is_err());

        if let Err(ConfigError::FlowFileNotFound) = result {
            // 期待通りのエラー
        } else {
            panic!("Expected FlowFileNotFound error");
        }

        if let Some(dir) = original_dir {
            let _ = std::env::set_current_dir(dir);
        }
    }

    #[test]
    #[serial]
    fn test_hidden_file_priority() {
        let temp_dir = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().ok();

        // .flow.local.kdl と fleet.kdl を作成
        fs::write(temp_dir.path().join(".flow.local.kdl"), "// hidden local").unwrap();
        fs::write(temp_dir.path().join("fleet.kdl"), "// visible").unwrap();

        std::env::set_current_dir(&temp_dir).unwrap();

        let result = find_flow_file().unwrap();

        // .flow.local.kdl が優先される
        assert!(result.ends_with(".flow.local.kdl"));

        if let Some(dir) = original_dir {
            let _ = std::env::set_current_dir(dir);
        }
    }
}
