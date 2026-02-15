//! Runtime event subscription and reporting utilities.
//!
//! This module configures JSON/event reporters and extension hook subscribers so
//! runtime execution emits structured diagnostics and audit records.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use tau_agent_core::{Agent, AgentConfig, AgentEvent};
use tau_ai::{LlmClient, Message, MessageRole};
use tau_events::{
    run_event_scheduler as run_core_event_scheduler, EventRunner,
    EventSchedulerConfig as CoreEventSchedulerConfig,
};
use tau_session::{SessionRuntime, SessionStore};

use crate::channel_store::ChannelLogEntry;
use crate::runtime_loop::{run_prompt_with_cancellation, PromptRunStatus};
use crate::runtime_types::RenderOptions;
use crate::tools::ToolPolicy;

pub(crate) use tau_events::{
    execute_events_dry_run_command, execute_events_inspect_command,
    execute_events_simulate_command, execute_events_template_write_command,
    execute_events_validate_command, EventDefinition,
};

#[derive(Clone)]
pub(crate) struct EventSchedulerConfig {
    pub client: Arc<dyn LlmClient>,
    pub model: String,
    pub system_prompt: String,
    pub max_turns: usize,
    pub tool_policy: ToolPolicy,
    pub turn_timeout_ms: u64,
    pub render_options: RenderOptions,
    pub session_lock_wait_ms: u64,
    pub session_lock_stale_ms: u64,
    pub channel_store_root: PathBuf,
    pub events_dir: PathBuf,
    pub state_path: PathBuf,
    pub poll_interval: Duration,
    pub queue_limit: usize,
    pub stale_immediate_max_age_seconds: u64,
}

pub(crate) async fn run_event_scheduler(config: EventSchedulerConfig) -> Result<()> {
    let runner = TauEventRunner::new(&config);
    let core = CoreEventSchedulerConfig {
        runner: Arc::new(runner),
        channel_store_root: config.channel_store_root,
        events_dir: config.events_dir,
        state_path: config.state_path,
        poll_interval: config.poll_interval,
        queue_limit: config.queue_limit,
        stale_immediate_max_age_seconds: config.stale_immediate_max_age_seconds,
    };
    run_core_event_scheduler(core).await
}

#[derive(Clone)]
struct TauEventRunner {
    client: Arc<dyn LlmClient>,
    model: String,
    system_prompt: String,
    max_turns: usize,
    tool_policy: ToolPolicy,
    turn_timeout_ms: u64,
    render_options: RenderOptions,
    session_lock_wait_ms: u64,
    session_lock_stale_ms: u64,
}

impl TauEventRunner {
    fn new(config: &EventSchedulerConfig) -> Self {
        Self {
            client: config.client.clone(),
            model: config.model.clone(),
            system_prompt: config.system_prompt.clone(),
            max_turns: config.max_turns,
            tool_policy: config.tool_policy.clone(),
            turn_timeout_ms: config.turn_timeout_ms,
            render_options: config.render_options,
            session_lock_wait_ms: config.session_lock_wait_ms,
            session_lock_stale_ms: config.session_lock_stale_ms,
        }
    }
}

#[async_trait]
impl EventRunner for TauEventRunner {
    async fn run_event(
        &self,
        event: &EventDefinition,
        now_unix_ms: u64,
        channel_store: &tau_runtime::channel_store::ChannelStore,
    ) -> Result<()> {
        let mut agent = Agent::new(
            self.client.clone(),
            AgentConfig {
                model: self.model.clone(),
                system_prompt: self.system_prompt.clone(),
                max_turns: self.max_turns,
                temperature: Some(0.0),
                max_tokens: None,
                ..AgentConfig::default()
            },
        );
        crate::tools::register_builtin_tools(&mut agent, self.tool_policy.clone());

        let usage = std::sync::Arc::new(std::sync::Mutex::new((0_u64, 0_u64, 0_u64)));
        agent.subscribe({
            let usage = usage.clone();
            move |agent_event| {
                if let AgentEvent::TurnEnd {
                    usage: turn_usage, ..
                } = agent_event
                {
                    if let Ok(mut guard) = usage.lock() {
                        guard.0 = guard.0.saturating_add(turn_usage.input_tokens);
                        guard.1 = guard.1.saturating_add(turn_usage.output_tokens);
                        guard.2 = guard.2.saturating_add(turn_usage.total_tokens);
                    }
                }
            }
        });

        let mut session_runtime = Some(initialize_channel_session_runtime(
            &channel_store.session_path(),
            &self.system_prompt,
            self.session_lock_wait_ms,
            self.session_lock_stale_ms,
            &mut agent,
        )?);

        let prompt = format!(
            "You are executing an autonomous scheduled task for channel {}.\n\nTask prompt:\n{}",
            event.channel, event.prompt
        );
        let start_index = agent.messages().len();
        let status = run_prompt_with_cancellation(
            &mut agent,
            &mut session_runtime,
            &prompt,
            self.turn_timeout_ms,
            std::future::pending::<()>(),
            self.render_options,
        )
        .await?;

        let assistant_reply = if status == PromptRunStatus::Cancelled {
            "Scheduled run cancelled before completion.".to_string()
        } else if status == PromptRunStatus::TimedOut {
            "Scheduled run timed out before completion.".to_string()
        } else {
            collect_assistant_reply(&agent.messages()[start_index..])
        };

        let (input_tokens, output_tokens, total_tokens) = usage
            .lock()
            .map_err(|_| anyhow!("usage lock poisoned"))?
            .to_owned();

        channel_store.sync_context_from_messages(agent.messages())?;
        channel_store.append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms: now_unix_ms,
            direction: "outbound".to_string(),
            event_key: Some(event.id.clone()),
            source: "events".to_string(),
            payload: serde_json::json!({
                "event_id": event.id,
                "status": format!("{:?}", status).to_lowercase(),
                "assistant_reply": assistant_reply,
                "tokens": {
                    "input": input_tokens,
                    "output": output_tokens,
                    "total": total_tokens,
                }
            }),
        })?;

        Ok(())
    }
}

fn initialize_channel_session_runtime(
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

fn collect_assistant_reply(messages: &[Message]) -> String {
    let content = messages
        .iter()
        .filter(|message| message.role == MessageRole::Assistant)
        .map(Message::text_content)
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if content.trim().is_empty() {
        "I couldn't generate a textual response for this event.".to_string()
    } else {
        content
    }
}
