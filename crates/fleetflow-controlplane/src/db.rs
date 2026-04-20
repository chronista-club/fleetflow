use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::types::RecordId;
use tracing::info;

use crate::model::*;

/// SurrealDB connection and schema management for Control Plane.
pub struct Database {
    db: Surreal<Any>,
}

/// Connection configuration.
#[derive(Debug)]
pub struct DbConfig {
    pub endpoint: String,
    pub namespace: String,
    pub database: String,
    pub username: String,
    pub password: String,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            endpoint: "ws://127.0.0.1:12000".into(),
            namespace: "fleetflow".into(),
            database: "control_plane".into(),
            username: "fleetflow-api".into(),
            password: String::new(),
        }
    }
}

impl Database {
    /// Connect to SurrealDB and apply schema.
    ///
    /// `mem://` エンドポイントの場合は認証をスキップ（インメモリDB）。
    pub async fn connect(config: &DbConfig) -> Result<Self> {
        let db = surrealdb::engine::any::connect(&config.endpoint)
            .await
            .context("SurrealDB 接続失敗")?;

        // mem:// (インメモリ) では認証不要
        if !config.endpoint.starts_with("mem://") {
            db.signin(surrealdb::opt::auth::Root {
                username: config.username.clone(),
                password: config.password.clone(),
            })
            .await
            .context("SurrealDB 認証失敗")?;
        }

        db.use_ns(&config.namespace)
            .use_db(&config.database)
            .await
            .context("namespace/database 選択失敗")?;

        let database = Self { db };
        database.apply_schema().await?;

        info!(
            endpoint = %config.endpoint,
            namespace = %config.namespace,
            database = %config.database,
            "SurrealDB 接続完了"
        );

        Ok(database)
    }

    /// Connect to in-memory SurrealDB for testing.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn connect_memory() -> Result<Self> {
        let db = surrealdb::engine::any::connect("mem://")
            .await
            .context("SurrealDB in-memory 接続失敗")?;

        db.use_ns("test").use_db("test").await?;

