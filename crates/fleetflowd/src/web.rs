//! WebUI Dashboard — HTTP API + 埋め込みダッシュボード
//!
//! axum ベースの HTTP サーバーで、CP のデータを JSON API として提供し、
//! 埋め込み HTML ダッシュボードを配信する。
//!
//! 注: ダッシュボードは CP の内部データのみ表示。全データは自サーバーの
//! DB から取得されるため、XSS リスクはない（外部入力なし）。

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use fleetflow_controlplane::server::AppState;
use serde_json::{Value, json};
use tokio::task::JoinHandle;

/// WebUI サーバーを起動
pub async fn start(state: Arc<AppState>, addr: &str) -> anyhow::Result<JoinHandle<()>> {
    let app = Router::new()
        // API routes
        .route("/api/health", get(api_health))
        .route("/api/projects", get(api_projects))
        .route("/api/servers", get(api_servers))
        .route("/api/overview", get(api_overview))
        .route("/api/dns", get(api_dns))
        .route("/api/health-check", post(api_health_check))
        .route("/api/deployments", get(api_deployments))
        .route("/api/dns/sync", post(api_dns_sync))
        // Dashboard
        .route("/", get(dashboard_html))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    Ok(handle)
}

// ============================================================================
// API ハンドラー
// ============================================================================

async fn api_health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn api_projects(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.db.list_projects_by_tenant("default").await {
        Ok(projects) => {
            let items: Vec<Value> = projects
                .iter()
                .map(|p| {
                    json!({
                        "slug": p.slug,
                        "name": p.name,
                        "description": p.description,
                        "created_at": p.created_at.map(|d| d.to_rfc3339()),
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "projects": items }))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn api_servers(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.db.list_servers_by_tenant("default").await {
        Ok(servers) => {
            let items: Vec<Value> = servers
                .iter()
                .map(|s| {
                    json!({
                        "slug": s.slug,
                        "provider": s.provider,
                        "ssh_host": s.ssh_host,
                        "status": s.status,
                        "last_heartbeat_at": s.last_heartbeat_at.map(|d| d.to_rfc3339()),
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "servers": items }))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn api_overview(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.db.list_all_stages_by_tenant("default").await {
        Ok(stages) => {
            let items: Vec<Value> = stages
                .iter()
                .map(|s| {
                    json!({
                        "project_slug": s.project_slug,
                        "project_name": s.project_name,
                        "stage": s.slug,
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "stages": items }))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn api_dns(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.db.list_dns_records_by_tenant("default").await {
        Ok(records) => {
            let items: Vec<Value> = records
                .iter()
                .map(|r| {
                    json!({
                        "name": r.name,
                        "record_type": r.record_type,
                        "content": r.content,
                        "proxied": r.proxied,
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "dns_records": items }))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn api_deployments(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.db.list_deployments("default", 20).await {
        Ok(deployments) => {
            let items: Vec<Value> = deployments
                .iter()
                .map(|d| {
                    json!({
                        "stage": d.stage,
                        "server_slug": d.server_slug,
                        "status": d.status,
                        "command": d.command,
                        "started_at": d.started_at.map(|t| t.to_rfc3339()),
                        "finished_at": d.finished_at.map(|t| t.to_rfc3339()),
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "deployments": items }))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn api_health_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    use chrono::Utc;
    use fleetflow_cloud::tailscale;
    use fleetflow_controlplane::model::ServerStatusUpdate;

    let peers = match tailscale::get_peers().await {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let servers = match state.db.list_all_servers().await {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let now = Utc::now();
    let updates: Vec<ServerStatusUpdate> = servers
        .iter()
        .map(|s| {
            let peer = peers
                .iter()
                .find(|p| p.hostname.eq_ignore_ascii_case(&s.slug));
            let (status, heartbeat) =
                fleetflow_cloud::tailscale::resolve_peer_status(peer, s.last_heartbeat_at, now);
            ServerStatusUpdate {
                slug: s.slug.clone(),
                status,
                last_heartbeat_at: heartbeat,
            }
        })
        .collect();

    let results: Vec<Value> = updates
        .iter()
        .map(|u| json!({ "slug": u.slug, "status": u.status }))
        .collect();

    match state.db.bulk_update_server_status(&updates).await {
        Ok(count) => (
            StatusCode::OK,
            Json(json!({ "updated": count, "results": results })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn api_dns_sync(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Cloudflare DNS との同期
    let cf_config = match fleetflow_cloud_cloudflare::dns::DnsConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Cloudflare 設定エラー: {e}") })),
            )
                .into_response();
        }
    };

    let cf = fleetflow_cloud_cloudflare::dns::CloudflareDns::new(cf_config);

    let cf_records = match cf.list_records().await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let db_records = match state.db.list_dns_records_by_tenant("default").await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let tenant = match state.db.get_tenant_by_slug("default").await {
        Ok(Some(t)) => t,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "tenant not found" })),
            )
                .into_response();
        }
    };

    let mut imported = 0u32;
    for cf_rec in &cf_records {
        let exists = db_records.iter().any(|db| db.name == cf_rec.name);
        if !exists {
            let record = fleetflow_controlplane::model::DnsRecord {
                id: None,
                tenant: tenant.id.clone().unwrap(),
                project: None,
                name: cf_rec.name.clone(),
                record_type: cf_rec.record_type.clone(),
                content: cf_rec.content.clone(),
                zone_id: None,
                cf_record_id: Some(cf_rec.id.clone()),
                proxied: cf_rec.proxied,
                created_at: None,
                updated_at: None,
            };
            if state.db.create_dns_record(&record).await.is_ok() {
                imported += 1;
            }
        }
    }

    let mut not_in_cf = Vec::new();
    for db_rec in &db_records {
        if !cf_records.iter().any(|cf| cf.name == db_rec.name) {
            not_in_cf.push(db_rec.name.clone());
        }
    }

    (
        StatusCode::OK,
        Json(json!({
            "imported": imported,
            "cf_total": cf_records.len(),
            "db_total": db_records.len() + imported as usize,
            "not_in_cloudflare": not_in_cf,
        })),
    )
        .into_response()
}

// ============================================================================
// Dashboard HTML（埋め込み）
// ============================================================================

async fn dashboard_html() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        Html(include_str!("dashboard.html")),
    )
}
