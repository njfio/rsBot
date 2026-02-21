//! Shared endpoint and policy constant definitions for gateway OpenResponses.

pub(super) const OPENRESPONSES_ENDPOINT: &str = "/v1/responses";
pub(super) const OPENAI_CHAT_COMPLETIONS_ENDPOINT: &str = "/v1/chat/completions";
pub(super) const OPENAI_COMPLETIONS_ENDPOINT: &str = "/v1/completions";
pub(super) const OPENAI_MODELS_ENDPOINT: &str = "/v1/models";
pub(super) const OPS_DASHBOARD_ENDPOINT: &str = "/ops";
pub(super) const OPS_DASHBOARD_AGENTS_ENDPOINT: &str = "/ops/agents";
pub(super) const OPS_DASHBOARD_AGENT_DETAIL_ENDPOINT: &str = "/ops/agents/{agent_id}";
pub(super) const OPS_DASHBOARD_CHAT_ENDPOINT: &str = "/ops/chat";
pub(super) const OPS_DASHBOARD_CHAT_NEW_ENDPOINT: &str = "/ops/chat/new";
pub(super) const OPS_DASHBOARD_CHAT_SEND_ENDPOINT: &str = "/ops/chat/send";
pub(super) const OPS_DASHBOARD_SESSIONS_ENDPOINT: &str = "/ops/sessions";
pub(super) const OPS_DASHBOARD_SESSION_DETAIL_ENDPOINT: &str = "/ops/sessions/{session_key}";
pub(super) const OPS_DASHBOARD_MEMORY_ENDPOINT: &str = "/ops/memory";
pub(super) const OPS_DASHBOARD_MEMORY_GRAPH_ENDPOINT: &str = "/ops/memory-graph";
pub(super) const OPS_DASHBOARD_TOOLS_JOBS_ENDPOINT: &str = "/ops/tools-jobs";
pub(super) const OPS_DASHBOARD_CHANNELS_ENDPOINT: &str = "/ops/channels";
pub(super) const OPS_DASHBOARD_CONFIG_ENDPOINT: &str = "/ops/config";
pub(super) const OPS_DASHBOARD_TRAINING_ENDPOINT: &str = "/ops/training";
pub(super) const OPS_DASHBOARD_SAFETY_ENDPOINT: &str = "/ops/safety";
pub(super) const OPS_DASHBOARD_DIAGNOSTICS_ENDPOINT: &str = "/ops/diagnostics";
pub(super) const OPS_DASHBOARD_DEPLOY_ENDPOINT: &str = "/ops/deploy";
pub(super) const OPS_DASHBOARD_LOGIN_ENDPOINT: &str = "/ops/login";
pub(super) const DASHBOARD_SHELL_ENDPOINT: &str = "/dashboard";
pub(super) const WEBCHAT_ENDPOINT: &str = "/webchat";
pub(super) const GATEWAY_STATUS_ENDPOINT: &str = "/gateway/status";
pub(super) const GATEWAY_WS_ENDPOINT: &str = "/gateway/ws";
pub(super) const GATEWAY_AUTH_BOOTSTRAP_ENDPOINT: &str = "/gateway/auth/bootstrap";
pub(super) const GATEWAY_AUTH_SESSION_ENDPOINT: &str = "/gateway/auth/session";
pub(super) const GATEWAY_SESSIONS_ENDPOINT: &str = "/gateway/sessions";
pub(super) const GATEWAY_SESSION_DETAIL_ENDPOINT: &str = "/gateway/sessions/{session_key}";
pub(super) const GATEWAY_SESSION_APPEND_ENDPOINT: &str = "/gateway/sessions/{session_key}/append";
pub(super) const GATEWAY_SESSION_RESET_ENDPOINT: &str = "/gateway/sessions/{session_key}/reset";
pub(super) const GATEWAY_MEMORY_ENDPOINT: &str = "/gateway/memory/{session_key}";
pub(super) const GATEWAY_MEMORY_ENTRY_ENDPOINT: &str = "/gateway/memory/{session_key}/{entry_id}";
pub(super) const GATEWAY_MEMORY_GRAPH_ENDPOINT: &str = "/gateway/memory-graph/{session_key}";
pub(super) const API_MEMORIES_GRAPH_ENDPOINT: &str = "/api/memories/graph";
pub(super) const GATEWAY_CHANNEL_LIFECYCLE_ENDPOINT: &str = "/gateway/channels/{channel}/lifecycle";
pub(super) const GATEWAY_CONFIG_ENDPOINT: &str = "/gateway/config";
pub(super) const GATEWAY_SAFETY_POLICY_ENDPOINT: &str = "/gateway/safety/policy";
pub(super) const GATEWAY_SAFETY_RULES_ENDPOINT: &str = "/gateway/safety/rules";
pub(super) const GATEWAY_SAFETY_TEST_ENDPOINT: &str = "/gateway/safety/test";
pub(super) const GATEWAY_AUDIT_SUMMARY_ENDPOINT: &str = "/gateway/audit/summary";
pub(super) const GATEWAY_AUDIT_LOG_ENDPOINT: &str = "/gateway/audit/log";
pub(super) const GATEWAY_TRAINING_STATUS_ENDPOINT: &str = "/gateway/training/status";
pub(super) const GATEWAY_TRAINING_ROLLOUTS_ENDPOINT: &str = "/gateway/training/rollouts";
pub(super) const GATEWAY_TRAINING_CONFIG_ENDPOINT: &str = "/gateway/training/config";
pub(super) const GATEWAY_TOOLS_ENDPOINT: &str = "/gateway/tools";
pub(super) const GATEWAY_TOOLS_STATS_ENDPOINT: &str = "/gateway/tools/stats";
pub(super) const GATEWAY_JOBS_ENDPOINT: &str = "/gateway/jobs";
pub(super) const GATEWAY_JOB_CANCEL_ENDPOINT_TEMPLATE: &str = "/gateway/jobs/{job_id}/cancel";
pub(super) const GATEWAY_DEPLOY_ENDPOINT: &str = "/gateway/deploy";
pub(super) const GATEWAY_AGENT_STOP_ENDPOINT_TEMPLATE: &str = "/gateway/agents/{agent_id}/stop";
pub(super) const GATEWAY_UI_TELEMETRY_ENDPOINT: &str = "/gateway/ui/telemetry";
pub(super) const CORTEX_CHAT_ENDPOINT: &str = "/cortex/chat";
pub(super) const CORTEX_STATUS_ENDPOINT: &str = "/cortex/status";
pub(super) const DASHBOARD_HEALTH_ENDPOINT: &str = "/dashboard/health";
pub(super) const DASHBOARD_WIDGETS_ENDPOINT: &str = "/dashboard/widgets";
pub(super) const DASHBOARD_QUEUE_TIMELINE_ENDPOINT: &str = "/dashboard/queue-timeline";
pub(super) const DASHBOARD_ALERTS_ENDPOINT: &str = "/dashboard/alerts";
pub(super) const DASHBOARD_ACTIONS_ENDPOINT: &str = "/dashboard/actions";
pub(super) const DASHBOARD_STREAM_ENDPOINT: &str = "/dashboard/stream";
pub(super) const EXTERNAL_CODING_AGENT_SESSIONS_ENDPOINT: &str =
    "/gateway/external-coding-agent/sessions";
