//! 統合ローダー
//!
//! ファイル発見、テンプレート展開、パースを統合

use crate::discovery::{DiscoveredFiles, discover_files_with_stage, find_project_root};
use crate::error::{FlowError, Result};
use crate::model::Flow;
use crate::parser::parse_kdl_string_with_stage;
use crate::template::{TemplateProcessor, Variables, extract_variables_with_stage};
use std::path::Path;
use tracing::{debug, info, instrument};

/// ファイルあたりの推定バイト数（容量事前確保用）
const ESTIMATED_BYTES_PER_FILE: usize = 500;

/// プロジェクト全体をロードしてFlowを生成
///
/// 以下の処理を実行:
/// 1. プロジェクトルートの検出
/// 2. ファイルの自動発見
/// 3. 変数の収集
/// 4. テンプレート展開
/// 5. KDLパース
#[instrument]
pub fn load_project() -> Result<Flow> {
    info!("Starting project load");
    let project_root = find_project_root()?;
    load_project_from_root(&project_root)
}

/// 指定されたルートディレクトリからプロジェクトをロード
#[instrument(skip(project_root), fields(project_root = %project_root.display()))]
pub fn load_project_from_root(project_root: &Path) -> Result<Flow> {
    load_project_from_root_with_stage(project_root, None)
}

/// ステージ指定でプロジェクトをロード
///
/// stage が指定されている場合、flow.{stage}.kdl も読み込んでマージします。
/// 読み込み順序: fleet.kdl → flow.{stage}.kdl → flow.local.kdl
#[instrument(skip(project_root), fields(project_root = %project_root.display()))]
pub fn load_project_from_root_with_stage(project_root: &Path, stage: Option<&str>) -> Result<Flow> {
    // 1. ファイル発見
    debug!("Step 1: Discovering files");
    let discovered = discover_files_with_stage(project_root, stage)?;

    // 2. 変数収集とテンプレート準備
    debug!("Step 2: Preparing template processor");
    let mut processor = prepare_template_processor(&discovered, project_root, stage)?;

    // 3. テンプレート展開
    debug!("Step 3: Expanding templates");
    let expanded_content = expand_all_files(&discovered, &mut processor)?;
    info!(
        content_size = expanded_content.len(),
        "Template expansion complete"
    );

    // 4. KDLパース
    debug!("Step 4: Parsing KDL");
    let name = project_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed")
        .to_string();
    let flow = parse_kdl_string_with_stage(&expanded_content, name, stage)?;
    info!(
        services = flow.services.len(),
        stages = flow.stages.len(),
        "Project loaded successfully"
    );

    Ok(flow)
}

/// テンプレートプロセッサを準備
fn prepare_template_processor(
    discovered: &DiscoveredFiles,
    project_root: &Path,
    stage: Option<&str>,
) -> Result<TemplateProcessor> {
    let mut processor = TemplateProcessor::new();
    let mut all_variables = Variables::new();

    // 0. ビルトイン変数を追加（PROJECT_ROOT）
    processor.add_variable(
        "PROJECT_ROOT",
        serde_json::Value::String(project_root.to_string_lossy().to_string()),
    );

    // 1. グローバル変数（fleet.kdl）とステージ固有変数
    if let Some(root_file) = &discovered.root {
        let content = std::fs::read_to_string(root_file).map_err(|e| FlowError::IoError {
            path: root_file.clone(),
            message: e.to_string(),
        })?;
        let vars = extract_variables_with_stage(&content, stage)?;
        all_variables.extend(vars);
    }

    // 2. variables/**/*.kdl
    for var_file in &discovered.variables {
        let content = std::fs::read_to_string(var_file).map_err(|e| FlowError::IoError {
            path: var_file.clone(),
            message: e.to_string(),
        })?;
        let vars = extract_variables_with_stage(&content, stage)?;
        all_variables.extend(vars);
    }

    // 3. .env ファイルから変数を追加
    if let Some(env_file) = &discovered.env_file {
        processor.add_env_file_variables(env_file)?;
    }

    // 4. .env.external ファイルから変数を追加（外部サービス用、.env を上書き）
    if let Some(external_env_file) = &discovered.external_env_file {
        processor.add_env_file_variables(external_env_file)?;
    }

    // 5. ステージ固有の .env.{stage} ファイルから変数を追加（.env, .env.external を上書き）
    if let Some(stage_env_file) = &discovered.stage_env_file {
        processor.add_env_file_variables(stage_env_file)?;
    }

    // 6. 環境変数を追加（FLEET_*, CI_*, APP_* プレフィックスのみ、最優先）
    processor.add_env_variables();

    // 7. 収集した変数を追加（最も優先度が高い）
    debug!(var_count = all_variables.len(), var_keys = ?all_variables.keys().collect::<Vec<_>>(), "Adding collected variables to processor");
    processor.add_variables(all_variables);

    Ok(processor)
}

