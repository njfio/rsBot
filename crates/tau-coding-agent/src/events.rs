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
use tau_agent_core::{extract_skip_response_reason, Agent, AgentConfig, AgentEvent};
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

const SKIP_REASON_CODE: &str = "skip_suppressed";
const REACT_REASON_CODE: &str = "react_requested";

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
        let skip_reason = if status == PromptRunStatus::Completed {
            extract_skip_response_reason(&agent.messages()[start_index..])
        } else {
            None
        };
        let react_directive = if status == PromptRunStatus::Completed {
            extract_react_response_directive(&agent.messages()[start_index..])
        } else {
            None
        };

        let (input_tokens, output_tokens, total_tokens) = usage
            .lock()
            .map_err(|_| anyhow!("usage lock poisoned"))?
            .to_owned();
        let payload = build_outbound_event_payload(
            &event.id,
            status,
            assistant_reply,
            EventTokenUsage {
                input: input_tokens,
                output: output_tokens,
                total: total_tokens,
            },
            skip_reason.as_deref(),
            react_directive.as_ref(),
        );

        channel_store.sync_context_from_messages(agent.messages())?;
        channel_store.append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms: now_unix_ms,
            direction: "outbound".to_string(),
            event_key: Some(event.id.clone()),
            source: "events".to_string(),
            payload,
        })?;

        Ok(())
    }
}

