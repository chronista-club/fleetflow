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
            .query(
                "CREATE tenant CONTENT { \
                    slug: $slug, name: $name, auth0_org_id: $auth0_org_id, plan: $plan, \
                    placement_policy: $placement_policy \
                }",
            )
            .bind(("slug", tenant.slug.clone()))
            .bind(("name", tenant.name.clone()))
            .bind(("auth0_org_id", tenant.auth0_org_id.clone()))
            .bind(("plan", tenant.plan.clone()))
            // FSC-26 Phase B-3: Placement Policy
            .bind(("placement_policy", tenant.placement_policy.clone()))
            .await
            .context("テナント作成失敗")?;
        let created: Option<Tenant> = result.take(0)?;
        created.context("テナント作成結果が空")
    }

    /// Placement Policy を更新 (FSC-26 Phase B-3)
    ///
    /// 注意: `PlacementPolicy` が内部に `serde_json::Value` を含むため、
    /// `.bind()` 経由だと connection 破壊 (Connection uninitialised) が起きる。
    /// struct ごと `$policy` へ bind する方式で回避する。
    pub async fn update_tenant_placement_policy(
        &self,
        tenant_slug: &str,
        policy: &PlacementPolicy,
    ) -> Result<()> {
        self.db
            .query("UPDATE tenant SET placement_policy = $policy, updated_at = time::now() WHERE slug = $slug")
            .bind(("slug", tenant_slug.to_string()))
            .bind(("policy", policy.clone()))
            .await
            .context("Placement Policy 更新失敗")?;
        Ok(())
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
            .query(
                "SELECT * FROM tenant_user WHERE auth0_sub = $sub AND deleted_at IS NONE LIMIT 1",
            )
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
            .query("SELECT * FROM tenant_user WHERE auth0_sub = $sub AND tenant.slug = $tenant_slug AND deleted_at IS NONE LIMIT 1")
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
            .query("SELECT * FROM tenant_user WHERE tenant.slug = $tenant_slug AND deleted_at IS NONE ORDER BY role, auth0_sub")
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
        // D#4 soft-delete: hard DELETE → UPDATE deleted_at で復旧可能 + feedback_no_data_deletion.md 整合
        let mut result = self
            .db
            .query("UPDATE tenant_user SET deleted_at = time::now() WHERE auth0_sub = $sub AND tenant.slug = $tenant_slug AND deleted_at IS NONE RETURN BEFORE")
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
        // FSC-33: SurrealDB v3 SDK (3.0.2) wss:// engine の bind バグ回避
        //
        // 個別 .bind() で Option<String>=None を渡すと wss:// engine が
        // "Connection uninitialised" を返す (mem:// は再現せず、Cloud 接続のみ)。
        // 単一 struct を $input に bind する形で個別 None bind 経路を避ける。
        use surrealdb::types::{RecordId, SurrealValue};
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, SurrealValue)]
        struct CreateProjectInput {
            tenant: RecordId,
            slug: String,
            name: String,
            description: Option<String>,
            repository_url: Option<String>,
        }
        let input = CreateProjectInput {
            tenant: project.tenant.clone(),
            slug: project.slug.clone(),
            name: project.name.clone(),
            description: project.description.clone(),
            repository_url: project.repository_url.clone(),
        };
        let mut result = self
            .db
            .query("CREATE project CONTENT $input")
            .bind(("input", input))
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
                "SELECT * FROM project WHERE slug = $project_slug AND tenant.slug = $tenant_slug AND deleted_at IS NONE LIMIT 1",
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
            .query("SELECT * FROM project WHERE tenant.slug = $tenant_slug AND deleted_at IS NONE ORDER BY slug")
            .bind(("tenant_slug", tenant_slug.to_string()))
            .await
            .context("プロジェクト一覧取得失敗")?;
        let projects: Vec<Project> = result.take(0)?;
        Ok(projects)
    }

    pub async fn delete_project(&self, tenant_slug: &str, project_slug: &str) -> Result<bool> {
        // D#4 soft-delete: hard DELETE → UPDATE deleted_at で復旧可能 + feedback_no_data_deletion.md 整合
        self.db
            .query("UPDATE project SET deleted_at = time::now() WHERE slug = $project_slug AND tenant.slug = $tenant_slug AND deleted_at IS NONE")
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
                    (SELECT count() FROM alert WHERE server_slug = $parent.server.slug AND tenant = $parent.project.tenant AND resolved = false GROUP ALL)[0].count AS alert_count
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

    /// tenant_id + project_slug で project を解決 (adopt_stage 用)
    async fn find_project_by_tenant_and_slug(
        &self,
        tenant_id: &RecordId,
        project_slug: &str,
    ) -> Result<Option<Project>> {
        let mut result = self
            .db
            .query("SELECT * FROM project WHERE tenant = $tenant AND slug = $slug AND deleted_at IS NONE LIMIT 1")
            .bind(("tenant", tenant_id.clone()))
            .bind(("slug", project_slug.to_string()))
            .await
            .context("project (tenant+slug) 取得失敗")?;
        let items: Vec<Project> = result.take(0)?;
        Ok(items.into_iter().next())
    }

    /// project_id + stage_slug で stage を解決 (adopt_stage 用)
    async fn find_stage_by_project_and_slug(
        &self,
        project_id: &RecordId,
        stage_slug: &str,
    ) -> Result<Option<Stage>> {
        let mut result = self
            .db
            .query("SELECT * FROM stage WHERE project = $project AND slug = $slug LIMIT 1")
            .bind(("project", project_id.clone()))
            .bind(("slug", stage_slug.to_string()))
            .await
            .context("stage (project+slug) 取得失敗")?;
        let items: Vec<Stage> = result.take(0)?;
        Ok(items.into_iter().next())
    }

    /// 既存の稼働中 stage を非破壊で fleetstage registry に adopt する (FSC-16)。
    ///
    /// docker や worker には一切触れず、CP DB に以下の record を作成する:
    /// * `project` — 同 tenant 内に同 slug が無ければ作成、あれば再利用
    /// * `stage`   — 同 project 内に同 slug が既にあればエラー (二重 adopt 防止)
    /// * `service` — 各 service spec ごとに新規作成 (desired_status = "running")
    ///
    /// Persistence Volume Tier の `adopt_volume` と同じ BYO 哲学。
    pub async fn adopt_stage(&self, req: &AdoptStageRequest<'_>) -> Result<AdoptStageOutcome> {
        if req.project_slug.is_empty() {
            anyhow::bail!("project_slug must not be empty");
        }
        if req.stage_slug.is_empty() {
            anyhow::bail!("stage_slug must not be empty");
        }

        // project upsert (tenant 内で slug がユニーク)
        let project = match self
            .find_project_by_tenant_and_slug(req.tenant_id, req.project_slug)
            .await?
        {
            Some(p) => p,
            None => {
                self.create_project(&Project {
                    id: None,
                    tenant: req.tenant_id.clone(),
                    slug: req.project_slug.to_string(),
                    name: req.project_name.unwrap_or(req.project_slug).to_string(),
                    description: None,
                    repository_url: None,
                    created_at: None,
                    updated_at: None,
                    deleted_at: None,
                })
                .await?
            }
        };
        let project_id = project
            .id
            .clone()
            .context("project.id should exist after create/fetch")?;

        // stage: 同 project 内で slug が既にあれば二重 adopt を防ぐ
        if self
            .find_stage_by_project_and_slug(&project_id, req.stage_slug)
            .await?
            .is_some()
        {
            anyhow::bail!(
                "stage `{}` already exists under project `{}` (use existing record)",
                req.stage_slug,
                req.project_slug
            );
        }

        // stage create
        let stage = self
            .create_stage(&Stage {
                id: None,
                project: project_id.clone(),
                slug: req.stage_slug.to_string(),
                description: req.description.map(String::from),
                server: Some(req.server_id.clone()),
                created_at: None,
                updated_at: None,
            })
            .await?;
        let stage_id = stage
            .id
            .clone()
            .context("stage.id should exist after create")?;

        // services: adopt 時点では image のみ記録し config は None、desired_status = running
        let mut created_services = Vec::with_capacity(req.services.len());
        for spec in req.services {
            if spec.slug.is_empty() {
                anyhow::bail!("service slug must not be empty");
            }
            if spec.image.is_empty() {
                anyhow::bail!("service image must not be empty (slug=`{}`)", spec.slug);
            }
            let svc = self
                .create_service(&Service {
                    id: None,
                    stage: stage_id.clone(),
                    slug: spec.slug.clone(),
                    image: spec.image.clone(),
                    config: None,
                    desired_status: "running".to_string(),
                    created_at: None,
                    updated_at: None,
                })
                .await?;
            created_services.push(svc);
        }

        Ok(AdoptStageOutcome {
            project,
            stage,
            services: created_services,
        })
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
        // B#3 fix: 旧実装は CREATE SQL に PR #153 で追加された lifecycle / infra metadata fields を bind せず silent drop していた。 全 field を明示 bind する。
        let mut result = self
            .db
            .query(
                "CREATE server CONTENT { \
                    tenant: $tenant, slug: $slug, provider: $provider, plan: $plan, \
                    ssh_host: $ssh_host, ssh_user: $ssh_user, deploy_path: $deploy_path, \
                    status: $status, labels: $labels, capacity: $capacity, \
                    allocated: $allocated, scheduling: $scheduling, pool_id: $pool_id, \
                    desired_state: $desired_state, purpose: $purpose, owner: $owner, \
                    sakura: $sakura, tailscale: $tailscale, dns: $dns, lifecycle: $lifecycle \
                }",
            )
            .bind(("tenant", server.tenant.clone()))
            .bind(("slug", server.slug.clone()))
            .bind(("provider", server.provider.clone()))
            .bind(("plan", server.plan.clone()))
            .bind(("ssh_host", server.ssh_host.clone()))
            .bind(("ssh_user", server.ssh_user.clone()))
            .bind(("deploy_path", server.deploy_path.clone()))
            .bind(("status", server.status.clone()))
            // FSC-26 Phase B-1: Worker Pool labels / capacity / allocated / scheduling
            .bind(("labels", server.labels.clone()))
            .bind(("capacity", server.capacity.clone()))
            .bind(("allocated", server.allocated.clone()))
            .bind(("scheduling", server.scheduling.clone()))
            // FSC-26 Phase B-2: Worker Pool 参照
            .bind(("pool_id", server.pool_id.clone()))
            // PR #153: lifecycle / infra metadata (single-table model)
            .bind(("desired_state", server.desired_state.clone()))
            .bind(("purpose", server.purpose.clone()))
            .bind(("owner", server.owner.clone()))
            .bind(("sakura", server.sakura.clone()))
            .bind(("tailscale", server.tailscale.clone()))
            .bind(("dns", server.dns.clone()))
            .bind(("lifecycle", server.lifecycle.clone()))
            .await
            .context("サーバー登録失敗")?;
        let created: Option<Server> = result.take(0)?;
        created.context("サーバー登録結果が空")
    }

    /// Server を Worker Pool に紐付け (FSC-26 Phase B-2)
    pub async fn update_server_pool(&self, server_slug: &str, pool_id: &RecordId) -> Result<()> {
        self.db
            .query(
                "UPDATE server SET pool_id = $pool_id, updated_at = time::now() WHERE slug = $slug",
            )
            .bind(("slug", server_slug.to_string()))
            .bind(("pool_id", pool_id.clone()))
            .await
            .context("Server pool_id 更新失敗")?;
        Ok(())
    }

    pub async fn list_servers_by_tenant(&self, tenant_slug: &str) -> Result<Vec<Server>> {
        let mut result = self
            .db
            .query("SELECT * FROM server WHERE tenant.slug = $tenant_slug AND deleted_at IS NONE ORDER BY slug")
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
            .query("SELECT * FROM server WHERE deleted_at IS NONE ORDER BY slug")
            .await
            .context("全サーバー一覧取得失敗")?;
        let servers: Vec<Server> = result.take(0)?;
        Ok(servers)
    }

    /// slug でサーバーを取得
    pub async fn get_server_by_slug(&self, slug: &str) -> Result<Option<Server>> {
        let mut result = self
            .db
            .query("SELECT * FROM server WHERE slug = $slug AND deleted_at IS NONE LIMIT 1")
            .bind(("slug", slug.to_string()))
            .await
            .context("サーバー取得失敗")?;
        let server: Option<Server> = result.take(0)?;
        Ok(server)
    }

    /// サーバーを DB から削除
    pub async fn delete_server(&self, slug: &str) -> Result<()> {
        // D#4 soft-delete: hard DELETE → UPDATE deleted_at で復旧可能 + feedback_no_data_deletion.md 整合
        self.db
            .query("UPDATE server SET deleted_at = time::now() WHERE slug = $slug AND deleted_at IS NONE")
            .bind(("slug", slug.to_string()))
            .await
            .context("サーバー削除失敗")?;
        Ok(())
    }

    // ─────────────────────────────────────────
    // WorkerPool CRUD (FSC-26 Phase B-2)
    // ─────────────────────────────────────────

    /// Worker Pool を新規作成 (FSC-26 Phase B-2)
    ///
    /// name の UNIQUE index があるため同名重複は失敗する。
    pub async fn create_worker_pool(&self, pool: &WorkerPool) -> Result<WorkerPool> {
        // id: None を明示し、SurrealDB 側で自動採番させる
        let insert = WorkerPool {
            id: None,
            ..pool.clone()
        };
        let mut result = self
            .db
            .query("CREATE worker_pool CONTENT $data")
            .bind(("data", insert))
            .await
            .context("Worker Pool 作成失敗")?;
        let created: Option<WorkerPool> = result.take(0)?;
        created.context("Worker Pool 作成結果が空")
    }

    /// name で Worker Pool を取得 (FSC-26 Phase B-2)
    pub async fn get_worker_pool_by_name(&self, name: &str) -> Result<Option<WorkerPool>> {
        let mut result = self
            .db
            .query("SELECT * FROM worker_pool WHERE name = $name LIMIT 1")
            .bind(("name", name.to_string()))
            .await
            .context("Worker Pool 取得失敗")?;
        let pools: Vec<WorkerPool> = result.take(0)?;
        Ok(pools.into_iter().next())
    }

    /// 全 Worker Pool を列挙 (FSC-26 Phase B-2)
    pub async fn list_worker_pools(&self) -> Result<Vec<WorkerPool>> {
        let mut result = self
            .db
            .query("SELECT * FROM worker_pool ORDER BY name")
            .await
            .context("Worker Pool 一覧取得失敗")?;
        let pools: Vec<WorkerPool> = result.take(0)?;
        Ok(pools)
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
            .query("SELECT * FROM dns_record WHERE tenant.slug = $tenant_slug AND deleted_at IS NONE ORDER BY name")
            .bind(("tenant_slug", tenant_slug.to_string()))
            .await
            .context("DNSレコード一覧取得失敗")?;
        let records: Vec<DnsRecord> = result.take(0)?;
        Ok(records)
    }

    pub async fn delete_dns_record(&self, record_name: &str) -> Result<bool> {
        // D#4 soft-delete: hard DELETE → UPDATE deleted_at で復旧可能 + feedback_no_data_deletion.md 整合
        let mut result = self
            .db
            .query("UPDATE dns_record SET deleted_at = time::now() WHERE name = $name AND deleted_at IS NONE RETURN BEFORE")
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
                    (UPDATE ONLY $existing[0].id SET
                        severity = $severity,
                        message = $message)
                ELSE
                    (CREATE ONLY alert CONTENT {
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
        // B#7 fix: UPDATE/CREATE ONLY で両 branch 単一 record 返却に統一。
        // 旧実装は UPDATE が Vec<Alert> を返していたため、 IF branch fire 時に
        // result.take(1) が型不一致で None を返し誤エラーを起こす可能性があった。
        // IF-ELSE 結果は statement index 1。
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
    pub async fn count_active_alerts_by_server(
        &self,
        server_slug: &str,
        tenant_slug: &str,
    ) -> Result<i64> {
        let mut result = self
            .db
            .query(
                "SELECT count() AS count FROM alert WHERE server_slug = $server_slug AND tenant.slug = $tenant_slug AND resolved = false GROUP ALL",
            )
            .bind(("server_slug", server_slug.to_string()))
            .bind(("tenant_slug", tenant_slug.to_string()))
            .await
            .context("アラートカウント取得失敗")?;
        let row: Option<serde_json::Value> = result.take(0)?;
        Ok(row.and_then(|v| v["count"].as_i64()).unwrap_or(0))
    }

    /// サーバーのアクティブアラート一覧（テナント境界チェック付き）
    pub async fn list_active_alerts_by_server(
        &self,
        server_slug: &str,
        tenant_slug: &str,
    ) -> Result<Vec<Alert>> {
        let mut result = self
            .db
            .query("SELECT * FROM alert WHERE server_slug = $server_slug AND tenant.slug = $tenant_slug AND resolved = false ORDER BY created_at DESC")
            .bind(("server_slug", server_slug.to_string()))
            .bind(("tenant_slug", tenant_slug.to_string()))
            .await
            .context("アラート一覧取得失敗")?;
        let alerts: Vec<Alert> = result.take(0)?;
        Ok(alerts)
    }

    // ─────────────────────────────────────────
    // Volume CRUD (Persistence Volume Tier P-1, 2026-04-23)
    //
    // 詳細設計: fleetstage repo docs/design/20-persistence-volume-tier.md
    // ─────────────────────────────────────────

    /// Volume を新規作成。
    ///
    /// `tier` と `state` は model の定数に従う (volume_tier / volume_state モジュール参照)。
    pub async fn create_volume(&self, volume: &Volume) -> Result<Volume> {
        if !volume_tier::is_valid(&volume.tier) {
            anyhow::bail!("invalid volume tier: {}", volume.tier);
        }
        if !volume_state::is_valid(&volume.state) {
            anyhow::bail!("invalid volume state: {}", volume.state);
        }
        let mut result = self
            .db
            .query(
                "CREATE volume CONTENT { \
                    tenant: $tenant, project: $project, stage: $stage, \
                    slug: $slug, tier: $tier, size_bytes: $size_bytes, mount: $mount, \
                    server: $server, provider: $provider, provider_resource_id: $provider_resource_id, \
                    encryption: $encryption, backup_policy: $backup_policy, \
                    bring_your_own: $bring_your_own, state: $state \
                }",
            )
            .bind(("tenant", volume.tenant.clone()))
            .bind(("project", volume.project.clone()))
            .bind(("stage", volume.stage.clone()))
            .bind(("slug", volume.slug.clone()))
            .bind(("tier", volume.tier.clone()))
            .bind(("size_bytes", volume.size_bytes))
            .bind(("mount", volume.mount.clone()))
            .bind(("server", volume.server.clone()))
            .bind(("provider", volume.provider.clone()))
            .bind(("provider_resource_id", volume.provider_resource_id.clone()))
            .bind(("encryption", volume.encryption))
            .bind(("backup_policy", volume.backup_policy.clone()))
            .bind(("bring_your_own", volume.bring_your_own))
            .bind(("state", volume.state.clone()))
            .await
            .context("Volume 作成失敗")?;
        let created: Option<Volume> = result.take(0)?;
        created.context("Volume 作成結果が空")
    }

    /// BYO (bring-your-own) volume を既存 tenant + server 配下に adopt する。
    /// 既存 disk に一切触れず、fleetstage registry に登録するのみ。
    pub async fn adopt_volume(
        &self,
        tenant_id: &RecordId,
        server_id: &RecordId,
        slug: &str,
        mount: &str,
        tier: &str,
    ) -> Result<Volume> {
        if !volume_tier::is_valid(tier) {
            anyhow::bail!("invalid volume tier: {}", tier);
        }
        let volume = Volume {
            id: None,
            tenant: tenant_id.clone(),
            project: None,
            stage: None,
            slug: slug.to_string(),
            tier: tier.to_string(),
            size_bytes: None,
            mount: mount.to_string(),
            server: Some(server_id.clone()),
            provider: "local".to_string(),
            provider_resource_id: None,
            encryption: false,
            backup_policy: None,
            bring_your_own: true,
            state: volume_state::ATTACHED.to_string(),
            created_at: None,
            updated_at: None,
        };
        self.create_volume(&volume).await
    }

    pub async fn get_volume_by_slug(
        &self,
        tenant_id: &RecordId,
        slug: &str,
    ) -> Result<Option<Volume>> {
        let mut result = self
            .db
            .query("SELECT * FROM volume WHERE tenant = $tenant AND slug = $slug LIMIT 1")
            .bind(("tenant", tenant_id.clone()))
            .bind(("slug", slug.to_string()))
            .await
            .context("Volume 取得失敗")?;
        let volumes: Vec<Volume> = result.take(0)?;
        Ok(volumes.into_iter().next())
    }

    pub async fn list_volumes_by_tenant(&self, tenant_id: &RecordId) -> Result<Vec<Volume>> {
        let mut result = self
            .db
            .query("SELECT * FROM volume WHERE tenant = $tenant ORDER BY slug")
            .bind(("tenant", tenant_id.clone()))
            .await
            .context("Volume 一覧取得失敗")?;
        let volumes: Vec<Volume> = result.take(0)?;
        Ok(volumes)
    }

    pub async fn update_volume_state(&self, volume_id: &RecordId, new_state: &str) -> Result<()> {
        if !volume_state::is_valid(new_state) {
            anyhow::bail!("invalid volume state: {}", new_state);
        }
        self.db
            .query("UPDATE $id SET state = $state, updated_at = time::now()")
            .bind(("id", volume_id.clone()))
            .bind(("state", new_state.to_string()))
            .await
            .context("Volume state 更新失敗")?;
        Ok(())
    }

    // ─────────────────────────────────────────
    // VolumeSnapshot CRUD
    // ─────────────────────────────────────────

    pub async fn create_volume_snapshot(
        &self,
        snapshot: &VolumeSnapshot,
    ) -> Result<VolumeSnapshot> {
        if !volume_snapshot_kind::is_valid(&snapshot.kind) {
            anyhow::bail!("invalid volume snapshot kind: {}", snapshot.kind);
        }
        let mut result = self
            .db
            .query(
                "CREATE volume_snapshot CONTENT { \
                    volume: $volume, kind: $kind, \
                    provider_resource_id: $provider_resource_id, \
                    location_url: $location_url, size_bytes: $size_bytes, \
                    retention_until: $retention_until \
                }",
            )
            .bind(("volume", snapshot.volume.clone()))
            .bind(("kind", snapshot.kind.clone()))
            .bind((
                "provider_resource_id",
                snapshot.provider_resource_id.clone(),
            ))
            .bind(("location_url", snapshot.location_url.clone()))
            .bind(("size_bytes", snapshot.size_bytes))
            .bind(("retention_until", snapshot.retention_until))
            .await
            .context("VolumeSnapshot 作成失敗")?;
        let created: Option<VolumeSnapshot> = result.take(0)?;
        created.context("VolumeSnapshot 作成結果が空")
    }

    pub async fn list_volume_snapshots(&self, volume_id: &RecordId) -> Result<Vec<VolumeSnapshot>> {
        let mut result = self
            .db
            .query("SELECT * FROM volume_snapshot WHERE volume = $volume ORDER BY taken_at DESC")
            .bind(("volume", volume_id.clone()))
            .await
            .context("VolumeSnapshot 一覧取得失敗")?;
        let snapshots: Vec<VolumeSnapshot> = result.take(0)?;
        Ok(snapshots)
    }

    // ─────────────────────────────────────────
    // BuildJob CRUD (Build Tier v1 MVP, 2026-04-23)
    //
    // 詳細設計: fleetstage repo docs/design/30-build-tier.md
    // ─────────────────────────────────────────

    /// BuildJob を新規作成 (state=queued でエンキュー)。
    ///
    /// `kind` と `state` は model の定数に従う (build_job_kind / build_job_state モジュール参照)。
    pub async fn create_build_job(&self, job: &BuildJob) -> Result<BuildJob> {
        if !build_job_kind::is_valid(&job.kind) {
            anyhow::bail!("invalid build job kind: {}", job.kind);
        }
        if !build_job_state::is_valid(&job.state) {
            anyhow::bail!("invalid build job state: {}", job.state);
        }
        // source / target は nested object のため flat scalar bind で展開する
        // (surrealdb の serde_json::Value bind は壊れる — project_surrealdb_bind_pitfall)
        let mut result = self
            .db
            .query(
                "CREATE build_job CONTENT { \
                    tenant: $tenant, project: $project, \
                    kind: $kind, \
                    source: { git_url: $git_url, git_ref: $git_ref, dockerfile: $dockerfile }, \
                    target: { image: $image, registry_secret: $registry_secret }, \
                    state: $state, server: $server, logs_url: $logs_url, \
                    submitted_at: $submitted_at, \
                    started_at: $started_at, finished_at: $finished_at, \
                    duration_seconds: $duration_seconds \
                }",
            )
            .bind(("tenant", job.tenant.clone()))
            .bind(("project", job.project.clone()))
            .bind(("kind", job.kind.clone()))
            .bind(("git_url", job.source.git_url.clone()))
            .bind(("git_ref", job.source.git_ref.clone()))
            .bind(("dockerfile", job.source.dockerfile.clone()))
            .bind(("image", job.target.image.clone()))
            .bind(("registry_secret", job.target.registry_secret.clone()))
            .bind(("state", job.state.clone()))
            .bind(("server", job.server.clone()))
            .bind(("logs_url", job.logs_url.clone()))
            // sprint-1 follow-up: schema には DEFAULT time::now() があるが Rust 側
            // (DateTime<Utc>) と DB 側で値が一致しないリスク回避のため明示 bind。
            .bind(("submitted_at", job.submitted_at))
            .bind(("started_at", job.started_at))
            .bind(("finished_at", job.finished_at))
            .bind(("duration_seconds", job.duration_seconds))
            .await
            .context("BuildJob 作成失敗")?;
        let created: Option<BuildJob> = result.take(0)?;
        created.context("BuildJob 作成結果が空")
    }

    /// BuildJob を ID で取得。
    pub async fn get_build_job_by_id(&self, job_id: &RecordId) -> Result<Option<BuildJob>> {
        let mut result = self
            .db
            .query("SELECT * FROM build_job WHERE id = $id LIMIT 1")
            .bind(("id", job_id.clone()))
            .await
            .context("BuildJob 取得失敗")?;
        let jobs: Vec<BuildJob> = result.take(0)?;
        Ok(jobs.into_iter().next())
    }

    /// テナント配下の BuildJob 一覧を取得 (submitted_at DESC)。
    pub async fn list_build_jobs_by_tenant(&self, tenant_id: &RecordId) -> Result<Vec<BuildJob>> {
        let mut result = self
            .db
            .query("SELECT * FROM build_job WHERE tenant = $tenant ORDER BY submitted_at DESC")
            .bind(("tenant", tenant_id.clone()))
            .await
            .context("BuildJob 一覧取得失敗")?;
        let jobs: Vec<BuildJob> = result.take(0)?;
        Ok(jobs)
    }

    /// BuildJob の state を更新。invalid state は拒否。
    pub async fn update_build_job_state(&self, job_id: &RecordId, new_state: &str) -> Result<()> {
        if !build_job_state::is_valid(new_state) {
            anyhow::bail!("invalid build job state: {}", new_state);
        }
        self.db
            .query("UPDATE $id SET state = $state")
            .bind(("id", job_id.clone()))
            .bind(("state", new_state.to_string()))
            .await
            .context("BuildJob state 更新失敗")?;
        Ok(())
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

-- FSC-26 Phase B-3: tenant.placement_policy (Scheduler 配置制約)
DEFINE FIELD IF NOT EXISTS placement_policy ON tenant TYPE option<object>;
DEFINE FIELD IF NOT EXISTS placement_policy.tier ON tenant TYPE option<string>;
DEFINE FIELD IF NOT EXISTS placement_policy.preferred_labels ON tenant TYPE option<object>;
-- wildcard: placement_policy.preferred_labels は自由 key の string map
DEFINE FIELD IF NOT EXISTS placement_policy.preferred_labels.* ON tenant TYPE string;
DEFINE FIELD IF NOT EXISTS placement_policy.resource_quota ON tenant TYPE option<object>;
DEFINE FIELD IF NOT EXISTS placement_policy.resource_quota.max_stages ON tenant TYPE option<int>;
DEFINE FIELD IF NOT EXISTS placement_policy.resource_quota.max_services_per_stage ON tenant TYPE option<int>;
DEFINE FIELD IF NOT EXISTS placement_policy.resource_quota.cpu_cores ON tenant TYPE option<int>;
DEFINE FIELD IF NOT EXISTS placement_policy.resource_quota.memory_gb ON tenant TYPE option<int>;
DEFINE FIELD IF NOT EXISTS placement_policy.fallback_policy ON tenant TYPE option<object>;
DEFINE FIELD IF NOT EXISTS placement_policy.fallback_policy.relax_order ON tenant TYPE option<array<string>>;
DEFINE FIELD IF NOT EXISTS placement_policy.fallback_policy.max_hops ON tenant TYPE option<int>;
DEFINE FIELD IF NOT EXISTS placement_policy.spread_constraint ON tenant TYPE option<object>;
DEFINE FIELD IF NOT EXISTS placement_policy.spread_constraint.topology_key ON tenant TYPE option<string>;
DEFINE FIELD IF NOT EXISTS placement_policy.spread_constraint.max_skew ON tenant TYPE option<int>;
DEFINE FIELD IF NOT EXISTS placement_policy.strategy ON tenant TYPE option<string>;

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
-- D#4 soft-delete: tombstone (None = active row、UPDATE で time::now() を入れる)
DEFINE FIELD IF NOT EXISTS deleted_at ON project TYPE option<datetime>;
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
-- Worker Pool membership 判定と Placement 決定に使うラベル群 (FSC-26 Phase B-1)
DEFINE FIELD IF NOT EXISTS labels ON server TYPE option<object>;
DEFINE FIELD IF NOT EXISTS labels.tier ON server TYPE option<string>;
DEFINE FIELD IF NOT EXISTS labels.region ON server TYPE option<string>;
DEFINE FIELD IF NOT EXISTS labels.class ON server TYPE option<string>;
DEFINE FIELD IF NOT EXISTS labels.arch ON server TYPE option<string>;
DEFINE FIELD IF NOT EXISTS labels.extras ON server TYPE option<object>;
-- 物理リソース上限 (FSC-26 Phase B-1)
DEFINE FIELD IF NOT EXISTS capacity ON server TYPE option<object>;
DEFINE FIELD IF NOT EXISTS capacity.cpu_cores ON server TYPE option<int>;
DEFINE FIELD IF NOT EXISTS capacity.memory_gb ON server TYPE option<int>;
DEFINE FIELD IF NOT EXISTS capacity.disk_gb ON server TYPE option<int>;
-- 現在使用中のリソース (FSC-26 Phase B-1、2-phase placement で increment/decrement)
DEFINE FIELD IF NOT EXISTS allocated ON server TYPE option<object>;
DEFINE FIELD IF NOT EXISTS allocated.cpu_cores ON server TYPE option<int>;
DEFINE FIELD IF NOT EXISTS allocated.memory_gb ON server TYPE option<int>;
-- 論理スケジューリング状態 (FSC-26 Phase B-1、物理 status と直交)
-- 'schedulable' | 'cordon' | 'drain' — k8s Node Condition + cordon/drain 相当
DEFINE FIELD IF NOT EXISTS scheduling ON server TYPE option<string> DEFAULT 'schedulable';

-- FSC-26 Phase B-2: server → worker_pool 参照
DEFINE FIELD IF NOT EXISTS pool_id ON server TYPE option<record<worker_pool>>;

-- Server lifecycle / infra metadata (single-table model, additive)
-- 全 field 可変、agent / controller / user CLI が役割ごとに書き込む
-- 'running' | 'cordoned' | 'decommissioned' — desired_state は user の意図、status は observed (heartbeat 起源)
DEFINE FIELD IF NOT EXISTS desired_state ON server TYPE option<string> DEFAULT 'running';
DEFINE FIELD IF NOT EXISTS purpose ON server TYPE option<string>;
DEFINE FIELD IF NOT EXISTS owner ON server TYPE option<string>;

-- Sakura cloud infrastructure metadata (provisioning controller が書き込み)
DEFINE FIELD IF NOT EXISTS sakura ON server TYPE option<object>;
DEFINE FIELD IF NOT EXISTS sakura.server_id ON server TYPE option<int>;
DEFINE FIELD IF NOT EXISTS sakura.disk_id ON server TYPE option<int>;
DEFINE FIELD IF NOT EXISTS sakura.archive_id ON server TYPE option<int>;
DEFINE FIELD IF NOT EXISTS sakura.zone ON server TYPE option<string>;

-- Tailscale tailnet metadata (provisioning controller が書き込み)
DEFINE FIELD IF NOT EXISTS tailscale ON server TYPE option<object>;
DEFINE FIELD IF NOT EXISTS tailscale.hostname ON server TYPE option<string>;
DEFINE FIELD IF NOT EXISTS tailscale.tailnet_ip ON server TYPE option<string>;
DEFINE FIELD IF NOT EXISTS tailscale.node_id ON server TYPE option<string>;
DEFINE FIELD IF NOT EXISTS tailscale.joined_at ON server TYPE option<datetime>;

-- DNS / public network metadata (Cloudflare 等の外部 DNS)
DEFINE FIELD IF NOT EXISTS dns ON server TYPE option<object>;
DEFINE FIELD IF NOT EXISTS dns.fqdn ON server TYPE option<string>;
DEFINE FIELD IF NOT EXISTS dns.cloudflare_record_id ON server TYPE option<string>;
DEFINE FIELD IF NOT EXISTS dns.public_ipv4 ON server TYPE option<string>;
DEFINE FIELD IF NOT EXISTS dns.public_ipv6 ON server TYPE option<string>;

-- Lifecycle audit (immutable once set)
-- replaced_from は graceful re-spawn 時に旧 record を指す (chain で履歴追える)
DEFINE FIELD IF NOT EXISTS lifecycle ON server TYPE option<object>;
DEFINE FIELD IF NOT EXISTS lifecycle.spawned_at ON server TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS lifecycle.last_replaced_at ON server TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS lifecycle.decommissioned_at ON server TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS lifecycle.replaced_from ON server TYPE option<record<server>>;

DEFINE FIELD IF NOT EXISTS created_at ON server TYPE option<datetime> DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS updated_at ON server TYPE option<datetime> DEFAULT time::now();
-- D#4 soft-delete: tombstone (None = active row)
DEFINE FIELD IF NOT EXISTS deleted_at ON server TYPE option<datetime>;
DEFINE INDEX IF NOT EXISTS idx_server_tenant_slug ON server FIELDS tenant, slug UNIQUE;

-- FSC-26 Phase B-2: Worker Pool (複数 Server を label で束ねた論理グループ)
DEFINE TABLE IF NOT EXISTS worker_pool SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS name ON worker_pool TYPE string;
DEFINE FIELD IF NOT EXISTS description ON worker_pool TYPE option<string>;
DEFINE FIELD IF NOT EXISTS required_labels ON worker_pool TYPE option<object>;
-- wildcard: required_labels は自由 key の string map として扱う
DEFINE FIELD IF NOT EXISTS required_labels.* ON worker_pool TYPE string;
DEFINE FIELD IF NOT EXISTS preferred_labels ON worker_pool TYPE option<object>;
DEFINE FIELD IF NOT EXISTS preferred_labels.* ON worker_pool TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON worker_pool TYPE option<datetime> DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS updated_at ON worker_pool TYPE option<datetime> DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS idx_worker_pool_name ON worker_pool FIELDS name UNIQUE;

-- 初期データ: migration 時の移行先 pool (冪等に INSERT IGNORE)
INSERT IGNORE INTO worker_pool {
    id: worker_pool:default,
    name: 'default',
    description: 'migration 時の移行先。全 server をここに割当',
    required_labels: {},
    preferred_labels: {}
};

-- 既存 server で pool_id 未設定のものを default pool に紐付け (idempotent、launch 後も安全)
UPDATE server SET pool_id = worker_pool:default WHERE pool_id = NONE;

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
-- D#4 soft-delete: tombstone (None = active row)
DEFINE FIELD IF NOT EXISTS deleted_at ON dns_record TYPE option<datetime>;
DEFINE INDEX IF NOT EXISTS idx_dns_record_name ON dns_record FIELDS name UNIQUE;

DEFINE TABLE IF NOT EXISTS tenant_user SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS auth0_sub ON tenant_user TYPE string;
DEFINE FIELD IF NOT EXISTS tenant ON tenant_user TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS role ON tenant_user TYPE string DEFAULT 'member';
DEFINE FIELD IF NOT EXISTS created_at ON tenant_user TYPE option<datetime> DEFAULT time::now();
-- D#4 soft-delete: tombstone (None = active row)
DEFINE FIELD IF NOT EXISTS deleted_at ON tenant_user TYPE option<datetime>;
-- B#2 fix: 旧 idx_tenant_user_sub は auth0_sub 単独 UNIQUE で multi-tenant
-- ユーザー (1 Auth0 user → 複数 tenant) を block していた。 (auth0_sub, tenant)
-- 複合 UNIQUE に切替。 REMOVE は schema 制約のみで data は不変。
REMOVE INDEX IF EXISTS idx_tenant_user_sub ON tenant_user;
DEFINE INDEX IF NOT EXISTS idx_tenant_user_sub_tenant ON tenant_user FIELDS auth0_sub, tenant UNIQUE;
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

-- CP-010: Volume (Persistence Volume Tier P-1, 2026-04-23)
-- 詳細設計: fleetstage repo docs/design/20-persistence-volume-tier.md
DEFINE TABLE IF NOT EXISTS volume SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS tenant ON volume TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS project ON volume TYPE option<record<project>>;
DEFINE FIELD IF NOT EXISTS stage ON volume TYPE option<record<stage>>;
DEFINE FIELD IF NOT EXISTS slug ON volume TYPE string;
DEFINE FIELD IF NOT EXISTS tier ON volume TYPE string
  ASSERT $value IN ["ephemeral", "local-volume", "attached-disk", "object-backed", "managed-cloud"];
DEFINE FIELD IF NOT EXISTS size_bytes ON volume TYPE option<int>;
DEFINE FIELD IF NOT EXISTS mount ON volume TYPE string;
DEFINE FIELD IF NOT EXISTS server ON volume TYPE option<record<server>>;
DEFINE FIELD IF NOT EXISTS provider ON volume TYPE string;
DEFINE FIELD IF NOT EXISTS provider_resource_id ON volume TYPE option<string>;
DEFINE FIELD IF NOT EXISTS encryption ON volume TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS backup_policy ON volume TYPE option<object>;
DEFINE FIELD IF NOT EXISTS backup_policy.schedule ON volume TYPE option<string>;
DEFINE FIELD IF NOT EXISTS backup_policy.logical ON volume TYPE option<bool>;
DEFINE FIELD IF NOT EXISTS backup_policy.physical_snapshot ON volume TYPE option<bool>;
DEFINE FIELD IF NOT EXISTS backup_policy.retention_days ON volume TYPE option<int>;
DEFINE FIELD IF NOT EXISTS backup_policy.destination ON volume TYPE option<string>;
DEFINE FIELD IF NOT EXISTS bring_your_own ON volume TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS state ON volume TYPE string
  ASSERT $value IN ["provisioning", "attached", "detached", "archived", "migrating", "failed"];
DEFINE FIELD IF NOT EXISTS created_at ON volume TYPE option<datetime> DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS updated_at ON volume TYPE option<datetime> DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS idx_volume_tenant_slug ON volume FIELDS tenant, slug UNIQUE;

-- CP-010b: VolumeSnapshot
DEFINE TABLE IF NOT EXISTS volume_snapshot SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS volume ON volume_snapshot TYPE record<volume>;
DEFINE FIELD IF NOT EXISTS kind ON volume_snapshot TYPE string
  ASSERT $value IN ["disk-snapshot", "surreal-export", "rsync-tar"];
DEFINE FIELD IF NOT EXISTS provider_resource_id ON volume_snapshot TYPE option<string>;
DEFINE FIELD IF NOT EXISTS location_url ON volume_snapshot TYPE option<string>;
DEFINE FIELD IF NOT EXISTS size_bytes ON volume_snapshot TYPE option<int>;
DEFINE FIELD IF NOT EXISTS taken_at ON volume_snapshot TYPE option<datetime> DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS retention_until ON volume_snapshot TYPE option<datetime>;
DEFINE INDEX IF NOT EXISTS idx_volume_snapshot_volume ON volume_snapshot FIELDS volume;

-- CP-011: BuildJob (Build Tier v1 MVP, 2026-04-23)
-- 詳細設計: fleetstage repo docs/design/30-build-tier.md
DEFINE TABLE IF NOT EXISTS build_job SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS tenant ON build_job TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS project ON build_job TYPE option<record<project>>;
DEFINE FIELD IF NOT EXISTS kind ON build_job TYPE string
  ASSERT $value IN ["docker-image", "cargo-binary", "static-site"];
DEFINE FIELD IF NOT EXISTS source ON build_job TYPE object;
DEFINE FIELD IF NOT EXISTS source.git_url ON build_job TYPE string;
DEFINE FIELD IF NOT EXISTS source.git_ref ON build_job TYPE string DEFAULT 'main';
DEFINE FIELD IF NOT EXISTS source.dockerfile ON build_job TYPE option<string>;
DEFINE FIELD IF NOT EXISTS target ON build_job TYPE object;
DEFINE FIELD IF NOT EXISTS target.image ON build_job TYPE option<string>;
DEFINE FIELD IF NOT EXISTS target.registry_secret ON build_job TYPE option<string>;
DEFINE FIELD IF NOT EXISTS state ON build_job TYPE string
  ASSERT $value IN ["queued", "assigned", "cloning", "building", "pushing", "success", "failed", "cancelled"];
DEFINE FIELD IF NOT EXISTS server ON build_job TYPE option<record<server>>;
DEFINE FIELD IF NOT EXISTS logs_url ON build_job TYPE option<string>;
DEFINE FIELD IF NOT EXISTS submitted_at ON build_job TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS started_at ON build_job TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS finished_at ON build_job TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS duration_seconds ON build_job TYPE option<int>;
DEFINE INDEX IF NOT EXISTS idx_build_job_tenant_state ON build_job FIELDS tenant, state;
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
            placement_policy: None,
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
                placement_policy: None,
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
            deleted_at: None,
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
                placement_policy: None,
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
            // FSC-26 Phase B-1: 新 field は全て None で backward-compat 動作を確認
            labels: None,
            capacity: None,
            allocated: None,
            scheduling: None,
            pool_id: None,
            desired_state: None,
            purpose: None,
            owner: None,
            sakura: None,
            tailscale: None,
            dns: None,
            lifecycle: None,
            created_at: None,
            updated_at: None,
            deleted_at: None,
        };
        let created = db.register_server(&server).await.unwrap();
        assert!(created.id.is_some());
        // Backward-compat: 新 field が None でも register 成功
        assert!(created.labels.is_none());
        assert!(created.capacity.is_none());

        db.update_server_heartbeat("vps-01").await.unwrap();

        let servers = db.list_servers_by_tenant("anycreative").await.unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].slug, "vps-01");
    }

    // FSC-26 Phase B-1: Server に labels / capacity / allocated / scheduling を
    // 付与して round-trip することを確認する
    #[tokio::test]
    async fn test_server_with_labels_crud() {
        use crate::model::{ServerAllocated, ServerCapacity, ServerLabels, scheduling_state};

        let db = Database::connect_memory().await.unwrap();

        let tenant = db
            .create_tenant(&Tenant {
                id: None,
                slug: "acme".into(),
                name: "Acme Corp".into(),
                auth0_org_id: None,
                plan: "pro".into(),
                dns_provider: None,
                dns_domain: None,
                dns_zone_id: None,
                dns_api_token_encrypted: None,
                placement_policy: None,
                created_at: None,
                updated_at: None,
            })
            .await
            .unwrap();

        let server = Server {
            id: None,
            tenant: tenant.id.unwrap(),
            slug: "worker-pro-01".into(),
            provider: "sakura-cloud".into(),
            plan: Some("4G/4CPU".into()),
            ssh_host: "100.64.0.10".into(),
            ssh_user: "root".into(),
            deploy_path: "/opt/fleet".into(),
            status: "online".into(),
            provision_version: None,
            tool_versions: None,
            last_heartbeat_at: None,
            labels: Some(ServerLabels {
                tier: Some("pro".into()),
                region: Some("tokyo".into()),
                class: Some("dedicated".into()),
                arch: Some("arm64".into()),
                extras: None,
            }),
            capacity: Some(ServerCapacity {
                cpu_cores: Some(4),
                memory_gb: Some(8),
                disk_gb: Some(80),
            }),
            allocated: Some(ServerAllocated {
                cpu_cores: Some(0),
                memory_gb: Some(0),
            }),
            scheduling: Some(scheduling_state::SCHEDULABLE.into()),
            pool_id: None,
            desired_state: None,
            purpose: None,
            owner: None,
            sakura: None,
            tailscale: None,
            dns: None,
            lifecycle: None,
            created_at: None,
            updated_at: None,
            deleted_at: None,
        };
        let created = db.register_server(&server).await.unwrap();
        assert!(created.id.is_some());

        // Labels round-trip
        let labels = created.labels.expect("labels should be persisted");
        assert_eq!(labels.tier.as_deref(), Some("pro"));
        assert_eq!(labels.region.as_deref(), Some("tokyo"));
        assert_eq!(labels.class.as_deref(), Some("dedicated"));
        assert_eq!(labels.arch.as_deref(), Some("arm64"));

        // Capacity round-trip
        let capacity = created.capacity.expect("capacity should be persisted");
        assert_eq!(capacity.cpu_cores, Some(4));
        assert_eq!(capacity.memory_gb, Some(8));
        assert_eq!(capacity.disk_gb, Some(80));

        // Allocated round-trip (初期値 0)
        let allocated = created.allocated.expect("allocated should be persisted");
        assert_eq!(allocated.cpu_cores, Some(0));
        assert_eq!(allocated.memory_gb, Some(0));

        // Scheduling state
        assert_eq!(
            created.scheduling.as_deref(),
            Some(scheduling_state::SCHEDULABLE)
        );

        // リスト取得でも labels が保持されていること
        let servers = db.list_servers_by_tenant("acme").await.unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(
            servers[0].labels.as_ref().and_then(|l| l.tier.as_deref()),
            Some("pro")
        );
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
            placement_policy: None,
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
            deleted_at: None,
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
            deleted_at: None,
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
                deleted_at: None,
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
                labels: None,
                capacity: None,
                allocated: None,
                scheduling: None,
                pool_id: None,
                desired_state: None,
                purpose: None,
                owner: None,
                sakura: None,
                tailscale: None,
                dns: None,
                lifecycle: None,
                created_at: None,
                updated_at: None,
                deleted_at: None,
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
            labels: None,
            capacity: None,
            allocated: None,
            scheduling: None,
            pool_id: None,
            desired_state: None,
            purpose: None,
            owner: None,
            sakura: None,
            tailscale: None,
            dns: None,
            lifecycle: None,
            created_at: None,
            updated_at: None,
            deleted_at: None,
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
        let count = db
            .count_active_alerts_by_server("vps-01", "test-tenant")
            .await
            .unwrap();
        assert_eq!(count, 1);

        // 別タイプのアラートを追加
        let alert3 = Alert {
            alert_type: "unhealthy".into(),
            severity: "warning".into(),
            message: "コンテナ web が unhealthy".into(),
            ..alert.clone()
        };
        db.upsert_alert(&alert3).await.unwrap();

        let count = db
            .count_active_alerts_by_server("vps-01", "test-tenant")
            .await
            .unwrap();
        assert_eq!(count, 2);

        // 解決
        db.resolve_alerts("vps-01", "web").await.unwrap();
        let count = db
            .count_active_alerts_by_server("vps-01", "test-tenant")
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    // ─────────────────────────────────────────
    // FSC-26 Phase B-2 / B-3: Worker Pool + Placement Policy
    // ─────────────────────────────────────────

    /// Worker Pool の CRUD round-trip を確認 (FSC-26 Phase B-2)
    #[tokio::test]
    async fn test_worker_pool_crud() {
        let db = Database::connect_memory().await.unwrap();

        // schema apply で default pool が自動投入されていることを確認
        let default_pool = db
            .get_worker_pool_by_name("default")
            .await
            .unwrap()
            .expect("default pool は schema apply で投入される");
        assert_eq!(default_pool.name, "default");
        assert!(default_pool.id.is_some());

        // 追加 pool を label 付きで作成
        let pro_pool = WorkerPool {
            id: None,
            name: "sakura-pro-tokyo".into(),
            description: Some("Pro tier 向け、東京リージョン専用".into()),
            required_labels: Some(std::collections::BTreeMap::from([
                ("tier".into(), "pro".into()),
                ("region".into(), "tokyo".into()),
            ])),
            preferred_labels: Some(std::collections::BTreeMap::from([(
                "class".into(),
                "dedicated".into(),
            )])),
            created_at: None,
            updated_at: None,
        };
        let created = db.create_worker_pool(&pro_pool).await.unwrap();
        assert!(created.id.is_some());
        assert_eq!(created.name, "sakura-pro-tokyo");

        // name で取得できてラベルも round-trip する
        let found = db
            .get_worker_pool_by_name("sakura-pro-tokyo")
            .await
            .unwrap()
            .expect("pro pool should be persisted");
        assert_eq!(
            found.description.as_deref(),
            Some("Pro tier 向け、東京リージョン専用")
        );
        let required = found.required_labels.expect("required_labels persisted");
        assert_eq!(required.get("tier").map(String::as_str), Some("pro"));
        assert_eq!(required.get("region").map(String::as_str), Some("tokyo"));

        // list で default + pro の 2 件
        let pools = db.list_worker_pools().await.unwrap();
        assert_eq!(pools.len(), 2);

        // name UNIQUE: 同名で作成すると失敗
        let dup = db.create_worker_pool(&pro_pool).await;
        assert!(dup.is_err(), "same name should fail due to UNIQUE index");
    }

    /// Server に pool_id を紐付けて round-trip (FSC-26 Phase B-2)
    #[tokio::test]
    async fn test_server_pool_association() {
        let db = Database::connect_memory().await.unwrap();
        let tenant = create_test_tenant(&db).await;

        // default pool を取得
        let default_pool = db
            .get_worker_pool_by_name("default")
            .await
            .unwrap()
            .expect("default pool exists");

        // pool_id を持つ server を登録
        let server = Server {
            id: None,
            tenant: tenant.id.clone().unwrap(),
            slug: "worker-01".into(),
            provider: "sakura-cloud".into(),
            plan: Some("4G/4CPU".into()),
            ssh_host: "100.64.0.20".into(),
            ssh_user: "root".into(),
            deploy_path: "/opt/fleet".into(),
            status: "online".into(),
            provision_version: None,
            tool_versions: None,
            last_heartbeat_at: None,
            labels: None,
            capacity: None,
            allocated: None,
            scheduling: None,
            pool_id: default_pool.id.clone(),
            desired_state: None,
            purpose: None,
            owner: None,
            sakura: None,
            tailscale: None,
            dns: None,
            lifecycle: None,
            created_at: None,
            updated_at: None,
            deleted_at: None,
        };
        let created = db.register_server(&server).await.unwrap();
        assert_eq!(
            created.pool_id, default_pool.id,
            "pool_id should round-trip"
        );

        // pool_id を別 pool に付け替え
        let pro_pool = db
            .create_worker_pool(&WorkerPool {
                id: None,
                name: "pro".into(),
                description: None,
                required_labels: None,
                preferred_labels: None,
                created_at: None,
                updated_at: None,
            })
            .await
            .unwrap();
        db.update_server_pool("worker-01", pro_pool.id.as_ref().unwrap())
            .await
            .unwrap();

        let moved = db
            .get_server_by_slug("worker-01")
            .await
            .unwrap()
            .expect("server exists");
        assert_eq!(moved.pool_id, pro_pool.id);
    }

    /// Tenant に placement_policy を設定して round-trip (FSC-26 Phase B-3)
    #[tokio::test]
    async fn test_tenant_placement_policy() {
        use crate::model::{
            FallbackPolicy, PlacementPolicy, ResourceQuota, SpreadConstraint, placement_strategy,
        };

        let db = Database::connect_memory().await.unwrap();

        // 最初は policy なし
        let tenant = Tenant {
            id: None,
            slug: "pro-customer".into(),
            name: "Pro Customer Inc".into(),
            auth0_org_id: None,
            plan: "pro".into(),
            dns_provider: None,
            dns_domain: None,
            dns_zone_id: None,
            dns_api_token_encrypted: None,
            placement_policy: None,
            created_at: None,
            updated_at: None,
        };
        let created = db.create_tenant(&tenant).await.unwrap();
        assert!(created.placement_policy.is_none());

        // Placement Policy を更新
        let policy = PlacementPolicy {
            tier: Some("pro".into()),
            preferred_labels: Some(std::collections::BTreeMap::from([(
                "region".into(),
                "tokyo".into(),
            )])),
            resource_quota: Some(ResourceQuota {
                max_stages: Some(10),
                max_services_per_stage: Some(20),
                cpu_cores: Some(16),
                memory_gb: Some(64),
            }),
            fallback_policy: Some(FallbackPolicy {
                relax_order: Some(vec!["class".into(), "region".into()]),
                max_hops: Some(2),
            }),
            spread_constraint: Some(SpreadConstraint {
                topology_key: Some("region".into()),
                max_skew: Some(1),
            }),
            strategy: Some(placement_strategy::SPREAD_ACROSS_POOL.into()),
        };
        db.update_tenant_placement_policy("pro-customer", &policy)
            .await
            .unwrap();

        // Round-trip 確認
        let found = db
            .get_tenant_by_slug("pro-customer")
            .await
            .unwrap()
            .expect("tenant exists");
        let loaded = found.placement_policy.expect("policy should be persisted");
        assert_eq!(loaded.tier.as_deref(), Some("pro"));
        assert_eq!(
            loaded.strategy.as_deref(),
            Some(placement_strategy::SPREAD_ACROSS_POOL)
        );

        let quota = loaded.resource_quota.expect("quota persisted");
        assert_eq!(quota.max_stages, Some(10));
        assert_eq!(quota.cpu_cores, Some(16));

        let fallback = loaded.fallback_policy.expect("fallback persisted");
        assert_eq!(
            fallback.relax_order.as_deref(),
            Some(&["class".to_string(), "region".to_string()][..])
        );
        assert_eq!(fallback.max_hops, Some(2));

        let spread = loaded.spread_constraint.expect("spread persisted");
        assert_eq!(spread.topology_key.as_deref(), Some("region"));
        assert_eq!(spread.max_skew, Some(1));
    }

    // ─────────────────────────────────────────
    // Volume tests (Persistence Volume Tier P-1, 2026-04-23)
    // ─────────────────────────────────────────

    async fn seed_tenant_and_server(db: &Database) -> (RecordId, RecordId) {
        let tenant = db
            .create_tenant(&Tenant {
                id: None,
                slug: "chronista-club".into(),
                name: "Chronista Club".into(),
                auth0_org_id: None,
                plan: "enterprise".into(),
                dns_provider: None,
                dns_domain: None,
                dns_zone_id: None,
                dns_api_token_encrypted: None,
                placement_policy: None,
                created_at: None,
                updated_at: None,
            })
            .await
            .unwrap();
        let server = db
            .register_server(&Server {
                id: None,
                tenant: tenant.id.clone().unwrap(),
                slug: "creo-prod".into(),
                provider: "sakura-cloud".into(),
                plan: None,
                ssh_host: "10.0.0.1".into(),
                ssh_user: "root".into(),
                deploy_path: "/opt/apps".into(),
                status: "offline".into(),
                provision_version: None,
                tool_versions: None,
                last_heartbeat_at: None,
                labels: None,
                capacity: None,
                allocated: None,
                scheduling: None,
                pool_id: None,
                desired_state: None,
                purpose: None,
                owner: None,
                sakura: None,
                tailscale: None,
                dns: None,
                lifecycle: None,
                created_at: None,
                updated_at: None,
                deleted_at: None,
            })
            .await
            .unwrap();
        (tenant.id.unwrap(), server.id.unwrap())
    }

    #[tokio::test]
    async fn volume_tier_constants_match_schema_assertions() {
        // schema の ASSERT $value IN [...] 列と volume_tier::ALL が一致することを保証する
        // (リネーム・追加時に片方だけ更新して drift する事故を予防)
        let expected = [
            "ephemeral",
            "local-volume",
            "attached-disk",
            "object-backed",
            "managed-cloud",
        ];
        assert_eq!(volume_tier::ALL, expected);
        for tier in &expected {
            assert!(volume_tier::is_valid(tier));
        }
        assert!(!volume_tier::is_valid("unknown-tier"));
    }

    #[tokio::test]
    async fn volume_state_constants_match_schema_assertions() {
        let expected = [
            "provisioning",
            "attached",
            "detached",
            "archived",
            "migrating",
            "failed",
        ];
        assert_eq!(volume_state::ALL, expected);
        for s in &expected {
            assert!(volume_state::is_valid(s));
        }
        assert!(!volume_state::is_valid("deleted"));
    }

    #[tokio::test]
    async fn adopt_volume_creates_byo_entry_without_touching_data() {
        let db = Database::connect_memory().await.unwrap();
        let (tenant_id, server_id) = seed_tenant_and_server(&db).await;

        // Creo 既存の SurrealDB ディスクを local-volume として adopt
        let adopted = db
            .adopt_volume(
                &tenant_id,
                &server_id,
                "surrealdb-legacy",
                "/var/lib/surrealdb/prod",
                volume_tier::LOCAL_VOLUME,
            )
            .await
            .unwrap();

        assert!(adopted.id.is_some());
        assert_eq!(adopted.slug, "surrealdb-legacy");
        assert_eq!(adopted.tier, "local-volume");
        assert_eq!(adopted.mount, "/var/lib/surrealdb/prod");
        assert!(adopted.bring_your_own, "adopt は BYO flag を立てる");
        assert_eq!(adopted.state, "attached");
        assert_eq!(adopted.provider, "local");
        assert!(adopted.size_bytes.is_none(), "BYO は size unknown");
        assert!(adopted.server.is_some());
    }

    #[tokio::test]
    async fn list_volumes_returns_tenant_scoped_results() {
        let db = Database::connect_memory().await.unwrap();
        let (tenant_id, server_id) = seed_tenant_and_server(&db).await;

        db.adopt_volume(
            &tenant_id,
            &server_id,
            "vol-a",
            "/mnt/a",
            volume_tier::LOCAL_VOLUME,
        )
        .await
        .unwrap();
        db.adopt_volume(
            &tenant_id,
            &server_id,
            "vol-b",
            "/mnt/b",
            volume_tier::ATTACHED_DISK,
        )
        .await
        .unwrap();

        let volumes = db.list_volumes_by_tenant(&tenant_id).await.unwrap();
        assert_eq!(volumes.len(), 2);
        let slugs: Vec<_> = volumes.iter().map(|v| v.slug.as_str()).collect();
        assert_eq!(slugs, vec!["vol-a", "vol-b"]);

        let fetched = db
            .get_volume_by_slug(&tenant_id, "vol-a")
            .await
            .unwrap()
            .expect("vol-a 取得できるべき");
        assert_eq!(fetched.mount, "/mnt/a");
    }

    #[tokio::test]
    async fn update_volume_state_transitions_through_lifecycle() {
        let db = Database::connect_memory().await.unwrap();
        let (tenant_id, server_id) = seed_tenant_and_server(&db).await;

        let vol = db
            .adopt_volume(
                &tenant_id,
                &server_id,
                "vol-x",
                "/mnt/x",
                volume_tier::LOCAL_VOLUME,
            )
            .await
            .unwrap();
        let vol_id = vol.id.unwrap();

        db.update_volume_state(&vol_id, volume_state::DETACHED)
            .await
            .unwrap();
        db.update_volume_state(&vol_id, volume_state::ARCHIVED)
            .await
            .unwrap();

        let after = db
            .get_volume_by_slug(&tenant_id, "vol-x")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            after.state, "archived",
            "削除禁止原則により最終状態は archived で保持される"
        );
    }

    #[tokio::test]
    async fn update_volume_state_rejects_invalid_value() {
        let db = Database::connect_memory().await.unwrap();
        let (tenant_id, server_id) = seed_tenant_and_server(&db).await;

        let vol = db
            .adopt_volume(
                &tenant_id,
                &server_id,
                "vol-y",
                "/mnt/y",
                volume_tier::LOCAL_VOLUME,
            )
            .await
            .unwrap();
        let vol_id = vol.id.unwrap();

        let err = db
            .update_volume_state(&vol_id, "deleted")
            .await
            .expect_err("invalid state は拒否されるべき");
        assert!(err.to_string().contains("invalid volume state"));
    }

    #[tokio::test]
    async fn create_volume_rejects_invalid_tier() {
        let db = Database::connect_memory().await.unwrap();
        let (tenant_id, _) = seed_tenant_and_server(&db).await;

        let bad = Volume {
            id: None,
            tenant: tenant_id,
            project: None,
            stage: None,
            slug: "bad".into(),
            tier: "nonexistent-tier".into(),
            size_bytes: None,
            mount: "/mnt/bad".into(),
            server: None,
            provider: "local".into(),
            provider_resource_id: None,
            encryption: false,
            backup_policy: None,
            bring_your_own: false,
            state: volume_state::PROVISIONING.to_string(),
            created_at: None,
            updated_at: None,
        };
        let err = db.create_volume(&bad).await.expect_err("invalid tier");
        assert!(err.to_string().contains("invalid volume tier"));
    }

    #[tokio::test]
    async fn volume_snapshot_crud_records_disk_snapshot() {
        let db = Database::connect_memory().await.unwrap();
        let (tenant_id, server_id) = seed_tenant_and_server(&db).await;

        let vol = db
            .adopt_volume(
                &tenant_id,
                &server_id,
                "vol-snap",
                "/mnt/snap",
                volume_tier::ATTACHED_DISK,
            )
            .await
            .unwrap();
        let vol_id = vol.id.unwrap();

        let snap = db
            .create_volume_snapshot(&VolumeSnapshot {
                id: None,
                volume: vol_id.clone(),
                kind: volume_snapshot_kind::DISK_SNAPSHOT.to_string(),
                provider_resource_id: Some("sakura:archive:xyz".into()),
                location_url: None,
                size_bytes: Some(12_345_678),
                taken_at: None,
                retention_until: None,
            })
            .await
            .unwrap();
        assert!(snap.id.is_some());

        let list = db.list_volume_snapshots(&vol_id).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind, "disk-snapshot");
        assert_eq!(list[0].size_bytes, Some(12_345_678));
    }

    #[tokio::test]
    async fn volume_snapshot_rejects_invalid_kind() {
        let db = Database::connect_memory().await.unwrap();
        let (tenant_id, server_id) = seed_tenant_and_server(&db).await;

        let vol = db
            .adopt_volume(
                &tenant_id,
                &server_id,
                "vol-k",
                "/mnt/k",
                volume_tier::LOCAL_VOLUME,
            )
            .await
            .unwrap();
        let err = db
            .create_volume_snapshot(&VolumeSnapshot {
                id: None,
                volume: vol.id.unwrap(),
                kind: "unknown-kind".into(),
                provider_resource_id: None,
                location_url: None,
                size_bytes: None,
                taken_at: None,
                retention_until: None,
            })
            .await
            .expect_err("invalid kind");
        assert!(err.to_string().contains("invalid volume snapshot kind"));
    }

    // ─────────────────────────────────────────
    // BuildJob tests (Build Tier v1 MVP, 2026-04-23)
    // ─────────────────────────────────────────

    fn make_build_job(tenant_id: RecordId) -> BuildJob {
        BuildJob {
            id: None,
            tenant: tenant_id,
            project: None,
            kind: build_job_kind::DOCKER_IMAGE.to_string(),
            source: BuildSource {
                git_url: "https://github.com/chronista-club/fleetflow".to_string(),
                git_ref: "main".to_string(),
                dockerfile: Some("infra/Dockerfile.fleetflowd".to_string()),
            },
            target: BuildTarget {
                image: Some("ghcr.io/chronista-club/fleetflowd:test".to_string()),
                registry_secret: None,
            },
            state: build_job_state::QUEUED.to_string(),
            server: None,
            logs_url: None,
            submitted_at: chrono::Utc::now(),
            started_at: None,
            finished_at: None,
            duration_seconds: None,
        }
    }

    #[tokio::test]
    async fn build_job_state_constants_match_schema_assertions() {
        // schema の ASSERT $value IN [...] 列と build_job_state::ALL が一致することを保証する
        // (リネーム・追加時に片方だけ更新して drift する事故を予防)
        let expected = [
            "queued",
            "assigned",
            "cloning",
            "building",
            "pushing",
            "success",
            "failed",
            "cancelled",
        ];
        assert_eq!(build_job_state::ALL, expected);
        for s in &expected {
            assert!(build_job_state::is_valid(s));
        }
        assert!(!build_job_state::is_valid("unknown-state"));
    }

    #[tokio::test]
    async fn build_job_kind_constants_match_schema_assertions() {
        let expected = ["docker-image", "cargo-binary", "static-site"];
        assert_eq!(build_job_kind::ALL, expected);
        for k in &expected {
            assert!(build_job_kind::is_valid(k));
        }
        assert!(!build_job_kind::is_valid("unknown-kind"));
    }

    #[tokio::test]
    async fn create_build_job_succeeds_minimum_fields() {
        let db = Database::connect_memory().await.unwrap();
        let (tenant_id, _) = seed_tenant_and_server(&db).await;

        let job = make_build_job(tenant_id);
        let created = db.create_build_job(&job).await.unwrap();

        assert!(created.id.is_some());
        assert_eq!(created.kind, "docker-image");
        assert_eq!(created.state, "queued");
        assert_eq!(
            created.source.git_url,
            "https://github.com/chronista-club/fleetflow"
        );
        assert_eq!(created.source.git_ref, "main");
        assert_eq!(
            created.target.image.as_deref(),
            Some("ghcr.io/chronista-club/fleetflowd:test")
        );
    }

    #[tokio::test]
    async fn list_build_jobs_returns_tenant_scoped() {
        let db = Database::connect_memory().await.unwrap();
        let (tenant_id, _) = seed_tenant_and_server(&db).await;

        // 別テナントを作成して別 job を登録
        let other_tenant = db
            .create_tenant(&Tenant {
                id: None,
                slug: "other-tenant".into(),
                name: "Other Tenant".into(),
                auth0_org_id: None,
                plan: "self-hosted".into(),
                dns_provider: None,
                dns_domain: None,
                dns_zone_id: None,
                dns_api_token_encrypted: None,
                placement_policy: None,
                created_at: None,
                updated_at: None,
            })
            .await
            .unwrap();
        let other_tenant_id = other_tenant.id.unwrap();

        db.create_build_job(&make_build_job(tenant_id.clone()))
            .await
            .unwrap();
        db.create_build_job(&make_build_job(tenant_id.clone()))
            .await
            .unwrap();
        db.create_build_job(&make_build_job(other_tenant_id))
            .await
            .unwrap();

        let jobs = db.list_build_jobs_by_tenant(&tenant_id).await.unwrap();
        assert_eq!(jobs.len(), 2, "テナントスコープで 2 件のみ取得できるべき");
    }

    #[tokio::test]
    async fn update_build_job_state_transitions_through_lifecycle() {
        let db = Database::connect_memory().await.unwrap();
        let (tenant_id, _) = seed_tenant_and_server(&db).await;

        let job = db
            .create_build_job(&make_build_job(tenant_id))
            .await
            .unwrap();
        let job_id = job.id.unwrap();

        db.update_build_job_state(&job_id, build_job_state::ASSIGNED)
            .await
            .unwrap();
        db.update_build_job_state(&job_id, build_job_state::CLONING)
            .await
            .unwrap();
        db.update_build_job_state(&job_id, build_job_state::BUILDING)
            .await
            .unwrap();
        db.update_build_job_state(&job_id, build_job_state::PUSHING)
            .await
            .unwrap();
        db.update_build_job_state(&job_id, build_job_state::SUCCESS)
            .await
            .unwrap();

        let updated = db
            .get_build_job_by_id(&job_id)
            .await
            .unwrap()
            .expect("job 存在");
        assert_eq!(updated.state, "success");
    }

    #[tokio::test]
    async fn create_build_job_rejects_invalid_kind() {
        let db = Database::connect_memory().await.unwrap();
        let (tenant_id, _) = seed_tenant_and_server(&db).await;

        let mut bad = make_build_job(tenant_id);
        bad.kind = "invalid-kind".to_string();

        let err = db
            .create_build_job(&bad)
            .await
            .expect_err("invalid kind は拒否されるべき");
        assert!(err.to_string().contains("invalid build job kind"));
    }

    #[tokio::test]
    async fn update_build_job_state_rejects_invalid_state() {
        let db = Database::connect_memory().await.unwrap();
        let (tenant_id, _) = seed_tenant_and_server(&db).await;

        let job = db
            .create_build_job(&make_build_job(tenant_id))
            .await
            .unwrap();
        let job_id = job.id.unwrap();

        let err = db
            .update_build_job_state(&job_id, "unknown-state")
            .await
            .expect_err("invalid state は拒否されるべき");
        assert!(err.to_string().contains("invalid build job state"));
    }

    // ─────────────────────────────────────────
    // Stage adopt tests (FSC-16, 2026-04-24)
    // ─────────────────────────────────────────

    fn gfp_services() -> Vec<AdoptServiceSpec> {
        vec![
            AdoptServiceSpec {
                slug: "gfp-estimate".into(),
                image: "ghcr.io/anycreative/gfp-estimate:latest".into(),
            },
            AdoptServiceSpec {
                slug: "gfp-web".into(),
                image: "ghcr.io/anycreative/gfp-web:latest".into(),
            },
        ]
    }

    fn adopt_req<'a>(
        tenant_id: &'a RecordId,
        server_id: &'a RecordId,
        project_slug: &'a str,
        project_name: Option<&'a str>,
        stage_slug: &'a str,
        description: Option<&'a str>,
        services: &'a [AdoptServiceSpec],
    ) -> AdoptStageRequest<'a> {
        AdoptStageRequest {
            tenant_id,
            server_id,
            project_slug,
            project_name,
            stage_slug,
            description,
            services,
        }
    }

    #[tokio::test]
    async fn adopt_stage_creates_project_stage_and_services() {
        let db = Database::connect_memory().await.unwrap();
        let (tenant_id, server_id) = seed_tenant_and_server(&db).await;
        let services = gfp_services();

        let outcome = db
            .adopt_stage(&adopt_req(
                &tenant_id,
                &server_id,
                "gfp",
                Some("GFP Live Fleet"),
                "dev",
                Some("GFP dev on fleet-worker-01"),
                &services,
            ))
            .await
            .unwrap();

        assert_eq!(outcome.project.slug, "gfp");
        assert_eq!(outcome.project.name, "GFP Live Fleet");
        assert_eq!(outcome.stage.slug, "dev");
        assert_eq!(
            outcome.stage.description.as_deref(),
            Some("GFP dev on fleet-worker-01")
        );
        assert_eq!(outcome.stage.server.as_ref(), Some(&server_id));
        assert_eq!(outcome.services.len(), 2);
        let slugs: Vec<_> = outcome.services.iter().map(|s| s.slug.as_str()).collect();
        assert_eq!(slugs, vec!["gfp-estimate", "gfp-web"]);
        for s in &outcome.services {
            assert_eq!(s.desired_status, "running");
        }
    }

    #[tokio::test]
    async fn adopt_stage_reuses_existing_project() {
        let db = Database::connect_memory().await.unwrap();
        let (tenant_id, server_id) = seed_tenant_and_server(&db).await;
        let services = gfp_services();

        // 1 回目: project=gfp, stage=dev を adopt (project が新規作成される)
        let first = db
            .adopt_stage(&adopt_req(
                &tenant_id,
                &server_id,
                "gfp",
                Some("GFP"),
                "dev",
                None,
                &services,
            ))
            .await
            .unwrap();
        let project_id_first = first.project.id.clone().unwrap();

        // 2 回目: 同じ project の別 stage を adopt — project は再利用されるべき
        let second = db
            .adopt_stage(&adopt_req(
                &tenant_id, &server_id, "gfp", None, "staging", None, &services,
            ))
            .await
            .unwrap();

        assert_eq!(second.project.id.as_ref(), Some(&project_id_first));
        assert_eq!(second.stage.slug, "staging");
    }

    #[tokio::test]
    async fn adopt_stage_rejects_duplicate_stage_under_same_project() {
        let db = Database::connect_memory().await.unwrap();
        let (tenant_id, server_id) = seed_tenant_and_server(&db).await;
        let services = gfp_services();

        db.adopt_stage(&adopt_req(
            &tenant_id,
            &server_id,
            "gfp",
            Some("GFP"),
            "dev",
            None,
            &services,
        ))
        .await
        .unwrap();

        let err = db
            .adopt_stage(&adopt_req(
                &tenant_id, &server_id, "gfp", None, "dev", None, &services,
            ))
            .await
            .expect_err("同一 project 配下の同 slug は拒否されるべき");
        assert!(
            err.to_string().contains("already exists"),
            "error message should mention already exists, got: {err}"
        );
    }

    #[tokio::test]
    async fn adopt_stage_rejects_empty_slugs() {
        let db = Database::connect_memory().await.unwrap();
        let (tenant_id, server_id) = seed_tenant_and_server(&db).await;
        let services = gfp_services();

        let err = db
            .adopt_stage(&adopt_req(
                &tenant_id, &server_id, "", None, "dev", None, &services,
            ))
            .await
            .expect_err("空 project_slug は拒否されるべき");
        assert!(err.to_string().contains("project_slug must not be empty"));

        let err = db
            .adopt_stage(&adopt_req(
                &tenant_id, &server_id, "gfp", None, "", None, &services,
            ))
            .await
            .expect_err("空 stage_slug は拒否されるべき");
        assert!(err.to_string().contains("stage_slug must not be empty"));
    }

    #[tokio::test]
    async fn adopt_stage_rejects_service_with_empty_image() {
        let db = Database::connect_memory().await.unwrap();
        let (tenant_id, server_id) = seed_tenant_and_server(&db).await;

        let broken = vec![AdoptServiceSpec {
            slug: "gfp-web".into(),
            image: String::new(),
        }];
        let err = db
            .adopt_stage(&adopt_req(
                &tenant_id,
                &server_id,
                "gfp",
                Some("GFP"),
                "dev",
                None,
                &broken,
            ))
            .await
            .expect_err("空 image は拒否されるべき");
        assert!(err.to_string().contains("service image must not be empty"));
    }
}
