//! FleetFlow MCP Server
//!
//! 公式 rmcp SDK を使用した MCP サーバー実装。
//! stdio トランスポートで動作し、FleetFlow の各種操作をツールとして提供する。

use anyhow::Result;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
    handler::server::{tool::ToolCallContext, tool::ToolRouter, wrapper::Parameters},
    model::*,
    service::RequestContext,
    tool, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::HashMap;
use tracing::error;

mod cp;

// ============================================================================
// パラメータ定義
// ============================================================================

/// ステージ名パラメータ
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct StageParam {
    /// ステージ名（例: local, dev, prod）
    pub stage: String,
}

/// ステージ停止パラメータ
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DownParam {
    /// ステージ名
    pub stage: String,
    /// コンテナとネットワークを完全に削除する場合は true
    #[serde(default)]
    pub remove: bool,
}

/// ログ取得パラメータ
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LogsParam {
    /// ステージ名
    pub stage: String,
    /// サービス名（オプション、未指定時は最初のサービス）
    pub service: Option<String>,
    /// 取得する行数（デフォルト: 50）
    pub tail: Option<u64>,
}

/// サービス再起動パラメータ
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RestartParam {
    /// ステージ名
    pub stage: String,
    /// 再起動するサービス名
    pub service: String,
}

/// ビルドパラメータ
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct BuildParam {
    /// ステージ名
    pub stage: String,
    /// ビルド対象のサービス名（オプション）
    pub service: Option<String>,
    /// キャッシュを使用せずにビルドする場合は true
    #[serde(default)]
    pub no_cache: bool,
}

// ============================================================================
// CP パラメータ定義（v2）
// ============================================================================

/// プロジェクト slug パラメータ
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ProjectSlugParam {
    /// プロジェクトの slug
    pub slug: String,
}

/// コンテナ操作パラメータ（CP 経由）
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CpContainerParam {
    /// プロジェクト slug
    pub project: String,
    /// ステージ名
    pub stage: String,
    /// サービス名
    pub service: String,
}

/// CP ログ取得パラメータ
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CpLogsParam {
    /// プロジェクト slug
    pub project: String,
    /// ステージ名
    pub stage: String,
    /// サービス名
    pub service: String,
    /// 取得する行数（デフォルト: 50）
    pub tail: Option<u64>,
}

/// ステージ指定パラメータ（project + stage）
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct StagePathParam {
    /// プロジェクト slug
    pub project: String,
    /// ステージ名
    pub stage: String,
}

/// fleet_restart パラメータ（FSC-18）
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FleetRestartParam {
    /// プロジェクト slug
    pub project: String,
    /// ステージ名
    pub stage: String,
    /// 再起動するサービス名
    pub service: String,
}

/// コンテナログ取得パラメータ
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ContainerLogParam {
    /// プロジェクト slug
    pub project: String,
    /// ステージ名
    pub stage: String,
    /// コンテナ名
    pub container: String,
}

// ============================================================================
// MCP サーバー
// ============================================================================

/// FleetFlow MCP サーバー
#[derive(Clone)]
pub struct FleetFlowServer {
    tool_router: ToolRouter<Self>,
}

