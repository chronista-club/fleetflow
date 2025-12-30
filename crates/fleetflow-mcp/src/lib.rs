use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use tracing::error;

#[derive(Debug, Deserialize, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    pub result: Option<Value>,
    pub error: Option<Value>,
}

pub struct McpServer {
    // 状態管理（将来的に Docker クライアントなどを保持）
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServer {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn run(&mut self) -> Result<()> {
        eprintln!("[MCP DEBUG] Starting FleetFlow MCP server...");
        eprintln!("[MCP DEBUG] Waiting for stdin...");
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        let mut line = String::new();

        eprintln!("[MCP DEBUG] Entering read loop...");
        while reader.read_line(&mut line)? > 0 {
            eprintln!("[MCP DEBUG] Received line: {} bytes", line.len());
            let request: Value = match serde_json::from_str(&line) {
                Ok(req) => {
                    eprintln!("[MCP DEBUG] Parsed JSON successfully");
                    req
                }
                Err(e) => {
                    eprintln!("[MCP DEBUG] Failed to parse JSON: {}", e);
                    error!("Failed to parse JSON-RPC request: {}", e);
                    line.clear();
                    continue;
                }
            };

            // 通知（idがないリクエスト）はレスポンスを返さない
            if request.get("id").is_none() {
                eprintln!("[MCP DEBUG] Notification (no id), skipping response");
                line.clear();
                continue;
            }

            let method = request
                .get("method")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            eprintln!("[MCP DEBUG] Processing method: {}", method);

            let req_obj: JsonRpcRequest = serde_json::from_value(request)?;
            let response = self.handle_request(req_obj).await?;
            let response_json = serde_json::to_string(&response)?;
            eprintln!(
                "[MCP DEBUG] Sending response: {} bytes",
                response_json.len()
            );
            println!("{}", response_json);
            io::stdout().flush()?;
            eprintln!("[MCP DEBUG] Response sent and flushed");

            line.clear();
        }

        eprintln!("[MCP DEBUG] Read loop ended (EOF or error)");
        Ok(())
    }

    async fn handle_request(&self, req: JsonRpcRequest) -> Result<JsonRpcResponse> {
        let id = req.id.unwrap_or(Value::Null);

        let result = match req.method.as_str() {
            "initialize" => Some(self.handle_initialize()?),
            "tools/list" => Some(self.handle_tools_list()?),
            "tools/call" => {
                match self
                    .handle_tool_call(req.params.unwrap_or(Value::Null))
                    .await
                {
                    Ok(res) => Some(res),
                    Err(e) => Some(json!({
                        "isError": true,
                        "content": [
                            {
                                "type": "text",
                                "text": format!("エラーが発生しました: {}", e)
                            }
                        ]
                    })),
                }
            }
            "resources/list" => Some(json!({ "resources": [] })),
            "prompts/list" => Some(json!({ "prompts": [] })),
            "notifications/initialized" => {
                return Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: Value::Null,
                    result: None,
                    error: None,
                });
            }
            _ => None,
        };

