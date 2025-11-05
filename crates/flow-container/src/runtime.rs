use anyhow::Result;
use flow_atom::FlowConfig;

/// コンテナランタイムのトレイト
#[allow(async_fn_in_trait)]
pub trait ContainerRuntime {
    async fn start(&self, config: &FlowConfig) -> Result<()>;
    async fn stop(&self, config: &FlowConfig) -> Result<()>;
    async fn status(&self) -> Result<Vec<ContainerStatus>>;
}

/// コンテナのステータス
#[derive(Debug, Clone)]
pub struct ContainerStatus {
    pub name: String,
    pub state: ContainerState,
    pub image: String,
}

/// コンテナの状態
#[derive(Debug, Clone, PartialEq)]
pub enum ContainerState {
    Running,
    Stopped,
    Paused,
    Unknown,
}