        let database = Self { db };
        database.apply_schema().await?;
        Ok(database)
    }

    /// Apply schema definitions to the database.
    async fn apply_schema(&self) -> Result<()> {
        self.db
            .query(SCHEMA_SQL)
            .await
            .context("スキーマ適用失敗")?;
        Ok(())
    }

    // ─────────────────────────────────────────
    // Tenant CRUD
    // ─────────────────────────────────────────

    pub async fn create_tenant(&self, tenant: &Tenant) -> Result<Tenant> {
        let mut result = self
            .db
            .query("CREATE tenant CONTENT { slug: $slug, name: $name, auth0_org_id: $auth0_org_id, plan: $plan }")
            .bind(("slug", tenant.slug.clone()))
            .bind(("name", tenant.name.clone()))
            .bind(("auth0_org_id", tenant.auth0_org_id.clone()))
            .bind(("plan", tenant.plan.clone()))
            .await
            .context("テナント作成失敗")?;
        let created: Option<Tenant> = result.take(0)?;
        created.context("テナント作成結果が空")
    }

    pub async fn get_tenant_by_id(&self, id: &RecordId) -> Result<Option<Tenant>> {
        let mut result = self
            .db
            .query("SELECT * FROM $id")
            .bind(("id", id.clone()))
            .await
            .context("テナント ID 取得失敗")?;
        let tenants: Vec<Tenant> = result.take(0)?;
        Ok(tenants.into_iter().next())
    }

    pub async fn get_tenant_by_slug(&self, slug: &str) -> Result<Option<Tenant>> {
        let mut result = self
            .db
            .query("SELECT * FROM tenant WHERE slug = $slug LIMIT 1")
            .bind(("slug", slug.to_string()))
            .await
            .context("テナント取得失敗")?;
        let tenants: Vec<Tenant> = result.take(0)?;
        Ok(tenants.into_iter().next())
    }

    pub async fn list_tenants(&self) -> Result<Vec<Tenant>> {
        let mut result = self
            .db
            .query("SELECT * FROM tenant ORDER BY slug")
            .await
            .context("テナント一覧取得失敗")?;
        let tenants: Vec<Tenant> = result.take(0)?;
        Ok(tenants)
    }

    // ─────────────────────────────────────────
    // TenantUser CRUD
    // ─────────────────────────────────────────

    /// Auth0 sub からテナントを解決する（auth middleware 用）
    pub async fn resolve_tenant_by_sub(&self, auth0_sub: &str) -> Result<Option<TenantUser>> {
        let mut result = self
            .db
            .query("SELECT * FROM tenant_user WHERE auth0_sub = $sub LIMIT 1")
            .bind(("sub", auth0_sub.to_string()))
            .await
            .context("テナントユーザー解決失敗")?;
        let users: Vec<TenantUser> = result.take(0)?;
        Ok(users.into_iter().next())
    }

    /// Auth0 sub + テナント slug でテナントユーザーを取得（テナント境界チェック付き）
    pub async fn resolve_tenant_user_scoped(
        &self,
        auth0_sub: &str,
        tenant_slug: &str,
    ) -> Result<Option<TenantUser>> {
        let mut result = self
            .db
            .query("SELECT * FROM tenant_user WHERE auth0_sub = $sub AND tenant.slug = $tenant_slug LIMIT 1")
            .bind(("sub", auth0_sub.to_string()))
            .bind(("tenant_slug", tenant_slug.to_string()))
            .await
            .context("テナントユーザー取得失敗（スコープ付き）")?;
        let users: Vec<TenantUser> = result.take(0)?;
        Ok(users.into_iter().next())
    }

    /// テナントユーザーを作成
    pub async fn create_tenant_user(&self, user: &TenantUser) -> Result<TenantUser> {
        let mut result = self
            .db
            .query("CREATE tenant_user CONTENT { auth0_sub: $auth0_sub, tenant: $tenant, role: $role }")
            .bind(("auth0_sub", user.auth0_sub.clone()))
            .bind(("tenant", user.tenant.clone()))
            .bind(("role", user.role.to_string()))
            .await
            .context("テナントユーザー作成失敗")?;
        let created: Option<TenantUser> = result.take(0)?;
        created.context("テナントユーザー作成結果が空")
    }

    /// テナントのユーザー一覧を取得
    pub async fn list_tenant_users(&self, tenant_slug: &str) -> Result<Vec<TenantUser>> {
        let mut result = self
            .db
            .query("SELECT * FROM tenant_user WHERE tenant.slug = $tenant_slug ORDER BY role, auth0_sub")
            .bind(("tenant_slug", tenant_slug.to_string()))
            .await
            .context("テナントユーザー一覧取得失敗")?;
        let users: Vec<TenantUser> = result.take(0)?;
        Ok(users)
    }

    /// テナントユーザーの role を更新（テナント境界チェック付き）
    pub async fn update_tenant_user_role(
        &self,
        auth0_sub: &str,
        new_role: &str,
        tenant_slug: &str,
    ) -> Result<bool> {
        let mut result = self
            .db
            .query("UPDATE tenant_user SET role = $role WHERE auth0_sub = $sub AND tenant.slug = $tenant_slug RETURN AFTER")
            .bind(("sub", auth0_sub.to_string()))
            .bind(("role", new_role.to_string()))
            .bind(("tenant_slug", tenant_slug.to_string()))
            .await
            .context("テナントユーザー role 更新失敗")?;
        let updated: Vec<TenantUser> = result.take(0)?;
        Ok(!updated.is_empty())
    }

    /// テナントユーザーを削除（テナント境界チェック付き）
    pub async fn delete_tenant_user(&self, auth0_sub: &str, tenant_slug: &str) -> Result<bool> {
        let mut result = self
            .db
            .query("DELETE FROM tenant_user WHERE auth0_sub = $sub AND tenant.slug = $tenant_slug RETURN BEFORE")
            .bind(("sub", auth0_sub.to_string()))
            .bind(("tenant_slug", tenant_slug.to_string()))
            .await
            .context("テナントユーザー削除失敗")?;
        let deleted: Vec<TenantUser> = result.take(0)?;
        Ok(!deleted.is_empty())
    }

    // ─────────────────────────────────────────
    // Project CRUD
    // ─────────────────────────────────────────

    pub async fn create_project(&self, project: &Project) -> Result<Project> {
        let mut result = self
            .db
            .query("CREATE project CONTENT { tenant: $tenant, slug: $slug, name: $name, description: $description, repository_url: $repository_url }")
            .bind(("tenant", project.tenant.clone()))
            .bind(("slug", project.slug.clone()))
            .bind(("name", project.name.clone()))
            .bind(("description", project.description.clone()))
            .bind(("repository_url", project.repository_url.clone()))
            .await
            .context("プロジェクト作成失敗")?;
        let created: Option<Project> = result.take(0)?;
        created.context("プロジェクト作成結果が空")
    }

    pub async fn get_project_by_slug(
        &self,
        tenant_slug: &str,
        project_slug: &str,
    ) -> Result<Option<Project>> {
        let mut result = self
            .db
            .query(
                "SELECT * FROM project WHERE slug = $project_slug AND tenant.slug = $tenant_slug LIMIT 1",
            )
            .bind(("tenant_slug", tenant_slug.to_string()))
            .bind(("project_slug", project_slug.to_string()))
            .await
            .context("プロジェクト取得失敗")?;
        let projects: Vec<Project> = result.take(0)?;
        Ok(projects.into_iter().next())
    }

    pub async fn list_projects_by_tenant(&self, tenant_slug: &str) -> Result<Vec<Project>> {
        let mut result = self
            .db
            .query("SELECT * FROM project WHERE tenant.slug = $tenant_slug ORDER BY slug")
            .bind(("tenant_slug", tenant_slug.to_string()))
            .await
            .context("プロジェクト一覧取得失敗")?;
        let projects: Vec<Project> = result.take(0)?;
        Ok(projects)
    }

    pub async fn delete_project(&self, tenant_slug: &str, project_slug: &str) -> Result<bool> {
        self.db
            .query("DELETE FROM project WHERE slug = $project_slug AND tenant.slug = $tenant_slug")
            .bind(("tenant_slug", tenant_slug.to_string()))
            .bind(("project_slug", project_slug.to_string()))
            .await
            .context("プロジェクト削除失敗")?;
        Ok(true)
    }

    // ─────────────────────────────────────────
    // Stage CRUD
    // ─────────────────────────────────────────

    pub async fn create_stage(&self, stage: &Stage) -> Result<Stage> {
        let mut result = self
            .db
            .query("CREATE stage CONTENT { project: $project, slug: $slug, description: $description, server: $server }")
            .bind(("project", stage.project.clone()))
            .bind(("slug", stage.slug.clone()))
            .bind(("description", stage.description.clone()))
            .bind(("server", stage.server.clone()))
            .await
            .context("ステージ作成失敗")?;
        let created: Option<Stage> = result.take(0)?;
        created.context("ステージ作成結果が空")
    }

    pub async fn list_stages_by_project(&self, project_id: &RecordId) -> Result<Vec<Stage>> {
        let mut result = self
            .db
            .query("SELECT * FROM stage WHERE project = $project_id ORDER BY slug")
            .bind(("project_id", project_id.clone()))
            .await
            .context("ステージ一覧取得失敗")?;
        let stages: Vec<Stage> = result.take(0)?;
        Ok(stages)
    }

    /// List all stages across all projects in a tenant (for dashboard overview)
    pub async fn list_all_stages_by_tenant(
        &self,
        tenant_slug: &str,
    ) -> Result<Vec<StageWithProject>> {
        let mut result = self
            .db
            .query(
                "SELECT id, slug, description, project.slug AS project_slug, project.name AS project_name, project.tenant.slug AS tenant_slug FROM stage WHERE project.tenant.slug = $tenant_slug ORDER BY project_slug, slug",
            )
            .bind(("tenant_slug", tenant_slug.to_string()))
            .await
            .context("テナント全ステージ取得失敗")?;
        let stages: Vec<StageWithProject> = result.take(0)?;
        Ok(stages)
    }

    /// Cross-project query: find stages with same slug across all projects
    pub async fn list_stages_across_projects(
        &self,
        tenant_slug: &str,
        stage_slug: &str,
    ) -> Result<Vec<StageWithProject>> {
        let mut result = self
            .db
            .query(
                "SELECT id, slug, description, project.slug AS project_slug, project.name AS project_name, project.tenant.slug AS tenant_slug FROM stage WHERE slug = $stage_slug AND project.tenant.slug = $tenant_slug ORDER BY project_slug",
            )
            .bind(("tenant_slug", tenant_slug.to_string()))
            .bind(("stage_slug", stage_slug.to_string()))
            .await
            .context("横断ステージ取得失敗")?;
        let stages: Vec<StageWithProject> = result.take(0)?;
        Ok(stages)
    }

    /// 1st ビュー用: テナントの全ステージ概要（サーバー・直近デプロイ付き）
    pub async fn list_stage_overviews(&self, tenant_slug: &str) -> Result<Vec<StageOverview>> {
        let mut result = self
            .db
            .query(
                r#"
                SELECT
                    id,
                    slug,
                    description,
                    project.slug AS project_slug,
                    project.name AS project_name,
                    server.slug AS server_slug,
                    server.status AS server_status,
                    server.last_heartbeat_at AS server_heartbeat,
                    (SELECT status, created_at FROM deployment WHERE project = $parent.project AND stage = $parent.slug ORDER BY created_at DESC LIMIT 1)[0].status AS last_deploy_status,
                    (SELECT status, created_at FROM deployment WHERE project = $parent.project AND stage = $parent.slug ORDER BY created_at DESC LIMIT 1)[0].created_at AS last_deploy_at,
                    (SELECT count() FROM alert WHERE server_slug = $parent.server.slug AND resolved = false GROUP ALL)[0].count AS alert_count
                FROM stage
                WHERE project.tenant.slug = $tenant_slug
                ORDER BY project_slug, slug
                "#,
            )
            .bind(("tenant_slug", tenant_slug.to_string()))
            .await
            .context("ステージ概要取得失敗")?;
        let stages: Vec<StageOverview> = result.take(0)?;
        Ok(stages)
    }

    // ─────────────────────────────────────────
    // Service CRUD
    // ─────────────────────────────────────────

    pub async fn create_service(&self, service: &Service) -> Result<Service> {
        let mut result = self
            .db
            .query("CREATE service CONTENT { stage: $stage, slug: $slug, image: $image, config: $config, desired_status: $desired_status }")
            .bind(("stage", service.stage.clone()))
            .bind(("slug", service.slug.clone()))
            .bind(("image", service.image.clone()))
            .bind(("config", service.config.clone()))
            .bind(("desired_status", service.desired_status.clone()))
            .await
            .context("サービス作成失敗")?;
        let created: Option<Service> = result.take(0)?;
        created.context("サービス作成結果が空")
    }

    pub async fn list_services_by_stage(&self, stage_id: &RecordId) -> Result<Vec<Service>> {
        let mut result = self
            .db
            .query("SELECT * FROM service WHERE stage = $stage_id ORDER BY slug")
            .bind(("stage_id", stage_id.clone()))
            .await
            .context("サービス一覧取得失敗")?;
        let services: Vec<Service> = result.take(0)?;
        Ok(services)
    }

    /// 展開ビュー用: プロジェクト slug + ステージ slug からサービス一覧を取得
    pub async fn list_services_by_project_stage(
        &self,
        tenant_slug: &str,
        project_slug: &str,
        stage_slug: &str,
    ) -> Result<Vec<Service>> {
        let mut result = self
            .db
            .query(
                r#"
                SELECT * FROM service
                WHERE stage.slug = $stage_slug
                  AND stage.project.slug = $project_slug
                  AND stage.project.tenant.slug = $tenant_slug
                ORDER BY slug
                "#,
            )
            .bind(("tenant_slug", tenant_slug.to_string()))
            .bind(("project_slug", project_slug.to_string()))
            .bind(("stage_slug", stage_slug.to_string()))
            .await
            .context("ステージサービス一覧取得失敗")?;
        let services: Vec<Service> = result.take(0)?;
        Ok(services)
    }

    /// 展開ビュー用: プロジェクト slug + ステージ slug からデプロイ履歴を取得
    pub async fn list_deployments_by_project_stage(
        &self,
        tenant_slug: &str,
        project_slug: &str,
        stage_slug: &str,
        limit: usize,
    ) -> Result<Vec<Deployment>> {
        let mut result = self
            .db
            .query(
                r#"
                SELECT * FROM deployment
                WHERE tenant.slug = $tenant_slug
                  AND project.slug = $project_slug
                  AND stage = $stage_slug
                ORDER BY created_at DESC
                LIMIT $limit
                "#,
            )
            .bind(("tenant_slug", tenant_slug.to_string()))
            .bind(("project_slug", project_slug.to_string()))
            .bind(("stage_slug", stage_slug.to_string()))
            .bind(("limit", limit as i64))
            .await
            .context("ステージデプロイ履歴取得失敗")?;
        let deployments: Vec<Deployment> = result.take(0)?;
        Ok(deployments)
    }

    /// デプロイログ取得（ID 文字列で検索、テナント境界チェック付き）
    pub async fn get_deployment_log(
        &self,
        id_key: &str,
        tenant_slug: &str,
    ) -> Result<Option<Deployment>> {
        let record_id = RecordId::new("deployment", id_key);
        let mut result = self
            .db
            .query("SELECT * FROM $id WHERE tenant.slug = $tenant_slug LIMIT 1")
            .bind(("id", record_id))
            .bind(("tenant_slug", tenant_slug.to_string()))
            .await
            .context("デプロイログ取得失敗")?;
        let deployments: Vec<Deployment> = result.take(0)?;
        Ok(deployments.into_iter().next())
    }

    // ─────────────────────────────────────────
    // Server CRUD
    // ─────────────────────────────────────────

    pub async fn register_server(&self, server: &Server) -> Result<Server> {
        let mut result = self
            .db
            .query("CREATE server CONTENT { tenant: $tenant, slug: $slug, provider: $provider, plan: $plan, ssh_host: $ssh_host, ssh_user: $ssh_user, deploy_path: $deploy_path, status: $status }")
            .bind(("tenant", server.tenant.clone()))
            .bind(("slug", server.slug.clone()))
            .bind(("provider", server.provider.clone()))
            .bind(("plan", server.plan.clone()))
            .bind(("ssh_host", server.ssh_host.clone()))
            .bind(("ssh_user", server.ssh_user.clone()))
            .bind(("deploy_path", server.deploy_path.clone()))
            .bind(("status", server.status.clone()))
            .await
            .context("サーバー登録失敗")?;
        let created: Option<Server> = result.take(0)?;
        created.context("サーバー登録結果が空")
    }

    pub async fn list_servers_by_tenant(&self, tenant_slug: &str) -> Result<Vec<Server>> {
        let mut result = self
            .db
            .query("SELECT * FROM server WHERE tenant.slug = $tenant_slug ORDER BY slug")
            .bind(("tenant_slug", tenant_slug.to_string()))
            .await
            .context("サーバー一覧取得失敗")?;
        let servers: Vec<Server> = result.take(0)?;
        Ok(servers)
    }

    pub async fn update_server_heartbeat(&self, server_slug: &str) -> Result<()> {
        self.db
            .query("UPDATE server SET last_heartbeat_at = time::now(), status = 'online', updated_at = time::now() WHERE slug = $slug")
            .bind(("slug", server_slug.to_string()))
            .await
            .context("ハートビート更新失敗")?;
        Ok(())
    }

    /// ハートビート + バージョン情報更新
    pub async fn update_server_versions(
        &self,
        server_slug: &str,
        provision_version: Option<&str>,
        tool_versions: Option<&serde_json::Value>,
    ) -> Result<()> {
        self.db
            .query("UPDATE server SET last_heartbeat_at = time::now(), status = 'online', provision_version = $pv, tool_versions = $tv, updated_at = time::now() WHERE slug = $slug")
            .bind(("slug", server_slug.to_string()))
            .bind(("pv", provision_version.map(String::from)))
            .bind(("tv", tool_versions.cloned()))
            .await
            .context("バージョン情報更新失敗")?;
        Ok(())
    }

    /// サーバーステータスを一括更新（Tailscale ヘルスチェック結果を反映）
    pub async fn bulk_update_server_status(&self, updates: &[ServerStatusUpdate]) -> Result<usize> {
        let mut updated = 0;
        for u in updates {
            let mut result = self
                .db
                .query("UPDATE server SET status = $status, last_heartbeat_at = $heartbeat, updated_at = time::now() WHERE slug = $slug")
                .bind(("slug", u.slug.clone()))
                .bind(("status", u.status.clone()))
                .bind(("heartbeat", u.last_heartbeat_at))
                .await
                .context("サーバーステータス一括更新失敗")?;
            let rows: Vec<Server> = result.take(0)?;
            if !rows.is_empty() {
                updated += 1;
            }
        }
        Ok(updated)
    }

    /// 全テナントのサーバー一覧を取得
    pub async fn list_all_servers(&self) -> Result<Vec<Server>> {
        let mut result = self
            .db
            .query("SELECT * FROM server ORDER BY slug")
            .await
            .context("全サーバー一覧取得失敗")?;
        let servers: Vec<Server> = result.take(0)?;
        Ok(servers)
    }

    /// slug でサーバーを取得
    pub async fn get_server_by_slug(&self, slug: &str) -> Result<Option<Server>> {
        let mut result = self
            .db
            .query("SELECT * FROM server WHERE slug = $slug LIMIT 1")
            .bind(("slug", slug.to_string()))
            .await
            .context("サーバー取得失敗")?;
        let server: Option<Server> = result.take(0)?;
        Ok(server)
    }

    /// サーバーを DB から削除
    pub async fn delete_server(&self, slug: &str) -> Result<()> {
        self.db
            .query("DELETE FROM server WHERE slug = $slug")
            .bind(("slug", slug.to_string()))
            .await
            .context("サーバー削除失敗")?;
        Ok(())
    }

    // ─────────────────────────────────────────
    // CostEntry CRUD
    // ─────────────────────────────────────────

    pub async fn create_cost_entry(&self, entry: &CostEntry) -> Result<CostEntry> {
        let mut result = self
            .db
            .query("CREATE cost_entry CONTENT { tenant: $tenant, project: $project, stage: $stage, provider: $provider, description: $description, amount_jpy: $amount_jpy, month: $month }")
            .bind(("tenant", entry.tenant.clone()))
            .bind(("project", entry.project.clone()))
            .bind(("stage", entry.stage.clone()))
            .bind(("provider", entry.provider.clone()))
            .bind(("description", entry.description.clone()))
            .bind(("amount_jpy", entry.amount_jpy))
            .bind(("month", entry.month.clone()))
            .await
            .context("コストエントリ作成失敗")?;
        let created: Option<CostEntry> = result.take(0)?;
        created.context("コストエントリ作成結果が空")
    }

    pub async fn list_costs_by_month(
        &self,
        tenant_slug: &str,
        month: &str,
    ) -> Result<Vec<CostEntry>> {
        let mut result = self
            .db
            .query("SELECT * FROM cost_entry WHERE tenant.slug = $tenant_slug AND month = $month ORDER BY provider, amount_jpy DESC")
            .bind(("tenant_slug", tenant_slug.to_string()))
            .bind(("month", month.to_string()))
            .await
            .context("月次コスト取得失敗")?;
        let entries: Vec<CostEntry> = result.take(0)?;
        Ok(entries)
    }

    pub async fn summarize_costs_by_month(
        &self,
        tenant_slug: &str,
        month: &str,
    ) -> Result<Vec<MonthlyCostSummary>> {
        let mut result = self
            .db
            .query("SELECT month, provider, project.slug AS project_slug, math::sum(amount_jpy) AS total_jpy FROM cost_entry WHERE tenant.slug = $tenant_slug AND month = $month GROUP BY month, provider, project ORDER BY provider")
            .bind(("tenant_slug", tenant_slug.to_string()))
            .bind(("month", month.to_string()))
            .await
            .context("コスト集計失敗")?;
        let summaries: Vec<MonthlyCostSummary> = result.take(0)?;
        Ok(summaries)
    }

    // ─────────────────────────────────────────
    // DnsRecord CRUD
    // ─────────────────────────────────────────

    pub async fn create_dns_record(&self, record: &DnsRecord) -> Result<DnsRecord> {
        let mut result = self
            .db
            .query("CREATE dns_record CONTENT { tenant: $tenant, project: $project, name: $name, record_type: $record_type, content: $content, zone_id: $zone_id, cf_record_id: $cf_record_id, proxied: $proxied }")
            .bind(("tenant", record.tenant.clone()))
            .bind(("project", record.project.clone()))
            .bind(("name", record.name.clone()))
            .bind(("record_type", record.record_type.clone()))
            .bind(("content", record.content.clone()))
            .bind(("zone_id", record.zone_id.clone()))
            .bind(("cf_record_id", record.cf_record_id.clone()))
            .bind(("proxied", record.proxied))
            .await
            .context("DNSレコード作成失敗")?;
        let created: Option<DnsRecord> = result.take(0)?;
        created.context("DNSレコード作成結果が空")
    }

    pub async fn list_dns_records_by_tenant(&self, tenant_slug: &str) -> Result<Vec<DnsRecord>> {
        let mut result = self
            .db
            .query("SELECT * FROM dns_record WHERE tenant.slug = $tenant_slug ORDER BY name")
            .bind(("tenant_slug", tenant_slug.to_string()))
            .await
            .context("DNSレコード一覧取得失敗")?;
        let records: Vec<DnsRecord> = result.take(0)?;
        Ok(records)
    }

    pub async fn delete_dns_record(&self, record_name: &str) -> Result<bool> {
        let mut result = self
            .db
            .query("DELETE FROM dns_record WHERE name = $name RETURN BEFORE")
            .bind(("name", record_name.to_string()))
            .await
            .context("DNSレコード削除失敗")?;
        let deleted: Vec<DnsRecord> = result.take(0)?;
        Ok(!deleted.is_empty())
    }

    // ─────────────────────────────────────────
    // Deployment CRUD
    // ─────────────────────────────────────────

    pub async fn create_deployment(&self, deploy: &Deployment) -> Result<Deployment> {
        let mut result = self
            .db
            .query("CREATE deployment CONTENT { tenant: $tenant, project: $project, stage: $stage, server_slug: $server_slug, status: $status, command: $command, log: $log, started_at: $started_at, finished_at: $finished_at }")
            .bind(("tenant", deploy.tenant.clone()))
            .bind(("project", deploy.project.clone()))
            .bind(("stage", deploy.stage.clone()))
            .bind(("server_slug", deploy.server_slug.clone()))
            .bind(("status", deploy.status.clone()))
            .bind(("command", deploy.command.clone()))
            .bind(("log", deploy.log.clone()))
            .bind(("started_at", deploy.started_at))
            .bind(("finished_at", deploy.finished_at))
            .await
            .context("デプロイ記録作成失敗")?;
        let created: Option<Deployment> = result.take(0)?;
        created.context("デプロイ記録作成結果が空")
    }

    pub async fn update_deployment_status(
        &self,
        id: &RecordId,
        status: &str,
        log: Option<&str>,
        finished_at: Option<DateTime<Utc>>,
    ) -> Result<()> {
        self.db
            .query("UPDATE $id SET status = $status, log = $log, finished_at = $finished_at")
            .bind(("id", id.clone()))
            .bind(("status", status.to_string()))
            .bind(("log", log.map(String::from)))
            .bind(("finished_at", finished_at))
            .await
            .context("デプロイステータス更新失敗")?;
        Ok(())
    }

    pub async fn list_deployments(
        &self,
        tenant_slug: &str,
        limit: usize,
    ) -> Result<Vec<Deployment>> {
        let mut result = self
            .db
            .query("SELECT * FROM deployment WHERE tenant.slug = $tenant_slug ORDER BY created_at DESC LIMIT $limit")
            .bind(("tenant_slug", tenant_slug.to_string()))
            .bind(("limit", limit as i64))
            .await
            .context("デプロイ履歴取得失敗")?;
        let deployments: Vec<Deployment> = result.take(0)?;
        Ok(deployments)
    }

    // ─────────────────────────────────────────
    // Alert CRUD
    // ─────────────────────────────────────────

    /// 同一 (server_slug, container_name, alert_type) で未解決アラートがあれば更新、なければ作成
    pub async fn upsert_alert(&self, alert: &Alert) -> Result<Alert> {
        let mut result = self
            .db
            .query(
                r#"
                LET $existing = (SELECT * FROM alert
                    WHERE server_slug = $server_slug
                    AND container_name = $container_name
                    AND alert_type = $alert_type
                    AND resolved = false
                    LIMIT 1);
                IF array::len($existing) > 0 THEN
                    (UPDATE $existing[0].id SET
                        severity = $severity,
                        message = $message)
                ELSE
                    (CREATE alert CONTENT {
                        tenant: $tenant,
                        server_slug: $server_slug,
                        container_name: $container_name,
                        alert_type: $alert_type,
                        severity: $severity,
                        message: $message,
                        resolved: false
                    })
                END
                "#,
            )
            .bind(("tenant", alert.tenant.clone()))
            .bind(("server_slug", alert.server_slug.clone()))
            .bind(("container_name", alert.container_name.clone()))
            .bind(("alert_type", alert.alert_type.clone()))
            .bind(("severity", alert.severity.clone()))
            .bind(("message", alert.message.clone()))
            .await
            .context("アラート upsert 失敗")?;
        // IF-ELSE 結果は statement index 1
        let created: Option<Alert> = result.take(1)?;
        created.context("アラート upsert 結果が空")
    }

    /// コンテナ正常復帰時に該当アラートを解決済みにする
    pub async fn resolve_alerts(&self, server_slug: &str, container_name: &str) -> Result<()> {
        self.db
            .query(
                "UPDATE alert SET resolved = true, resolved_at = time::now() WHERE server_slug = $server_slug AND container_name = $container_name AND resolved = false",
            )
            .bind(("server_slug", server_slug.to_string()))
            .bind(("container_name", container_name.to_string()))
            .await
            .context("アラート解決失敗")?;
        Ok(())
    }

    /// サーバーのアクティブアラート数を取得
    pub async fn count_active_alerts_by_server(&self, server_slug: &str) -> Result<i64> {
        let mut result = self
            .db
            .query(
                "SELECT count() AS count FROM alert WHERE server_slug = $server_slug AND resolved = false GROUP ALL",
            )
            .bind(("server_slug", server_slug.to_string()))
            .await
            .context("アラートカウント取得失敗")?;
        let row: Option<serde_json::Value> = result.take(0)?;
        Ok(row.and_then(|v| v["count"].as_i64()).unwrap_or(0))
    }
}