impl Default for FleetFlowServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router]
impl FleetFlowServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    /// プロジェクト情報を取得
    #[tool(
        description = "カレントディレクトリにある FleetFlow プロジェクト（fleet.kdl 等）を解析し、定義されているサービス名、イメージ名、ステージ名、環境変数などの情報を取得します。"
    )]
    async fn fleetflow_inspect_project(&self) -> Result<String, String> {
        let project_root = fleetflow_core::find_project_root()
            .map_err(|e| format!("プロジェクトルートが見つかりません: {}", e))?;
        let config = fleetflow_core::load_project_from_root(&project_root)
            .map_err(|e| format!("設定の読み込みに失敗: {}", e))?;

        let mut info = format!("Project: {}\n\n", config.name);

        info.push_str("Stages:\n");
        for (stage_name, stage) in &config.stages {
            info.push_str(&format!(
                "  - {} ({} services)\n",
                stage_name,
                stage.services.len()
            ));
        }

        info.push_str("\nServices:\n");
        for (service_name, service) in &config.services {
            let image = service.image.as_deref().unwrap_or("(no image)");
            info.push_str(&format!("  - {}: {}\n", service_name, image));
        }

        Ok(info)
    }

    /// コンテナ一覧を表示
    #[tool(
        description = "コンテナの一覧を表示します。プロジェクトに関連するコンテナの稼働状況を確認できます。"
    )]
    async fn fleetflow_ps(&self) -> Result<String, String> {
        let docker = bollard::Docker::connect_with_local_defaults()
            .map_err(|e| format!("Docker接続エラー: {}", e))?;
        let project_root = fleetflow_core::find_project_root()
            .map_err(|e| format!("プロジェクトルートが見つかりません: {}", e))?;
        let config = fleetflow_core::load_project_from_root(&project_root)
            .map_err(|e| format!("設定の読み込みに失敗: {}", e))?;

        let mut filter = HashMap::new();
        filter.insert(
            "label".to_string(),
            vec![format!("fleetflow.project={}", config.name)],
        );

        let options = bollard::query_parameters::ListContainersOptions {
            all: true,
            filters: Some(filter),
            ..Default::default()
        };

        let containers = docker
            .list_containers(Some(options))
            .await
            .map_err(|e| format!("コンテナ一覧取得エラー: {}", e))?;

        let mut status = format!("Status for project: {}\n\n", config.name);
        if containers.is_empty() {
            status.push_str("No containers found.");
        } else {
            for c in containers {
                let name = c
                    .names
                    .and_then(|n| n.first().cloned())
                    .unwrap_or_else(|| "unnamed".to_string());
                let status_text = c.status.unwrap_or_else(|| "unknown".to_string());
                let image = c.image.unwrap_or_else(|| "unknown".to_string());
                status.push_str(&format!(
                    "- {}: {} ({})\n",
                    name.trim_start_matches('/'),
                    status_text,
                    image
                ));
            }
        }

        Ok(status)
    }

    /// ステージを起動
    #[tool(
        description = "指定されたステージのコンテナを起動します。ネットワークの作成や、既に存在するコンテナの再起動も行います。"
    )]
    async fn fleetflow_up(&self, params: Parameters<StageParam>) -> Result<String, String> {
        let stage = &params.0.stage;

        let project_root = fleetflow_core::find_project_root()
            .map_err(|e| format!("プロジェクトルートが見つかりません: {}", e))?;
        let config = fleetflow_core::load_project_from_root(&project_root)
            .map_err(|e| format!("設定の読み込みに失敗: {}", e))?;

        let runtime = fleetflow_container::Runtime::new(project_root)
            .map_err(|e| format!("Runtimeの初期化に失敗: {}", e))?;

        match runtime.up(&config, stage, false).await {
            Ok(_) => Ok(format!(
                "ステージ '{}' の全サービスを正常に起動しました。",
                stage
            )),
            Err(e) => Err(format!("ステージ '{}' の起動に失敗しました: {}", stage, e)),
        }
    }

    /// ステージを停止
    #[tool(
        description = "指定されたステージのコンテナを停止します。remove=true でコンテナとネットワークを完全に削除します。"
    )]
    async fn fleetflow_down(&self, params: Parameters<DownParam>) -> Result<String, String> {
        let stage = &params.0.stage;
        let remove = params.0.remove;

        let project_root = fleetflow_core::find_project_root()
            .map_err(|e| format!("プロジェクトルートが見つかりません: {}", e))?;
        let config = fleetflow_core::load_project_from_root(&project_root)
            .map_err(|e| format!("設定の読み込みに失敗: {}", e))?;

        let runtime = fleetflow_container::Runtime::new(project_root)
            .map_err(|e| format!("Runtimeの初期化に失敗: {}", e))?;

        match runtime.down(&config, stage, remove).await {
            Ok(_) => Ok(format!(
                "ステージ '{}' の停止が完了しました。{}",
                stage,
                if remove {
                    "コンテナとネットワークも削除されました。"
                } else {
                    "コンテナは停止した状態で残っています。"
                }
            )),
            Err(e) => Err(format!(
                "ステージ '{}' の停止処理中にエラーが発生しました: {}",
                stage, e
            )),
        }
    }

    /// ログを取得
    #[tool(
        description = "指定されたステージのコンテナログを取得します。特定のサービスを指定することも可能です。"
    )]
    async fn fleetflow_logs(&self, params: Parameters<LogsParam>) -> Result<String, String> {
        use futures_util::StreamExt;

        let stage = &params.0.stage;
        let service = params.0.service.as_deref();
        let tail = params.0.tail.unwrap_or(50) as usize;

        let project_root = fleetflow_core::find_project_root()
            .map_err(|e| format!("プロジェクトルートが見つかりません: {}", e))?;
        let config = fleetflow_core::load_project_from_root(&project_root)
            .map_err(|e| format!("設定の読み込みに失敗: {}", e))?;
        let docker = bollard::Docker::connect_with_local_defaults()
            .map_err(|e| format!("Docker接続エラー: {}", e))?;

        let container_name = if let Some(svc) = service {
            format!("{}-{}-{}", config.name, stage, svc)
        } else {
            let stage_config = config
                .stages
                .get(stage)
                .ok_or_else(|| format!("Stage '{}' not found", stage))?;
            let first_service = stage_config
                .services
                .first()
                .ok_or_else(|| format!("No services in stage '{}'", stage))?;
            format!("{}-{}-{}", config.name, stage, first_service)
        };

        let options = bollard::query_parameters::LogsOptions {
            stdout: true,
            stderr: true,
            tail: tail.to_string(),
            ..Default::default()
        };

        let mut logs_stream = docker.logs(&container_name, Some(options));
        let mut logs = String::new();

        while let Some(log_result) = logs_stream.next().await {
            match log_result {
                Ok(log) => logs.push_str(&log.to_string()),
                Err(e) => {
                    return Err(format!("ログ取得エラー: {}", e));
                }
            }
        }

        Ok(format!("Logs for {}:\n\n{}", container_name, logs))
    }

    /// サービスを再起動
    #[tool(description = "指定されたサービスのコンテナを再起動します。")]
    async fn fleetflow_restart(&self, params: Parameters<RestartParam>) -> Result<String, String> {
        let stage = &params.0.stage;
        let service = &params.0.service;

        let project_root = fleetflow_core::find_project_root()
            .map_err(|e| format!("プロジェクトルートが見つかりません: {}", e))?;
        let config = fleetflow_core::load_project_from_root(&project_root)
            .map_err(|e| format!("設定の読み込みに失敗: {}", e))?;
        let docker = bollard::Docker::connect_with_local_defaults()
            .map_err(|e| format!("Docker接続エラー: {}", e))?;

        let container_name = format!("{}-{}-{}", config.name, stage, service);

        docker
            .restart_container(
                &container_name,
                None::<bollard::query_parameters::RestartContainerOptions>,
            )
            .await
            .map_err(|e| format!("再起動に失敗しました: {}", e))?;

        Ok(format!(
            "サービス '{}' を再起動しました (コンテナ: {})",
            service, container_name
        ))
    }

    /// 設定を検証
    #[tool(description = "FleetFlow設定ファイル（fleet.kdl等）の構文と整合性を検証します。")]
    async fn fleetflow_validate(&self) -> Result<String, String> {
        let project_root = fleetflow_core::find_project_root()
            .map_err(|e| format!("プロジェクトルートが見つかりません: {}", e))?;

        match fleetflow_core::load_project_from_root(&project_root) {
            Ok(config) => {
                let mut result = "✓ 設定は有効です\n\n".to_string();
                result.push_str(&format!("プロジェクト: {}\n", config.name));
                result.push_str(&format!("ステージ数: {}\n", config.stages.len()));
                result.push_str(&format!("サービス数: {}\n", config.services.len()));

                if !config.stages.is_empty() {
                    result.push_str("\nステージ:\n");
                    for (name, stage) in &config.stages {
                        result.push_str(&format!(
                            "  - {} ({} services)\n",
                            name,
                            stage.services.len()
                        ));
                    }
                }

                Ok(result)
            }
            Err(e) => Err(format!("✗ 設定エラー: {}", e)),
        }
    }

    /// イメージをビルド
    #[tool(description = "指定されたサービスのDockerイメージをビルドします。")]
    async fn fleetflow_build(&self, params: Parameters<BuildParam>) -> Result<String, String> {
        let stage = &params.0.stage;
        let service_filter = params.0.service.as_deref();
        let no_cache = params.0.no_cache;

        let project_root = fleetflow_core::find_project_root()
            .map_err(|e| format!("プロジェクトルートが見つかりません: {}", e))?;
        let config = fleetflow_core::load_project_from_root(&project_root)
            .map_err(|e| format!("設定の読み込みに失敗: {}", e))?;

        let stage_config = config.stages.get(stage).ok_or_else(|| {
            format!(
                "ステージ '{}' が見つかりません。利用可能: {}",
                stage,
                config.stages.keys().cloned().collect::<Vec<_>>().join(", ")
            )
        })?;

        let docker = bollard::Docker::connect_with_local_defaults()
            .map_err(|e| format!("Docker接続エラー: {}", e))?;
        let resolver = fleetflow_build::BuildResolver::new(project_root.clone());
        let builder = fleetflow_build::ImageBuilder::new(docker);

        let mut built_services = Vec::new();
        let mut skipped_services = Vec::new();
        let mut errors = Vec::new();

        let services_to_build: Vec<String> = if let Some(svc) = service_filter {
            if stage_config.services.contains(&svc.to_string()) {
                vec![svc.to_string()]
            } else {
                return Err(format!(
                    "サービス '{}' はステージ '{}' に含まれていません",
                    svc, stage
                ));
            }
        } else {
            stage_config.services.clone()
        };

        for service_name in &services_to_build {
            let svc = match config.services.get(service_name) {
                Some(s) => s,
                None => {
                    skipped_services.push(format!("{} (定義なし)", service_name));
                    continue;
                }
            };

            let dockerfile = match resolver.resolve_dockerfile(service_name, svc) {
                Ok(Some(path)) => path,
                Ok(None) => {
                    skipped_services.push(format!("{} (Dockerfileなし)", service_name));
                    continue;
                }
                Err(e) => {
                    errors.push(format!("{}: {}", service_name, e));
                    continue;
                }
            };

            let context_path = match resolver.resolve_context(svc) {
                Ok(path) => path,
                Err(e) => {
                    errors.push(format!("{}: {}", service_name, e));
                    continue;
                }
            };

            let image_tag = resolver.resolve_image_tag(service_name, svc, &config.name, stage);
            let build_args = resolver.resolve_build_args(svc, &HashMap::new());

            let context_data =
                match fleetflow_build::ContextBuilder::create_context(&context_path, &dockerfile) {
                    Ok(data) => data,
                    Err(e) => {
                        errors.push(format!("{}: コンテキスト作成失敗 - {}", service_name, e));
                        continue;
                    }
                };

            match builder
                .build_image(context_data, &image_tag, build_args, None, no_cache)
                .await
            {
                Ok(_) => {
                    built_services.push(format!("{} → {}", service_name, image_tag));
                }
                Err(e) => {
                    errors.push(format!("{}: ビルド失敗 - {}", service_name, e));
                }
            }
        }

        let mut result = String::new();

        if !built_services.is_empty() {
            result.push_str("✓ ビルド成功:\n");
            for s in &built_services {
                result.push_str(&format!("  - {}\n", s));
            }
        }

        if !skipped_services.is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str("⊘ スキップ:\n");
            for s in &skipped_services {
                result.push_str(&format!("  - {}\n", s));
            }
        }

        if !errors.is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str("✗ エラー:\n");
            for e in &errors {
                result.push_str(&format!("  - {}\n", e));
            }
        }

        if result.is_empty() {
            result = "ビルド対象のサービスがありません".to_string();
        }

        if errors.is_empty() {
            Ok(result)
        } else {
            Err(result)
        }
    }

    // ========================================================================
    // CP 経由ツール（v2）
    // ========================================================================

    /// CP 接続状態を確認
    #[tool(
        description = "Control Plane への接続状態を確認します。ログイン済みか、トークンの有効期限、テナント情報を表示します。"
    )]
    async fn fleetflow_cp_status(&self) -> Result<String, String> {
        let creds_path = dirs::config_dir()
            .ok_or("設定ディレクトリが見つかりません")?
            .join("fleetflow/credentials.json");

        if !creds_path.exists() {
            return Ok("未ログイン。`fleet login` でログインしてください。".to_string());
        }

        let content = std::fs::read_to_string(&creds_path)
            .map_err(|e| format!("credentials 読み込みエラー: {}", e))?;
        let creds: cp::Credentials = serde_json::from_str(&content)
            .map_err(|e| format!("credentials パースエラー: {}", e))?;

        let expired = chrono::DateTime::parse_from_rfc3339(&creds.expires_at)
            .map(|exp| exp < chrono::Utc::now())
            .unwrap_or(false);

        let mut info = String::new();
        info.push_str("CP 接続情報:\n\n");
        info.push_str(&format!(
            "  Email:    {}\n",
            creds.email.as_deref().unwrap_or("N/A")
        ));
        info.push_str(&format!(
            "  Tenant:   {}\n",
            creds.tenant_slug.as_deref().unwrap_or("default")
        ));
        info.push_str(&format!("  Endpoint: {}\n", creds.api_endpoint));
        info.push_str(&format!(
            "  Status:   {}\n",
            if expired { "期限切れ" } else { "有効" }
        ));

        Ok(info)
    }

    /// CP 経由でプロジェクト一覧を取得
    #[tool(
        description = "Control Plane に登録されている全プロジェクトの一覧を取得します。CP にログイン済みである必要があります。"
    )]
    async fn fleetflow_cp_projects(&self) -> Result<String, String> {
        let (client, _creds) = cp::connect().await.map_err(|e| e.to_string())?;

        let resp = cp::request(&client, "project", "list", serde_json::json!({}))
            .await
            .map_err(|e| e.to_string())?;

        client.disconnect().await.ok();

        let mut result = "プロジェクト一覧:\n\n".to_string();

        if let Some(projects) = resp["projects"].as_array() {
            if projects.is_empty() {
                result.push_str("  (プロジェクトなし)");
            } else {
                result.push_str(&format!("  {:<20} {:<25} {}\n", "SLUG", "NAME", "CREATED"));
                result.push_str(&format!("  {}\n", "─".repeat(65)));
                for p in projects {
                    result.push_str(&format!(
                        "  {:<20} {:<25} {}\n",
                        p["slug"].as_str().unwrap_or("N/A"),
                        p["name"].as_str().unwrap_or("N/A"),
                        p["created_at"].as_str().unwrap_or("N/A"),
                    ));
                }
            }
        }

        Ok(result)
    }

    /// CP 経由でサーバー一覧を取得
    #[tool(
        description = "Control Plane に登録されている全サーバーの一覧を取得します。各サーバーのプロバイダ、IP、稼働状態を表示します。"
    )]
    async fn fleetflow_cp_servers(&self) -> Result<String, String> {
        let (client, _creds) = cp::connect().await.map_err(|e| e.to_string())?;

        let resp = cp::request(&client, "server", "list", serde_json::json!({}))
            .await
            .map_err(|e| e.to_string())?;

        client.disconnect().await.ok();

        let mut result = "サーバー一覧:\n\n".to_string();

        if let Some(servers) = resp["servers"].as_array() {
            if servers.is_empty() {
                result.push_str("  (サーバーなし)");
            } else {
                result.push_str(&format!(
                    "  {:<15} {:<15} {:<15} {:<10}\n",
                    "SLUG", "PROVIDER", "SSH HOST", "STATUS"
                ));
                result.push_str(&format!("  {}\n", "─".repeat(55)));
                for s in servers {
                    result.push_str(&format!(
                        "  {:<15} {:<15} {:<15} {:<10}\n",
                        s["slug"].as_str().unwrap_or("N/A"),
                        s["provider"].as_str().unwrap_or("N/A"),
                        s["ssh_host"].as_str().unwrap_or("N/A"),
                        s["status"].as_str().unwrap_or("unknown"),
                    ));
                }
            }
        }

        Ok(result)
    }

    /// CP 経由で全プロジェクト横断のステージ状態を取得
    #[tool(
        description = "全プロジェクトのステージ横断状態を取得します。各プロジェクト × ステージのサービス稼働数を表示します。"
    )]
    async fn fleetflow_cp_overview(&self) -> Result<String, String> {
        let (client, creds) = cp::connect().await.map_err(|e| e.to_string())?;

        // B#6 fix: tenant_slug を渡さないと handler 側の `WHERE project.tenant.slug = $tenant_slug`
        // が空 string で match し、 tenant isolation が効かない (テナント越境リスク)
        let tenant_slug = creds.tenant_slug.as_deref().unwrap_or("default");
        let resp = cp::request(
            &client,
            "stage",
            "list_across_projects",
            serde_json::json!({ "tenant_slug": tenant_slug }),
        )
        .await
        .map_err(|e| e.to_string())?;

        client.disconnect().await.ok();

        let mut result = "プロジェクト横断ステータス:\n\n".to_string();

        if let Some(stages) = resp["stages"].as_array() {
            if stages.is_empty() {
                result.push_str("  (ステージなし)");
            } else {
                result.push_str(&format!(
                    "  {:<20} {:<10} {:<10} {}\n",
                    "PROJECT", "STAGE", "SERVICES", "STATUS"
                ));
                result.push_str(&format!("  {}\n", "─".repeat(55)));
                for s in stages {
                    let svc_count = s["services"].as_array().map(|arr| arr.len()).unwrap_or(0);
                    let running = s["services"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter(|svc| svc["status"].as_str() == Some("running"))
                                .count()
                        })
                        .unwrap_or(0);
                    let status = if running == svc_count && svc_count > 0 {
                        format!("{}/{} running", running, svc_count)
                    } else if running > 0 {
                        format!("{}/{} partial", running, svc_count)
                    } else {
                        "stopped".to_string()
                    };
                    result.push_str(&format!(
                        "  {:<20} {:<10} {:<10} {}\n",
                        s["project_name"].as_str().unwrap_or("N/A"),
                        s["name"].as_str().unwrap_or("N/A"),
                        svc_count,
                        status,
                    ));
                }
            }
        }

        Ok(result)
    }

    // ============================================================================
    // v3: Dashboard HTTP API 経由のツール（v0.13.0 追加分）
    // ============================================================================

    /// ステージ概要（1st ビュー）— アラート・デプロイ・サーバー状態付き
    #[tool(
        description = "テナントの全ステージ概要を取得します。アラート数、直近デプロイ結果、サーバー状態を含む優先度ソート済みの一覧です。"
    )]
    async fn fleetflow_cp_stages(&self) -> Result<String, String> {
        let resp = cp::http_get("/api/stages")
            .await
            .map_err(|e| e.to_string())?;
        let stages = resp["stages"].as_array();
        let mut result = "ステージ一覧（優先度順）:\n\n".to_string();
        if let Some(stages) = stages {
            if stages.is_empty() {
                result.push_str("  (ステージなし)");
            } else {
                result.push_str(&format!(
                    "  {:<20} {:<10} {:<12} {:<10} {:<10} {}\n",
                    "PROJECT", "STAGE", "SERVER", "STATUS", "DEPLOY", "ALERTS"
                ));
                result.push_str(&format!("  {}\n", "─".repeat(75)));
                for s in stages {
                    let alerts = s["alert_count"].as_i64().unwrap_or(0);
                    let alert_str = if alerts > 0 {
                        format!("⚠ {}", alerts)
                    } else {
                        "-".into()
                    };
                    result.push_str(&format!(
                        "  {:<20} {:<10} {:<12} {:<10} {:<10} {}\n",
                        s["project_name"].as_str().unwrap_or("N/A"),
                        s["stage"].as_str().unwrap_or("N/A"),
                        s["server_slug"].as_str().unwrap_or("-"),
                        s["server_status"].as_str().unwrap_or("-"),
                        s["last_deploy_status"].as_str().unwrap_or("-"),
                        alert_str,
                    ));
                }
            }
        }
        Ok(result)
    }

    /// ステージのサービス一覧
    #[tool(
        description = "指定ステージのサービス一覧を取得します。サービス名、Docker イメージ、稼働状態を表示します。"
    )]
    async fn fleetflow_cp_stage_services(
        &self,
        params: Parameters<StagePathParam>,
    ) -> Result<String, String> {
        let p = &params.0;
        let path = format!("/api/stages/{}/{}/services", p.project, p.stage);
        let resp = cp::http_get(&path).await.map_err(|e| e.to_string())?;
        let services = resp["services"].as_array();
        let mut result = format!("{}/{} サービス一覧:\n\n", p.project, p.stage);
        if let Some(svcs) = services {
            for s in svcs {
                result.push_str(&format!(
                    "  {} — {} ({})\n",
                    s["slug"].as_str().unwrap_or("?"),
                    s["image"].as_str().unwrap_or("?"),
                    s["desired_status"].as_str().unwrap_or("?")
                ));
            }
        }
        Ok(result)
    }

    /// ステージのデプロイ履歴
    #[tool(
        description = "指定ステージの直近デプロイ履歴を取得します。コマンド、ステータス、実行時刻を表示します。"
    )]
    async fn fleetflow_cp_stage_deployments(
        &self,
        params: Parameters<StagePathParam>,
    ) -> Result<String, String> {
        let p = &params.0;
        let path = format!("/api/stages/{}/{}/deployments", p.project, p.stage);
        let resp = cp::http_get(&path).await.map_err(|e| e.to_string())?;
        let deploys = resp["deployments"].as_array();
        let mut result = format!("{}/{} デプロイ履歴:\n\n", p.project, p.stage);
        if let Some(deps) = deploys {
            for d in deps {
                result.push_str(&format!(
                    "  [{}] {} — {} ({})\n",
                    d["status"].as_str().unwrap_or("?"),
                    d["command"].as_str().unwrap_or("?"),
                    d["server_slug"].as_str().unwrap_or("?"),
                    d["started_at"].as_str().unwrap_or("?"),
                ));
            }
        }
        Ok(result)
    }

    /// Agent 経由で再デプロイ
    #[tool(
        description = "指定ステージを Fleet Agent 経由で再デプロイします。CP → Agent → docker compose up の流れで実行されます。"
    )]
    async fn fleetflow_cp_redeploy(
        &self,
        params: Parameters<StagePathParam>,
    ) -> Result<String, String> {
        let p = &params.0;
        let path = format!("/api/stages/{}/{}/redeploy", p.project, p.stage);
        let resp = cp::http_post(&path).await.map_err(|e| e.to_string())?;
        Ok(serde_json::to_string_pretty(&resp).unwrap_or_else(|_| "OK".into()))
    }

    /// Agent 経由でサービス再起動
    #[tool(
        description = "指定サービスを Fleet Agent 経由で再起動します。CP → Agent → docker restart の流れで実行されます。"
    )]
    async fn fleetflow_cp_service_restart(
        &self,
        params: Parameters<CpContainerParam>,
    ) -> Result<String, String> {
        let p = &params.0;
        let path = format!(
            "/api/stages/{}/{}/restart/{}",
            p.project, p.stage, p.service
        );
        let resp = cp::http_post(&path).await.map_err(|e| e.to_string())?;
        Ok(serde_json::to_string_pretty(&resp).unwrap_or_else(|_| "OK".into()))
    }

    /// コンテナログ取得（LogRouter 経由）
    #[tool(
        description = "指定コンテナの直近ログを取得します。LogRouter のキャッシュから info 以上のログを返します。"
    )]
    async fn fleetflow_cp_container_logs_v2(
        &self,
        params: Parameters<ContainerLogParam>,
    ) -> Result<String, String> {
        let p = &params.0;
        let path = format!("/api/stages/{}/{}/logs/{}", p.project, p.stage, p.container);
        let resp = cp::http_get(&path).await.map_err(|e| e.to_string())?;
        let logs = resp["logs"].as_array();
        let mut result = format!("{} ログ:\n\n", p.container);
        if let Some(entries) = logs {
            if entries.is_empty() {
                result.push_str("  (ログなし — Agent 未接続 or コンテナ未稼働)");
            } else {
                for l in entries {
                    result.push_str(&format!(
                        "[{}] [{}] {}\n",
                        l["timestamp"].as_str().unwrap_or("?"),
                        l["level"].as_str().unwrap_or("?"),
                        l["message"].as_str().unwrap_or(""),
                    ));
                }
            }
        }
        Ok(result)
    }

    /// ステージのアクティブアラート一覧
    #[tool(
        description = "指定ステージのアクティブなアラート一覧を取得します。コンテナ名、タイプ、重要度、メッセージを表示します。"
    )]
    async fn fleetflow_cp_alerts(
        &self,
        params: Parameters<StagePathParam>,
    ) -> Result<String, String> {
        let p = &params.0;
        let path = format!("/api/stages/{}/{}/alerts", p.project, p.stage);
        let resp = cp::http_get(&path).await.map_err(|e| e.to_string())?;
        let alerts = resp["alerts"].as_array();
        let mut result = format!("{}/{} アラート:\n\n", p.project, p.stage);
        if let Some(items) = alerts {
            if items.is_empty() {
                result.push_str("  (アラートなし)");
            } else {
                for a in items {
                    result.push_str(&format!(
                        "  [{}] {} — {} — {}\n",
                        a["severity"].as_str().unwrap_or("?"),
                        a["container_name"].as_str().unwrap_or("?"),
                        a["alert_type"].as_str().unwrap_or("?"),
                        a["message"].as_str().unwrap_or(""),
                    ));
                }
            }
        }
        Ok(result)
    }

    /// 接続中の Fleet Agent 一覧
    #[tool(
        description = "Control Plane に接続中の Fleet Agent 一覧を取得します。各サーバーの Agent バージョンを表示します。"
    )]
    async fn fleetflow_cp_agents(&self) -> Result<String, String> {
        let resp = cp::http_get("/api/agents")
            .await
            .map_err(|e| e.to_string())?;
        let agents = resp["agents"].as_array();
        let mut result = "接続中 Agent:\n\n".to_string();
        if let Some(items) = agents {
            if items.is_empty() {
                result.push_str("  (接続中の Agent なし)");
            } else {
                for a in items {
                    result.push_str(&format!(
                        "  {} — v{}\n",
                        a["server_slug"].as_str().unwrap_or("?"),
                        a["version"].as_str().unwrap_or("?"),
                    ));
                }
            }
        }
        Ok(result)
    }

    /// テナントユーザー一覧
    #[tool(
        description = "現在のテナントに所属するユーザー一覧を取得します。owner/admin のみアクセス可能です。"
    )]
    async fn fleetflow_cp_tenant_users(&self) -> Result<String, String> {
        let resp = cp::http_get("/api/tenant/users")
            .await
            .map_err(|e| e.to_string())?;
        let users = resp["users"].as_array();
        let mut result = "テナントユーザー:\n\n".to_string();
        if let Some(items) = users {
            for u in items {
                result.push_str(&format!(
                    "  {} — {}\n",
                    u["auth0_sub"].as_str().unwrap_or("?"),
                    u["role"].as_str().unwrap_or("?"),
                ));
            }
        }
        Ok(result)
    }

    // ============================================================
    // FSC-17/18/19: Stage Runtime Status + Service Restart v1
    // ============================================================

    /// ステージの実 runtime status を取得 (FSC-17)
    #[tool(
        description = "ステージに含まれる各サービスの実 runtime status（running/stopped/restarting）と uptime_seconds を取得します。Agent → docker ps の結果を整形して返します。"
    )]
    async fn fleet_status(&self, params: Parameters<StagePathParam>) -> Result<String, String> {
        let p = &params.0;
        let path = format!("/api/v1/stages/{}/{}/status", p.project, p.stage);
        let resp = cp::http_get(&path).await.map_err(|e| e.to_string())?;
        Ok(serde_json::to_string_pretty(&resp).unwrap_or_else(|_| "OK".into()))
    }

    /// ステージのサービスを再起動 (FSC-18)
    #[tool(
        description = "指定ステージの指定サービスを Agent 経由で再起動します（docker restart）。owner/admin 権限が必要です。"
    )]
    async fn fleet_restart(&self, params: Parameters<FleetRestartParam>) -> Result<String, String> {
        let p = &params.0;
        let path = format!(
            "/api/v1/stages/{}/{}/services/{}/restart",
            p.project, p.stage, p.service,
        );
        let resp = cp::http_post(&path).await.map_err(|e| e.to_string())?;
        Ok(serde_json::to_string_pretty(&resp).unwrap_or_else(|_| "OK".into()))
    }

    /// CP 経由でプロジェクトの詳細を取得
    #[tool(
        description = "指定プロジェクトの詳細情報を取得します。プロジェクト名、説明、作成日時を表示します。"
    )]
    async fn fleetflow_cp_project_detail(
        &self,
        params: Parameters<ProjectSlugParam>,
    ) -> Result<String, String> {
        let slug = &params.0.slug;
        let (client, _creds) = cp::connect().await.map_err(|e| e.to_string())?;

        let resp = cp::request(
            &client,
            "project",
            "get",
            serde_json::json!({ "slug": slug }),
        )
        .await
        .map_err(|e| e.to_string())?;

        client.disconnect().await.ok();

        if let Some(project) = resp.get("project") {
            let mut result = format!("プロジェクト: {}\n\n", slug);
            result.push_str(&format!(
                "  Name:        {}\n",
                project["name"].as_str().unwrap_or("N/A")
            ));
            result.push_str(&format!(
                "  Slug:        {}\n",
                project["slug"].as_str().unwrap_or("N/A")
            ));
            result.push_str(&format!(
                "  Description: {}\n",
                project["description"].as_str().unwrap_or("N/A")
            ));
            result.push_str(&format!(
                "  Created:     {}\n",
                project["created_at"].as_str().unwrap_or("N/A")
            ));
            Ok(result)
        } else {
            Err(format!("プロジェクト '{}' が見つかりません", slug))
        }
    }

    /// CP 経由でコンテナを起動
    #[tool(
        description = "Control Plane 経由で指定プロジェクト/ステージ/サービスのコンテナを起動します。"
    )]
    async fn fleetflow_cp_container_start(
        &self,
        params: Parameters<CpContainerParam>,
    ) -> Result<String, String> {
        let p = &params.0;
        let (client, _creds) = cp::connect().await.map_err(|e| e.to_string())?;

        let container_name = format!("{}-{}-{}", p.project, p.stage, p.service);
        let resp = cp::request(
            &client,
            "container",
            "start",
            serde_json::json!({ "container_name": container_name }),
        )
        .await
        .map_err(|e| e.to_string())?;

        client.disconnect().await.ok();

        if resp.get("error").is_some() {
            Err(format!("起動失敗: {}", resp))
        } else {
            Ok(format!("コンテナ '{}' を起動しました", container_name))
        }
    }

    /// CP 経由でコンテナを停止
    #[tool(
        description = "Control Plane 経由で指定プロジェクト/ステージ/サービスのコンテナを停止します。"
    )]
    async fn fleetflow_cp_container_stop(
        &self,
        params: Parameters<CpContainerParam>,
    ) -> Result<String, String> {
        let p = &params.0;
        let (client, _creds) = cp::connect().await.map_err(|e| e.to_string())?;

        let container_name = format!("{}-{}-{}", p.project, p.stage, p.service);
        let resp = cp::request(
            &client,
            "container",
            "stop",
            serde_json::json!({ "container_name": container_name }),
        )
        .await
        .map_err(|e| e.to_string())?;

        client.disconnect().await.ok();

        if resp.get("error").is_some() {
            Err(format!("停止失敗: {}", resp))
        } else {
            Ok(format!("コンテナ '{}' を停止しました", container_name))
        }
    }

    /// CP 経由でコンテナを再起動
    #[tool(
        description = "Control Plane 経由で指定プロジェクト/ステージ/サービスのコンテナを再起動します。"
    )]
    async fn fleetflow_cp_container_restart(
        &self,
        params: Parameters<CpContainerParam>,
    ) -> Result<String, String> {
        let p = &params.0;
        let (client, _creds) = cp::connect().await.map_err(|e| e.to_string())?;

        let container_name = format!("{}-{}-{}", p.project, p.stage, p.service);
        let resp = cp::request(
            &client,
            "container",
            "restart",
            serde_json::json!({ "container_name": container_name }),
        )
        .await
        .map_err(|e| e.to_string())?;

        client.disconnect().await.ok();

        if resp.get("error").is_some() {
            Err(format!("再起動失敗: {}", resp))
        } else {
            Ok(format!("コンテナ '{}' を再起動しました", container_name))
        }
    }

    /// CP 経由でコンテナログを取得
    #[tool(
        description = "Control Plane 経由で指定プロジェクト/ステージ/サービスのコンテナログを取得します。"
    )]
    async fn fleetflow_cp_container_logs(
        &self,
        params: Parameters<CpLogsParam>,
    ) -> Result<String, String> {
        let p = &params.0;
        let tail = p.tail.unwrap_or(50);
        let (client, _creds) = cp::connect().await.map_err(|e| e.to_string())?;

        let container_name = format!("{}-{}-{}", p.project, p.stage, p.service);
        let resp = cp::request(
            &client,
            "container",
            "logs",
            serde_json::json!({
                "container_name": container_name,
                "tail": tail,
            }),
        )
        .await
        .map_err(|e| e.to_string())?;

        client.disconnect().await.ok();

        if let Some(logs) = resp["logs"].as_str() {
            Ok(format!("Logs for {}:\n\n{}", container_name, logs))
        } else {
            Ok(format!("Logs for {}:\n\n(ログなし)", container_name))
        }
    }
}

