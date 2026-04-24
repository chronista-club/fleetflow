//! WebUI Dashboard — HTTP API + 埋め込みダッシュボード
//!
//! axum ベースの HTTP サーバーで、CP のデータを JSON API として提供し、
//! 埋め込み HTML ダッシュボードを配信する。

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use fleetflow_controlplane::model::{AuthContext, TenantRole};
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
        .route("/api/tenant/overview", get(api_tenant_overview))
        .route("/api/dns", get(api_dns))
        .route("/api/health-check", post(api_health_check))
        .route("/api/deployments", get(api_deployments))
        .route("/api/stages", get(api_stages))
        .route(
            "/api/tenant/users",
            get(api_tenant_users).post(api_tenant_users_create),
        )
        .route(
            "/api/tenant/users/{sub}",
            axum::routing::put(api_tenant_users_update).delete(api_tenant_users_delete),
        )
        .route(
            "/api/stages/{project}/{stage}/services",
            get(api_stage_services),
        )
        .route(
            "/api/stages/{project}/{stage}/deployments",
            get(api_stage_deployments),
        )
        .route("/api/deployments/{id}/log", get(api_deployment_log))
        .route(
            "/api/stages/{project}/{stage}/redeploy",
            post(api_stage_redeploy),
        )
        .route(
            "/api/stages/{project}/{stage}/restart/{service}",
            post(api_service_restart),
        )
        .route(
            "/api/stages/{project}/{stage}/alerts",
            get(api_stage_alerts),
        )
        .route("/api/agents", get(api_agents))
        .route(
            "/api/stages/{project}/{stage}/logs/{container}",
            get(api_container_logs),
        )
        .route("/api/dns/sync", post(api_dns_sync))
        // Persistence Volume Tier P-2 (2026-04-23)
        .route("/api/v1/volumes", get(api_volumes))
        .route("/api/v1/volumes/adopt", post(api_volume_adopt))
        // Build Tier v1 MVP (2026-04-23)
        .route("/api/v1/builds", get(api_build_list).post(api_build_submit))
        .route("/api/v1/builds/{id}", get(api_build_show))
        .route("/api/v1/builds/{id}/logs", get(api_build_logs))
        .route("/api/v1/builds/{id}/cancel", post(api_build_cancel))
        // Stage Runtime Status + Service Restart v1 (FSC-17/18, 2026-04-22)
        .route(
            "/api/v1/stages/{project}/{stage}/status",
            get(api_v1_stage_status),
        )
        .route(
            "/api/v1/stages/{project}/{stage}/services/{service}/restart",
            post(api_v1_service_restart),
        )
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