/// SurrealDB schema definition.
const SCHEMA_SQL: &str = r#"
DEFINE TABLE IF NOT EXISTS tenant SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS slug ON tenant TYPE string;
DEFINE FIELD IF NOT EXISTS name ON tenant TYPE string;
DEFINE FIELD IF NOT EXISTS auth0_org_id ON tenant TYPE option<string>;
DEFINE FIELD IF NOT EXISTS plan ON tenant TYPE string DEFAULT 'self-hosted';
DEFINE FIELD IF NOT EXISTS dns_provider ON tenant TYPE option<string>;
DEFINE FIELD IF NOT EXISTS dns_domain ON tenant TYPE option<string>;
DEFINE FIELD IF NOT EXISTS dns_zone_id ON tenant TYPE option<string>;
DEFINE FIELD IF NOT EXISTS dns_api_token_encrypted ON tenant TYPE option<string>;
DEFINE FIELD IF NOT EXISTS created_at ON tenant TYPE option<datetime> DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS updated_at ON tenant TYPE option<datetime> DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS idx_tenant_slug ON tenant FIELDS slug UNIQUE;

DEFINE TABLE IF NOT EXISTS project SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS tenant ON project TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS slug ON project TYPE string;
DEFINE FIELD IF NOT EXISTS name ON project TYPE string;
DEFINE FIELD IF NOT EXISTS description ON project TYPE option<string>;
DEFINE FIELD IF NOT EXISTS repository_url ON project TYPE option<string>;
DEFINE FIELD IF NOT EXISTS created_at ON project TYPE option<datetime> DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS updated_at ON project TYPE option<datetime> DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS idx_project_tenant_slug ON project FIELDS tenant, slug UNIQUE;

