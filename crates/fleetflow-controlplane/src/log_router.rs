//! LogRouter: コンテナログの Pub/Sub 配信
//!
//! Fleet Agent からのログを topic ベースで subscriber に配信する。
//! topic 形式: `logs/{server_slug}/{container_name}`
//!
//! retained: 各 topic の直近 N 行をキャッシュし、新規 subscribe 時に初期配信。

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, mpsc};
use tracing::debug;

/// ログエントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub server_slug: String,
    pub container_name: String,
    /// "stdout" or "stderr"
    pub stream: String,
    /// "debug" / "info" / "warning" / "error" / "unknown"
    pub level: String,
    pub message: String,
}

/// Retained キャッシュの上限
const RETAINED_MAX_LINES: usize = 200;

/// Subscriber ID
type SubId = u64;

/// 個別の subscriber
struct LogSubscription {
    id: SubId,
    /// topic プレフィックスフィルタ（空文字列 = 全 topic）
    topic_prefix: String,
    /// 最小ログレベル（"debug" < "info" < "warning" < "error"）
    min_level: u8,
    /// ログ配信チャネル
    tx: mpsc::Sender<LogEntry>,
}

/// コンテナログの Pub/Sub ルーター
pub struct LogRouter {
    /// topic → 直近 N 行のキャッシュ
    retained: Arc<RwLock<HashMap<String, VecDeque<LogEntry>>>>,
    /// アクティブな subscriber
    subscribers: Arc<RwLock<Vec<LogSubscription>>>,
    /// subscriber ID 採番
    next_id: AtomicU64,
}

impl LogRouter {
    pub fn new() -> Self {
        Self {
            retained: Arc::new(RwLock::new(HashMap::new())),
            subscribers: Arc::new(RwLock::new(Vec::new())),
            next_id: AtomicU64::new(0),
        }
    }

    /// ログエントリを publish（Agent → CP 経由で呼ばれる）
    pub async fn publish(&self, entry: LogEntry) {
        let topic = format!("logs/{}/{}", entry.server_slug, entry.container_name);

        // Retained キャッシュに追加
        {
            let mut retained = self.retained.write().await;
            let buffer = retained
                .entry(topic.clone())
                .or_insert_with(|| VecDeque::with_capacity(RETAINED_MAX_LINES));
            buffer.push_back(entry.clone());
            if buffer.len() > RETAINED_MAX_LINES {
                buffer.pop_front();
            }
        }

        // マッチする subscriber に配信
        let level_num = level_to_num(&entry.level);
        let subs = self.subscribers.read().await;
        for sub in subs.iter() {
            if level_num >= sub.min_level
                && (sub.topic_prefix.is_empty() || topic.starts_with(&sub.topic_prefix))
            {
                let _ = sub.tx.try_send(entry.clone());
            }
        }
    }

    /// Subscribe（ログストリームを受信）
    ///
    /// 返り値: (subscriber_id, receiver)
    /// receiver から LogEntry を非同期で受信できる。
    /// retained キャッシュがあれば初期配信する。
    pub async fn subscribe(
        &self,
        topic_prefix: &str,
        min_level: &str,
        buffer_size: usize,
    ) -> (SubId, mpsc::Receiver<LogEntry>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel(buffer_size);
        let min_level_num = level_to_num(min_level);

        // Retained キャッシュから初期配信
        {
            let retained = self.retained.read().await;
            for (topic, entries) in retained.iter() {
                if topic_prefix.is_empty() || topic.starts_with(topic_prefix) {
                    for entry in entries.iter() {
                        if level_to_num(&entry.level) >= min_level_num {
                            let _ = tx.try_send(entry.clone());
                        }
                    }
                }
            }
        }

        // Subscriber 登録
        self.subscribers.write().await.push(LogSubscription {
            id,
            topic_prefix: topic_prefix.to_string(),
            min_level: min_level_num,
            tx,
        });

        debug!(id, topic_prefix, min_level, "LogRouter: subscriber 登録");
        (id, rx)
    }

    /// Unsubscribe
    pub async fn unsubscribe(&self, id: SubId) {
        let mut subs = self.subscribers.write().await;
        subs.retain(|s| s.id != id);
        debug!(id, "LogRouter: subscriber 解除");
    }

    /// 特定 topic の直近ログを取得（スナップショット）
    pub async fn get_recent(
        &self,
        topic_prefix: &str,
        min_level: &str,
        limit: usize,
    ) -> Vec<LogEntry> {
        let retained = self.retained.read().await;
        let min_level_num = level_to_num(min_level);
        let mut results = Vec::new();

        for (topic, entries) in retained.iter() {
            if topic_prefix.is_empty() || topic.starts_with(topic_prefix) {
                for entry in entries.iter() {
                    if level_to_num(&entry.level) >= min_level_num {
                        results.push(entry.clone());
                    }
                }
            }
        }

        // timestamp でソートして最新 limit 件
        results.sort_by_key(|r| r.timestamp);
        if results.len() > limit {
            results = results.split_off(results.len() - limit);
        }
        results
    }
}

impl Default for LogRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// ログレベル文字列 → 数値（フィルタリング用）
fn level_to_num(level: &str) -> u8 {
    match level {
        "debug" => 0,
        "info" => 1,
        "warning" | "warn" => 2,
        "error" => 3,
        "critical" | "fatal" => 4,
        _ => 1, // unknown → info 扱い
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_entry(server: &str, container: &str, level: &str, msg: &str) -> LogEntry {
        LogEntry {
            timestamp: Utc::now(),
            server_slug: server.into(),
            container_name: container.into(),
            stream: "stdout".into(),
            level: level.into(),
            message: msg.into(),
        }
    }

    #[tokio::test]
    async fn test_publish_and_subscribe() {
        let router = LogRouter::new();

        let (_id, mut rx) = router.subscribe("logs/vps-01", "info", 100).await;

        router
            .publish(make_entry("vps-01", "web", "info", "started"))
            .await;
        router
            .publish(make_entry("vps-01", "web", "debug", "trace msg"))
            .await;
        router
            .publish(make_entry("vps-02", "db", "error", "connection lost"))
            .await;

        // vps-01 の info 以上のみ受信
        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.message, "started");

        // debug は min_level フィルタで除外
        // vps-02 は topic_prefix フィルタで除外
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_retained_cache() {
        let router = LogRouter::new();

        // 先に publish
        router
            .publish(make_entry("vps-01", "web", "info", "cached msg"))
            .await;

        // 後から subscribe → retained から初期配信
        let (_id, mut rx) = router.subscribe("logs/vps-01", "info", 100).await;

        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.message, "cached msg");
    }

    #[tokio::test]
    async fn test_get_recent() {
        let router = LogRouter::new();

        for i in 0..5 {
            router
                .publish(make_entry("vps-01", "web", "info", &format!("line {i}")))
                .await;
        }

        let recent = router.get_recent("logs/vps-01", "info", 3).await;
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].message, "line 2");
        assert_eq!(recent[2].message, "line 4");
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let router = LogRouter::new();

        let (id, mut rx) = router.subscribe("", "info", 100).await;
        router.unsubscribe(id).await;

        router
            .publish(make_entry("vps-01", "web", "info", "after unsub"))
            .await;

        assert!(rx.try_recv().is_err());
    }
}