/// Authorization: Bearer <token> を検証し、テナント解決して AuthContext を request extensions に格納。
/// Auth0 domain が未設定の場合は dev mode（"default" テナント）。
async fn auth_middleware(
    State(state): State<Arc<WebState>>,
    mut req: Request,
    next: Next,
) -> Response {
    // Auth0 未設定時は dev mode: "default" テナントで通す
    if state.auth0_domain.is_empty() {
        req.extensions_mut().insert(AuthContext {
            sub: "dev-user".into(),
            email: Some("dev@localhost".into()),
            tenant_slug: "default".into(),
            role: TenantRole::Owner,
        });
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

    let claims = match state.app.auth.verify(token).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "JWT 検証失敗");
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Unauthorized" })),
            )
                .into_response();
        }
    };

    // SurrealDB でテナント解決
    let tenant_user = match state.app.db.resolve_tenant_by_sub(&claims.sub).await {
        Ok(Some(tu)) => tu,
        Ok(None) => {
            tracing::warn!(sub = %claims.sub, "テナント未紐付けユーザー");
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "No tenant associated with this user" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(error = %e, "テナント解決エラー");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Tenant resolution failed" })),
            )
                .into_response();
        }
    };

    // tenant RecordId → slug 解決
    let tenant_slug = match state.app.db.get_tenant_by_id(&tenant_user.tenant).await {
        Ok(Some(t)) => t.slug,
        Ok(None) => {
            tracing::error!(tenant_id = ?tenant_user.tenant, "テナントが見つからない");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Tenant not found" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(error = %e, "テナント取得エラー");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Tenant lookup failed" })),
            )
                .into_response();
        }
    };

    let role = tenant_user.tenant_role();
    debug!(sub = %claims.sub, tenant = %tenant_slug, role = %role, "API 認証成功");
    req.extensions_mut().insert(AuthContext {
        sub: claims.sub,
        email: claims.email,
        tenant_slug,
        role,
    });
    next.run(req).await
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
    match req.extensions().get::<AuthContext>() {
        Some(ctx) => (
            StatusCode::OK,
            Json(json!({
                "sub": ctx.sub,
                "email": ctx.email,
                "tenant": ctx.tenant_slug,
                "role": ctx.role.to_string(),
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

async fn api_projects(State(state): State<Arc<WebState>>, req: Request) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();
    match state.app.db.list_projects_by_tenant(&ctx.tenant_slug).await {
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

async fn api_servers(State(state): State<Arc<WebState>>, req: Request) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();
    match state.app.db.list_servers_by_tenant(&ctx.tenant_slug).await {
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

async fn api_overview(State(state): State<Arc<WebState>>, req: Request) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();
    match state
        .app
        .db
        .list_all_stages_by_tenant(&ctx.tenant_slug)
        .await
    {
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

/// `/api/tenant/overview` — operator console 用に tenant + projects + stages を 1 shot で返す
///
/// 返す shape (fleetstage-operator UI `OperatorOverview.tsx` 互換):
/// ```json
/// {
///   "tenant": { "slug": "...", "name": "..." },
///   "projects": [{
///     "slug": "...", "name": "...",
///     "stages": [{ "slug": "...", "status": "running|pending|failed|unknown" }]
///   }]
/// }
/// ```
///
/// `status` は stage の直近デプロイ (`last_deploy_status`) から導出。
/// デプロイ履歴が無ければ `"unknown"`。
async fn api_tenant_overview(
    State(state): State<Arc<WebState>>,
    req: Request,
) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();

    // 1. tenant 情報
    let tenant = match state.app.db.get_tenant_by_slug(&ctx.tenant_slug).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "tenant not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    // 2. stage overviews (server_status, last_deploy_status 同梱)
    let overviews = match state.app.db.list_stage_overviews(&ctx.tenant_slug).await {
        Ok(o) => o,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    // 3. project_slug ごとに stages を grouping (BTreeMap で slug 順に安定化)
    use std::collections::BTreeMap;
    let mut projects: BTreeMap<String, (String, Vec<Value>)> = BTreeMap::new();
    for ov in overviews {
        // last_deploy_status を優先、なければ "unknown"
        let status = ov
            .last_deploy_status
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let stage_json = json!({ "slug": ov.slug, "status": status });
        projects
            .entry(ov.project_slug.clone())
            .or_insert_with(|| (ov.project_name.clone(), vec![]))
            .1
            .push(stage_json);
    }
    let projects_json: Vec<Value> = projects
        .into_iter()
        .map(|(slug, (name, stages))| json!({ "slug": slug, "name": name, "stages": stages }))
        .collect();

    (
        StatusCode::OK,
        Json(json!({
            "tenant": { "slug": tenant.slug, "name": tenant.name },
            "projects": projects_json,
        })),
    )
        .into_response()
}

async fn api_dns(State(state): State<Arc<WebState>>, req: Request) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();
    match state
        .app
        .db
        .list_dns_records_by_tenant(&ctx.tenant_slug)
        .await
    {
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

async fn api_deployments(State(state): State<Arc<WebState>>, req: Request) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();
    match state.app.db.list_deployments(&ctx.tenant_slug, 20).await {
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

async fn api_health_check(State(state): State<Arc<WebState>>, req: Request) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();
    if !ctx.can_operate() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Insufficient permissions" })),
        )
            .into_response();
    }

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

    let servers = match state.app.db.list_servers_by_tenant(&ctx.tenant_slug).await {
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

async fn api_dns_sync(State(state): State<Arc<WebState>>, req: Request) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap().clone();

    // 認可チェック: owner/admin のみ（インフラ操作）
    if !ctx.can_operate() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Insufficient permissions" })),
        )
            .into_response();
    }

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

    let db_records = match state
        .app
        .db
        .list_dns_records_by_tenant(&ctx.tenant_slug)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let tenant = match state.app.db.get_tenant_by_slug(&ctx.tenant_slug).await {
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
// Stages API（1st ビュー）
// ============================================================================

/// テナントのステージ一覧（サーバー・デプロイ情報付き、優先度ソート済み）
async fn api_stages(State(state): State<Arc<WebState>>, req: Request) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();
    match state.app.db.list_stage_overviews(&ctx.tenant_slug).await {
        Ok(mut stages) => {
            // 優先度ソート: 異常が上に来る
            stages.sort_by(|a, b| {
                let priority = |s: &fleetflow_controlplane::model::StageOverview| -> u8 {
                    // アクティブアラートあり
                    if s.alert_count.unwrap_or(0) > 0 {
                        return 0;
                    }
                    // デプロイ失敗
                    if s.last_deploy_status.as_deref() == Some("failed") {
                        return 1;
                    }
                    // サーバー offline
                    if s.server_status.as_deref() == Some("offline") {
                        return 2;
                    }
                    // デプロイ進行中
                    if matches!(
                        s.last_deploy_status.as_deref(),
                        Some("running") | Some("pending")
                    ) {
                        return 3;
                    }
                    // 正常
                    4
                };
                priority(a).cmp(&priority(b))
            });

            let items: Vec<Value> = stages
                .iter()
                .map(|s| {
                    json!({
                        "project_slug": s.project_slug,
                        "project_name": s.project_name,
                        "stage": s.slug,
                        "description": s.description,
                        "server_slug": s.server_slug,
                        "server_status": s.server_status,
                        "server_heartbeat": s.server_heartbeat.map(|d| d.to_rfc3339()),
                        "last_deploy_status": s.last_deploy_status,
                        "last_deploy_at": s.last_deploy_at.map(|d| d.to_rfc3339()),
                        "alert_count": s.alert_count.unwrap_or(0),
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

// ============================================================================
// Stage Detail API（展開ビュー）
// ============================================================================

/// ステージのサービス一覧
async fn api_stage_services(
    State(state): State<Arc<WebState>>,
    Path((project, stage)): Path<(String, String)>,
    req: Request,
) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();
    match state
        .app
        .db
        .list_services_by_project_stage(&ctx.tenant_slug, &project, &stage)
        .await
    {
        Ok(services) => {
            let items: Vec<Value> = services
                .iter()
                .map(|s| {
                    json!({
                        "slug": s.slug,
                        "image": s.image,
                        "desired_status": s.desired_status,
                        "config": s.config,
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "services": items }))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// ステージのデプロイ履歴
async fn api_stage_deployments(
    State(state): State<Arc<WebState>>,
    Path((project, stage)): Path<(String, String)>,
    req: Request,
) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();
    match state
        .app
        .db
        .list_deployments_by_project_stage(&ctx.tenant_slug, &project, &stage, 10)
        .await
    {
        Ok(deployments) => {
            let items: Vec<Value> = deployments
                .iter()
                .map(|d| {
                    json!({
                        "id": d.id.as_ref().map(|id| serde_json::to_value(id).ok()),
                        "status": d.status,
                        "command": d.command,
                        "server_slug": d.server_slug,
                        "started_at": d.started_at.map(|t| t.to_rfc3339()),
                        "finished_at": d.finished_at.map(|t| t.to_rfc3339()),
                        "has_log": d.log.is_some(),
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

/// デプロイログ取得
async fn api_deployment_log(
    State(state): State<Arc<WebState>>,
    Path(id): Path<String>,
    req: Request,
) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();

    match state.app.db.get_deployment_log(&id, &ctx.tenant_slug).await {
        Ok(Some(d)) => (
            StatusCode::OK,
            Json(json!({
                "status": d.status,
                "command": d.command,
                "log": d.log,
                "started_at": d.started_at.map(|t| t.to_rfc3339()),
                "finished_at": d.finished_at.map(|t| t.to_rfc3339()),
            })),
        )
            .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Deployment not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

// ============================================================================
// Agent Action API（再デプロイ・再起動）
// ============================================================================

/// ステージ再デプロイ（CP → Agent → docker compose up）
async fn api_stage_redeploy(
    State(state): State<Arc<WebState>>,
    Path((project, stage)): Path<(String, String)>,
    req: Request,
) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();

    // 認可チェック: owner/admin のみ（インフラ操作）
    if !ctx.can_operate() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Insufficient permissions" })),
        )
            .into_response();
    }

    // ステージのサーバーを特定
    let stages = match state.app.db.list_stage_overviews(&ctx.tenant_slug).await {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let target = stages
        .iter()
        .find(|s| s.project_slug == project && s.slug == stage);

    let server_slug = match target.and_then(|s| s.server_slug.as_deref()) {
        Some(s) => s.to_string(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Stage has no server assigned" })),
            )
                .into_response();
        }
    };

    // Agent にデプロイコマンド送信
    let payload = json!({
        "project_slug": project,
        "stage": stage,
        "compose_path": format!("/opt/apps/{}/{}/docker-compose.yml", project, stage),
        "command": "up -d",
    });

    match state
        .app
        .agent_registry
        .send_command(&server_slug, "deploy", payload)
        .await
    {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "error": e }))).into_response(),
    }
}

/// サービス再起動（CP → Agent → docker restart）
async fn api_service_restart(
    State(state): State<Arc<WebState>>,
    Path((project, stage, service)): Path<(String, String, String)>,
    req: Request,
) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();

    // 認可チェック: owner/admin のみ（インフラ操作）
    if !ctx.can_operate() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Insufficient permissions" })),
        )
            .into_response();
    }

    // ステージのサーバーを特定
    let stages = match state.app.db.list_stage_overviews(&ctx.tenant_slug).await {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let target = stages
        .iter()
        .find(|s| s.project_slug == project && s.slug == stage);

    let server_slug = match target.and_then(|s| s.server_slug.as_deref()) {
        Some(s) => s.to_string(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Stage has no server assigned" })),
            )
                .into_response();
        }
    };

    let payload = json!({ "service": format!("{}-{}-{}", project, stage, service) });

    match state
        .app
        .agent_registry
        .send_command(&server_slug, "restart", payload)
        .await
    {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "error": e }))).into_response(),
    }
}

/// ステージのアクティブアラート一覧
async fn api_stage_alerts(
    State(state): State<Arc<WebState>>,
    Path((project, stage)): Path<(String, String)>,
    req: Request,
) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();

    // ステージのサーバーを特定
    let stages = match state.app.db.list_stage_overviews(&ctx.tenant_slug).await {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let target = stages
        .iter()
        .find(|s| s.project_slug == project && s.slug == stage);

    let server_slug = match target.and_then(|s| s.server_slug.as_deref()) {
        Some(s) => s.to_string(),
        None => {
            return (StatusCode::OK, Json(json!({ "alerts": [] }))).into_response();
        }
    };

    match state
        .app
        .db
        .list_active_alerts_by_server(&server_slug, &ctx.tenant_slug)
        .await
    {
        Ok(alerts) => {
            let items: Vec<Value> = alerts
                .iter()
                .map(|a| {
                    json!({
                        "container_name": a.container_name,
                        "alert_type": a.alert_type,
                        "severity": a.severity,
                        "message": a.message,
                        "created_at": a.created_at.map(|d| d.to_rfc3339()),
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "alerts": items }))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// 接続中の Agent 一覧（テナントスコープ）
async fn api_agents(State(state): State<Arc<WebState>>, req: Request) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();

    // テナントのサーバー一覧を取得してフィルタ
    let tenant_servers = match state.app.db.list_servers_by_tenant(&ctx.tenant_slug).await {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let tenant_slugs: Vec<&str> = tenant_servers.iter().map(|s| s.slug.as_str()).collect();

    let all_agents = state.app.agent_registry.list().await;
    let items: Vec<Value> = all_agents
        .iter()
        .filter(|(slug, _)| tenant_slugs.contains(&slug.as_str()))
        .map(|(slug, version)| json!({ "server_slug": slug, "version": version }))
        .collect();
    (StatusCode::OK, Json(json!({ "agents": items }))).into_response()
}

/// コンテナログ取得（スナップショット、テナント分離付き）
async fn api_container_logs(
    State(state): State<Arc<WebState>>,
    Path((project, stage, container)): Path<(String, String, String)>,
    req: Request,
) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();

    // テナント分離: ステージのサーバーを特定してから LogRouter を検索
    let stages = match state.app.db.list_stage_overviews(&ctx.tenant_slug).await {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let target = stages
        .iter()
        .find(|s| s.project_slug == project && s.slug == stage);

    let server_slug = match target.and_then(|s| s.server_slug.as_deref()) {
        Some(s) => s.to_string(),
        None => {
            return Json(json!({ "logs": [] })).into_response();
        }
    };

    // topic: logs/{server_slug}/{container_name}
    let topic_prefix = format!("logs/{}/{}", server_slug, container);
    let logs = state
        .app
        .log_router
        .get_recent(&topic_prefix, "info", 100)
        .await;

    let items: Vec<Value> = logs
        .iter()
        .map(|l| {
            json!({
                "timestamp": l.timestamp.to_rfc3339(),
                "container": l.container_name,
                "stream": l.stream,
                "level": l.level,
                "message": l.message,
            })
        })
        .collect();

    Json(json!({ "logs": items })).into_response()
}

// ============================================================================
// Tenant Users API
// ============================================================================

/// テナントユーザー一覧（owner/admin のみ）
async fn api_tenant_users(State(state): State<Arc<WebState>>, req: Request) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();

    if !ctx.can_manage_users() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Insufficient permissions" })),
        )
            .into_response();
    }

    match state.app.db.list_tenant_users(&ctx.tenant_slug).await {
        Ok(users) => {
            let items: Vec<Value> = users
                .iter()
                .map(|u| {
                    json!({
                        "auth0_sub": u.auth0_sub,
                        "role": u.role,
                        "created_at": u.created_at.map(|d| d.to_rfc3339()),
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "users": items }))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// テナントユーザー作成（owner/admin のみ）
async fn api_tenant_users_create(
    State(state): State<Arc<WebState>>,
    req: Request,
) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap().clone();

    if !ctx.can_manage_users() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Insufficient permissions" })),
        )
            .into_response();
    }

    // body を取得
    let body = match axum::body::to_bytes(req.into_body(), 1024 * 16).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Invalid request body" })),
            )
                .into_response();
        }
    };
    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Invalid JSON" })),
            )
                .into_response();
        }
    };

    let auth0_sub = match payload.get("auth0_sub").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "auth0_sub is required" })),
            )
                .into_response();
        }
    };
    let role_str = payload
        .get("role")
        .and_then(|v| v.as_str())
        .unwrap_or("member");

    // role バリデーション（owner は API 経由で作成不可）
    if !matches!(role_str, "admin" | "member") {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "role must be admin or member" })),
        )
            .into_response();
    }

    // テナント取得
    let tenant = match state.app.db.get_tenant_by_slug(&ctx.tenant_slug).await {
        Ok(Some(t)) => t,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Tenant not found" })),
            )
                .into_response();
        }
    };

    let tenant_id = match tenant.id {
        Some(id) => id,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "tenant has no id" })),
            )
                .into_response();
        }
    };

    let user = fleetflow_controlplane::model::TenantUser {
        id: None,
        auth0_sub,
        tenant: tenant_id,
        role: role_str.to_string(),
        created_at: None,
    };

    match state.app.db.create_tenant_user(&user).await {
        Ok(created) => (
            StatusCode::CREATED,
            Json(json!({
                "auth0_sub": created.auth0_sub,
                "role": created.role,
                "created_at": created.created_at.map(|d| d.to_rfc3339()),
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// テナントユーザー role 更新（owner/admin のみ、owner の role 変更は不可）
async fn api_tenant_users_update(
    State(state): State<Arc<WebState>>,
    Path(sub): Path<String>,
    req: Request,
) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap().clone();

    if !ctx.can_manage_users() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Insufficient permissions" })),
        )
            .into_response();
    }

    // 対象ユーザーの現在の role を確認（テナント境界チェック付き）
    let target = match state
        .app
        .db
        .resolve_tenant_user_scoped(&sub, &ctx.tenant_slug)
        .await
    {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "User not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    // owner の role 変更はブロック
    if target.tenant_role() == TenantRole::Owner {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Cannot change owner role" })),
        )
            .into_response();
    }

    // admin が admin を操作するのはブロック（owner のみ admin を操作可能）
    if ctx.role == TenantRole::Admin && target.tenant_role() == TenantRole::Admin {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Admin cannot modify another admin" })),
        )
            .into_response();
    }

    let body = match axum::body::to_bytes(req.into_body(), 1024 * 16).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Invalid request body" })),
            )
                .into_response();
        }
    };
    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Invalid JSON" })),
            )
                .into_response();
        }
    };

    let new_role = match payload.get("role").and_then(|v| v.as_str()) {
        Some(r) if matches!(r, "admin" | "member") => r,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "role must be admin or member" })),
            )
                .into_response();
        }
    };

    match state
        .app
        .db
        .update_tenant_user_role(&sub, new_role, &ctx.tenant_slug)
        .await
    {
        Ok(true) => (StatusCode::OK, Json(json!({ "updated": true }))).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "User not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// テナントユーザー削除（owner/admin のみ、owner は削除不可）
async fn api_tenant_users_delete(
    State(state): State<Arc<WebState>>,
    Path(sub): Path<String>,
    req: Request,
) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();

    if !ctx.can_manage_users() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Insufficient permissions" })),
        )
            .into_response();
    }

    // owner は削除不可（テナント境界チェック付き）
    let target = match state
        .app
        .db
        .resolve_tenant_user_scoped(&sub, &ctx.tenant_slug)
        .await
    {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "User not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    if target.tenant_role() == TenantRole::Owner {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Cannot delete owner" })),
        )
            .into_response();
    }

    // admin が admin を削除するのはブロック
    if ctx.role == TenantRole::Admin && target.tenant_role() == TenantRole::Admin {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Admin cannot delete another admin" })),
        )
            .into_response();
    }

    match state
        .app
        .db
        .delete_tenant_user(&sub, &ctx.tenant_slug)
        .await
    {
        Ok(true) => (StatusCode::OK, Json(json!({ "deleted": true }))).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "User not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

// ============================================================================
// Volume API (Persistence Volume Tier P-2, 2026-04-23)
//
// tenant の永続データを格納する disk 抽象を操作する endpoint 群。
// 詳細設計: fleetstage repo docs/design/20-persistence-volume-tier.md
// ============================================================================

/// GET /api/v1/volumes — tenant scoped volume 一覧
async fn api_volumes(State(state): State<Arc<WebState>>, req: Request) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();

    let tenant = match state.app.db.get_tenant_by_slug(&ctx.tenant_slug).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Tenant not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let tenant_id = tenant.id.expect("tenant.id should exist after fetch");
    match state.app.db.list_volumes_by_tenant(&tenant_id).await {
        Ok(volumes) => {
            let items: Vec<Value> = volumes
                .iter()
                .map(|v| {
                    json!({
                        "slug": v.slug,
                        "tier": v.tier,
                        "mount": v.mount,
                        "size_bytes": v.size_bytes,
                        "provider": v.provider,
                        "provider_resource_id": v.provider_resource_id,
                        "encryption": v.encryption,
                        "bring_your_own": v.bring_your_own,
                        "state": v.state,
                        "created_at": v.created_at.map(|d| d.to_rfc3339()),
                        "updated_at": v.updated_at.map(|d| d.to_rfc3339()),
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "volumes": items }))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// POST /api/v1/volumes/adopt — 既存 disk を BYO (bring-your-own) で registry 登録
///
/// リクエスト body:
/// ```json
/// {
///   "server": "creo-prod",
///   "slug": "surrealdb-legacy",
///   "mount": "/var/lib/surrealdb/prod",
///   "tier": "local-volume"
/// }
/// ```
///
/// データには一切触れない。fleetstage CP DB に record を作成するだけ。
async fn api_volume_adopt(State(state): State<Arc<WebState>>, req: Request) -> impl IntoResponse {
    let ctx = req
        .extensions()
        .get::<AuthContext>()
        .expect("AuthContext missing")
        .clone();

    // 認可チェック: owner/admin のみ (インフラ操作)
    if !ctx.can_operate() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Insufficient permissions" })),
        )
            .into_response();
    }

    let body = match axum::body::to_bytes(req.into_body(), 1024 * 16).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("invalid body: {}", e) })),
            )
                .into_response();
        }
    };
    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("invalid JSON: {}", e) })),
            )
                .into_response();
        }
    };

    let server_slug = match payload["server"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "`server` required" })),
            )
                .into_response();
        }
    };
    let slug = match payload["slug"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "`slug` required" })),
            )
                .into_response();
        }
    };
    let mount = match payload["mount"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "`mount` required" })),
            )
                .into_response();
        }
    };
    let tier = payload["tier"]
        .as_str()
        .unwrap_or(fleetflow_controlplane::model::volume_tier::LOCAL_VOLUME)
        .to_string();

    // tenant 解決
    let tenant = match state.app.db.get_tenant_by_slug(&ctx.tenant_slug).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Tenant not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };
    let tenant_id = tenant.id.expect("tenant.id should exist after fetch");

    // server 解決 (tenant 配下の server であることを確認)
    let server = match state.app.db.get_server_by_slug(&server_slug).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("Server `{}` not found", server_slug) })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };
    if server.tenant != tenant_id {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Server does not belong to this tenant" })),
        )
            .into_response();
    }
    let server_id = server.id.expect("server.id should exist after fetch");

    match state
        .app
        .db
        .adopt_volume(&tenant_id, &server_id, &slug, &mount, &tier)
        .await
    {
        Ok(volume) => (
            StatusCode::CREATED,
            Json(json!({
                "volume": {
                    "slug": volume.slug,
                    "tier": volume.tier,
                    "mount": volume.mount,
                    "bring_your_own": volume.bring_your_own,
                    "state": volume.state,
                    "server_slug": server_slug,
                }
            })),
        )
            .into_response(),
        Err(e) => {
            let msg = e.to_string();
            let status = if msg.contains("invalid volume tier") || msg.contains("invalid") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(json!({ "error": msg }))).into_response()
        }
    }
}

