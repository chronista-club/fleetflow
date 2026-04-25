use std::sync::Arc;

use serde_json::json;
use surrealdb::types::RecordId;
use tracing::{error, info};
use unison::network::channel::UnisonChannel;
use unison::network::server::ProtocolServer;

use crate::model::DnsRecord;
use crate::server::AppState;

pub async fn register(server: &ProtocolServer, state: Arc<AppState>) {
    server
        .register_channel("dns", move |_ctx, stream| {
            let state = state.clone();
            Box::pin(async move {
                let channel = UnisonChannel::new(stream);
                loop {
                    let msg = channel.recv().await?;
                    let payload = msg.payload_as_value()?;

                    match msg.method.as_str() {
                        "create" => {
                            let tenant_slug = payload["tenant_slug"].as_str().unwrap_or("default");
                            let project_slug = payload["project_slug"].as_str();
                            let name = payload["name"].as_str().unwrap_or_default();
                            let record_type = payload["record_type"].as_str().unwrap_or("A");
                            let content = payload["content"].as_str().unwrap_or_default();
                            let proxied = payload["proxied"].as_bool().unwrap_or(false);

                            let tenant = match state.db.get_tenant_by_slug(tenant_slug).await {
                                Ok(Some(t)) => t,
                                Ok(None) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "create",
                                            &json!({ "error": "tenant not found" }),
                                        )
                                        .await?;
                                    continue;
                                }
                                Err(e) => {
                                    error!(error = %e, "tenant lookup 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "create",
                                            &json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };
                            let tenant_id = match tenant.id {
                                Some(id) => id,
                                None => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "create",
                                            &json!({ "error": "tenant has no id" }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };
                            let project_id = project_slug.map(|s| RecordId::new("project", s));

                            // 1. Cloudflare に実レコード作成（環境変数から認証）
                            let mut zone_id: Option<String> = payload["zone_id"].as_str().map(String::from);
                            let mut cf_record_id: Option<String> = None;

                            if let Ok(cf_config) = fleetflow_cloud_cloudflare::dns::DnsConfig::from_env() {
                                zone_id = Some(cf_config.zone_id.clone());
                                let cf = fleetflow_cloud_cloudflare::dns::CloudflareDns::new(cf_config);

                                // subdomain 部分を抽出（FQDN からドメインを除く、または name をそのまま使用）
                                let domain_suffix = format!(".{}", cf.domain());
                                let subdomain = name
                                    .strip_suffix(&domain_suffix)
                                    .unwrap_or(name);

                                info!(subdomain, content, "dns.create: Cloudflare にレコード作成中");

                                match cf.ensure_record(subdomain, content).await {
                                    Ok(cf_rec) => {
                                        cf_record_id = Some(cf_rec.id.clone());
                                        info!(
                                            cf_id = %cf_rec.id,
                                            name = %cf_rec.name,
                                            "dns.create: Cloudflare レコード作成完了"
                                        );
                                    }
                                    Err(e) => {
                                        error!(error = %e, "dns.create: Cloudflare 作成失敗（DB のみ登録）");
                                        // Cloudflare 失敗でも DB には登録する
                                    }
                                }
                            }

                            // 2. DB に登録
                            let record = DnsRecord {
                                id: None,
                                tenant: tenant_id,
                                project: project_id,
                                name: name.into(),
                                record_type: record_type.into(),
                                content: content.into(),
                                zone_id,
                                cf_record_id,
                                proxied,
                                created_at: None,
                                updated_at: None,
                            };

                            match state.db.create_dns_record(&record).await {
                                Ok(created) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "create",
                                            &json!({ "dns_record": created }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "dns.create 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "create",
                                            &json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "list" => {
                            let tenant_slug = payload["tenant_slug"].as_str().unwrap_or("default");

                            match state.db.list_dns_records_by_tenant(tenant_slug).await {
                                Ok(records) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "list",
                                            &json!({ "dns_records": records }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "dns.list 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "list",
                                            &json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "delete" => {
                            let name = payload["name"].as_str().unwrap_or_default();

                            // 1. Cloudflare からレコード削除
                            if let Ok(cf_config) = fleetflow_cloud_cloudflare::dns::DnsConfig::from_env() {
                                let cf = fleetflow_cloud_cloudflare::dns::CloudflareDns::new(cf_config);
                                let domain_suffix = format!(".{}", cf.domain());
                                let subdomain = name
                                    .strip_suffix(&domain_suffix)
                                    .unwrap_or(name);

                                info!(subdomain, "dns.delete: Cloudflare からレコード削除中");
                                if let Err(e) = cf.remove_record(subdomain).await {
                                    error!(error = %e, "dns.delete: Cloudflare 削除失敗（DB からは削除する）");
                                }
                            }

                            // 2. DB から削除
                            match state.db.delete_dns_record(name).await {
                                Ok(deleted) => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "delete",
                                            &json!({ "deleted": deleted }),
                                        )
                                        .await?;
                                }
                                Err(e) => {
                                    error!(error = %e, "dns.delete 失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "delete",
                                            &json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                }
                            }
                        }
                        "sync" => {
                            // Cloudflare DNS との双方向同期
                            // 環境変数から認証情報を取得
                            let cf_config =
                                match fleetflow_cloud_cloudflare::dns::DnsConfig::from_env() {
                                    Ok(c) => c,
                                    Err(e) => {
                                        channel
                                            .send_response(
                                                msg.id,
                                                "sync",
                                                &json!({ "error": format!("Cloudflare 設定エラー: {e}") }),
                                            )
                                            .await?;
                                        continue;
                                    }
                                };

                            let cf = fleetflow_cloud_cloudflare::dns::CloudflareDns::new(cf_config);
                            let tenant_slug =
                                payload["tenant_slug"].as_str().unwrap_or("default");

                            // 1. Cloudflare からレコード取得
                            let cf_records = match cf.list_records().await {
                                Ok(r) => r,
                                Err(e) => {
                                    error!(error = %e, "Cloudflare レコード取得失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "sync",
                                            &json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };

                            // 2. DB からレコード取得
                            let db_records = match state
                                .db
                                .list_dns_records_by_tenant(tenant_slug)
                                .await
                            {
                                Ok(r) => r,
                                Err(e) => {
                                    error!(error = %e, "DB レコード取得失敗");
                                    channel
                                        .send_response(
                                            msg.id,
                                            "sync",
                                            &json!({ "error": e.to_string() }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };

                            // 3. Cloudflare にあって DB にないレコードを DB に追加
                            let mut imported = 0u32;

                            let tenant = match state.db.get_tenant_by_slug(tenant_slug).await {
                                Ok(Some(t)) => t,
                                _ => {
                                    channel
                                        .send_response(
                                            msg.id,
                                            "sync",
                                            &json!({ "error": "tenant not found" }),
                                        )
                                        .await?;
                                    continue;
                                }
                            };

                            for cf_rec in &cf_records {
                                let exists_in_db =
                                    db_records.iter().any(|db| db.name == cf_rec.name);

                                if !exists_in_db {
                                    let Some(ref tenant_id) = tenant.id else {
                                        continue;
                                    };
                                    let record = DnsRecord {
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
                                    if state.db.create_dns_record(&record).await.is_ok() {
                                        imported += 1;
                                    }
                                }
                            }

                            // 4. DB にあって Cloudflare にないレコードは報告のみ
                            let mut not_in_cf = Vec::new();
                            for db_rec in &db_records {
                                let exists_in_cf =
                                    cf_records.iter().any(|cf| cf.name == db_rec.name);
                                if !exists_in_cf {
                                    not_in_cf.push(db_rec.name.clone());
                                }
                            }

                            info!(
                                imported,
                                not_in_cf = not_in_cf.len(),
                                cf_total = cf_records.len(),
                                db_total = db_records.len(),
                                "dns.sync 完了"
                            );

                            channel
                                .send_response(
                                    msg.id,
                                    "sync",
                                    &json!({
                                        "imported": imported,
                                        "cf_total": cf_records.len(),
                                        "db_total": db_records.len() + imported as usize,
                                        "not_in_cloudflare": not_in_cf,
                                    }),
                                )
                                .await?;
                        }
                        method => {
                            info!(method, "dns: 不明なメソッド");
                            channel
                                .send_response(
                                    msg.id,
                                    method,
                                    &json!({ "error": format!("unknown method: {}", method) }),
                                )
                                .await?;
                        }
                    }
                }
            })
        })
        .await;
}