impl ServerHandler for FleetFlowServer {
    fn get_info(&self) -> ServerInfo {
        // rmcp 1.1 で ServerInfo が #[non_exhaustive] 化 → struct literal 不可。
        // Default を base に、mutable で instructions のみ上書き
        let mut info = ServerInfo::default();
        info.instructions = Some(
            "FleetFlow MCP サーバー。KDLベースのコンテナオーケストレーションツールです。"
                .to_string(),
        );
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: self.tool_router.list_all(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let tool_context = ToolCallContext::new(self, request, context);
        self.tool_router.call(tool_context).await
    }
}

/// MCP サーバーを起動（stdio トランスポート）
pub async fn run_server() -> Result<()> {
    let server = FleetFlowServer::new();
    let transport = (tokio::io::stdin(), tokio::io::stdout());

    let service = server.serve(transport).await.map_err(|e| {
        error!("MCP server initialization failed: {}", e);
        anyhow::anyhow!("MCP server initialization failed: {}", e)
    })?;

    // サーバーが終了するまで待機
    service.waiting().await.map_err(|e| {
        error!("MCP server error: {}", e);
        anyhow::anyhow!("MCP server error: {}", e)
    })?;

    Ok(())
}

// ============================================================================
// テスト
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ------------------------------------------------------------------------
    // パラメータのデシリアライズ
    // ------------------------------------------------------------------------