// ============================================================================
// Build API (Build Tier v1 MVP, 2026-04-23)
//
// tenant の build ジョブを操作する endpoint 群。
// 詳細設計: fleetstage repo docs/design/30-build-tier.md
// ============================================================================

/// GET /api/v1/builds — テナント配下の build job 一覧
async fn api_build_list(State(state): State<Arc<WebState>>, req: Request) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();

    let tenant = match state.app.db.get_tenant_by_slug(&ctx.tenant_slug).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Tenant not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let tenant_id = tenant.id.expect("tenant.id should exist after fetch");
    match state.app.db.list_build_jobs_by_tenant(&tenant_id).await {
        Ok(jobs) => {
            let items: Vec<Value> = jobs
                .iter()
                .map(|j| {
                    json!({
                        "id": j.id,
                        "kind": j.kind,
                        "state": j.state,
                        "git_url": j.source.git_url,
                        "git_ref": j.source.git_ref,
                        "dockerfile": j.source.dockerfile,
                        "image": j.target.image,
                        "logs_url": j.logs_url,
                        "submitted_at": j.submitted_at.map(|d| d.to_rfc3339()),
                        "started_at": j.started_at.map(|d| d.to_rfc3339()),
                        "finished_at": j.finished_at.map(|d| d.to_rfc3339()),
                        "duration_seconds": j.duration_seconds,
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "build_jobs": items }))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// POST /api/v1/builds — build job を submit (state=queued でエンキュー)
///
/// リクエスト body:
/// ```json
/// {
///   "git_url": "https://github.com/chronista-club/fleetflow",
///   "git_ref": "main",
///   "dockerfile": "infra/Dockerfile.fleetflowd",
///   "image": "ghcr.io/chronista-club/fleetflowd:latest",
///   "kind": "docker-image"
/// }
/// ```
async fn api_build_submit(State(state): State<Arc<WebState>>, req: Request) -> impl IntoResponse {
    use fleetflow_controlplane::model::{
        BuildJob, BuildSource, BuildTarget, build_job_kind, build_job_state,
    };

    let ctx = req
        .extensions()
        .get::<AuthContext>()
        .expect("AuthContext missing")
        .clone();

    // 認可チェック: owner/admin のみ (インフラ操作)
    if !ctx.can_operate() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Insufficient permissions" })),
        )
            .into_response();
    }

    let body = match axum::body::to_bytes(req.into_body(), 1024 * 16).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("invalid body: {}", e) })),
            )
                .into_response();
        }
    };
    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("invalid JSON: {}", e) })),
            )
                .into_response();
        }
    };

    let git_url = match payload["git_url"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "`git_url` required" })),
            )
                .into_response();
        }
    };

    let kind = payload["kind"]
        .as_str()
        .unwrap_or(build_job_kind::DOCKER_IMAGE);
    if !build_job_kind::is_valid(kind) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("invalid kind: {}", kind) })),
        )
            .into_response();
    }

    let tenant = match state.app.db.get_tenant_by_slug(&ctx.tenant_slug).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Tenant not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };
    let tenant_id = tenant.id.expect("tenant.id");

    let job = BuildJob {
        id: None,
        tenant: tenant_id,
        project: None,
        kind: kind.to_string(),
        source: BuildSource {
            git_url,
            git_ref: payload["git_ref"].as_str().unwrap_or("main").to_string(),
            dockerfile: payload["dockerfile"].as_str().map(|s| s.to_string()),
        },
        target: BuildTarget {
            image: payload["image"].as_str().map(|s| s.to_string()),
            registry_secret: payload["registry_secret"].as_str().map(|s| s.to_string()),
        },
        state: build_job_state::QUEUED.to_string(),
        server: None,
        logs_url: None,
        submitted_at: None,
        started_at: None,
        finished_at: None,
        duration_seconds: None,
    };

    match state.app.db.create_build_job(&job).await {
        Ok(created) => (
            StatusCode::CREATED,
            Json(json!({
                "build_job": {
                    "id": created.id,
                    "kind": created.kind,
                    "state": created.state,
                    "git_url": created.source.git_url,
                    "git_ref": created.source.git_ref,
                    "dockerfile": created.source.dockerfile,
                    "image": created.target.image,
                    "submitted_at": created.submitted_at.map(|d| d.to_rfc3339()),
                }
            })),
        )
            .into_response(),
        Err(e) => {
            let msg = e.to_string();
            let status = if msg.contains("invalid build job") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(json!({ "error": msg }))).into_response()
        }
    }
}