DEFINE TABLE IF NOT EXISTS stage SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS project ON stage TYPE record<project>;
DEFINE FIELD IF NOT EXISTS slug ON stage TYPE string;
DEFINE FIELD IF NOT EXISTS description ON stage TYPE option<string>;
DEFINE FIELD IF NOT EXISTS server ON stage TYPE option<record<server>>;
DEFINE FIELD IF NOT EXISTS created_at ON stage TYPE option<datetime> DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS updated_at ON stage TYPE option<datetime> DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS idx_stage_project_slug ON stage FIELDS project, slug UNIQUE;

DEFINE TABLE IF NOT EXISTS service SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS stage ON service TYPE record<stage>;
DEFINE FIELD IF NOT EXISTS slug ON service TYPE string;
DEFINE FIELD IF NOT EXISTS image ON service TYPE string;
DEFINE FIELD IF NOT EXISTS config ON service TYPE option<object>;
DEFINE FIELD IF NOT EXISTS desired_status ON service TYPE string DEFAULT 'stopped';
DEFINE FIELD IF NOT EXISTS created_at ON service TYPE option<datetime> DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS updated_at ON service TYPE option<datetime> DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS idx_service_stage_slug ON service FIELDS stage, slug UNIQUE;

DEFINE TABLE IF NOT EXISTS container SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS service ON container TYPE record<service>;
DEFINE FIELD IF NOT EXISTS container_id ON container TYPE string;
DEFINE FIELD IF NOT EXISTS container_name ON container TYPE string;
DEFINE FIELD IF NOT EXISTS status ON container TYPE string DEFAULT 'unknown';
DEFINE FIELD IF NOT EXISTS health ON container TYPE option<string>;
DEFINE FIELD IF NOT EXISTS server ON container TYPE option<record<server>>;
DEFINE FIELD IF NOT EXISTS started_at ON container TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS last_seen_at ON container TYPE option<datetime> DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS created_at ON container TYPE option<datetime> DEFAULT time::now();