    #[test]
    fn stage_param_deserialize() {
        let v = json!({"stage": "local"});
        let p: StageParam = serde_json::from_value(v).unwrap();
        assert_eq!(p.stage, "local");
    }

    #[test]
    fn down_param_remove_defaults_to_false() {
        let v = json!({"stage": "dev"});
        let p: DownParam = serde_json::from_value(v).unwrap();
        assert_eq!(p.stage, "dev");
        assert!(!p.remove, "remove should default to false");
    }

    #[test]
    fn down_param_remove_true() {
        let v = json!({"stage": "dev", "remove": true});
        let p: DownParam = serde_json::from_value(v).unwrap();
        assert!(p.remove);
    }

    #[test]
    fn logs_param_optional_fields() {
        let v = json!({"stage": "prod"});
        let p: LogsParam = serde_json::from_value(v).unwrap();
        assert_eq!(p.stage, "prod");
        assert!(p.service.is_none());
        assert!(p.tail.is_none());
    }

    #[test]
    fn logs_param_with_all_fields() {
        let v = json!({"stage": "local", "service": "web", "tail": 100});
        let p: LogsParam = serde_json::from_value(v).unwrap();
        assert_eq!(p.stage, "local");
        assert_eq!(p.service.as_deref(), Some("web"));
        assert_eq!(p.tail, Some(100));
    }