/// GET /api/v1/builds/{id} — build job 詳細取得
async fn api_build_show(
    State(state): State<Arc<WebState>>,
    Path(id): Path<String>,
    req: Request,
) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();

    let tenant = match state.app.db.get_tenant_by_slug(&ctx.tenant_slug).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Tenant not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };
    let tenant_id = tenant.id.expect("tenant.id");

    let record_id = fleetflow_controlplane::RecordId::new("build_job", id.as_str());
    match state.app.db.get_build_job_by_id(&record_id).await {
        Ok(Some(job)) => {
            if job.tenant != tenant_id {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": "Access denied" })),
                )
                    .into_response();
            }
            (
                StatusCode::OK,
                Json(json!({
                    "build_job": {
                        "id": job.id,
                        "kind": job.kind,
                        "state": job.state,
                        "git_url": job.source.git_url,
                        "git_ref": job.source.git_ref,
                        "dockerfile": job.source.dockerfile,
                        "image": job.target.image,
                        "logs_url": job.logs_url,
                        "submitted_at": job.submitted_at.map(|d| d.to_rfc3339()),
                        "started_at": job.started_at.map(|d| d.to_rfc3339()),
                        "finished_at": job.finished_at.map(|d| d.to_rfc3339()),
                        "duration_seconds": job.duration_seconds,
                    }
                })),
            )
                .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Build job not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// GET /api/v1/builds/{id}/logs — build ログ参照 (v1: polling、logs_url を返すのみ)
async fn api_build_logs(
    State(state): State<Arc<WebState>>,
    Path(id): Path<String>,
    req: Request,
) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();

    let tenant = match state.app.db.get_tenant_by_slug(&ctx.tenant_slug).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Tenant not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };
    let tenant_id = tenant.id.expect("tenant.id");

    let record_id = fleetflow_controlplane::RecordId::new("build_job", id.as_str());
    match state.app.db.get_build_job_by_id(&record_id).await {
        Ok(Some(job)) => {
            if job.tenant != tenant_id {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": "Access denied" })),
                )
                    .into_response();
            }
            // v1: logs_url を返すのみ (SSE stream は v2)
            (
                StatusCode::OK,
                Json(json!({
                    "state": job.state,
                    "logs_url": job.logs_url,
                    "note": "v1 は polling。logs_url が Some の場合はそこから取得してください。",
                })),
            )
                .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Build job not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// POST /api/v1/builds/{id}/cancel — build job をキャンセル
async fn api_build_cancel(
    State(state): State<Arc<WebState>>,
    Path(id): Path<String>,
    req: Request,
) -> impl IntoResponse {
    use fleetflow_controlplane::model::build_job_state;

    let ctx = req
        .extensions()
        .get::<AuthContext>()
        .expect("AuthContext missing")
        .clone();

    if !ctx.can_operate() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Insufficient permissions" })),
        )
            .into_response();
    }

    let tenant = match state.app.db.get_tenant_by_slug(&ctx.tenant_slug).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Tenant not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };
    let tenant_id = tenant.id.expect("tenant.id");

    let record_id = fleetflow_controlplane::RecordId::new("build_job", id.as_str());
    match state.app.db.get_build_job_by_id(&record_id).await {
        Ok(Some(job)) => {
            if job.tenant != tenant_id {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": "Access denied" })),
                )
                    .into_response();
            }
            if job.state == build_job_state::SUCCESS
                || job.state == build_job_state::FAILED
                || job.state == build_job_state::CANCELLED
            {
                return (
                    StatusCode::CONFLICT,
                    Json(json!({
                        "error": format!("cannot cancel job in state: {}", job.state)
                    })),
                )
                    .into_response();
            }
            match state
                .app
                .db
                .update_build_job_state(&record_id, build_job_state::CANCELLED)
                .await
            {
                Ok(()) => (StatusCode::OK, Json(json!({ "status": "cancelled" }))).into_response(),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
                    .into_response(),
            }
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Build job not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

// ============================================================================
// Stage Runtime Status + Service Restart v1 (FSC-17/18)
// ============================================================================

/// docker `Status` 文字列（例: "Up 3 days", "Up 2 hours"）から uptime を秒に変換する。
/// 停止中・exited などは None を返す。
pub fn uptime_from_docker_status(status: &str) -> Option<u64> {
    // "Up X unit" 形式のみ対象
    let lower = status.to_lowercase();
    if !lower.starts_with("up ") {
        return None;
    }
    let rest = lower.trim_start_matches("up ").trim();
    // "X unit" を split して数値と単位を取る
    let mut parts = rest.splitn(2, ' ');
    let num: u64 = parts.next()?.parse().ok()?;
    let unit = parts.next()?.trim().trim_end_matches('s'); // days → day, hours → hour
    let seconds = match unit {
        "second" => num,
        "minute" => num * 60,
        "hour" => num * 3600,
        "day" => num * 86400,
        "week" => num * 604800,
        "month" => num * 2592000, // 30-day approximation
        "year" => num * 31536000,
        _ => return None,
    };
    Some(seconds)
}

/// docker `State` 文字列をレスポンス用の state 文字列に正規化する。
pub fn state_from_docker_state(docker_state: &str) -> &'static str {
    match docker_state.to_lowercase().as_str() {
        "running" => "running",
        "restarting" => "restarting",
        "paused" => "stopped",
        "exited" => "stopped",
        "dead" => "stopped",
        "created" => "stopped",
        "removing" => "stopped",
        _ => "unknown",
    }
}

