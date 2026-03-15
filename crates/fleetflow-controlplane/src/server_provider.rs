//! サーバープロバイダーの enum ディスパッチ
//!
//! Rust ネイティブ async fn in trait は object-safe でないため、
//! `Box<dyn ServerProvider>` の代わりに enum で具象型を包んでディスパッチする。

use fleetflow_cloud::{CreateServerRequest, ServerSpec};

/// サーバープロバイダーの種類
///
/// 実行時にどのクラウドプロバイダーを使うかを enum で表現。
pub enum ServerProviderKind {
    /// さくらクラウド（usacloud CLI 経由）
    Sakura(fleetflow_cloud_sakura::SakuraCloudProvider),

    /// テスト用モック
    #[cfg(feature = "test-utils")]
    Mock(MockServerProvider),
}

impl ServerProviderKind {
    pub fn provider_name(&self) -> &str {
        match self {
            Self::Sakura(p) => fleetflow_cloud::server_provider::ServerProvider::provider_name(p),
            #[cfg(feature = "test-utils")]
            Self::Mock(m) => &m.name,
        }
    }

    pub async fn list_servers(&self) -> fleetflow_cloud::Result<Vec<ServerSpec>> {
        match self {
            Self::Sakura(p) => {
                fleetflow_cloud::server_provider::ServerProvider::list_servers(p).await
            }
            #[cfg(feature = "test-utils")]
            Self::Mock(_) => Ok(vec![]),
        }
    }

    pub async fn get_server(&self, server_id: &str) -> fleetflow_cloud::Result<ServerSpec> {
        match self {
            Self::Sakura(p) => {
                fleetflow_cloud::server_provider::ServerProvider::get_server(p, server_id).await
            }
            #[cfg(feature = "test-utils")]
            Self::Mock(m) => m.get_server(server_id),
        }
    }

    pub async fn create_server(
        &self,
        request: &CreateServerRequest,
    ) -> fleetflow_cloud::Result<ServerSpec> {
        match self {
            Self::Sakura(p) => {
                fleetflow_cloud::server_provider::ServerProvider::create_server(p, request).await
            }
            #[cfg(feature = "test-utils")]
            Self::Mock(m) => m.create_server(request),
        }
    }

    pub async fn delete_server(
        &self,
        server_id: &str,
        with_disks: bool,
    ) -> fleetflow_cloud::Result<()> {
        match self {
            Self::Sakura(p) => {
                fleetflow_cloud::server_provider::ServerProvider::delete_server(
                    p, server_id, with_disks,
                )
                .await
            }
            #[cfg(feature = "test-utils")]
            Self::Mock(m) => m.delete_server(server_id, with_disks),
        }
    }

    pub async fn power_on(&self, server_id: &str) -> fleetflow_cloud::Result<()> {
        match self {
            Self::Sakura(p) => {
                fleetflow_cloud::server_provider::ServerProvider::power_on(p, server_id).await
            }
            #[cfg(feature = "test-utils")]
            Self::Mock(m) => m.power_on(server_id),
        }
    }

    pub async fn power_off(&self, server_id: &str) -> fleetflow_cloud::Result<()> {
        match self {
            Self::Sakura(p) => {
                fleetflow_cloud::server_provider::ServerProvider::power_off(p, server_id).await
            }
            #[cfg(feature = "test-utils")]
            Self::Mock(m) => m.power_off(server_id),
        }
    }
}

// ─────────────────────────────────────────
// テスト用モック
// ─────────────────────────────────────────

#[cfg(feature = "test-utils")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "test-utils")]
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum MockCall {
    ListServers,
    GetServer(String),
    CreateServer(String),
    DeleteServer(String, bool),
    PowerOn(String),
    PowerOff(String),
}

#[cfg(feature = "test-utils")]
pub struct MockServerProvider {
    pub name: String,
    pub calls: Arc<Mutex<Vec<MockCall>>>,
}

#[cfg(feature = "test-utils")]
impl MockServerProvider {
    pub fn new() -> (Self, Arc<Mutex<Vec<MockCall>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                name: "mock-provider".into(),
                calls: calls.clone(),
            },
            calls,
        )
    }

    fn get_server(&self, server_id: &str) -> fleetflow_cloud::Result<ServerSpec> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::GetServer(server_id.into()));
        Ok(ServerSpec {
            id: server_id.into(),
            name: "mock-server".into(),
            cpu: Some(2),
            memory_gb: Some(4),
            disk_gb: Some(40),
            status: fleetflow_cloud::ServerStatus::Running,
            ip_address: Some("203.0.113.1".into()),
            provider: "mock-provider".into(),
            zone: Some("mock-zone".into()),
            tags: vec![],
        })
    }

    fn create_server(&self, request: &CreateServerRequest) -> fleetflow_cloud::Result<ServerSpec> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::CreateServer(request.name.clone()));
        Ok(ServerSpec {
            id: "mock-12345".into(),
            name: request.name.clone(),
            cpu: Some(request.cpu),
            memory_gb: Some(request.memory_gb),
            disk_gb: request.disk_gb,
            status: fleetflow_cloud::ServerStatus::Running,
            ip_address: Some("203.0.113.50".into()),
            provider: "mock-provider".into(),
            zone: Some("mock-zone".into()),
            tags: request.tags.clone(),
        })
    }

    fn delete_server(&self, server_id: &str, with_disks: bool) -> fleetflow_cloud::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::DeleteServer(server_id.into(), with_disks));
        Ok(())
    }

    fn power_on(&self, server_id: &str) -> fleetflow_cloud::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::PowerOn(server_id.into()));
        Ok(())
    }

    fn power_off(&self, server_id: &str) -> fleetflow_cloud::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(MockCall::PowerOff(server_id.into()));
        Ok(())
    }
}