    #[test]
    fn restart_param_deserialize() {
        let v = json!({"stage": "dev", "service": "api"});
        let p: RestartParam = serde_json::from_value(v).unwrap();
        assert_eq!(p.stage, "dev");
        assert_eq!(p.service, "api");
    }

    #[test]
    fn restart_param_missing_service_fails() {
        let v = json!({"stage": "dev"});
        let result = serde_json::from_value::<RestartParam>(v);
        assert!(result.is_err(), "service is required");
    }

    #[test]
    fn build_param_defaults() {
        let v = json!({"stage": "local"});
        let p: BuildParam = serde_json::from_value(v).unwrap();
        assert_eq!(p.stage, "local");
        assert!(p.service.is_none());
        assert!(!p.no_cache, "no_cache should default to false");
    }

    #[test]
    fn build_param_with_all_fields() {
        let v = json!({"stage": "prod", "service": "api", "no_cache": true});
        let p: BuildParam = serde_json::from_value(v).unwrap();
        assert_eq!(p.stage, "prod");
        assert_eq!(p.service.as_deref(), Some("api"));
        assert!(p.no_cache);
    }

    // CP v2 パラメータテスト

    #[test]
    fn project_slug_param_deserialize() {
        let v = json!({"slug": "my-project"});
        let p: ProjectSlugParam = serde_json::from_value(v).unwrap();
        assert_eq!(p.slug, "my-project");
    }