/// 全ファイルをテンプレート展開して結合
fn expand_all_files(
    discovered: &DiscoveredFiles,
    processor: &mut TemplateProcessor,
) -> Result<String> {
    // ファイル数から概算容量を計算
    let file_count = discovered.services.len()
        + discovered.stages.len()
        + if discovered.root.is_some() { 1 } else { 0 }
        + if discovered.cloud.is_some() { 1 } else { 0 }
        + if discovered.stage_override.is_some() {
            1
        } else {
            0
        }
        + if discovered.local_override.is_some() {
            1
        } else {
            0
        };
    let estimated_capacity = file_count * ESTIMATED_BYTES_PER_FILE;

    let mut expanded = String::with_capacity(estimated_capacity);

    // 0. cloud.kdl（クラウドインフラ定義 - プロバイダー、サーバー）
    if let Some(cloud_file) = &discovered.cloud {
        debug!(file = %cloud_file.display(), "Rendering cloud config file");
        let rendered = processor.render_file(cloud_file)?;
        expanded.push_str(&rendered);
        expanded.push_str("\n\n");
    }

    // 1. fleet.kdl
    if let Some(root_file) = &discovered.root {
        debug!(file = %root_file.display(), "Rendering root file");
        let rendered = processor.render_file(root_file)?;
        expanded.push_str(&rendered);
        expanded.push_str("\n\n");
    }

    // 2. services/**/*.kdl
    for service_file in &discovered.services {
        debug!(file = %service_file.display(), "Rendering service file");
        let rendered = processor.render_file(service_file)?;
        expanded.push_str(&rendered);
        expanded.push_str("\n\n");
    }

    // 3. stages/**/*.kdl
    for stage_file in &discovered.stages {
        debug!(file = %stage_file.display(), "Rendering stage file");
        let rendered = processor.render_file(stage_file)?;
        expanded.push_str(&rendered);
        expanded.push_str("\n\n");
    }

    // 4. flow.{stage}.kdl（ステージオーバーライド）
    if let Some(stage_file) = &discovered.stage_override {
        debug!(file = %stage_file.display(), "Rendering stage override file");
        let rendered = processor.render_file(stage_file)?;
        expanded.push_str(&rendered);
        expanded.push_str("\n\n");
    }

    // 5. flow.local.kdl（ローカルオーバーライド）
    if let Some(local_file) = &discovered.local_override {
        debug!(file = %local_file.display(), "Rendering local override file");
        let rendered = processor.render_file(local_file)?;
        expanded.push_str(&rendered);
        expanded.push_str("\n\n");
    }

    Ok(expanded)
}