/// docker container name（例: "gfp-dev-gfp-web"）から service 名を逆引きする。
/// フォーマット: `{project}-{stage}-{service_slug}` だが service_slug 自体に `-` を含む場合がある。
/// → project と stage prefix を取り除く。
pub fn container_name_to_service<'a>(
    container_name: &'a str,
    project: &str,
    stage: &str,
) -> Option<&'a str> {
    let prefix = format!("{}-{}-", project, stage);
    container_name.strip_prefix(&prefix)
}

/// GET /api/v1/stages/{project}/{stage}/status — ステージの実 runtime status
async fn api_v1_stage_status(
    State(state): State<Arc<WebState>>,
    Path((project, stage)): Path<(String, String)>,
    req: Request,
) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();

    // ステージ一覧から対象を特定（tenant scope 検証を兼ねる）
    let stages = match state.app.db.list_stage_overviews(&ctx.tenant_slug).await {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let target = stages
        .iter()
        .find(|s| s.project_slug == project && s.slug == stage);

    let server_slug = match target.and_then(|s| s.server_slug.as_deref()) {
        Some(s) => s.to_string(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Stage has no server assigned" })),
            )
                .into_response();
        }
    };

    // CP に登録されているサービス一覧を取得（期待するサービス名の基準）
    let registered_services = match state
        .app
        .db
        .list_services_by_project_stage(&ctx.tenant_slug, &project, &stage)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    // Agent から docker ps 結果を取得
    let agent_result = state
        .app
        .agent_registry
        .send_command(&server_slug, "status", json!({}))
        .await;

    let containers_json = match agent_result {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": format!("agent unreachable: {}", e) })),
            )
                .into_response();
        }
    };

    // docker ps の結果をパース: containers は配列
    let containers = containers_json["containers"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    // 登録サービスごとに、対応するコンテナを検索して status を組み立てる
    let services: Vec<Value> = registered_services
        .iter()
        .map(|svc| {
            // docker container の Names フィールドから service 名を逆引きして突合
            let found = containers.iter().find(|c| {
                c["Names"]
                    .as_str()
                    .map(|n| {
                        let bare = n.trim_start_matches('/');
                        container_name_to_service(bare, &project, &stage)
                            .map(|s| s == svc.slug)
                            .unwrap_or(false)
                    })
                    .unwrap_or(false)
            });
            match found {
                Some(c) => {
                    let docker_state = c["State"].as_str().unwrap_or("unknown");
                    let state_str = state_from_docker_state(docker_state);
                    let uptime = c["Status"]
                        .as_str()
                        .and_then(uptime_from_docker_status)
                        .map(|s| json!(s))
                        .unwrap_or(Value::Null);
                    json!({
                        "name": svc.slug,
                        "state": state_str,
                        "uptime_seconds": uptime,
                        "image": c["Image"].as_str(),
                        "container_id": c["ID"].as_str(),
                    })
                }
                None => {
                    json!({
                        "name": svc.slug,
                        "state": "stopped",
                        "uptime_seconds": Value::Null,
                        "image": Value::Null,
                        "container_id": Value::Null,
                    })
                }
            }
        })
        .collect();

    (
        StatusCode::OK,
        Json(json!({
            "project": project,
            "stage": stage,
            "server": server_slug,
            "services": services,
        })),
    )
        .into_response()
}