fn build_outbound_event_payload(
    event_id: &str,
    status: PromptRunStatus,
    assistant_reply: String,
    token_usage: EventTokenUsage,
    skip_reason: Option<&str>,
    react_directive: Option<&EventReactDirective>,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "event_id": event_id,
        "status": format!("{:?}", status).to_lowercase(),
        "assistant_reply": assistant_reply,
        "tokens": {
            "input": token_usage.input,
            "output": token_usage.output,
            "total": token_usage.total,
        }
    });
    if let Some(reason) = skip_reason
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
    {
        if let Some(map) = payload.as_object_mut() {
            map.insert(
                "skip_reason".to_string(),
                serde_json::Value::String(reason.to_string()),
            );
            map.insert(
                "reason_code".to_string(),
                serde_json::Value::String(SKIP_REASON_CODE.to_string()),
            );
        }
    } else if let Some(react_directive) = react_directive {
        if let Some(map) = payload.as_object_mut() {
            map.insert(
                "reason_code".to_string(),
                serde_json::Value::String(react_directive.reason_code.clone()),
            );
            map.insert(
                "reaction".to_string(),
                serde_json::json!({
                    "emoji": react_directive.emoji,
                    "message_id": react_directive.message_id,
                }),
            );
        }
    }
    payload
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
    if extract_skip_response_reason(messages).is_some()
        || extract_react_response_directive(messages).is_some()
    {
        return String::new();
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct EventReactDirective {
    emoji: String,
    message_id: Option<String>,
    reason_code: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EventTokenUsage {
    input: u64,
    output: u64,
    total: u64,
}

fn extract_react_response_directive(messages: &[Message]) -> Option<EventReactDirective> {
    messages.iter().rev().find_map(|message| {
        if message.role != MessageRole::Tool || message.is_error {
            return None;
        }
        if message.tool_name.as_deref() != Some("react") {
            return None;
        }
        let text = message.text_content();
        if text.trim().is_empty() {
            return None;
        }
        let parsed = serde_json::from_str::<serde_json::Value>(text.trim()).ok()?;
        parse_react_response_directive_payload(&parsed)
    })
}

fn parse_react_response_directive_payload(
    payload: &serde_json::Value,
) -> Option<EventReactDirective> {
    let object = payload.as_object()?;
    let react_response = object
        .get("react_response")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let action_react = object
        .get("action")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value.trim() == "react_response");
    if !react_response && !action_react {
        return None;
    }
    let suppress_response = object
        .get("suppress_response")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    if !suppress_response {
        return None;
    }
    let emoji = object
        .get("emoji")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let message_id = object
        .get("message_id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let reason_code = object
        .get("reason_code")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(REACT_REASON_CODE)
        .to_string();
    Some(EventReactDirective {
        emoji,
        message_id,
        reason_code,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use tau_ai::{ChatRequest, ChatResponse, ChatUsage, ContentBlock, LlmClient, TauAiError};
    use tau_events::{EventDefinition, EventRunner, EventSchedule};
    use tau_runtime::channel_store::ChannelStore;

    #[derive(Clone)]
    struct QueueClient {
        responses: Arc<Mutex<VecDeque<ChatResponse>>>,
    }

    #[async_trait]
    impl LlmClient for QueueClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            let mut guard = self.responses.lock().expect("queue lock");
            Ok(guard.pop_front().unwrap_or(ChatResponse {
                message: Message::assistant_text("done"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }))
        }
    }

    #[test]
    fn spec_2514_c03_events_log_records_skip_reason_for_suppressed_reply() {
        let messages = vec![Message::tool_result(
            "call_skip_1",
            "skip",
            r#"{"skip_response":true,"reason":"maintenance-window","reason_code":"skip_suppressed"}"#,
            false,
        )];
        let assistant_reply = collect_assistant_reply(&messages);
        let skip_reason = extract_skip_response_reason(&messages);
        let payload = build_outbound_event_payload(
            "event-1",
            PromptRunStatus::Completed,
            assistant_reply,
            EventTokenUsage {
                input: 1,
                output: 2,
                total: 3,
            },
            skip_reason.as_deref(),
            None,
        );
        assert_eq!(payload["skip_reason"].as_str(), Some("maintenance-window"));
        assert_eq!(payload["reason_code"].as_str(), Some("skip_suppressed"));
    }

    #[test]
    fn regression_2514_events_log_omits_skip_reason_when_not_suppressed() {
        let payload = build_outbound_event_payload(
            "event-1",
            PromptRunStatus::Completed,
            "normal response".to_string(),
            EventTokenUsage {
                input: 1,
                output: 2,
                total: 3,
            },
            None,
            None,
        );
        assert!(payload.get("skip_reason").is_none());
        assert!(payload.get("reason_code").is_none());
    }

    #[test]
    fn regression_2520_collect_assistant_reply_keeps_non_suppressed_assistant_text() {
        let messages = vec![Message::assistant_text("normal response")];
        assert_eq!(collect_assistant_reply(&messages), "normal response");
    }

    #[test]
    fn regression_2520_collect_assistant_reply_ignores_error_react_tool_results() {
        let messages = vec![
            Message::tool_result(
                "call_react_err_1",
                "react",
                r#"{"react_response":true,"emoji":"👍","message_id":"42","suppress_response":true}"#,
                true,
            ),
            Message::assistant_text("normal response"),
        ];
        assert_eq!(collect_assistant_reply(&messages), "normal response");
    }

    #[test]
    fn spec_2520_collect_assistant_reply_suppresses_action_only_react_payload() {
        let messages = vec![
            Message::tool_result(
                "call_react_action_1",
                "react",
                r#"{"action":"react_response","emoji":"👍","suppress_response":true}"#,
                false,
            ),
            Message::assistant_text("normal response"),
        ];
        assert_eq!(collect_assistant_reply(&messages), "");
    }

    #[test]
    fn regression_2520_collect_assistant_reply_rejects_invalid_react_action_marker() {
        let messages = vec![
            Message::tool_result(
                "call_react_invalid_action_1",
                "react",
                r#"{"action":"not_react","emoji":"👍","suppress_response":true}"#,
                false,
            ),
            Message::assistant_text("normal response"),
        ];
        assert_eq!(collect_assistant_reply(&messages), "normal response");
    }

    #[test]
    fn regression_2520_parse_react_payload_defaults_empty_reason_code() {
        let payload = serde_json::json!({
            "react_response": true,
            "emoji": "👍",
            "suppress_response": true,
            "reason_code": "   ",
        });

        let directive = parse_react_response_directive_payload(&payload).expect("react directive");
        assert_eq!(directive.reason_code, REACT_REASON_CODE);
    }

    #[tokio::test]
    async fn integration_spec_2514_c03_runner_persists_skip_reason_and_suppresses_reply() {
        let temp = tempfile::tempdir().expect("tempdir");
        let channel_store =
            ChannelStore::open(temp.path(), "events", "skip-channel").expect("channel store");
        let event = EventDefinition {
            id: "evt-skip-1".to_string(),
            channel: "events/skip-channel".to_string(),
            prompt: "Run skip flow".to_string(),
            schedule: EventSchedule::Immediate,
            enabled: true,
            created_unix_ms: Some(1),
        };

        let first_response = ChatResponse {
            message: Message::assistant_blocks(vec![ContentBlock::ToolCall {
                id: "call_skip_1".to_string(),
                name: "skip".to_string(),
                arguments: serde_json::json!({ "reason": "maintenance-window" }),
            }]),
            finish_reason: Some("tool_calls".to_string()),
            usage: ChatUsage::default(),
        };
        let client = Arc::new(QueueClient {
            responses: Arc::new(Mutex::new(VecDeque::from([first_response]))),
        });

        let runner = TauEventRunner {
            client,
            model: "openai/gpt-4o-mini".to_string(),
            system_prompt: "base".to_string(),
            max_turns: 4,
            tool_policy: ToolPolicy::new(vec![temp.path().to_path_buf()]),
            turn_timeout_ms: 10_000,
            render_options: RenderOptions {
                stream_output: false,
                stream_delay_ms: 0,
            },
            session_lock_wait_ms: 1,
            session_lock_stale_ms: 1,
        };

        runner
            .run_event(&event, 123, &channel_store)
            .await
            .expect("run event");

        let logs = channel_store.load_log_entries().expect("load logs");
        let outbound = logs
            .iter()
            .find(|entry| {
                entry.source == "events" && entry.event_key.as_deref() == Some("evt-skip-1")
            })
            .expect("events outbound entry");
        assert_eq!(
            outbound.payload["skip_reason"].as_str(),
            Some("maintenance-window")
        );
        assert_eq!(
            outbound.payload["reason_code"].as_str(),
            Some("skip_suppressed")
        );
        assert_eq!(outbound.payload["assistant_reply"].as_str(), Some(""));
    }

    #[tokio::test]
    async fn integration_spec_2520_c05_runner_persists_reaction_payload_and_suppresses_text_reply()
    {
        let temp = tempfile::tempdir().expect("tempdir");
        let channel_store =
            ChannelStore::open(temp.path(), "events", "react-channel").expect("channel store");
        let event = EventDefinition {
            id: "evt-react-1".to_string(),
            channel: "events/react-channel".to_string(),
            prompt: "Run react flow".to_string(),
            schedule: EventSchedule::Immediate,
            enabled: true,
            created_unix_ms: Some(1),
        };

        let first_response = ChatResponse {
            message: Message::assistant_blocks(vec![ContentBlock::ToolCall {
                id: "call_react_1".to_string(),
                name: "react".to_string(),
                arguments: serde_json::json!({
                    "emoji": "👍",
                    "message_id": "42"
                }),
            }]),
            finish_reason: Some("tool_calls".to_string()),
            usage: ChatUsage::default(),
        };
        let client = Arc::new(QueueClient {
            responses: Arc::new(Mutex::new(VecDeque::from([first_response]))),
        });

        let runner = TauEventRunner {
            client,
            model: "openai/gpt-4o-mini".to_string(),
            system_prompt: "base".to_string(),
            max_turns: 4,
            tool_policy: ToolPolicy::new(vec![temp.path().to_path_buf()]),
            turn_timeout_ms: 10_000,
            render_options: RenderOptions {
                stream_output: false,
                stream_delay_ms: 0,
            },
            session_lock_wait_ms: 1,
            session_lock_stale_ms: 1,
        };

        runner
            .run_event(&event, 123, &channel_store)
            .await
            .expect("run event");

        let logs = channel_store.load_log_entries().expect("load logs");
        let outbound = logs
            .iter()
            .find(|entry| {
                entry.source == "events" && entry.event_key.as_deref() == Some("evt-react-1")
            })
            .expect("events outbound entry");
        assert_eq!(
            outbound.payload["reason_code"].as_str(),
            Some("react_requested")
        );
        assert_eq!(outbound.payload["reaction"]["emoji"].as_str(), Some("👍"));
        assert_eq!(
            outbound.payload["reaction"]["message_id"].as_str(),
            Some("42")
        );
        assert_eq!(outbound.payload["assistant_reply"].as_str(), Some(""));
    }
}