DEFINE TABLE IF NOT EXISTS server SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS tenant ON server TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS slug ON server TYPE string;
DEFINE FIELD IF NOT EXISTS provider ON server TYPE string;
DEFINE FIELD IF NOT EXISTS plan ON server TYPE option<string>;
DEFINE FIELD IF NOT EXISTS ssh_host ON server TYPE string;
DEFINE FIELD IF NOT EXISTS ssh_user ON server TYPE string DEFAULT 'root';
DEFINE FIELD IF NOT EXISTS deploy_path ON server TYPE string;
DEFINE FIELD IF NOT EXISTS status ON server TYPE string DEFAULT 'offline';
DEFINE FIELD IF NOT EXISTS provision_version ON server TYPE option<string>;
DEFINE FIELD IF NOT EXISTS tool_versions ON server TYPE option<object>;
DEFINE FIELD IF NOT EXISTS last_heartbeat_at ON server TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS created_at ON server TYPE option<datetime> DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS updated_at ON server TYPE option<datetime> DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS idx_server_tenant_slug ON server FIELDS tenant, slug UNIQUE;

DEFINE TABLE IF NOT EXISTS cost_entry SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS tenant ON cost_entry TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS project ON cost_entry TYPE option<record<project>>;
DEFINE FIELD IF NOT EXISTS stage ON cost_entry TYPE option<string>;
DEFINE FIELD IF NOT EXISTS provider ON cost_entry TYPE string;
DEFINE FIELD IF NOT EXISTS description ON cost_entry TYPE string;
DEFINE FIELD IF NOT EXISTS amount_jpy ON cost_entry TYPE int;
DEFINE FIELD IF NOT EXISTS month ON cost_entry TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON cost_entry TYPE option<datetime> DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS idx_cost_entry_tenant_month ON cost_entry FIELDS tenant, month;

