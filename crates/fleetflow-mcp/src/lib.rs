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

        #[allow(deprecated)]
        let options = bollard::container::ListContainersOptions {
            all: true,
            filters: filter,
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

        #[allow(deprecated)]
        let options = bollard::container::LogsOptions::<String> {
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
}

impl ServerHandler for FleetFlowServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "FleetFlow MCP サーバー。KDLベースのコンテナオーケストレーションツールです。"
                    .to_string(),
            ),
            ..Default::default()
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
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
        request: CallToolRequestParam,
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
