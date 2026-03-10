use anyhow::{Context, Result};
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
            db.signin(surrealdb::opt::auth::Namespace {
                namespace: config.namespace.clone(),
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
}

/// SurrealDB schema definition.
const SCHEMA_SQL: &str = r#"
DEFINE TABLE IF NOT EXISTS tenant SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS slug ON tenant TYPE string;
DEFINE FIELD IF NOT EXISTS name ON tenant TYPE string;
DEFINE FIELD IF NOT EXISTS auth0_org_id ON tenant TYPE option<string>;
DEFINE FIELD IF NOT EXISTS plan ON tenant TYPE string DEFAULT 'self-hosted';
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
}