        let error = if result.is_none() && req.method != "notifications/initialized" {
            Some(json!({
                "code": -32601,
                "message": format!("Method not found: {}", req.method)
            }))
        } else {
            None
        };

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result,
            error,
        })
    }

    async fn handle_tool_call(&self, params: Value) -> Result<Value> {
        let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let arguments = params.get("arguments").unwrap_or(&Value::Null);

        match name {
            "fleetflow_inspect_project" => {
                let project_root = fleetflow_core::find_project_root()?;
                let config = fleetflow_core::load_project_from_root(&project_root)?;

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

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": info
                        }
                    ]
                }))
            }
            "fleetflow_ps" => {
                let docker = bollard::Docker::connect_with_local_defaults()?;
                let project_root = fleetflow_core::find_project_root()?;
                let config = fleetflow_core::load_project_from_root(&project_root)?;

                let mut filter = std::collections::HashMap::new();
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

                let containers = docker.list_containers(Some(options)).await?;

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

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": status
                        }
                    ]
                }))
            }
            "fleetflow_up" => {
                let stage = arguments
                    .get("stage")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing stage argument"))?;
                let project_root = fleetflow_core::find_project_root()?;
                let config = fleetflow_core::load_project_from_root(&project_root)?;

                let runtime = fleetflow_container::Runtime::new(project_root)?;
                match runtime.up(&config, stage, false).await {
                    Ok(_) => Ok(json!({
                        "content": [
                            {
                                "type": "text",
                                "text": format!("ステージ '{}' の全サービスを正常に起動しました。ネットワークの作成とコンテナの配置が完了しています。", stage)
                            }
                        ]
                    })),
                    Err(e) => Ok(json!({
                        "isError": true,
                        "content": [
                            {
                                "type": "text",
                                "text": format!("ステージ '{}' の起動に失敗しました。理由: {}\nDockerが起動しているか、設定ファイルに誤りがないか確認してください。", stage, e)
                            }
                        ]
                    })),
                }
            }
            "fleetflow_down" => {
                let stage = arguments
                    .get("stage")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing stage argument"))?;
                let remove = arguments
                    .get("remove")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let project_root = fleetflow_core::find_project_root()?;
                let config = fleetflow_core::load_project_from_root(&project_root)?;

                let runtime = fleetflow_container::Runtime::new(project_root)?;
                match runtime.down(&config, stage, remove).await {
                    Ok(_) => Ok(json!({
                        "content": [
                            {
                                "type": "text",
                                "text": format!("ステージ '{}' の停止が完了しました。{}", stage, if remove { "コンテナとネットワークも削除されました。" } else { "コンテナは停止した状態で残っています。" })
                            }
                        ]
                    })),
                    Err(e) => Ok(json!({
                        "isError": true,
                        "content": [
                            {
                                "type": "text",
                                "text": format!("ステージ '{}' の停止処理中にエラーが発生しました。理由: {}", stage, e)
                            }
                        ]
                    })),
                }
            }
            "fleetflow_logs" => {
                use futures_util::StreamExt;

                let stage = arguments
                    .get("stage")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing stage argument"))?;
                let service = arguments.get("service").and_then(|v| v.as_str());
                let tail = arguments.get("tail").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

                let project_root = fleetflow_core::find_project_root()?;
                let config = fleetflow_core::load_project_from_root(&project_root)?;
                let docker = bollard::Docker::connect_with_local_defaults()?;

                let container_name = if let Some(svc) = service {
                    format!("{}-{}-{}", config.name, stage, svc)
                } else {
                    let stage_config = config
                        .stages
                        .get(stage)
                        .ok_or_else(|| anyhow::anyhow!("Stage '{}' not found", stage))?;
                    let first_service = stage_config
                        .services
                        .first()
                        .ok_or_else(|| anyhow::anyhow!("No services in stage '{}'", stage))?;
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
                            return Ok(json!({
                                "isError": true,
                                "content": [{ "type": "text", "text": format!("ログ取得エラー: {}", e) }]
                            }));
                        }
                    }
                }

                Ok(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Logs for {}:\n\n{}", container_name, logs)
                        }
                    ]
                }))
            }
            "fleetflow_restart" => {
                let stage = arguments
                    .get("stage")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing stage argument"))?;
                let service = arguments
                    .get("service")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing service argument"))?;

                let project_root = fleetflow_core::find_project_root()?;
                let config = fleetflow_core::load_project_from_root(&project_root)?;
                let docker = bollard::Docker::connect_with_local_defaults()?;

                let container_name = format!("{}-{}-{}", config.name, stage, service);

                // コンテナを再起動
                match docker
                    .restart_container(
                        &container_name,
                        None::<bollard::query_parameters::RestartContainerOptions>,
                    )
                    .await
                {
                    Ok(_) => Ok(json!({
                        "content": [{
                            "type": "text",
                            "text": format!("サービス '{}' を再起動しました (コンテナ: {})", service, container_name)
                        }]
                    })),
                    Err(e) => Ok(json!({
                        "isError": true,
                        "content": [{
                            "type": "text",
                            "text": format!("再起動に失敗しました: {}", e)
                        }]
                    })),
                }
            }
            "fleetflow_validate" => {
                let project_root = match fleetflow_core::find_project_root() {
                    Ok(root) => root,
                    Err(e) => {
                        return Ok(json!({
                            "isError": true,
                            "content": [{
                                "type": "text",
                                "text": format!("プロジェクトルートが見つかりません: {}", e)
                            }]
                        }));
                    }
                };

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

                        Ok(json!({
                            "content": [{
                                "type": "text",
                                "text": result
                            }]
                        }))
                    }
                    Err(e) => Ok(json!({
                        "isError": true,
                        "content": [{
                            "type": "text",
                            "text": format!("✗ 設定エラー: {}", e)
                        }]
                    })),
                }
            }
            "fleetflow_build" => {
                let stage = arguments
                    .get("stage")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing stage argument"))?;
                let service_filter = arguments.get("service").and_then(|v| v.as_str());
                let no_cache = arguments
                    .get("no_cache")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let project_root = fleetflow_core::find_project_root()?;
                let config = fleetflow_core::load_project_from_root(&project_root)?;

                // ステージのサービス一覧を取得
                let stage_config = match config.stages.get(stage) {
                    Some(s) => s,
                    None => {
                        return Ok(json!({
                            "isError": true,
                            "content": [{
                                "type": "text",
                                "text": format!("ステージ '{}' が見つかりません。利用可能: {}",
                                    stage, config.stages.keys().cloned().collect::<Vec<_>>().join(", "))
                            }]
                        }));
                    }
                };

                let docker = bollard::Docker::connect_with_local_defaults()?;
                let resolver = fleetflow_build::BuildResolver::new(project_root.clone());
                let builder = fleetflow_build::ImageBuilder::new(docker);

                let mut built_services = Vec::new();
                let mut skipped_services = Vec::new();
                let mut errors = Vec::new();

                // ビルド対象のサービスを決定
                let services_to_build: Vec<String> = if let Some(svc) = service_filter {
                    if stage_config.services.contains(&svc.to_string()) {
                        vec![svc.to_string()]
                    } else {
                        return Ok(json!({
                            "isError": true,
                            "content": [{
                                "type": "text",
                                "text": format!("サービス '{}' はステージ '{}' に含まれていません", svc, stage)
                            }]
                        }));
                    }
                } else {
                    stage_config.services.clone()
                };

                for service_name in &services_to_build {
                    let service = match config.services.get(service_name) {
                        Some(s) => s,
                        None => {
                            skipped_services.push(format!("{} (定義なし)", service_name));
                            continue;
                        }
                    };

                    // Dockerfileの解決
                    let dockerfile = match resolver.resolve_dockerfile(service_name, service) {
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

                    // コンテキストとタグの解決
                    let context_path = match resolver.resolve_context(service) {
                        Ok(path) => path,
                        Err(e) => {
                            errors.push(format!("{}: {}", service_name, e));
                            continue;
                        }
                    };

                    let image_tag =
                        resolver.resolve_image_tag(service_name, service, &config.name, stage);
                    let build_args =
                        resolver.resolve_build_args(service, &std::collections::HashMap::new());

                    // ビルドコンテキストの作成
                    let context_data = match fleetflow_build::ContextBuilder::create_context(
                        &context_path,
                        &dockerfile,
                    ) {
                        Ok(data) => data,
                        Err(e) => {
                            errors.push(format!("{}: コンテキスト作成失敗 - {}", service_name, e));
                            continue;
                        }
                    };

                    // ビルド実行
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

                // 結果のフォーマット
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

                Ok(json!({
                    "isError": !errors.is_empty(),
                    "content": [{
                        "type": "text",
                        "text": result
                    }]
                }))
            }
            _ => Ok(json!({
                "isError": true,
                "content": [
                    {
                        "type": "text",
                        "text": format!("Unknown tool: {}", name)
                    }
                ]
            })),
        }
    }

    fn handle_initialize(&self) -> Result<Value> {
        Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "fleetflow",
                "version": env!("CARGO_PKG_VERSION")
            }
        }))
    }

    fn handle_tools_list(&self) -> Result<Value> {
        Ok(json!({
            "tools": [
                {
                    "name": "fleetflow_inspect_project",
                    "description": "カレントディレクトリにある FleetFlow プロジェクト（fleet.kdl 等）を解析し、定義されているサービス名、イメージ名、ステージ名、環境変数などの情報を取得します。プロジェクトの全体像を把握するために最初に使用してください。",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "fleetflow_ps",
                    "description": "コンテナの一覧を表示します。プロジェクトに関連するコンテナの稼働状況を確認できます。CLIの 'fleet ps' と同等。",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "stage": {
                                "type": "string",
                                "description": "フィルタリング対象のステージ名（オプション）"
                            }
                        }
                    }
                },
                {
                    "name": "fleetflow_up",
                    "description": "指定されたステージのコンテナを起動します。ネットワークの作成や、既に存在するコンテナの再起動も行います。",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "stage": {
                                "type": "string",
                                "description": "起動するステージ名（例: local, production）"
                            }
                        },
                        "required": ["stage"]
                    }
                },
                {
                    "name": "fleetflow_down",
                    "description": "指定されたステージのコンテナを停止します。オプションでコンテナやネットワークの削除も行います。",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "stage": {
                                "type": "string",
                                "description": "停止するステージ名"
                            },
                            "remove": {
                                "type": "boolean",
                                "description": "コンテナとネットワークを完全に削除する場合は true"
                            }
                        },
                        "required": ["stage"]
                    }
                },
                {
                    "name": "fleetflow_logs",
                    "description": "指定されたステージのコンテナログを取得します。特定のサービスを指定することも可能です。",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "stage": {
                                "type": "string",
                                "description": "ステージ名"
                            },
                            "service": {
                                "type": "string",
                                "description": "サービス名（オプション、未指定時は最初のサービス）"
                            },
                            "tail": {
                                "type": "integer",
                                "description": "取得する行数（デフォルト: 50）"
                            }
                        },
                        "required": ["stage"]
                    }
                },
                {
                    "name": "fleetflow_restart",
                    "description": "指定されたサービスのコンテナを再起動します。設定変更後やアプリケーションのリセットに使用します。",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "stage": {
                                "type": "string",
                                "description": "ステージ名（例: local, dev, stg, prod）"
                            },
                            "service": {
                                "type": "string",
                                "description": "再起動するサービス名"
                            }
                        },
                        "required": ["stage", "service"]
                    }
                },
                {
                    "name": "fleetflow_validate",
                    "description": "FleetFlow設定ファイル（fleet.kdl等）の構文と整合性を検証します。エラーがあれば詳細を報告します。",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "fleetflow_build",
                    "description": "指定されたサービスのDockerイメージをビルドします。Dockerfileが設定されているサービスのみ対象。",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "stage": {
                                "type": "string",
                                "description": "ステージ名（例: local, dev, stg, prod）"
                            },
                            "service": {
                                "type": "string",
                                "description": "ビルド対象のサービス名（オプション、未指定時は全てのビルド可能サービス）"
                            },
                            "no_cache": {
                                "type": "boolean",
                                "description": "キャッシュを使用せずにビルドする場合は true"
                            }
                        },
                        "required": ["stage"]
                    }
                }
            ]
        }))
    }
}