DEFINE TABLE IF NOT EXISTS dns_record SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS tenant ON dns_record TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS project ON dns_record TYPE option<record<project>>;
DEFINE FIELD IF NOT EXISTS name ON dns_record TYPE string;
DEFINE FIELD IF NOT EXISTS record_type ON dns_record TYPE string;
DEFINE FIELD IF NOT EXISTS content ON dns_record TYPE string;
DEFINE FIELD IF NOT EXISTS zone_id ON dns_record TYPE option<string>;
DEFINE FIELD IF NOT EXISTS cf_record_id ON dns_record TYPE option<string>;
DEFINE FIELD IF NOT EXISTS proxied ON dns_record TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS created_at ON dns_record TYPE option<datetime> DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS updated_at ON dns_record TYPE option<datetime> DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS idx_dns_record_name ON dns_record FIELDS name UNIQUE;

DEFINE TABLE IF NOT EXISTS tenant_user SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS auth0_sub ON tenant_user TYPE string;
DEFINE FIELD IF NOT EXISTS tenant ON tenant_user TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS role ON tenant_user TYPE string DEFAULT 'member';
DEFINE FIELD IF NOT EXISTS created_at ON tenant_user TYPE option<datetime> DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS idx_tenant_user_sub ON tenant_user FIELDS auth0_sub UNIQUE;
DEFINE INDEX IF NOT EXISTS idx_tenant_user_tenant ON tenant_user FIELDS tenant;

DEFINE TABLE IF NOT EXISTS deployment SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS tenant ON deployment TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS project ON deployment TYPE record<project>;
DEFINE FIELD IF NOT EXISTS stage ON deployment TYPE string;
DEFINE FIELD IF NOT EXISTS server_slug ON deployment TYPE string;
DEFINE FIELD IF NOT EXISTS status ON deployment TYPE string DEFAULT 'pending';
DEFINE FIELD IF NOT EXISTS command ON deployment TYPE string;
DEFINE FIELD IF NOT EXISTS log ON deployment TYPE option<string>;
DEFINE FIELD IF NOT EXISTS started_at ON deployment TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS finished_at ON deployment TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS created_at ON deployment TYPE option<datetime> DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS idx_deployment_project_stage ON deployment FIELDS project, stage;