/// POST /api/v1/stages/{project}/{stage}/services/{service}/restart — サービス再起動 v1 path
///
/// 既存 `/api/stages/{p}/{s}/restart/{service}` の REST-idiomatic 版。
/// 旧 path は deprecate せず残す。
async fn api_v1_service_restart(
    State(state): State<Arc<WebState>>,
    Path((project, stage, service)): Path<(String, String, String)>,
    req: Request,
) -> impl IntoResponse {
    let ctx = req.extensions().get::<AuthContext>().unwrap();

    // 認可チェック: owner/admin のみ（インフラ操作）
    if !ctx.can_operate() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Insufficient permissions" })),
        )
            .into_response();
    }

    // ステージのサーバーを特定
    let stages = match state.app.db.list_stage_overviews(&ctx.tenant_slug).await {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let target = stages
        .iter()
        .find(|s| s.project_slug == project && s.slug == stage);

    let server_slug = match target.and_then(|s| s.server_slug.as_deref()) {
        Some(s) => s.to_string(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Stage has no server assigned" })),
            )
                .into_response();
        }
    };

    let payload = json!({ "service": format!("{}-{}-{}", project, stage, service) });

    match state
        .app
        .agent_registry
        .send_command(&server_slug, "restart", payload)
        .await
    {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({ "error": e }))).into_response(),
    }
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

