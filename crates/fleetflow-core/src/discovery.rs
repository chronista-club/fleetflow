//! ファイル自動発見機能
//!
//! 規約ベースのディレクトリ構造からKDLファイルを自動的に発見します。

use crate::error::{FlowError, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// 発見されたファイル群
#[derive(Debug, Clone, Default)]
pub struct DiscoveredFiles {
    /// ルートファイル (flow.kdl)
    pub root: Option<PathBuf>,
    /// クラウドインフラ定義ファイル (cloud.kdl)
    pub cloud: Option<PathBuf>,
    /// サービス定義ファイル (services/**/*.kdl)
    pub services: Vec<PathBuf>,
    /// ワークロード定義ファイル (workloads/*.kdl)
    pub workloads: Vec<PathBuf>,
    /// ステージ定義ファイル (stages/**/*.kdl)
    pub stages: Vec<PathBuf>,
    /// 変数定義ファイル (variables/**/*.kdl)
    pub variables: Vec<PathBuf>,
    /// ローカルオーバーライドファイル (flow.local.kdl)
    pub local_override: Option<PathBuf>,
    /// ステージ固有オーバーライドファイル (flow.{stage}.kdl)
    pub stage_override: Option<PathBuf>,
    /// 環境変数ファイル (.env)
    pub env_file: Option<PathBuf>,
}

/// プロジェクトルートを検出
///
/// 以下の優先順位で検索:
/// 1. 環境変数 FLEETFLOW_PROJECT_ROOT
/// 2. カレントディレクトリから上に向かって以下を探す:
///    - flow.kdl
///    - .fleetflow/flow.kdl
#[tracing::instrument]
pub fn find_project_root() -> Result<PathBuf> {
    // 1. 環境変数
    if let Ok(root) = std::env::var("FLEETFLOW_PROJECT_ROOT") {
        let path = PathBuf::from(&root);
        debug!(env_root = %root, "Checking FLEETFLOW_PROJECT_ROOT");
        if path.join("flow.kdl").exists() || path.join(".fleetflow/flow.kdl").exists() {
            info!(project_root = %path.display(), "Found project root from environment variable");
            return Ok(path);
        }
    }

    // 2. カレントディレクトリから上に向かって探す
    let start_dir = std::env::current_dir()?;
    let mut current = start_dir.clone();
    debug!(start_dir = %start_dir.display(), "Searching for project root");

    loop {
        // flow.kdl をチェック
        let flow_file = current.join("flow.kdl");
        debug!(checking = %current.display(), "Looking for flow.kdl");
        if flow_file.exists() {
            info!(project_root = %current.display(), "Found project root (flow.kdl)");
            return Ok(current);
        }

        // .fleetflow/flow.kdl をチェック
        let fleetflow_dir_file = current.join(".fleetflow/flow.kdl");
        if fleetflow_dir_file.exists() {
            info!(project_root = %current.display(), "Found project root (.fleetflow/flow.kdl)");
            return Ok(current);
        }

        // 親ディレクトリへ
        if !current.pop() {
            break;
        }
    }

    warn!(start_dir = %start_dir.display(), "Project root not found");
    Err(FlowError::ProjectRootNotFound(start_dir))
}

/// プロジェクトルートからファイルを自動発見
#[tracing::instrument(skip(project_root), fields(project_root = %project_root.display()))]
pub fn discover_files(project_root: &Path) -> Result<DiscoveredFiles> {
    discover_files_with_stage(project_root, None)
}

/// ステージ指定でプロジェクトルートからファイルを自動発見
///
/// stage が指定されている場合、flow.{stage}.kdl も検出します。
#[tracing::instrument(skip(project_root), fields(project_root = %project_root.display()))]
pub fn discover_files_with_stage(
    project_root: &Path,
    stage: Option<&str>,
) -> Result<DiscoveredFiles> {
    debug!("Starting file discovery");
    let mut discovered = DiscoveredFiles::default();

    // flow.kdl または .fleetflow/flow.kdl
    let root_file = project_root.join("flow.kdl");
    let fleetflow_root_file = project_root.join(".fleetflow/flow.kdl");
    let actual_root = if root_file.exists() {
        debug!(file = %root_file.display(), "Found root file");
        discovered.root = Some(root_file.clone());
        Some(root_file)
    } else if fleetflow_root_file.exists() {
        debug!(file = %fleetflow_root_file.display(), "Found root file in .fleetflow/");
        discovered.root = Some(fleetflow_root_file.clone());
        Some(fleetflow_root_file)
    } else {
        None
    };

    // cloud.kdl または .fleetflow/cloud.kdl（クラウドインフラ定義）
    let cloud_file = project_root.join("cloud.kdl");
    let fleetflow_cloud_file = project_root.join(".fleetflow/cloud.kdl");
    if cloud_file.exists() {
        debug!(file = %cloud_file.display(), "Found cloud config file");
        discovered.cloud = Some(cloud_file);
    } else if fleetflow_cloud_file.exists() {
        debug!(file = %fleetflow_cloud_file.display(), "Found cloud config file in .fleetflow/");
        discovered.cloud = Some(fleetflow_cloud_file);
    }

    // workload 宣言の解析とファイル発見
    if let Some(root_path) = actual_root
        && let Ok(content) = std::fs::read_to_string(&root_path)
    {
        let workload_names = extract_workload_names(&content);
        for name in workload_names {
            // 1. workloads/{name}.kdl
            let direct_file = project_root.join(format!("workloads/{}.kdl", name));
            if direct_file.exists() {
                discovered.workloads.push(direct_file);
            }

            // 2. workloads/{name}/*.kdl
            let workload_dir = project_root.join(format!("workloads/{}", name));
            if workload_dir.is_dir()
                && let Ok(files) = discover_kdl_files(&workload_dir)
            {
                discovered.workloads.extend(files);
            }
        }
    }

    // services/**/*.kdl
    let services_dir = project_root.join("services");
    if services_dir.is_dir() {
        discovered.services = discover_kdl_files(&services_dir)?;
        info!(
            service_count = discovered.services.len(),
            "Discovered service files"
        );
    }

    // stages/**/*.kdl
    let stages_dir = project_root.join("stages");
    if stages_dir.is_dir() {
        discovered.stages = discover_kdl_files(&stages_dir)?;
        info!(
            stage_count = discovered.stages.len(),
            "Discovered stage files"
        );
    }

    // variables/**/*.kdl
    let variables_dir = project_root.join("variables");
    if variables_dir.is_dir() {
        discovered.variables = discover_kdl_files(&variables_dir)?;
        info!(
            variable_count = discovered.variables.len(),
            "Discovered variable files"
        );
    }

    // flow.{stage}.kdl または .fleetflow/flow.{stage}.kdl（ステージ指定時のみ）
    if let Some(stage_name) = stage {
        let stage_file = project_root.join(format!("flow.{}.kdl", stage_name));
        let fleetflow_stage_file = project_root.join(format!(".fleetflow/flow.{}.kdl", stage_name));
        if stage_file.exists() {
            debug!(file = %stage_file.display(), stage = %stage_name, "Found stage override file");
            discovered.stage_override = Some(stage_file);
        } else if fleetflow_stage_file.exists() {
            debug!(file = %fleetflow_stage_file.display(), stage = %stage_name, "Found stage override file in .fleetflow/");
            discovered.stage_override = Some(fleetflow_stage_file);
        }
    }

    // flow.local.kdl または .fleetflow/flow.local.kdl
    let local_override = project_root.join("flow.local.kdl");
    let fleetflow_local_override = project_root.join(".fleetflow/flow.local.kdl");
    if local_override.exists() {
        discovered.local_override = Some(local_override);
    } else if fleetflow_local_override.exists() {
        discovered.local_override = Some(fleetflow_local_override);
    }

    // .env または .fleetflow/.env
    let env_file = project_root.join(".env");
    let fleetflow_env_file = project_root.join(".fleetflow/.env");
    if env_file.exists() {
        debug!(file = %env_file.display(), "Found .env file");
        discovered.env_file = Some(env_file);
    } else if fleetflow_env_file.exists() {
        debug!(file = %fleetflow_env_file.display(), "Found .env file in .fleetflow/");
        discovered.env_file = Some(fleetflow_env_file);
    }

    Ok(discovered)
}

/// ディレクトリ配下の .kdl ファイルを再帰的に発見
///
/// アルファベット順にソートして返す
fn discover_kdl_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut visited = HashSet::new();

    visit_dir(dir, &mut files, &mut visited)?;

    // アルファベット順にソート
    files.sort();

    Ok(files)
}

