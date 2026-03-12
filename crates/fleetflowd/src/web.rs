//! WebUI Dashboard — HTTP API + 埋め込みダッシュボード
//!
//! axum ベースの HTTP サーバーで、CP のデータを JSON API として提供し、
//! 埋め込み HTML ダッシュボードを配信する。

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use fleetflow_controlplane::auth::Claims;
use fleetflow_controlplane::server::AppState;
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use tracing::debug;

/// Web サーバー用の共有状態
pub struct WebState {
    pub app: Arc<AppState>,
    pub auth0_domain: String,
    pub auth0_client_id: String,
    pub auth0_audience: String,
}

/// WebUI サーバーを起動
pub async fn start(
    state: Arc<AppState>,
    addr: &str,
    auth0_client_id: &str,
) -> anyhow::Result<JoinHandle<()>> {
    let auth0_domain = state.auth.domain().to_string();
    let auth0_audience = state.auth.audience().to_string();

    let web_state = Arc::new(WebState {
        app: state,
        auth0_domain,
        auth0_client_id: auth0_client_id.to_string(),
        auth0_audience,
    });

    // 認証不要のルート
    let public = Router::new()
        .route("/", get(dashboard_html))
        .route("/api/health", get(api_health))
        .route("/api/auth/config", get(api_auth_config));

    // 認証必須のルート
    let protected = Router::new()
        .route("/api/me", get(api_me))
        .route("/api/projects", get(api_projects))
        .route("/api/servers", get(api_servers))
        .route("/api/overview", get(api_overview))
        .route("/api/dns", get(api_dns))
        .route("/api/health-check", post(api_health_check))
        .route("/api/deployments", get(api_deployments))
        .route("/api/dns/sync", post(api_dns_sync))
        .layer(middleware::from_fn_with_state(
            web_state.clone(),
            auth_middleware,
        ));

    let app = public.merge(protected).with_state(web_state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    Ok(handle)
}

// ============================================================================
// Auth ミドルウェア
// ============================================================================

/// Authorization: Bearer <token> を検証し、Claims を request extensions に格納。
/// Auth0 domain が未設定の場合は認証をスキップ（dev mode）。
async fn auth_middleware(
    State(state): State<Arc<WebState>>,
    mut req: Request,
    next: Next,
) -> Response {
    // Auth0 未設定時は認証スキップ（dev mode）
    if state.auth0_domain.is_empty() {
        return next.run(req).await;
    }

    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let token = match auth_header {
        Some(h) if h.starts_with("Bearer ") => &h[7..],
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Missing or invalid Authorization header" })),
            )
                .into_response();
        }
    };

    match state.app.auth.verify(token).await {
        Ok(claims) => {
            debug!(sub = %claims.sub, "API 認証成功");
            req.extensions_mut().insert(claims);
            next.run(req).await
        }
        Err(e) => {
            tracing::warn!(error = %e, "JWT 検証失敗");
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Unauthorized" })),
            )
                .into_response()
        }
    }
}

// ============================================================================
// Auth エンドポイント
// ============================================================================

/// Auth0 設定を返す（フロントエンド SPA 用）
async fn api_auth_config(State(state): State<Arc<WebState>>) -> Json<Value> {
    Json(json!({
        "domain": state.auth0_domain,
        "clientId": state.auth0_client_id,
        "audience": state.auth0_audience,
    }))
}

/// 認証済みユーザー情報を返す
async fn api_me(req: Request) -> impl IntoResponse {
    match req.extensions().get::<Claims>() {
        Some(claims) => (
            StatusCode::OK,
            Json(json!({
                "sub": claims.sub,
                "email": claims.email,
                "permissions": claims.permissions,
            })),
        )
            .into_response(),
        None => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Not authenticated" })),
        )
            .into_response(),
    }
}

// ============================================================================
// API ハンドラー
// ============================================================================

async fn api_health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn api_projects(State(state): State<Arc<WebState>>) -> impl IntoResponse {
    match state.app.db.list_projects_by_tenant("default").await {
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

async fn api_servers(State(state): State<Arc<WebState>>) -> impl IntoResponse {
    match state.app.db.list_servers_by_tenant("default").await {
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

async fn api_overview(State(state): State<Arc<WebState>>) -> impl IntoResponse {
    match state.app.db.list_all_stages_by_tenant("default").await {
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

async fn api_dns(State(state): State<Arc<WebState>>) -> impl IntoResponse {
    match state.app.db.list_dns_records_by_tenant("default").await {
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

async fn api_deployments(State(state): State<Arc<WebState>>) -> impl IntoResponse {
    match state.app.db.list_deployments("default", 20).await {
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

async fn api_health_check(State(state): State<Arc<WebState>>) -> impl IntoResponse {
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

    let servers = match state.app.db.list_all_servers().await {
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

    match state.app.db.bulk_update_server_status(&updates).await {
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

async fn api_dns_sync(State(state): State<Arc<WebState>>) -> impl IntoResponse {
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

    let db_records = match state.app.db.list_dns_records_by_tenant("default").await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let tenant = match state.app.db.get_tenant_by_slug("default").await {
        Ok(Some(t)) => t,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "tenant not found" })),
            )
                .into_response();
        }
    };

    let tenant_id = match tenant.id.clone() {
        Some(id) => id,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "tenant has no id" })),
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
                tenant: tenant_id.clone(),
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
            if state.app.db.create_dns_record(&record).await.is_ok() {
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