    #[test]
    fn cp_container_param_deserialize() {
        let v = json!({"project": "web", "stage": "prod", "service": "api"});
        let p: CpContainerParam = serde_json::from_value(v).unwrap();
        assert_eq!(p.project, "web");
        assert_eq!(p.stage, "prod");
        assert_eq!(p.service, "api");
    }

    #[test]
    fn cp_logs_param_deserialize() {
        let v = json!({"project": "web", "stage": "local", "service": "db"});
        let p: CpLogsParam = serde_json::from_value(v).unwrap();
        assert_eq!(p.project, "web");
        assert_eq!(p.stage, "local");
        assert_eq!(p.service, "db");
        assert!(p.tail.is_none());
    }

    #[test]
    fn cp_logs_param_with_tail() {
        let v = json!({"project": "web", "stage": "prod", "service": "api", "tail": 200});
        let p: CpLogsParam = serde_json::from_value(v).unwrap();
        assert_eq!(p.tail, Some(200));
    }

    // ------------------------------------------------------------------------
    // ツールルーター・ツール定義
    // ------------------------------------------------------------------------

    /// 期待する全ツール名
    const EXPECTED_TOOLS: &[&str] = &[
        // v1: ローカル Docker 操作
        "fleetflow_inspect_project",
        "fleetflow_ps",
        "fleetflow_up",
        "fleetflow_down",
        "fleetflow_logs",
        "fleetflow_restart",
        "fleetflow_validate",
        "fleetflow_build",
        // v2: CP 経由管理操作
        "fleetflow_cp_status",
        "fleetflow_cp_projects",
        "fleetflow_cp_servers",
        "fleetflow_cp_overview",
        "fleetflow_cp_project_detail",
        "fleetflow_cp_container_start",
        "fleetflow_cp_container_stop",
        "fleetflow_cp_container_restart",
        "fleetflow_cp_container_logs",
        // v3: Dashboard HTTP API 経由（v0.13.0 追加）
        "fleetflow_cp_stages",
        "fleetflow_cp_stage_services",
        "fleetflow_cp_stage_deployments",
        "fleetflow_cp_redeploy",
        "fleetflow_cp_service_restart",
        "fleetflow_cp_container_logs_v2",
        "fleetflow_cp_alerts",
        "fleetflow_cp_agents",
        "fleetflow_cp_tenant_users",
        // FSC-17/18/19 stage runtime status + restart
        "fleet_status",
        "fleet_restart",
    ];

