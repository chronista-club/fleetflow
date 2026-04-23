//! Build channel handler (Build Tier v1 MVP, 2026-04-23)
//!
//! `fleet cp build <subcommand>` からの Unison Protocol リクエストを処理する。
//! build_job の submit / list / get / cancel を提供する。
//!
//! 詳細設計: fleetstage repo `docs/design/30-build-tier.md`

use std::sync::Arc;

use serde_json::json;
use tracing::{error, info};
use unison::network::channel::UnisonChannel;
use unison::network::server::ProtocolServer;

use crate::model::{BuildJob, BuildSource, BuildTarget, build_job_kind, build_job_state};
use crate::server::AppState;

pub async fn register(server: &ProtocolServer, state: Arc<AppState>) {
    server
        .register_channel("build", move |_ctx, stream| {
            let state = state.clone();
            Box::pin(async move {
                let channel = UnisonChannel::new(stream);
                loop {
                    let msg = channel.recv().await?;
                    let payload = msg.payload_as_value()?;

                    match msg.method.as_str() {
                        "submit" => {
                            let tenant_slug =
                                payload["tenant_slug"].as_str().unwrap_or_default();

                            // tenant 解決
                            let tenant = match state.db.get_tenant_by_slug(tenant_slug).await {
                                Ok(Some(t)) => t,
                                Ok(None) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "submit",
                                            json!({ "error": "tenant not found" }),
                                        )
                                        .await?;
                                    continue;
                                }
                                Err(e) => {
                                    error!(error = %e, "tenant lookup 失敗 (build.submit)");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "submit",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };
                            let tenant_id = tenant.id.expect("tenant.id");

                            let git_url = payload["git_url"].as_str().unwrap_or_default();
                            let git_ref = payload["git_ref"]
                                .as_str()
                                .unwrap_or("main")
                                .to_string();
                            let kind = payload["kind"]
                                .as_str()
                                .unwrap_or(build_job_kind::DOCKER_IMAGE);

                            if git_url.is_empty() {
                                channel
                                    .send_response(
                                        msg.id,
                                        "submit",
                                        json!({ "error": "`git_url` required" }),
                                    )
                                    .await?;
                                continue;
                            }

                            if !build_job_kind::is_valid(kind) {
                                channel
                                    .send_response(
                                        msg.id,
                                        "submit",
                                        json!({ "error": format!("invalid kind: {}", kind) }),
                                    )
                                    .await?;
                                continue;
                            }

                            let job = BuildJob {
                                id: None,
                                tenant: tenant_id,
                                project: None,
                                kind: kind.to_string(),
                                source: BuildSource {
                                    git_url: git_url.to_string(),
                                    git_ref,
                                    dockerfile: payload["dockerfile"]
                                        .as_str()
                                        .map(|s| s.to_string()),
                                },
                                target: BuildTarget {
                                    image: payload["image"].as_str().map(|s| s.to_string()),
                                    registry_secret: payload["registry_secret"]
                                        .as_str()
                                        .map(|s| s.to_string()),
                                },
                                state: build_job_state::QUEUED.to_string(),
                                server: None,
                                logs_url: None,
                                submitted_at: None,
                                started_at: None,
                                finished_at: None,
                                duration_seconds: None,
                            };

                            match state.db.create_build_job(&job).await {
                                Ok(created) => {
                                    info!(
                                        job_id = ?created.id,
                                        kind = %created.kind,
                                        git_url = %created.source.git_url,
                                        "build job submitted (queued)"
                                    );
                                    channel
                                        .send_response(
                                            msg.id,
                                            "submit",
                                            json!({ "build_job": created }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "build.submit 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "submit",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "list" => {
                            let tenant_slug =
                                payload["tenant_slug"].as_str().unwrap_or_default();

                            let tenant = match state.db.get_tenant_by_slug(tenant_slug).await {
                                Ok(Some(t)) => t,
                                Ok(None) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "list",
                                            json!({ "error": "tenant not found" }),
                                        )
                                        .await?;
                                    continue;
                                }
                                Err(e) => {
                                    error!(error = %e, "tenant lookup 失敗 (build.list)");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "list",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };
                            let tenant_id = tenant.id.expect("tenant.id");

                            match state.db.list_build_jobs_by_tenant(&tenant_id).await {
                                Ok(jobs) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "list",
                                            json!({ "build_jobs": jobs }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "build.list 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "list",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "get" => {
                            let tenant_slug =
                                payload["tenant_slug"].as_str().unwrap_or_default();
                            let job_id_str = payload["job_id"].as_str().unwrap_or_default();

                            if job_id_str.is_empty() {
                                channel
                                    .send_response(
                                        msg.id,
                                        "get",
                                        json!({ "error": "`job_id` required" }),
                                    )
                                    .await?;
                                continue;
                            }

                            // tenant scope 検証
                            let tenant = match state.db.get_tenant_by_slug(tenant_slug).await {
                                Ok(Some(t)) => t,
                                Ok(None) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "get",
                                            json!({ "error": "tenant not found" }),
                                        )
                                        .await?;
                                    continue;
                                }
                                Err(e) => {
                                    error!(error = %e, "tenant lookup 失敗 (build.get)");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "get",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };
                            let tenant_id = tenant.id.expect("tenant.id");

                            // RecordId 構築
                            let record_id =
                                surrealdb::types::RecordId::new("build_job", job_id_str);

                            match state.db.get_build_job_by_id(&record_id).await {
                                Ok(Some(job)) => {
                                    // tenant scope チェック
                                    if job.tenant != tenant_id {
                                        channel
                                            .send_response(
                                                msg.id,
                                                "get",
                                                json!({ "error": "job does not belong to this tenant" }),
                                            )
                                            .await?;
                                        continue;
                                    }
                                    channel
                                        .send_response(
                                            msg.id,
                                            "get",
                                            json!({ "build_job": job }),
                                        )
                                        .await?;
                                }
                                Ok(None) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "get",
                                            json!({ "error": "build job not found" }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "build.get 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "get",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "cancel" => {
                            let tenant_slug =
                                payload["tenant_slug"].as_str().unwrap_or_default();
                            let job_id_str = payload["job_id"].as_str().unwrap_or_default();

                            if job_id_str.is_empty() {
                                channel
                                    .send_response(
                                        msg.id,
                                        "cancel",
                                        json!({ "error": "`job_id` required" }),
                                    )
                                    .await?;
                                continue;
                            }

                            let tenant = match state.db.get_tenant_by_slug(tenant_slug).await {
                                Ok(Some(t)) => t,
                                Ok(None) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "cancel",
                                            json!({ "error": "tenant not found" }),
                                        )
                                        .await?;
                                    continue;
                                }
                                Err(e) => {
                                    error!(error = %e, "tenant lookup 失敗 (build.cancel)");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "cancel",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };
                            let tenant_id = tenant.id.expect("tenant.id");

                            let record_id =
                                surrealdb::types::RecordId::new("build_job", job_id_str);

                            // job の存在と tenant scope を確認してから cancel
                            match state.db.get_build_job_by_id(&record_id).await {
                                Ok(Some(job)) => {
                                    if job.tenant != tenant_id {
                                        channel
                                            .send_response(
                                                msg.id,
                                                "cancel",
                                                json!({ "error": "job does not belong to this tenant" }),
                                            )
                                            .await?;
                                        continue;
                                    }
                                    // queued / assigned のみ cancel 可能 (running 中はエラー)
                                    if job.state == build_job_state::SUCCESS
                                        || job.state == build_job_state::FAILED
                                        || job.state == build_job_state::CANCELLED
                                    {
                                        channel
                                            .send_response(
                                                msg.id,
                                                "cancel",
                                                json!({ "error": format!("cannot cancel job in state: {}", job.state) }),
                                            )
                                            .await?;
                                        continue;
                                    }
                                    match state
                                        .db
                                        .update_build_job_state(&record_id, build_job_state::CANCELLED)
                                        .await
                                    {
                                        Ok(()) => {
                                            info!(job_id = %job_id_str, "build job cancelled");
                                            channel
                                                .send_response(
                                                    msg.id,
                                                    "cancel",
                                                    json!({ "status": "cancelled" }),
                                                )
                                                .await?;
                                        }
                                        Err(e) => {
                                            error!(error = %e, "build.cancel state update 失敗");
                                            channel
                                                .send_response(
                                                    msg.id,
                                                    "cancel",
                                                    json!({ "error": e.to_string() }),
                                                )
                                                .await?;
                                        }
                                    }
                                }
                                Ok(None) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "cancel",
                                            json!({ "error": "build job not found" }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "build.cancel lookup 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "cancel",
                                            json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        other => {
                            channel
                                .send_response(
                                    msg.id,
                                    other,
                                    json!({ "error": format!("unknown method: {}", other) }),
                                )
                                .await?;
                        }
                    }
                }
            })
        })
        .await;
}
