//! Registry発見ロジック
//!
//! fleet-registry.kdl を自動的に発見する。
//! fleetflow-core::discovery と同じパターン（環境変数 → 上方向探索）。

use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Registry設定ファイル名
const REGISTRY_FILENAME: &str = "fleet-registry.kdl";

/// Registry設定ファイルの環境変数
const REGISTRY_PATH_ENV: &str = "FLEET_REGISTRY_PATH";

/// fleet-registry.kdl を発見する
///
/// 検索順序:
/// 1. FLEET_REGISTRY_PATH 環境変数
/// 2. カレントディレクトリの fleet-registry.kdl
/// 3. 上方向探索
///
/// Registryは必須ではないため、見つからない場合は None を返す。
#[tracing::instrument]
pub fn find_registry() -> Option<PathBuf> {
    // 1. 環境変数
    if let Ok(path_str) = std::env::var(REGISTRY_PATH_ENV) {
        let path = PathBuf::from(&path_str);
        debug!(env_path = %path_str, "Checking FLEET_REGISTRY_PATH");
        if path.exists() {
            info!(registry_path = %path.display(), "Found registry from environment variable");
            return Some(path);
        }
        warn!(env_path = %path_str, "FLEET_REGISTRY_PATH is set but file does not exist");
    }

    // 2. カレントディレクトリから上に向かって探す
    let start_dir = std::env::current_dir().ok()?;
    find_registry_from(&start_dir)
}

/// 指定ディレクトリから上方向に fleet-registry.kdl を探す
pub fn find_registry_from(start_dir: &Path) -> Option<PathBuf> {
    let mut current = start_dir.to_path_buf();
    debug!(start_dir = %start_dir.display(), "Searching for {}", REGISTRY_FILENAME);

    loop {
        let registry_file = current.join(REGISTRY_FILENAME);
        if registry_file.exists() {
            info!(registry_path = %registry_file.display(), "Found registry file");
            return Some(registry_file);
        }

        if !current.pop() {
            break;
        }
    }

    debug!("Registry file not found");
    None
}

/// Registry ファイルのパスから Registry ルートディレクトリを取得
pub fn registry_root(registry_path: &Path) -> Option<&Path> {
    registry_path.parent()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_registry_from_with_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        // fleet-registry.kdl を作成
        std::fs::write(root.join(REGISTRY_FILENAME), "registry \"test\"").unwrap();

        let result = find_registry_from(root);
        assert!(result.is_some());
        assert!(result.unwrap().ends_with(REGISTRY_FILENAME));
    }

    #[test]
    fn test_find_registry_from_subdirectory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        // Registry を root に作成
        std::fs::write(root.join(REGISTRY_FILENAME), "registry \"test\"").unwrap();

        // サブディレクトリから探索
        let sub_dir = root.join("fleets").join("creo");
        std::fs::create_dir_all(&sub_dir).unwrap();

        let result = find_registry_from(&sub_dir);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), root.join(REGISTRY_FILENAME));
    }

    #[test]
    fn test_find_registry_from_not_found() {
        let temp_dir = tempfile::tempdir().unwrap();
        let result = find_registry_from(temp_dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_registry_root() {
        let path = PathBuf::from("/home/user/chronista-fleet/fleet-registry.kdl");
        let root = registry_root(&path);
        assert_eq!(root, Some(Path::new("/home/user/chronista-fleet")));
    }
}