DEFINE TABLE IF NOT EXISTS alert SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS tenant ON alert TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS server_slug ON alert TYPE string;
DEFINE FIELD IF NOT EXISTS container_name ON alert TYPE string;
DEFINE FIELD IF NOT EXISTS alert_type ON alert TYPE string;
DEFINE FIELD IF NOT EXISTS severity ON alert TYPE string DEFAULT 'warning';
DEFINE FIELD IF NOT EXISTS message ON alert TYPE string;
DEFINE FIELD IF NOT EXISTS resolved ON alert TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS resolved_at ON alert TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS created_at ON alert TYPE option<datetime> DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS idx_alert_tenant ON alert FIELDS tenant;
DEFINE INDEX IF NOT EXISTS idx_alert_active ON alert FIELDS server_slug, container_name, alert_type, resolved;
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connect_memory() {
        let db = Database::connect_memory().await;
        assert!(db.is_ok(), "in-memory 接続に失敗: {:?}", db.err());
    }

    #[tokio::test]
    async fn test_tenant_crud() {
        let db = Database::connect_memory().await.unwrap();

        let tenant = Tenant {
            id: None,
            slug: "anycreative".into(),
            name: "ANYCREATIVE Inc".into(),
            auth0_org_id: None,
            plan: "self-hosted".into(),
            dns_provider: None,
            dns_domain: None,
            dns_zone_id: None,
            dns_api_token_encrypted: None,
            created_at: None,
            updated_at: None,
        };
        let created = db.create_tenant(&tenant).await.unwrap();
        assert!(created.id.is_some());
        assert_eq!(created.slug, "anycreative");

        let found = db.get_tenant_by_slug("anycreative").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "ANYCREATIVE Inc");

        let tenants = db.list_tenants().await.unwrap();
        assert_eq!(tenants.len(), 1);

        let not_found = db.get_tenant_by_slug("nonexistent").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_project_crud() {
        let db = Database::connect_memory().await.unwrap();

        let tenant = db
            .create_tenant(&Tenant {
                id: None,
                slug: "anycreative".into(),
                name: "ANYCREATIVE Inc".into(),
                auth0_org_id: None,
                plan: "self-hosted".into(),
                dns_provider: None,
                dns_domain: None,
                dns_zone_id: None,
                dns_api_token_encrypted: None,
                created_at: None,
                updated_at: None,
            })
            .await
            .unwrap();

        let project = Project {
            id: None,
            tenant: tenant.id.unwrap(),
            slug: "creo-memories".into(),
            name: "Creo Memories".into(),
            description: Some("永続記憶サービス".into()),
            repository_url: Some("https://github.com/chronista-club/creo-memories".into()),
            created_at: None,
            updated_at: None,
        };
        let created = db.create_project(&project).await.unwrap();
        assert!(created.id.is_some());
        assert_eq!(created.slug, "creo-memories");

        let projects = db.list_projects_by_tenant("anycreative").await.unwrap();
        assert_eq!(projects.len(), 1);
    }

    #[tokio::test]
    async fn test_server_crud() {
        let db = Database::connect_memory().await.unwrap();

        let tenant = db
            .create_tenant(&Tenant {
                id: None,
                slug: "anycreative".into(),
                name: "ANYCREATIVE Inc".into(),
                auth0_org_id: None,
                plan: "self-hosted".into(),
                dns_provider: None,
                dns_domain: None,
                dns_zone_id: None,
                dns_api_token_encrypted: None,
                created_at: None,
                updated_at: None,
            })
            .await
            .unwrap();

        let server = Server {
            id: None,
            tenant: tenant.id.unwrap(),
            slug: "vps-01".into(),
            provider: "sakura-cloud".into(),
            plan: Some("2G/3CPU".into()),
            ssh_host: "153.xxx.xxx.xxx".into(),
            ssh_user: "root".into(),
            deploy_path: "/opt/apps".into(),
            status: "offline".into(),
            provision_version: None,
            tool_versions: None,
            last_heartbeat_at: None,
            created_at: None,
            updated_at: None,
        };
        let created = db.register_server(&server).await.unwrap();
        assert!(created.id.is_some());

        db.update_server_heartbeat("vps-01").await.unwrap();

        let servers = db.list_servers_by_tenant("anycreative").await.unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].slug, "vps-01");
    }

    /// テスト用ヘルパー: テナント作成
    async fn create_test_tenant(db: &Database) -> Tenant {
        db.create_tenant(&Tenant {
            id: None,
            slug: "test-tenant".into(),
            name: "Test Tenant".into(),
            auth0_org_id: None,
            plan: "self-hosted".into(),
            dns_provider: None,
            dns_domain: None,
            dns_zone_id: None,
            dns_api_token_encrypted: None,
            created_at: None,
            updated_at: None,
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn test_tenant_user_crud() {
        let db = Database::connect_memory().await.unwrap();
        let tenant = create_test_tenant(&db).await;
        let tenant_id = tenant.id.unwrap();

        // owner 作成
        let user = TenantUser {
            id: None,
            auth0_sub: "auth0|owner123".into(),
            tenant: tenant_id.clone(),
            role: "owner".into(),
            created_at: None,
        };
        let created = db.create_tenant_user(&user).await.unwrap();
        assert!(created.id.is_some());
        assert_eq!(created.tenant_role(), TenantRole::Owner);

        // resolve
        let resolved = db.resolve_tenant_by_sub("auth0|owner123").await.unwrap();
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().tenant_role(), TenantRole::Owner);

        // 存在しない sub
        let not_found = db.resolve_tenant_by_sub("auth0|unknown").await.unwrap();
        assert!(not_found.is_none());

        // member 追加
        db.create_tenant_user(&TenantUser {
            id: None,
            auth0_sub: "auth0|member456".into(),
            tenant: tenant_id.clone(),
            role: "member".into(),
            created_at: None,
        })
        .await
        .unwrap();

        // 一覧
        let users = db.list_tenant_users("test-tenant").await.unwrap();
        assert_eq!(users.len(), 2);

        // role 更新（テナント境界チェック付き）
        let updated = db
            .update_tenant_user_role("auth0|member456", "admin", "test-tenant")
            .await
            .unwrap();
        assert!(updated);
        let resolved = db
            .resolve_tenant_by_sub("auth0|member456")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resolved.tenant_role(), TenantRole::Admin);

        // テナント境界チェック: 別テナントからの更新は失敗
        let cross_tenant = db
            .update_tenant_user_role("auth0|member456", "member", "other-tenant")
            .await
            .unwrap();
        assert!(!cross_tenant);

        // 削除（テナント境界チェック付き）
        let deleted = db
            .delete_tenant_user("auth0|member456", "test-tenant")
            .await
            .unwrap();
        assert!(deleted);
        let users = db.list_tenant_users("test-tenant").await.unwrap();
        assert_eq!(users.len(), 1);

        // テナント境界チェック: 別テナントからの削除は失敗
        let cross_delete = db
            .delete_tenant_user("auth0|owner123", "other-tenant")
            .await
            .unwrap();
        assert!(!cross_delete);
    }

    #[tokio::test]
    async fn test_get_tenant_by_id() {
        let db = Database::connect_memory().await.unwrap();
        let tenant = create_test_tenant(&db).await;
        let tenant_id = tenant.id.unwrap();

        let found = db.get_tenant_by_id(&tenant_id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().slug, "test-tenant");
    }

    #[tokio::test]
    async fn test_stage_overviews() {
        let db = Database::connect_memory().await.unwrap();
        let tenant = create_test_tenant(&db).await;
        let tenant_id = tenant.id.unwrap();

        // プロジェクト作成
        let project = db
            .create_project(&Project {
                id: None,
                tenant: tenant_id.clone(),
                slug: "myapp".into(),
                name: "My App".into(),
                description: None,
                repository_url: None,
                created_at: None,
                updated_at: None,
            })
            .await
            .unwrap();

        // サーバー作成
        let server = db
            .register_server(&Server {
                id: None,
                tenant: tenant_id.clone(),
                slug: "vps-01".into(),
                provider: "sakura-cloud".into(),
                plan: None,
                ssh_host: "10.0.0.1".into(),
                ssh_user: "root".into(),
                deploy_path: "/opt/apps".into(),
                status: "online".into(),
                provision_version: None,
                tool_versions: None,
                last_heartbeat_at: None,
                created_at: None,
                updated_at: None,
            })
            .await
            .unwrap();

        // ステージ作成（サーバー紐付き）
        db.create_stage(&Stage {
            id: None,
            project: project.id.clone().unwrap(),
            slug: "live".into(),
            description: Some("本番環境".into()),
            server: server.id.clone(),
            created_at: None,
            updated_at: None,
        })
        .await
        .unwrap();

        // ステージ作成（サーバーなし）
        db.create_stage(&Stage {
            id: None,
            project: project.id.clone().unwrap(),
            slug: "dev".into(),
            description: None,
            server: None,
            created_at: None,
            updated_at: None,
        })
        .await
        .unwrap();

        // デプロイ作成
        db.create_deployment(&Deployment {
            id: None,
            tenant: tenant_id.clone(),
            project: project.id.clone().unwrap(),
            stage: "live".into(),
            server_slug: "vps-01".into(),
            status: "success".into(),
            command: "deploy live".into(),
            log: None,
            started_at: None,
            finished_at: None,
            created_at: None,
        })
        .await
        .unwrap();

        // 概要取得
        let overviews = db.list_stage_overviews("test-tenant").await.unwrap();
        assert_eq!(overviews.len(), 2);

        // dev は先（アルファベット順）、live は後
        let dev = overviews.iter().find(|s| s.slug == "dev").unwrap();
        assert_eq!(dev.project_slug, "myapp");
        assert!(dev.server_slug.is_none());
        assert!(dev.last_deploy_status.is_none());

        let live = overviews.iter().find(|s| s.slug == "live").unwrap();
        assert_eq!(live.server_slug.as_deref(), Some("vps-01"));
        assert_eq!(live.server_status.as_deref(), Some("online"));
        assert_eq!(live.last_deploy_status.as_deref(), Some("success"));
    }

    #[tokio::test]
    async fn test_alert_crud() {
        let db = Database::connect_memory().await.unwrap();
        let tenant = create_test_tenant(&db).await;
        let tenant_id = tenant.id.unwrap();

        // サーバー作成
        db.register_server(&Server {
            id: None,
            tenant: tenant_id.clone(),
            slug: "vps-01".into(),
            provider: "sakura-cloud".into(),
            plan: None,
            ssh_host: "10.0.0.1".into(),
            ssh_user: "root".into(),
            deploy_path: "/opt/apps".into(),
            status: "online".into(),
            provision_version: None,
            tool_versions: None,
            last_heartbeat_at: None,
            created_at: None,
            updated_at: None,
        })
        .await
        .unwrap();

        // アラート作成
        let alert = Alert {
            id: None,
            tenant: tenant_id.clone(),
            server_slug: "vps-01".into(),
            container_name: "web".into(),
            alert_type: "restart_loop".into(),
            severity: "critical".into(),
            message: "コンテナ web がリスタートループ".into(),
            resolved: false,
            resolved_at: None,
            created_at: None,
        };
        let created = db.upsert_alert(&alert).await.unwrap();
        assert!(created.id.is_some());
        assert_eq!(created.server_slug, "vps-01");
        assert_eq!(created.alert_type, "restart_loop");
        assert!(!created.resolved);

        // 同一コンテナ・同一タイプで upsert → 更新される（新規作成されない）
        let alert2 = Alert {
            message: "更新されたメッセージ".into(),
            ..alert.clone()
        };
        let updated = db.upsert_alert(&alert2).await.unwrap();
        assert_eq!(updated.message, "更新されたメッセージ");

        // アクティブアラートカウント
        let count = db.count_active_alerts_by_server("vps-01").await.unwrap();
        assert_eq!(count, 1);

        // 別タイプのアラートを追加
        let alert3 = Alert {
            alert_type: "unhealthy".into(),
            severity: "warning".into(),
            message: "コンテナ web が unhealthy".into(),
            ..alert.clone()
        };
        db.upsert_alert(&alert3).await.unwrap();

        let count = db.count_active_alerts_by_server("vps-01").await.unwrap();
        assert_eq!(count, 2);

        // 解決
        db.resolve_alerts("vps-01", "web").await.unwrap();
        let count = db.count_active_alerts_by_server("vps-01").await.unwrap();
        assert_eq!(count, 0);
    }
}