pub(super) const EXTERNAL_CODING_AGENT_SESSION_DETAIL_ENDPOINT: &str =
    "/gateway/external-coding-agent/sessions/{session_id}";
pub(super) const EXTERNAL_CODING_AGENT_SESSION_PROGRESS_ENDPOINT: &str =
    "/gateway/external-coding-agent/sessions/{session_id}/progress";
pub(super) const EXTERNAL_CODING_AGENT_SESSION_FOLLOWUPS_ENDPOINT: &str =
    "/gateway/external-coding-agent/sessions/{session_id}/followups";
pub(super) const EXTERNAL_CODING_AGENT_SESSION_FOLLOWUPS_DRAIN_ENDPOINT: &str =
    "/gateway/external-coding-agent/sessions/{session_id}/followups/drain";
pub(super) const EXTERNAL_CODING_AGENT_SESSION_STREAM_ENDPOINT: &str =
    "/gateway/external-coding-agent/sessions/{session_id}/stream";
pub(super) const EXTERNAL_CODING_AGENT_SESSION_CLOSE_ENDPOINT: &str =
    "/gateway/external-coding-agent/sessions/{session_id}/close";
pub(super) const EXTERNAL_CODING_AGENT_REAP_ENDPOINT: &str = "/gateway/external-coding-agent/reap";
pub(super) const DEFAULT_SESSION_KEY: &str = "default";
pub(super) const INPUT_BODY_SIZE_MULTIPLIER: usize = 8;
pub(super) const GATEWAY_WS_HEARTBEAT_REQUEST_ID: &str = "gateway-heartbeat";
pub(super) const SESSION_WRITE_POLICY_GATE: &str = "allow_session_write";
pub(super) const MEMORY_WRITE_POLICY_GATE: &str = "allow_memory_write";