/// デバッグ情報を表示しながらロード
///
/// 環境変数 `FLEET_STAGE` が設定されている場合、ステージ固有の設定も読み込みます。
pub fn load_project_with_debug(project_root: &Path) -> Result<Flow> {
    // FLEET_STAGE 環境変数を取得
    let stage = std::env::var("FLEET_STAGE").ok();
    let stage_ref = stage.as_deref();

    println!("🔍 プロジェクト検出");
    println!("  ルート: {}", project_root.display());
    if let Some(s) = &stage {
        println!("  ステージ: {}", s);
    }

    // ファイル発見（ステージ指定あり）
    let discovered = discover_files_with_stage(project_root, stage_ref)?;

    if discovered.root.is_some() {
        println!("  fleet.kdl: ✓ 検出");
    } else {
        println!("  fleet.kdl: ✗ 未検出");
    }

    if discovered.cloud.is_some() {
        println!("  cloud.kdl: ✓ 検出");
    } else {
        println!("  cloud.kdl: ✗ 未検出");
    }

    println!("\n🔍 ディレクトリスキャン");
    println!(
        "  services/: {}",
        if discovered.services.is_empty() {
            "未検出"
        } else {
            "✓ 検出"
        }
    );
    println!(
        "  stages/: {}",
        if discovered.stages.is_empty() {
            "未検出"
        } else {
            "✓ 検出"
        }
    );

    if !discovered.services.is_empty() {
        println!("\n📂 ファイル発見 (services/)");
        for service in &discovered.services {
            println!("  ✓ {}", service.display());
        }
    }

    if !discovered.stages.is_empty() {
        println!("\n📂 ファイル発見 (stages/)");
        for stage in &discovered.stages {
            println!("  ✓ {}", stage.display());
        }
    }

    if !discovered.variables.is_empty() {
        println!("\n📂 ファイル発見 (variables/)");
        for var in &discovered.variables {
            println!("  ✓ {}", var.display());
        }
    }

    // .env ファイルの表示
    if discovered.env_file.is_some()
        || discovered.external_env_file.is_some()
        || discovered.stage_env_file.is_some()
    {
        println!("\n🔐 環境変数ファイル");
        if let Some(env_file) = &discovered.env_file {
            println!("  ✓ {} (base)", env_file.display());
        }
        if let Some(external_env_file) = &discovered.external_env_file {
            println!("  ✓ {} (external services)", external_env_file.display());
        }
        if let Some(stage_env_file) = &discovered.stage_env_file {
            println!("  ✓ {} (stage-specific)", stage_env_file.display());
        }
    }

    println!("\n📖 変数収集");
    let mut processor = prepare_template_processor(&discovered, project_root, stage_ref)?;
    println!("  ✓ 完了");

    println!("\n📝 テンプレート展開");
    let expanded = expand_all_files(&discovered, &mut processor)?;
    println!("  ✓ 完了 ({}バイト)", expanded.len());

    println!("\n⚙️  KDLパース");
    let name = project_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed")
        .to_string();
    let flow = parse_kdl_string_with_stage(&expanded, name, stage_ref)?;
    println!("  サービス: {}個", flow.services.len());
    println!("  ステージ: {}個", flow.stages.len());

    println!("\n✅ ロード完了\n");

    Ok(flow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_project(base: &Path) -> Result<()> {
        // .fleetflow/fleet.kdl
        fs::create_dir_all(base.join(".fleetflow"))?;
        fs::write(
            base.join(".fleetflow/fleet.kdl"),
            r#"
variables {
    app_version "1.0.0"
    registry "ghcr.io/myorg"
}
"#,
        )?;

        // services/api.kdl
        fs::create_dir_all(base.join("services"))?;
        fs::write(
            base.join("services/api.kdl"),
            r#"
service "api" {
    image "{{ registry }}/api:{{ app_version }}"
}
"#,
        )?;

        // services/postgres.kdl
        fs::write(
            base.join("services/postgres.kdl"),
            r#"
service "postgres" {
    image "postgres:16"
}
"#,
        )?;

        // stages/local.kdl
        fs::create_dir_all(base.join("stages"))?;
        fs::write(
            base.join("stages/local.kdl"),
            r#"
stage "local" {
    service "api"
    service "postgres"
}
"#,
        )?;

        Ok(())
    }

    #[test]
    fn test_load_project_basic() -> Result<()> {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path();

        create_test_project(project_root)?;

        let config = load_project_from_root(project_root)?;

        // サービス
        assert_eq!(config.services.len(), 2);
        assert!(config.services.contains_key("api"));
        assert!(config.services.contains_key("postgres"));

        // テンプレート展開の確認
        let api = &config.services["api"];
        assert_eq!(api.image.as_ref().unwrap(), "ghcr.io/myorg/api:1.0.0");

        // ステージ
        assert_eq!(config.stages.len(), 1);
        assert!(config.stages.contains_key("local"));

        let local = &config.stages["local"];
        assert_eq!(local.services.len(), 2);
        assert!(local.services.contains(&"api".to_string()));
        assert!(local.services.contains(&"postgres".to_string()));

        Ok(())
    }

    #[test]
    fn test_load_project_with_variables_dir() -> Result<()> {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path();

        // .fleetflow/fleet.kdl
        fs::create_dir_all(project_root.join(".fleetflow"))?;
        fs::write(project_root.join(".fleetflow/fleet.kdl"), "")?;

        // variables/common.kdl
        fs::create_dir_all(project_root.join("variables"))?;
        fs::write(
            project_root.join("variables/common.kdl"),
            r#"
variables {
    image_registry "myregistry"
    version "2.0.0"
}
"#,
        )?;

        // services/api.kdl
        fs::create_dir_all(project_root.join("services"))?;
        fs::write(
            project_root.join("services/api.kdl"),
            r#"
service "api" {
    image "{{ image_registry }}/api:{{ version }}"
}
"#,
        )?;

        let config = load_project_from_root(project_root)?;

        let api = &config.services["api"];
        assert_eq!(api.image.as_ref().unwrap(), "myregistry/api:2.0.0");

        Ok(())
    }

    #[test]
    fn test_load_project_with_local_override() -> Result<()> {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path();

        // .fleetflow/fleet.kdl
        fs::create_dir_all(project_root.join(".fleetflow"))?;
        fs::write(project_root.join(".fleetflow/fleet.kdl"), "")?;

        // services/api.kdl
        fs::create_dir_all(project_root.join("services"))?;
        fs::write(
            project_root.join("services/api.kdl"),
            r#"
service "api" {
    image "myapp:15"
    version "15"
}
"#,
        )?;

        // flow.local.kdl（オーバーライド）
        fs::write(
            project_root.join("flow.local.kdl"),
            r#"
service "api" {
    version "16"
}
"#,
        )?;

        let config = load_project_from_root(project_root)?;

        // flow.local.kdl の定義が優先される
        let api = &config.services["api"];
        assert_eq!(api.version.as_ref().unwrap(), "16");

        Ok(())
    }

    #[test]
    fn test_load_project_with_stage_override() -> Result<()> {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path();

        // .fleetflow/fleet.kdl（ベース）
        fs::create_dir_all(project_root.join(".fleetflow"))?;
        fs::write(
            project_root.join(".fleetflow/fleet.kdl"),
            r#"
service "api" {
    image "myapp"
    version "1.0.0"
}
"#,
        )?;

        // flow.prod.kdl（ステージオーバーライド）
        fs::write(
            project_root.join("flow.prod.kdl"),
            r#"
service "api" {
    image "myapp"
    version "2.0.0"
}
"#,
        )?;

        // ステージ指定なし
        let config_no_stage = load_project_from_root(project_root)?;
        let api = &config_no_stage.services["api"];
        assert_eq!(api.version.as_ref().unwrap(), "1.0.0");

        // ステージ指定あり（prod）
        let config_prod = load_project_from_root_with_stage(project_root, Some("prod"))?;
        let api = &config_prod.services["api"];
        assert_eq!(api.version.as_ref().unwrap(), "2.0.0");
        assert_eq!(api.image.as_ref().unwrap(), "myapp"); // 上書きされた値

        Ok(())
    }

    #[test]
    fn test_load_project_with_stage_and_local_override() -> Result<()> {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path();

        // .fleetflow/fleet.kdl（ベース）
        fs::create_dir_all(project_root.join(".fleetflow"))?;
        fs::write(
            project_root.join(".fleetflow/fleet.kdl"),
            r#"
service "api" {
    image "myapp"
    version "1.0.0"
}
"#,
        )?;

        // flow.prod.kdl（ステージオーバーライド）
        fs::write(
            project_root.join("flow.prod.kdl"),
            r#"
service "api" {
    image "myapp"
    version "2.0.0"
}
"#,
        )?;

        // flow.local.kdl（ローカルオーバーライド）
        // これはステージオーバーライドより優先される
        fs::write(
            project_root.join("flow.local.kdl"),
            r#"
service "api" {
    image "myapp"
    version "local-dev"
}
"#,
        )?;

        let config = load_project_from_root_with_stage(project_root, Some("prod"))?;
        let api = &config.services["api"];

        // flow.local.kdl が最後に読み込まれるので "local-dev" になる
        assert_eq!(api.version.as_ref().unwrap(), "local-dev");
        assert_eq!(api.image.as_ref().unwrap(), "myapp");

        Ok(())
    }

    #[test]
    fn test_load_project_with_env_file() -> Result<()> {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path();

        // .fleetflow ディレクトリを作成
        fs::create_dir_all(project_root.join(".fleetflow"))?;

        // .fleetflow/.env
        fs::write(
            project_root.join(".fleetflow/.env"),
            r#"
REGISTRY=ghcr.io/myorg
IMAGE_TAG=v1.2.3
"#,
        )?;

        // .fleetflow/fleet.kdl
        fs::write(
            project_root.join(".fleetflow/fleet.kdl"),
            r#"
service "api" {
    image "{{ REGISTRY }}/api:{{ IMAGE_TAG }}"
}
"#,
        )?;

        let config = load_project_from_root(project_root)?;
        let api = &config.services["api"];
        assert_eq!(api.image.as_ref().unwrap(), "ghcr.io/myorg/api:v1.2.3");

        Ok(())
    }

    /// Issue #28: ステージ固有の変数がvolume定義で正しく解決されること
    #[test]
    fn test_stage_specific_variables_in_volumes() -> Result<()> {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_root = temp_dir.path();

        // .fleetflow ディレクトリを作成
        fs::create_dir_all(project_root.join(".fleetflow"))?;

        // .fleetflow/fleet.kdl
        fs::write(
            project_root.join(".fleetflow/fleet.kdl"),
            r#"
project "test-issue-28"

stage "local" {
    service "db"
    variables {
        DATA_DIR "./data/local"
    }
}

stage "dev" {
    service "db"
    variables {
        DATA_DIR "/opt/data/dev"
    }
}

service "db" {
    image "postgres:16"
    volumes {
        volume host="{{ DATA_DIR }}/postgres" container="/var/lib/postgresql/data"
    }
}
"#,
        )?;

        // localステージでロード
        let local_config = load_project_from_root_with_stage(project_root, Some("local"))?;
        let local_db = &local_config.services["db"];
        let local_volume = &local_db.volumes[0];
        assert_eq!(
            local_volume.host.to_string_lossy(),
            "./data/local/postgres",
            "localステージのDATA_DIR変数が使用されるべき"
        );

        // devステージでロード
        let dev_config = load_project_from_root_with_stage(project_root, Some("dev"))?;
        let dev_db = &dev_config.services["db"];
        let dev_volume = &dev_db.volumes[0];
        assert_eq!(
            dev_volume.host.to_string_lossy(),
            "/opt/data/dev/postgres",
            "devステージのDATA_DIR変数が使用されるべき"
        );

        Ok(())
    }
}
