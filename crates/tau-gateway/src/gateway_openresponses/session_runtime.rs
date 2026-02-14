//! Session runtime orchestration for OpenResponses requests, response streaming, and persistence.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tau_agent_core::Agent;
use tau_ai::{Message, MessageRole};
use tau_session::SessionStore;

#[derive(Debug)]
pub(super) struct SessionRuntime {
    pub(super) store: SessionStore,
    pub(super) active_head: Option<u64>,
}

pub(super) fn persist_messages(
    session_runtime: &mut Option<SessionRuntime>,
    new_messages: &[Message],
) -> Result<()> {
    let Some(runtime) = session_runtime.as_mut() else {
        return Ok(());
    };

    runtime.active_head = runtime
        .store
        .append_messages(runtime.active_head, new_messages)?;
    Ok(())
}

pub(super) fn gateway_session_path(state_dir: &Path, session_key: &str) -> PathBuf {
    state_dir
        .join("openresponses")
        .join("sessions")
        .join(format!("{session_key}.jsonl"))
}

pub(super) fn initialize_gateway_session_runtime(
    session_path: &Path,
    system_prompt: &str,
    lock_wait_ms: u64,
    lock_stale_ms: u64,
    agent: &mut Agent,
) -> Result<SessionRuntime> {
    if let Some(parent) = session_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let mut store = SessionStore::load(session_path)?;
    store.set_lock_policy(lock_wait_ms.max(1), lock_stale_ms);
    let active_head = store.ensure_initialized(system_prompt)?;
    let lineage = store.lineage_messages(active_head)?;
    if !lineage.is_empty() {
        agent.replace_messages(lineage);
    }
    Ok(SessionRuntime { store, active_head })
}

pub(super) fn collect_assistant_reply(messages: &[Message]) -> String {
    let content = messages
        .iter()
        .filter(|message| message.role == MessageRole::Assistant)
        .map(Message::text_content)
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if content.trim().is_empty() {
        "I couldn't generate a textual response for this request.".to_string()
    } else {
        content
    }
}