    #[test]
    fn server_registers_all_tools() {
        let server = FleetFlowServer::new();
        let tools = server.tool_router.list_all();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

        for expected in EXPECTED_TOOLS {
            assert!(
                names.contains(expected),
                "tool '{}' should be registered, got: {:?}",
                expected,
                names,
            );
        }

        assert_eq!(
            tools.len(),
            EXPECTED_TOOLS.len(),
            "tool count mismatch: expected {}, got {} ({:?})",
            EXPECTED_TOOLS.len(),
            tools.len(),
            names,
        );
    }

    #[test]
    fn all_tools_have_descriptions() {
        let server = FleetFlowServer::new();
        let tools = server.tool_router.list_all();

        for tool in &tools {
            assert!(
                tool.description.is_some(),
                "tool '{}' should have a description",
                tool.name,
            );
            let desc = tool.description.as_deref().unwrap();
            assert!(
                !desc.is_empty(),
                "tool '{}' description should not be empty",
                tool.name,
            );
        }
    }

    #[test]
    fn all_tools_have_input_schema() {
        let server = FleetFlowServer::new();
        let tools = server.tool_router.list_all();

        for tool in &tools {
            // input_schema は JsonObject (serde_json::Map) — 空でないことを確認
            assert!(
                !tool.input_schema.is_empty(),
                "tool '{}' should have a non-empty input_schema",
                tool.name,
            );
            // type: "object" が含まれていること（JSON Schema の基本）
            assert_eq!(
                tool.input_schema.get("type").and_then(|v| v.as_str()),
                Some("object"),
                "tool '{}' input_schema should have type=object",
                tool.name,
            );
        }
    }