// ============================================================================
// Unit tests (FSC-17 純粋ヘルパー)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- uptime_from_docker_status ---

    #[test]
    fn uptime_from_docker_status_days() {
        assert_eq!(uptime_from_docker_status("Up 3 days"), Some(259200));
    }

    #[test]
    fn uptime_from_docker_status_hours() {
        assert_eq!(uptime_from_docker_status("Up 2 hours"), Some(7200));
    }

    #[test]
    fn uptime_from_docker_status_minutes() {
        assert_eq!(uptime_from_docker_status("Up 5 minutes"), Some(300));
    }

    #[test]
    fn uptime_from_docker_status_seconds() {
        assert_eq!(uptime_from_docker_status("Up 45 seconds"), Some(45));
    }

    #[test]
    fn uptime_from_docker_status_exited_returns_none() {
        assert_eq!(uptime_from_docker_status("Exited (0) 1 day ago"), None);
    }

    #[test]
    fn uptime_from_docker_status_stopped_returns_none() {
        assert_eq!(uptime_from_docker_status(""), None);
    }

    // --- state_from_docker_state ---

    #[test]
    fn state_from_docker_state_running() {
        assert_eq!(state_from_docker_state("running"), "running");
    }

    #[test]
    fn state_from_docker_state_exited() {
        assert_eq!(state_from_docker_state("exited"), "stopped");
    }

    #[test]
    fn state_from_docker_state_restarting() {
        assert_eq!(state_from_docker_state("restarting"), "restarting");
    }

    #[test]
    fn state_from_docker_state_unknown() {
        assert_eq!(state_from_docker_state("dead"), "stopped");
    }

    // --- container_name_to_service ---

    #[test]
    fn container_name_to_service_basic() {
        assert_eq!(
            container_name_to_service("gfp-dev-gfp-web", "gfp", "dev"),
            Some("gfp-web")
        );
    }

    #[test]
    fn container_name_to_service_no_match() {
        assert_eq!(
            container_name_to_service("other-prod-svc", "gfp", "dev"),
            None
        );
    }

    #[test]
    fn container_name_to_service_hyphen_in_service_name() {
        assert_eq!(
            container_name_to_service("gfp-dev-gfp-estimate-worker", "gfp", "dev"),
            Some("gfp-estimate-worker")
        );
    }
}
