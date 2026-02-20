//! Background Cortex bulletin refresh loop for gateway runtime.

use std::sync::Arc;
use std::time::Duration;

use tau_agent_core::Cortex;
use tau_ai::LlmClient;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

#[derive(Debug)]
pub(super) struct CortexBulletinRuntimeHandle {
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl CortexBulletinRuntimeHandle {
    pub(super) fn disabled() -> Self {
        Self {
            shutdown_tx: None,
            task: None,
        }
    }

    pub(super) async fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

pub(super) fn start_cortex_bulletin_runtime(
    cortex: Arc<Cortex>,
    client: Arc<dyn LlmClient>,
    model: String,
    heartbeat_enabled: bool,
    heartbeat_interval: Duration,
) -> CortexBulletinRuntimeHandle {
    if !heartbeat_enabled {
        return CortexBulletinRuntimeHandle::disabled();
    }

    let interval = heartbeat_interval.max(Duration::from_millis(1));
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let _ = cortex.refresh_once(client.as_ref(), model.as_str()).await;
                }
                _ = &mut shutdown_rx => {
                    break;
                }
            }
        }
    });

    CortexBulletinRuntimeHandle {
        shutdown_tx: Some(shutdown_tx),
        task: Some(task),
    }
}

#[cfg(test)]
mod tests {
    use super::start_cortex_bulletin_runtime;
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use tau_agent_core::{Cortex, CortexConfig};
    use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, TauAiError};
    use tau_memory::memory_contract::{MemoryEntry, MemoryScope};

    #[derive(Debug, Clone)]
    struct CountingClient {
        calls: Arc<AtomicUsize>,
        replies: Arc<tokio::sync::Mutex<VecDeque<String>>>,
    }

    impl CountingClient {
        fn new(replies: Vec<String>) -> Self {
            Self {
                calls: Arc::new(AtomicUsize::new(0)),
                replies: Arc::new(tokio::sync::Mutex::new(replies.into_iter().collect())),
            }
        }
    }

    #[async_trait]
    impl LlmClient for CountingClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let mut replies = self.replies.lock().await;
            let text = replies
                .pop_front()
                .unwrap_or_else(|| "fallback bulletin".to_string());
            Ok(ChatResponse {
                message: Message::assistant_text(text),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            })
        }
    }

    fn write_memory(session_root: &Path, memory_id: &str, summary: &str) {
        let store = tau_memory::runtime::FileMemoryStore::new(session_root);
        let scope = MemoryScope {
            workspace_id: "workspace-cortex-runtime".to_string(),
            channel_id: "channel-cortex-runtime".to_string(),
            actor_id: "assistant".to_string(),
        };
        let entry = MemoryEntry {
            memory_id: memory_id.to_string(),
            summary: summary.to_string(),
            tags: vec!["cortex".to_string()],
            facts: vec!["fact".to_string()],
            source_event_key: format!("source-{memory_id}"),
            recency_weight_bps: 0,
            confidence_bps: 1_000,
        };
        store
            .write_entry_with_metadata(&scope, entry, None, Some(0.8))
            .expect("write memory");
    }

    #[tokio::test]
    async fn integration_spec_2717_c02_cortex_bulletin_runtime_executes_on_heartbeat_interval() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("openresponses/memory-store");
        let session = root.join("alpha");
        std::fs::create_dir_all(&session).expect("create session dir");
        write_memory(&session, "alpha-1", "alpha summary");

        let cortex = Arc::new(Cortex::new(CortexConfig::new(root)));
        let client = Arc::new(CountingClient::new(vec![
            "heartbeat bulletin".to_string(),
            "heartbeat bulletin 2".to_string(),
        ]));
        let mut handle = start_cortex_bulletin_runtime(
            Arc::clone(&cortex),
            client.clone(),
            "openai/gpt-4o-mini".to_string(),
            true,
            Duration::from_millis(20),
        );

        tokio::time::sleep(Duration::from_millis(80)).await;
        handle.shutdown().await;

        assert!(
            client.calls.load(Ordering::Relaxed) >= 1,
            "expected heartbeat loop to trigger at least one refresh call"
        );
        assert!(
            cortex
                .bulletin_snapshot()
                .contains("## Cortex Memory Bulletin"),
            "expected refreshed bulletin snapshot"
        );
    }
}
