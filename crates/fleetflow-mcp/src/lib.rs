use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use tracing::{debug, error, info};

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

impl McpServer {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("Starting FleetFlow MCP server...");
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        let mut line = String::new();

        while reader.read_line(&mut line)? > 0 {
            let request: Value = match serde_json::from_str(&line) {
                Ok(req) => req,
                Err(e) => {
                    error!("Failed to parse JSON-RPC request: {}", e);
                    line.clear();
                    continue;
                }
            };

            // 通知（idがないリクエスト）はレスポンスを返さない
            if request.get("id").is_none() {
                line.clear();
                continue;
            }

            let req_obj: JsonRpcRequest = serde_json::from_value(request)?;
            let response = self.handle_request(req_obj).await?;
            let response_json = serde_json::to_string(&response)?;
            println!("{}", response_json);
            io::stdout().flush()?;

            line.clear();
        }

        Ok(())
    }

    async fn handle_request(&self, req: JsonRpcRequest) -> Result<JsonRpcResponse> {
        let id = req.id.unwrap_or(Value::Null);

        let result = match req.method.as_str() {
            "initialize" => Some(self.handle_initialize()?),
            "tools/list" => Some(self.handle_tools_list()?),
            "tools/call" => {
                match self.handle_tool_call(req.params.unwrap_or(Value::Null)).await {
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
            },
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
                let project_root = fleetflow_atom::find_project_root()?;
                let discovered = fleetflow_atom::discover_files(&project_root)?;
                let config = fleetflow_atom::load_project_from_root(&project_root)?;
                
                let mut info = format!("Project: {}\n\n", config.name);
                
                if !discovered.workloads.is_empty() {
                    info.push_str("Detected Workloads:\n");
                    for w in &discovered.workloads {
                        info.push_str(&format!("  - {}\n", w.display()));
                    }
                    info.push('\n');
                }

                info.push_str("Stages:\n");
                for (stage_name, stage) in &config.stages {
                    info.push_str(&format!("  - {} ({} services)\n", stage_name, stage.services.len()));
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
            },
            "fleetflow_status" => {
                let docker = bollard::Docker::connect_with_local_defaults()?;
                let project_root = fleetflow_atom::find_project_root()?;
                let config = fleetflow_atom::load_project_from_root(&project_root)?;
                
                let mut filter = std::collections::HashMap::new();
                filter.insert("label".to_string(), vec![format!("fleetflow.project={}", config.name)]);
                
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
                        let name = c.names.and_then(|n| n.first().cloned()).unwrap_or_else(|| "unnamed".to_string());
                        let status_text = c.status.unwrap_or_else(|| "unknown".to_string());
                        let image = c.image.unwrap_or_else(|| "unknown".to_string());
                        status.push_str(&format!("- {}: {} ({})\n", name.trim_start_matches('/'), status_text, image));
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
            },
            "fleetflow_up" => {
                let stage = arguments.get("stage").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("Missing stage argument"))?;
                let project_root = fleetflow_atom::find_project_root()?;
                let config = fleetflow_atom::load_project_from_root(&project_root)?;
                
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
            },
            "fleetflow_down" => {
                let stage = arguments.get("stage").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("Missing stage argument"))?;
                let remove = arguments.get("remove").and_then(|v| v.as_bool()).unwrap_or(false);
                let project_root = fleetflow_atom::find_project_root()?;
                let config = fleetflow_atom::load_project_from_root(&project_root)?;
                
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
            },
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
                    "description": "カレントディレクトリにある FleetFlow プロジェクト（flow.kdl 等）を解析し、定義されているサービス名、イメージ名、ステージ名、環境変数などの情報を取得します。プロジェクトの全体像を把握するために最初に使用してください。",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "fleetflow_status",
                    "description": "現在実行中または停止中のコンテナの稼働状況を取得します。プロジェクトに関連するコンテナのみをフィルタリングして表示します。デバッグや現状確認に便利です。",
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
                }
            ]
        }))
    }
}
