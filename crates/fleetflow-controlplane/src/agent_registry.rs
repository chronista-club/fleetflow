//! AgentRegistry — 接続中の Fleet Agent を管理
//!
//! Agent が CP に接続すると、server_slug → mpsc::Sender のマッピングを登録。
//! Dashboard API や他のハンドラーから Agent にコマンドを送信できる。

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{Mutex, mpsc, oneshot};
use tracing::{info, warn};

/// Agent に送るコマンド
#[derive(Debug)]
pub struct AgentCommand {
    /// コマンド名（"deploy", "restart", "status" 等）
    pub method: String,
    /// ペイロード
    pub payload: Value,
    /// 結果を返すチャネル（Agent からの応答を待つ場合）
    pub response_tx: Option<oneshot::Sender<Value>>,
}

/// 接続中の Agent 情報
#[derive(Debug)]
struct AgentEntry {
    /// Agent にコマンドを送るチャネル
    command_tx: mpsc::Sender<AgentCommand>,
    /// Agent のバージョン
    version: String,
}

/// 接続中の Agent を管理するレジストリ
#[derive(Debug, Clone)]
pub struct AgentRegistry {
    agents: Arc<Mutex<HashMap<String, AgentEntry>>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Agent を登録し、コマンド受信用の Receiver を返す
    pub async fn register(&self, server_slug: &str, version: &str) -> mpsc::Receiver<AgentCommand> {
        let (tx, rx) = mpsc::channel(64);
        let mut agents = self.agents.lock().await;

        if agents.contains_key(server_slug) {
            warn!(server = server_slug, "Agent 再接続（既存エントリを上書き）");
        }

        agents.insert(
            server_slug.to_string(),
            AgentEntry {
                command_tx: tx,
                version: version.to_string(),
            },
        );

        info!(server = server_slug, version, "Agent 登録完了");
        rx
    }

    /// Agent の登録を解除
    pub async fn unregister(&self, server_slug: &str) {
        let mut agents = self.agents.lock().await;
        if agents.remove(server_slug).is_some() {
            info!(server = server_slug, "Agent 登録解除");
        }
    }

    /// Agent にコマンドを送信（応答を待つ）
    pub async fn send_command(
        &self,
        server_slug: &str,
        method: &str,
        payload: Value,
    ) -> Result<Value, String> {
        let agents = self.agents.lock().await;
        let entry = agents
            .get(server_slug)
            .ok_or_else(|| format!("Agent '{}' が接続されていません", server_slug))?;

        let (response_tx, response_rx) = oneshot::channel();

        entry
            .command_tx
            .send(AgentCommand {
                method: method.to_string(),
                payload,
                response_tx: Some(response_tx),
            })
            .await
            .map_err(|_| format!("Agent '{}' へのコマンド送信失敗", server_slug))?;

        drop(agents); // ロック解放

        // Agent からの応答を待つ（タイムアウト 60 秒）
        tokio::time::timeout(std::time::Duration::from_secs(60), response_rx)
            .await
            .map_err(|_| format!("Agent '{}' からの応答タイムアウト", server_slug))?
            .map_err(|_| format!("Agent '{}' の応答チャネルがドロップ", server_slug))
    }

    /// Agent にコマンドを送信（応答を待たない、fire-and-forget）
    pub async fn send_command_fire_and_forget(
        &self,
        server_slug: &str,
        method: &str,
        payload: Value,
    ) -> Result<(), String> {
        let agents = self.agents.lock().await;
        let entry = agents
            .get(server_slug)
            .ok_or_else(|| format!("Agent '{}' が接続されていません", server_slug))?;

        entry
            .command_tx
            .send(AgentCommand {
                method: method.to_string(),
                payload,
                response_tx: None,
            })
            .await
            .map_err(|_| format!("Agent '{}' へのコマンド送信失敗", server_slug))
    }

    /// 接続中の Agent 一覧
    pub async fn list(&self) -> Vec<(String, String)> {
        let agents = self.agents.lock().await;
        agents
            .iter()
            .map(|(slug, entry)| (slug.clone(), entry.version.clone()))
            .collect()
    }

    /// 指定の Agent が接続中か
    pub async fn is_connected(&self, server_slug: &str) -> bool {
        let agents = self.agents.lock().await;
        agents.contains_key(server_slug)
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