    #[test]
    fn parameterless_tools_have_no_required_fields() {
        let server = FleetFlowServer::new();
        let tools = server.tool_router.list_all();

        let parameterless = [
            "fleetflow_inspect_project",
            "fleetflow_ps",
            "fleetflow_validate",
            "fleetflow_cp_status",
            "fleetflow_cp_projects",
            "fleetflow_cp_servers",
            "fleetflow_cp_overview",
            "fleetflow_cp_stages",
            "fleetflow_cp_agents",
            "fleetflow_cp_tenant_users",
        ];
        for tool in &tools {
            if parameterless.contains(&tool.name.as_ref()) {
                // required は存在しないか、空配列であるべき
                let required = tool.input_schema.get("required");
                if let Some(req) = required {
                    let arr = req.as_array().expect("required should be an array");
                    assert!(
                        arr.is_empty(),
                        "tool '{}' should have no required params, got: {:?}",
                        tool.name,
                        arr,
                    );
                }
            }
        }
    }

    #[test]
    fn up_tool_requires_stage_parameter() {
        let server = FleetFlowServer::new();
        let tools = server.tool_router.list_all();
        let up_tool = tools.iter().find(|t| t.name == "fleetflow_up").unwrap();

        let required = up_tool
            .input_schema
            .get("required")
            .and_then(|v| v.as_array())
            .expect("fleetflow_up should have required fields");

        let required_names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        assert!(
            required_names.contains(&"stage"),
            "fleetflow_up should require 'stage', got: {:?}",
            required_names,
        );
    }

    #[test]
    fn down_tool_requires_stage_parameter() {
        let server = FleetFlowServer::new();
        let tools = server.tool_router.list_all();
        let tool = tools.iter().find(|t| t.name == "fleetflow_down").unwrap();

        let required = tool
            .input_schema
            .get("required")
            .and_then(|v| v.as_array())
            .expect("fleetflow_down should have required fields");

        let required_names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        assert!(required_names.contains(&"stage"));
        // remove はデフォルト値があるので required でないはず
        assert!(
            !required_names.contains(&"remove"),
            "'remove' should not be required (has serde default)",
        );
    }

    #[test]
    fn logs_tool_schema_has_optional_service_and_tail() {
        let server = FleetFlowServer::new();
        let tools = server.tool_router.list_all();
        let tool = tools.iter().find(|t| t.name == "fleetflow_logs").unwrap();

        let required = tool
            .input_schema
            .get("required")
            .and_then(|v| v.as_array())
            .expect("fleetflow_logs should have required fields");

        let required_names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        assert!(required_names.contains(&"stage"));
        assert!(
            !required_names.contains(&"service"),
            "'service' should be optional",
        );
        assert!(
            !required_names.contains(&"tail"),
            "'tail' should be optional",
        );
    }

    #[test]
    fn restart_tool_requires_stage_and_service() {
        let server = FleetFlowServer::new();
        let tools = server.tool_router.list_all();
        let tool = tools
            .iter()
            .find(|t| t.name == "fleetflow_restart")
            .unwrap();

        let required = tool
            .input_schema
            .get("required")
            .and_then(|v| v.as_array())
            .expect("fleetflow_restart should have required fields");

        let required_names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        assert!(required_names.contains(&"stage"));
        assert!(required_names.contains(&"service"));
    }

    #[test]
    fn build_tool_requires_only_stage() {
        let server = FleetFlowServer::new();
        let tools = server.tool_router.list_all();
        let tool = tools.iter().find(|t| t.name == "fleetflow_build").unwrap();

        let required = tool
            .input_schema
            .get("required")
            .and_then(|v| v.as_array())
            .expect("fleetflow_build should have required fields");

        let required_names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        assert!(required_names.contains(&"stage"));
        assert!(
            !required_names.contains(&"service"),
            "'service' should be optional"
        );
        assert!(
            !required_names.contains(&"no_cache"),
            "'no_cache' should not be required (has serde default)",
        );
    }

    // ------------------------------------------------------------------------
    // ServerHandler::get_info
    // ------------------------------------------------------------------------

    #[test]
    fn server_info_has_instructions() {
        let server = FleetFlowServer::new();
        let info = server.get_info();
        let instructions = info.instructions.expect("instructions should be set");
        assert!(
            instructions.contains("FleetFlow"),
            "instructions should mention FleetFlow",
        );
    }

    // ------------------------------------------------------------------------
    // Default trait
    // ------------------------------------------------------------------------

    #[test]
    fn server_default_creates_valid_instance() {
        let server = FleetFlowServer::default();
        let tools = server.tool_router.list_all();
        assert_eq!(tools.len(), EXPECTED_TOOLS.len());
    }
}
