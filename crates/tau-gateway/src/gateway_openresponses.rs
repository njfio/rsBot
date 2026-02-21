//! OpenResponses-compatible gateway server and request flow handlers.

use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use axum::body::Bytes;
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{header::AUTHORIZATION, HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tau_agent_core::{
    default_safety_rule_set, scan_safety_rules, validate_safety_rule_set, Agent, AgentConfig,
    AgentEvent, Cortex, CortexConfig, SafetyMode, SafetyPolicy, SafetyRuleSet,
};
#[cfg(test)]
use tau_ai::MessageRole;
use tau_ai::{LlmClient, StreamDeltaHandler};
use tau_core::{current_unix_timestamp, current_unix_timestamp_ms, write_text_atomic};
use tau_dashboard_ui::TauOpsDashboardRoute;
use tau_runtime::{
    inspect_runtime_heartbeat, start_runtime_heartbeat_scheduler, ExternalCodingAgentBridge,
    ExternalCodingAgentBridgeConfig, RuntimeHeartbeatSchedulerConfig, TransportHealthSnapshot,
};
#[cfg(test)]
use tau_session::SessionStore;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::remote_profile::GatewayOpenResponsesAuthMode;

mod audit_runtime;
mod auth_runtime;
mod auth_session_handler;
mod channel_telemetry_runtime;
mod compat_state_runtime;
mod config_runtime;
mod cortex_bulletin_runtime;
mod cortex_runtime;
mod dashboard_runtime;
mod dashboard_shell_page;
mod dashboard_status;
mod deploy_runtime;
mod endpoints;
mod entry_handlers;
mod events_status;
mod external_agent_runtime;
mod jobs_runtime;
mod memory_runtime;
mod multi_channel_status;
mod openai_compat;
mod openai_compat_runtime;
mod openresponses_entry_handler;
mod ops_dashboard_shell;
mod ops_shell_controls;
mod ops_shell_handlers;
mod request_preflight;
mod request_translation;
mod safety_runtime;
mod server_bootstrap;
mod server_state;
mod session_api_runtime;
mod session_runtime;
mod status_runtime;
mod stream_response_handler;
#[cfg(test)]
mod tests;
mod tool_registrar;
mod tools_runtime;
mod training_runtime;
mod types;
mod webchat_page;
mod websocket;
mod ws_stream_handlers;

use audit_runtime::{handle_gateway_audit_log, handle_gateway_audit_summary};
use auth_runtime::{
    authorize_gateway_request, collect_gateway_auth_status_report, enforce_gateway_rate_limit,
    issue_gateway_session_token, GatewayAuthRuntimeState,
};
use auth_session_handler::handle_gateway_auth_session;
use channel_telemetry_runtime::{
    handle_gateway_channel_lifecycle_action, handle_gateway_ui_telemetry,
};
use compat_state_runtime::{
    GatewayOpenAiCompatRuntimeState, GatewayOpenAiCompatSurface, GatewayUiTelemetryRuntimeState,
};
use config_runtime::{handle_gateway_config_get, handle_gateway_config_patch};
use cortex_bulletin_runtime::start_cortex_bulletin_runtime;
use cortex_runtime::{
    handle_cortex_chat, handle_cortex_status, record_cortex_external_followup_event,
    record_cortex_external_progress_event, record_cortex_external_session_closed,
    record_cortex_external_session_opened, record_cortex_memory_entry_delete_event,
    record_cortex_memory_entry_write_event, record_cortex_memory_write_event,
    record_cortex_session_append_event, record_cortex_session_reset_event,
};
use dashboard_runtime::{
    authorize_dashboard_request, handle_dashboard_action, handle_dashboard_alerts,
    handle_dashboard_health, handle_dashboard_queue_timeline, handle_dashboard_stream,
    handle_dashboard_widgets, handle_gateway_training_status,
};
#[cfg(test)]
use dashboard_shell_page::render_gateway_dashboard_shell_page;
use dashboard_status::{
    apply_gateway_dashboard_action, collect_gateway_dashboard_snapshot,
    collect_tau_ops_dashboard_command_center_snapshot, GatewayDashboardActionRequest,
};
use deploy_runtime::{handle_gateway_agent_stop, handle_gateway_deploy};
use endpoints::*;
use entry_handlers::{
    handle_dashboard_shell_page, handle_gateway_auth_bootstrap, handle_webchat_page,
};
use events_status::collect_gateway_events_status_report;
use external_agent_runtime::{
    handle_external_coding_agent_open_session, handle_external_coding_agent_reap,
    handle_external_coding_agent_session_close, handle_external_coding_agent_session_detail,
    handle_external_coding_agent_session_followup,
    handle_external_coding_agent_session_followups_drain,
    handle_external_coding_agent_session_progress, handle_external_coding_agent_session_stream,
};
use jobs_runtime::{handle_gateway_job_cancel, handle_gateway_jobs_list};
use memory_runtime::{
    gateway_memory_store, gateway_memory_stores_root, handle_api_memories_graph,
    handle_gateway_memory_entry_delete, handle_gateway_memory_entry_read,
    handle_gateway_memory_entry_write, handle_gateway_memory_graph, handle_gateway_memory_read,
    handle_gateway_memory_write,
};
use multi_channel_status::collect_gateway_multi_channel_status_report;
#[cfg(test)]
use openai_compat::{translate_chat_completions_request, OpenAiChatCompletionsRequest};
use openai_compat_runtime::{
    handle_openai_chat_completions, handle_openai_completions, handle_openai_models,
};
use openresponses_entry_handler::handle_openresponses;
use ops_dashboard_shell::{
    handle_ops_dashboard_chat_new, handle_ops_dashboard_chat_send,
    handle_ops_dashboard_memory_create, handle_ops_dashboard_session_detail_reset,
    handle_ops_dashboard_sessions_branch, render_tau_ops_dashboard_shell_for_route,
};
use ops_shell_controls::OpsShellControlsQuery;
use ops_shell_handlers::{
    handle_ops_dashboard_agent_detail_shell_page, handle_ops_dashboard_agents_shell_page,
    handle_ops_dashboard_channels_shell_page, handle_ops_dashboard_chat_shell_page,
    handle_ops_dashboard_config_shell_page, handle_ops_dashboard_deploy_shell_page,
    handle_ops_dashboard_diagnostics_shell_page, handle_ops_dashboard_login_shell_page,
    handle_ops_dashboard_memory_graph_shell_page, handle_ops_dashboard_memory_shell_page,
    handle_ops_dashboard_safety_shell_page, handle_ops_dashboard_session_detail_shell_page,
    handle_ops_dashboard_sessions_shell_page, handle_ops_dashboard_shell_page,
    handle_ops_dashboard_tools_jobs_shell_page, handle_ops_dashboard_training_shell_page,
};
use request_preflight::{
    authorize_and_enforce_gateway_limits, enforce_policy_gate, parse_gateway_json_body,
    system_time_to_unix_ms, validate_gateway_request_body_size,
};
use request_translation::{sanitize_session_key, translate_openresponses_request};
use safety_runtime::{
    handle_gateway_safety_policy_get, handle_gateway_safety_policy_put,
    handle_gateway_safety_rules_get, handle_gateway_safety_rules_put, handle_gateway_safety_test,
};
#[cfg(test)]
use server_bootstrap::build_gateway_openresponses_router;
pub use server_bootstrap::run_gateway_openresponses_server;
pub use server_state::GatewayOpenResponsesServerConfig;
use server_state::GatewayOpenResponsesServerState;
use session_api_runtime::{
    handle_gateway_session_append, handle_gateway_session_detail, handle_gateway_session_reset,
    handle_gateway_sessions_list,
};
use session_runtime::{
    collect_assistant_reply, gateway_session_path, initialize_gateway_session_runtime,
    persist_messages, persist_session_usage_delta,
};
use status_runtime::handle_gateway_status;
use stream_response_handler::stream_openresponses;
pub use tool_registrar::{GatewayToolRegistrar, GatewayToolRegistrarFn, NoopGatewayToolRegistrar};
use tools_runtime::{handle_gateway_tools_inventory, handle_gateway_tools_stats};
use training_runtime::{handle_gateway_training_config_patch, handle_gateway_training_rollouts};
use types::{
    GatewayAuthSessionRequest, GatewayAuthSessionResponse, GatewayMemoryEntryDeleteRequest,
    GatewayMemoryEntryUpsertRequest, GatewayMemoryGraphEdge, GatewayMemoryGraphFilterSummary,
    GatewayMemoryGraphNode, GatewayMemoryGraphQuery, GatewayMemoryGraphResponse,
    GatewayMemoryReadQuery, GatewayMemoryUpdateRequest, GatewaySafetyPolicyUpdateRequest,
    GatewaySafetyRulesUpdateRequest, GatewaySafetyTestRequest, OpenResponsesApiError,
    OpenResponsesExecutionResult, OpenResponsesOutputItem, OpenResponsesOutputTextItem,
    OpenResponsesPrompt, OpenResponsesRequest, OpenResponsesResponse, OpenResponsesUsage,
    OpenResponsesUsageSummary, SseFrame,
};
#[cfg(test)]
use webchat_page::render_gateway_webchat_page;
use websocket::run_gateway_ws_connection;
use ws_stream_handlers::{handle_gateway_ws_upgrade, run_dashboard_stream_loop};

async fn execute_openresponses_request(
    state: Arc<GatewayOpenResponsesServerState>,
    request: OpenResponsesRequest,
    stream_sender: Option<mpsc::UnboundedSender<SseFrame>>,
) -> Result<OpenResponsesExecutionResult, OpenResponsesApiError> {
    let mut translated = translate_openresponses_request(&request, state.config.max_input_chars)?;
    if request.model.is_some() {
        translated.ignored_fields.push("model".to_string());
    }

    let response_id = state.next_response_id();
    let created = current_unix_timestamp();

    if let Some(sender) = &stream_sender {
        let _ = sender.send(SseFrame::Json {
            event: "response.created",
            payload: json!({
                "type": "response.created",
                "response": {
                    "id": response_id,
                    "object": "response",
                    "status": "in_progress",
                    "model": state.config.model,
                    "created": created,
                }
            }),
        });
    }

    let preflight_input_tokens = derive_gateway_preflight_token_limit(state.config.max_input_chars);
    let resolved_system_prompt = state.resolved_system_prompt();
    let mut agent = Agent::new(
        state.config.client.clone(),
        AgentConfig {
            model: state.config.model.clone(),
            model_input_cost_per_million: state.config.model_input_cost_per_million,
            model_cached_input_cost_per_million: state.config.model_cached_input_cost_per_million,
            model_output_cost_per_million: state.config.model_output_cost_per_million,
            system_prompt: resolved_system_prompt.clone(),
            max_turns: state.config.max_turns,
            temperature: Some(0.0),
            max_tokens: None,
            // Fail closed on preflight limits: reject over-budget requests instead of compacting them.
            max_estimated_input_tokens: None,
            max_estimated_total_tokens: preflight_input_tokens,
            ..AgentConfig::default()
        },
    );
    state.config.tool_registrar.register(&mut agent);

    let usage = Arc::new(Mutex::new(OpenResponsesUsageSummary::default()));
    agent.subscribe({
        let usage = usage.clone();
        move |event| {
            if let AgentEvent::TurnEnd {
                usage: turn_usage, ..
            } = event
            {
                if let Ok(mut guard) = usage.lock() {
                    guard.input_tokens = guard.input_tokens.saturating_add(turn_usage.input_tokens);
                    guard.output_tokens =
                        guard.output_tokens.saturating_add(turn_usage.output_tokens);
                    guard.total_tokens = guard.total_tokens.saturating_add(turn_usage.total_tokens);
                }
            }
        }
    });

    let session_path = gateway_session_path(&state.config.state_dir, &translated.session_key);
    let mut session_runtime = Some(
        initialize_gateway_session_runtime(
            &session_path,
            &resolved_system_prompt,
            state.config.session_lock_wait_ms,
            state.config.session_lock_stale_ms,
            &mut agent,
        )
        .map_err(|error| {
            OpenResponsesApiError::internal(format!(
                "failed to initialize gateway session runtime: {error}"
            ))
        })?,
    );

    let start_index = agent.messages().len();
    let stream_handler = stream_sender.as_ref().map(|sender| {
        let sender = sender.clone();
        let response_id = response_id.clone();
        Arc::new(move |delta: String| {
            if delta.is_empty() {
                return;
            }
            let _ = sender.send(SseFrame::Json {
                event: "response.output_text.delta",
                payload: json!({
                    "type": "response.output_text.delta",
                    "response_id": response_id,
                    "delta": delta,
                }),
            });
        }) as StreamDeltaHandler
    });

    let pre_prompt_cost = agent.cost_snapshot();
    let prompt_result = if state.config.turn_timeout_ms == 0 {
        agent
            .prompt_with_stream(&translated.prompt, stream_handler)
            .await
    } else {
        match tokio::time::timeout(
            Duration::from_millis(state.config.turn_timeout_ms),
            agent.prompt_with_stream(&translated.prompt, stream_handler),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => {
                return Err(OpenResponsesApiError::timeout(
                    "response generation timed out before completion",
                ));
            }
        }
    };
    let post_prompt_cost = agent.cost_snapshot();
    persist_session_usage_delta(&mut session_runtime, &pre_prompt_cost, &post_prompt_cost)
        .map_err(|error| {
            OpenResponsesApiError::internal(format!(
                "failed to persist gateway session usage summary: {error}"
            ))
        })?;

    let new_messages = prompt_result.map_err(|error| {
        OpenResponsesApiError::gateway_failure(format!("gateway runtime failed: {error}"))
    })?;
    persist_messages(&mut session_runtime, &new_messages).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to persist gateway session messages: {error}"
        ))
    })?;

    let output_text = collect_assistant_reply(&agent.messages()[start_index..]);
    let usage = usage
        .lock()
        .map_err(|_| OpenResponsesApiError::internal("prompt usage lock is poisoned"))?
        .clone();

    let mut ignored = BTreeSet::new();
    for field in translated.ignored_fields {
        if !field.trim().is_empty() {
            ignored.insert(field);
        }
    }

    let response = OpenResponsesResponse {
        id: response_id,
        object: "response",
        created,
        status: "completed",
        model: state.config.model.clone(),
        output: vec![OpenResponsesOutputItem {
            id: state.next_output_message_id(),
            kind: "message",
            role: "assistant",
            content: vec![OpenResponsesOutputTextItem {
                kind: "output_text",
                text: output_text.clone(),
            }],
        }],
        output_text,
        usage: OpenResponsesUsage {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.total_tokens,
        },
        ignored_fields: ignored.into_iter().collect(),
    };

    Ok(OpenResponsesExecutionResult { response })
}

fn derive_gateway_preflight_token_limit(max_input_chars: usize) -> Option<u32> {
    if max_input_chars == 0 {
        return None;
    }
    let chars = u32::try_from(max_input_chars).unwrap_or(u32::MAX);
    Some(chars.saturating_add(3) / 4)
}

#[cfg(test)]
fn validate_gateway_openresponses_bind(bind: &str) -> Result<SocketAddr> {
    bind.parse::<SocketAddr>()
        .with_context(|| format!("invalid gateway socket address '{bind}'"))
}