/// ディレクトリを再帰的に走査
fn visit_dir(dir: &Path, files: &mut Vec<PathBuf>, visited: &mut HashSet<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }

    // 正規化されたパスを取得してループを検出
    let canonical_dir = dir.canonicalize().map_err(|e| FlowError::DiscoveryError {
        path: dir.to_path_buf(),
        message: format!("パスの正規化に失敗: {}", e),
    })?;

    // ループ検出: 既に訪問済みなら終了
    if visited.contains(&canonical_dir) {
        warn!(dir = %canonical_dir.display(), "Symlink loop detected, skipping");
        return Ok(());
    }

    // 訪問済みとしてマーク
    visited.insert(canonical_dir.clone());

    let entries = std::fs::read_dir(dir).map_err(|e| FlowError::DiscoveryError {
        path: dir.to_path_buf(),
        message: format!("ディレクトリの読み込みに失敗: {}", e),
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| FlowError::DiscoveryError {
            path: dir.to_path_buf(),
            message: format!("ディレクトリエントリの読み込みに失敗: {}", e),
        })?;
        let path = entry.path();

        if path.is_dir() {
            // 再帰的に探索
            visit_dir(&path, files, visited)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("kdl") {
            files.push(path);
        }
    }

    Ok(())
}

/// KDL文字列から workload 名を抽出する簡易スキャナ
fn extract_workload_names(content: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    for line in content.lines() {
        let line = line.trim();
        // コメント行を無視
        if line.starts_with("//") || line.starts_with('/') {
            continue;
        }

        // workload "name" 形式を探す
        if line.starts_with("workload") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let name = parts[1].trim_matches('"').trim_matches('\'').to_string();
                if !name.is_empty() {
                    names.insert(name);
                }
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_project(base: &Path) -> Result<()> {
        // flow.kdl
        fs::write(base.join("flow.kdl"), "// root")?;

        // services/
        fs::create_dir_all(base.join("services"))?;
        fs::write(base.join("services/api.kdl"), "service \"api\" {}")?;
        fs::write(
            base.join("services/postgres.kdl"),
            "service \"postgres\" {}",
        )?;

        // services/backend/
        fs::create_dir_all(base.join("services/backend"))?;
        fs::write(
            base.join("services/backend/worker.kdl"),
            "service \"worker\" {}",
        )?;

        // stages/
        fs::create_dir_all(base.join("stages"))?;
        fs::write(base.join("stages/local.kdl"), "stage \"local\" {}")?;
        fs::write(base.join("stages/prod.kdl"), "stage \"prod\" {}")?;

        // variables/
        fs::create_dir_all(base.join("variables"))?;
        fs::write(base.join("variables/common.kdl"), "variables {}")?;

        // flow.local.kdl
        fs::write(base.join("flow.local.kdl"), "// local override")?;

        Ok(())
    }

    #[test]
    fn test_discover_files() -> Result<()> {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path();

        create_test_project(project_root)?;

        let discovered = discover_files(project_root)?;

        // flow.kdl
        assert!(discovered.root.is_some());

        // services
        assert_eq!(discovered.services.len(), 3);
        assert!(discovered.services[0].ends_with("services/api.kdl"));
        assert!(discovered.services[1].ends_with("services/backend/worker.kdl"));
        assert!(discovered.services[2].ends_with("services/postgres.kdl"));

        // stages
        assert_eq!(discovered.stages.len(), 2);
        assert!(discovered.stages[0].ends_with("stages/local.kdl"));
        assert!(discovered.stages[1].ends_with("stages/prod.kdl"));

        // variables
        assert_eq!(discovered.variables.len(), 1);
        assert!(discovered.variables[0].ends_with("variables/common.kdl"));

        // flow.local.kdl
        assert!(discovered.local_override.is_some());

        Ok(())
    }

    #[test]
    fn test_discover_files_minimal() -> Result<()> {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path();

        // 最小構成: flow.kdl のみ
        fs::write(project_root.join("flow.kdl"), "// root")?;

        let discovered = discover_files(project_root)?;

        assert!(discovered.root.is_some());
        assert_eq!(discovered.services.len(), 0);
        assert_eq!(discovered.stages.len(), 0);
        assert_eq!(discovered.variables.len(), 0);
        assert!(discovered.local_override.is_none());

        Ok(())
    }

    #[test]
    fn test_alphabetical_order() -> Result<()> {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path();

        fs::write(project_root.join("flow.kdl"), "// root")?;
        fs::create_dir_all(project_root.join("services"))?;

        // アルファベット順ではない順序で作成
        fs::write(project_root.join("services/zebra.kdl"), "")?;
        fs::write(project_root.join("services/alpha.kdl"), "")?;
        fs::write(project_root.join("services/beta.kdl"), "")?;

        let discovered = discover_files(project_root)?;

        // アルファベット順にソートされていることを確認
        assert!(discovered.services[0].ends_with("services/alpha.kdl"));
        assert!(discovered.services[1].ends_with("services/beta.kdl"));
        assert!(discovered.services[2].ends_with("services/zebra.kdl"));

        Ok(())
    }

    #[test]
    fn test_discover_files_in_fleetflow_dir() -> Result<()> {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path();

        // .fleetflow/ ディレクトリに flow.kdl を配置
        fs::create_dir_all(project_root.join(".fleetflow"))?;
        fs::write(
            project_root.join(".fleetflow/flow.kdl"),
            "// root in .fleetflow",
        )?;
        fs::write(
            project_root.join(".fleetflow/flow.local.kdl"),
            "// local override",
        )?;

        let discovered = discover_files(project_root)?;

        // .fleetflow/flow.kdl が発見される
        assert!(discovered.root.is_some());
        assert!(
            discovered
                .root
                .as_ref()
                .unwrap()
                .ends_with(".fleetflow/flow.kdl")
        );

        // .fleetflow/flow.local.kdl が発見される
        assert!(discovered.local_override.is_some());
        assert!(
            discovered
                .local_override
                .as_ref()
                .unwrap()
                .ends_with(".fleetflow/flow.local.kdl")
        );

        Ok(())
    }

    #[test]
    fn test_root_file_priority_over_fleetflow_dir() -> Result<()> {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path();

        // 両方に flow.kdl を配置
        fs::write(project_root.join("flow.kdl"), "// root")?;
        fs::create_dir_all(project_root.join(".fleetflow"))?;
        fs::write(
            project_root.join(".fleetflow/flow.kdl"),
            "// root in .fleetflow",
        )?;

        let discovered = discover_files(project_root)?;

        // ./flow.kdl が優先される
        assert!(discovered.root.is_some());
        assert!(discovered.root.as_ref().unwrap().ends_with("flow.kdl"));
        assert!(
            !discovered
                .root
                .as_ref()
                .unwrap()
                .to_string_lossy()
                .contains(".fleetflow")
        );

        Ok(())
    }
}
