//! Channel-store admin command handling and transport-health reporting.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tau_access::pairing_policy_for_state_dir;
use tau_cli::{Cli, CliGatewayRemoteProfile};
use tau_core::current_unix_timestamp_ms;
use tau_release_channel::{default_release_channel_path, load_release_channel_store};
use tau_runtime::channel_store::ChannelStore;

use crate::daemon_runtime::{inspect_tau_daemon, TauDaemonConfig};
use crate::transport_health::TransportHealthSnapshot;

mod command_parsing_helpers;
mod render_helpers;
mod transport_health_helpers;

use command_parsing_helpers::{parse_github_repo_slug, parse_transport_health_inspect_target};
#[cfg(test)]
use render_helpers::render_transport_health_row;
use render_helpers::{
    render_custom_command_status_report, render_dashboard_status_report,
    render_deployment_status_report, render_gateway_status_report, render_github_status_report,
    render_multi_agent_status_report, render_multi_channel_status_report,
    render_operator_control_summary_diff_report, render_operator_control_summary_report,
    render_transport_health_rows, render_voice_status_report,
};
use transport_health_helpers::{collect_transport_health_rows, sanitize_for_path};

#[derive(Debug, Clone, PartialEq, Eq)]
enum TransportHealthInspectTarget {
    Slack,
    GithubAll,
    GithubRepo { owner: String, repo: String },
    MultiChannel,
    MultiAgent,
    BrowserAutomation,
    Memory,
    Dashboard,
    Gateway,
    Deployment,
    CustomCommand,
    Voice,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct TransportHealthInspectRow {
    transport: String,
    target: String,
    state_path: String,
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct TransportHealthStateFile {
    #[serde(default)]
    health: TransportHealthSnapshot,
}

fn gateway_service_default_status() -> String {
    "running".to_string()
}

fn normalize_gateway_service_status(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "stopped" => "stopped",
        _ => "running",
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DashboardStatusStateFile {
    #[serde(default)]
    processed_case_keys: Vec<String>,
    #[serde(default)]
    widget_views: Vec<serde_json::Value>,
    #[serde(default)]
    control_audit: Vec<serde_json::Value>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct MultiChannelStatusStateFile {
    #[serde(default)]
    processed_event_keys: Vec<String>,
    #[serde(default)]
    health: TransportHealthSnapshot,
    #[serde(default)]
    telemetry: MultiChannelStatusTelemetryState,
    #[serde(default)]
    telemetry_policy: MultiChannelStatusTelemetryPolicyState,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct MultiChannelStatusTelemetryState {
    #[serde(default)]
    typing_events_emitted: usize,
    #[serde(default)]
    presence_events_emitted: usize,
    #[serde(default)]
    usage_summary_records: usize,
    #[serde(default)]
    usage_response_chars: usize,
    #[serde(default)]
    usage_chunks: usize,
    #[serde(default)]
    usage_estimated_cost_micros: u64,
    #[serde(default)]
    typing_events_by_transport: BTreeMap<String, usize>,
    #[serde(default)]
    presence_events_by_transport: BTreeMap<String, usize>,
    #[serde(default)]
    usage_summary_records_by_transport: BTreeMap<String, usize>,
    #[serde(default)]
    usage_response_chars_by_transport: BTreeMap<String, usize>,
    #[serde(default)]
    usage_chunks_by_transport: BTreeMap<String, usize>,
    #[serde(default)]
    usage_estimated_cost_micros_by_transport: BTreeMap<String, u64>,
}

fn multi_channel_telemetry_typing_presence_default() -> bool {
    true
}

fn multi_channel_telemetry_usage_summary_default() -> bool {
    true
}

fn multi_channel_telemetry_min_response_chars_default() -> usize {
    120
}

#[derive(Debug, Clone, Deserialize)]
struct MultiChannelStatusTelemetryPolicyState {
    #[serde(default = "multi_channel_telemetry_typing_presence_default")]
    typing_presence_enabled: bool,
    #[serde(default = "multi_channel_telemetry_usage_summary_default")]
    usage_summary_enabled: bool,
    #[serde(default)]
    include_identifiers: bool,
    #[serde(default = "multi_channel_telemetry_min_response_chars_default")]
    typing_presence_min_response_chars: usize,
}

impl Default for MultiChannelStatusTelemetryPolicyState {
    fn default() -> Self {
        Self {
            typing_presence_enabled: true,
            usage_summary_enabled: true,
            include_identifiers: false,
            typing_presence_min_response_chars: 120,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct GithubStatusStateFile {
    #[serde(default)]
    last_issue_scan_at: Option<String>,
    #[serde(default)]
    processed_event_keys: Vec<String>,
    #[serde(default)]
    issue_sessions: BTreeMap<String, GithubStatusIssueSessionState>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct GithubStatusIssueSessionState {
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    last_comment_id: Option<u64>,
    #[serde(default)]
    last_run_id: Option<String>,
    #[serde(default)]
    active_run_id: Option<String>,
    #[serde(default)]
    last_event_key: Option<String>,
    #[serde(default)]
    last_event_kind: Option<String>,
    #[serde(default)]
    last_actor_login: Option<String>,
    #[serde(default)]
    last_reason_code: Option<String>,
    #[serde(default)]
    last_processed_unix_ms: Option<u64>,
    #[serde(default)]
    total_processed_events: u64,
    #[serde(default)]
    total_duplicate_events: u64,
    #[serde(default)]
    total_failed_events: u64,
    #[serde(default)]
    total_denied_events: u64,
    #[serde(default)]
    total_runs_started: u64,
    #[serde(default)]
    total_runs_completed: u64,
    #[serde(default)]
    total_runs_failed: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct MultiAgentStatusStateFile {
    #[serde(default)]
    processed_case_keys: Vec<String>,
    #[serde(default)]
    routed_cases: Vec<MultiAgentStatusRoutedCase>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct GatewayStatusStateFile {
    #[serde(default)]
    processed_case_keys: Vec<String>,
    #[serde(default)]
    requests: Vec<GatewayStatusRequestRecord>,
    #[serde(default)]
    health: TransportHealthSnapshot,
    #[serde(default)]
    guardrail: GatewayStatusGuardrailState,
    #[serde(default)]
    service: GatewayStatusServiceState,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct CustomCommandStatusStateFile {
    #[serde(default)]
    processed_case_keys: Vec<String>,
    #[serde(default)]
    commands: Vec<CustomCommandStatusCommandRecord>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct VoiceStatusStateFile {
    #[serde(default)]
    processed_case_keys: Vec<String>,
    #[serde(default)]
    interactions: Vec<VoiceStatusInteractionRecord>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DeploymentStatusStateFile {
    #[serde(default)]
    processed_case_keys: Vec<String>,
    #[serde(default)]
    rollouts: Vec<DeploymentStatusPlanRecord>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct MultiAgentStatusRoutedCase {
    #[serde(default)]
    phase: String,
    #[serde(default)]
    selected_role: String,
    #[serde(default)]
    category: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct GatewayStatusRequestRecord {
    #[serde(default)]
    method: String,
    #[serde(default)]
    status_code: u16,
    #[serde(default)]
    error_code: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct GatewayStatusGuardrailState {
    #[serde(default)]
    gate: String,
    #[serde(default)]
    reason_code: String,
    #[serde(default)]
    failure_streak_threshold: usize,
    #[serde(default)]
    retryable_failures_threshold: usize,
    #[serde(default)]
    failure_streak: usize,
    #[serde(default)]
    last_failed_cases: usize,
    #[serde(default)]
    last_retryable_failures: usize,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct GatewayStatusServiceState {
    #[serde(default = "gateway_service_default_status")]
    status: String,
    #[serde(default)]
    startup_attempts: u64,
    #[serde(default)]
    startup_failure_streak: usize,
    #[serde(default)]
    last_startup_error: String,
    #[serde(default)]
    last_started_unix_ms: u64,
    #[serde(default)]
    last_stopped_unix_ms: u64,
    #[serde(default)]
    last_transition_unix_ms: u64,
    #[serde(default)]
    last_stop_reason: String,
}

impl Default for GatewayStatusServiceState {
    fn default() -> Self {
        Self {
            status: gateway_service_default_status(),
            startup_attempts: 0,
            startup_failure_streak: 0,
            last_startup_error: String::new(),
            last_started_unix_ms: 0,
            last_stopped_unix_ms: 0,
            last_transition_unix_ms: 0,
            last_stop_reason: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct CustomCommandStatusCommandRecord {
    #[serde(default)]
    command_name: String,
    #[serde(default)]
    operation: String,
    #[serde(default)]
    last_status_code: u16,
    #[serde(default)]
    last_outcome: String,
    #[serde(default)]
    run_count: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct VoiceStatusInteractionRecord {
    #[serde(default)]
    mode: String,
    #[serde(default)]
    speaker_id: String,
    #[serde(default)]
    last_status_code: u16,
    #[serde(default)]
    last_outcome: String,
    #[serde(default)]
    utterance: String,
    #[serde(default)]
    run_count: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DeploymentStatusPlanRecord {
    #[serde(default)]
    deploy_target: String,
    #[serde(default)]
    runtime_profile: String,
    #[serde(default)]
    environment: String,
    #[serde(default)]
    status_code: u16,
    #[serde(default)]
    outcome: String,
    #[serde(default)]
    error_code: String,
    #[serde(default)]
    replicas: u16,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DashboardCycleReportLine {
    #[serde(default)]
    reason_codes: Vec<String>,
    #[serde(default)]
    health_reason: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct MultiChannelCycleReportLine {
    #[serde(default)]
    reason_codes: Vec<String>,
    #[serde(default)]
    health_reason: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct MultiAgentCycleReportLine {
    #[serde(default)]
    reason_codes: Vec<String>,
    #[serde(default)]
    health_reason: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct GatewayCycleReportLine {
    #[serde(default)]
    reason_codes: Vec<String>,
    #[serde(default)]
    health_reason: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct CustomCommandCycleReportLine {
    #[serde(default)]
    reason_codes: Vec<String>,
    #[serde(default)]
    health_reason: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct VoiceCycleReportLine {
    #[serde(default)]
    reason_codes: Vec<String>,
    #[serde(default)]
    health_reason: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DeploymentCycleReportLine {
    #[serde(default)]
    reason_codes: Vec<String>,
    #[serde(default)]
    health_reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DashboardCycleReportSummary {
    events_log_present: bool,
    cycle_reports: usize,
    invalid_cycle_reports: usize,
    last_reason_codes: Vec<String>,
    last_health_reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct MultiChannelCycleReportSummary {
    events_log_present: bool,
    cycle_reports: usize,
    invalid_cycle_reports: usize,
    last_reason_codes: Vec<String>,
    last_health_reason: String,
    reason_code_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct GithubInboundLogSummary {
    log_present: bool,
    records: usize,
    invalid_records: usize,
    kind_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct GithubOutboundLogSummary {
    log_present: bool,
    records: usize,
    invalid_records: usize,
    command_counts: BTreeMap<String, usize>,
    status_counts: BTreeMap<String, usize>,
    reason_code_counts: BTreeMap<String, usize>,
    last_reason_codes: Vec<String>,
    last_event_key: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct MultiAgentCycleReportSummary {
    events_log_present: bool,
    cycle_reports: usize,
    invalid_cycle_reports: usize,
    last_reason_codes: Vec<String>,
    last_health_reason: String,
    reason_code_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct GatewayCycleReportSummary {
    events_log_present: bool,
    cycle_reports: usize,
    invalid_cycle_reports: usize,
    last_reason_codes: Vec<String>,
    last_health_reason: String,
    reason_code_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CustomCommandCycleReportSummary {
    events_log_present: bool,
    cycle_reports: usize,
    invalid_cycle_reports: usize,
    last_reason_codes: Vec<String>,
    last_health_reason: String,
    reason_code_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct VoiceCycleReportSummary {
    events_log_present: bool,
    cycle_reports: usize,
    invalid_cycle_reports: usize,
    last_reason_codes: Vec<String>,
    last_health_reason: String,
    reason_code_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DeploymentCycleReportSummary {
    events_log_present: bool,
    cycle_reports: usize,
    invalid_cycle_reports: usize,
    last_reason_codes: Vec<String>,
    last_health_reason: String,
    reason_code_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct DashboardStatusInspectReport {
    state_path: String,
    events_log_path: String,
    events_log_present: bool,
    health_state: String,
    health_reason: String,
    rollout_gate: String,
    processed_case_count: usize,
    widget_count: usize,
    control_audit_count: usize,
    cycle_reports: usize,
    invalid_cycle_reports: usize,
    last_reason_codes: Vec<String>,
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct MultiChannelStatusTelemetryPolicyInspectReport {
    typing_presence_enabled: bool,
    usage_summary_enabled: bool,
    include_identifiers: bool,
    typing_presence_min_response_chars: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct MultiChannelStatusTelemetryInspectReport {
    typing_events_emitted: usize,
    presence_events_emitted: usize,
    usage_summary_records: usize,
    usage_response_chars: usize,
    usage_chunks: usize,
    usage_estimated_cost_micros: u64,
    typing_events_by_transport: BTreeMap<String, usize>,
    presence_events_by_transport: BTreeMap<String, usize>,
    usage_summary_records_by_transport: BTreeMap<String, usize>,
    usage_response_chars_by_transport: BTreeMap<String, usize>,
    usage_chunks_by_transport: BTreeMap<String, usize>,
    usage_estimated_cost_micros_by_transport: BTreeMap<String, u64>,
    policy: MultiChannelStatusTelemetryPolicyInspectReport,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct MultiChannelStatusInspectReport {
    state_path: String,
    events_log_path: String,
    events_log_present: bool,
    health_state: String,
    health_reason: String,
    rollout_gate: String,
    processed_event_count: usize,
    transport_counts: BTreeMap<String, usize>,
    cycle_reports: usize,
    invalid_cycle_reports: usize,
    last_reason_codes: Vec<String>,
    reason_code_counts: BTreeMap<String, usize>,
    health: TransportHealthSnapshot,
    telemetry: MultiChannelStatusTelemetryInspectReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    connectors: Option<tau_multi_channel::MultiChannelLiveConnectorsStatusReport>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct GithubStatusIssueSessionInspectRow {
    issue_number: String,
    session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_comment_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_event_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_event_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_actor_login: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_reason_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_processed_unix_ms: Option<u64>,
    total_processed_events: u64,
    total_duplicate_events: u64,
    total_failed_events: u64,
    total_denied_events: u64,
    total_runs_started: u64,
    total_runs_completed: u64,
    total_runs_failed: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct GithubStatusInspectReport {
    repo: String,
    state_path: String,
    inbound_log_path: String,
    outbound_log_path: String,
    inbound_log_present: bool,
    outbound_log_present: bool,
    health_state: String,
    health_reason: String,
    rollout_gate: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_issue_scan_at: Option<String>,
    processed_event_count: usize,
    issue_session_count: usize,
    issue_sessions: Vec<GithubStatusIssueSessionInspectRow>,
    inbound_records: usize,
    inbound_invalid_records: usize,
    inbound_kind_counts: BTreeMap<String, usize>,
    outbound_records: usize,
    outbound_invalid_records: usize,
    outbound_command_counts: BTreeMap<String, usize>,
    outbound_status_counts: BTreeMap<String, usize>,
    outbound_reason_code_counts: BTreeMap<String, usize>,
    outbound_last_reason_codes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    outbound_last_event_key: Option<String>,
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct MultiAgentStatusInspectReport {
    state_path: String,
    events_log_path: String,
    events_log_present: bool,
    health_state: String,
    health_reason: String,
    rollout_gate: String,
    processed_case_count: usize,
    routed_case_count: usize,
    phase_counts: BTreeMap<String, usize>,
    selected_role_counts: BTreeMap<String, usize>,
    category_counts: BTreeMap<String, usize>,
    cycle_reports: usize,
    invalid_cycle_reports: usize,
    last_reason_codes: Vec<String>,
    reason_code_counts: BTreeMap<String, usize>,
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct GatewayStatusInspectReport {
    state_path: String,
    events_log_path: String,
    events_log_present: bool,
    health_state: String,
    health_reason: String,
    rollout_gate: String,
    rollout_reason_code: String,
    service_status: String,
    service_last_transition_unix_ms: u64,
    service_last_started_unix_ms: u64,
    service_last_stopped_unix_ms: u64,
    service_last_stop_reason: String,
    service_startup_attempts: u64,
    service_startup_failure_streak: usize,
    service_last_startup_error: String,
    guardrail_gate: String,
    guardrail_reason_code: String,
    processed_case_count: usize,
    request_count: usize,
    method_counts: BTreeMap<String, usize>,
    status_code_counts: BTreeMap<String, usize>,
    error_code_counts: BTreeMap<String, usize>,
    guardrail_failure_streak_threshold: usize,
    guardrail_retryable_failures_threshold: usize,
    guardrail_failure_streak: usize,
    guardrail_last_failed_cases: usize,
    guardrail_last_retryable_failures: usize,
    cycle_reports: usize,
    invalid_cycle_reports: usize,
    last_reason_codes: Vec<String>,
    reason_code_counts: BTreeMap<String, usize>,
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct CustomCommandStatusInspectReport {
    state_path: String,
    events_log_path: String,
    events_log_present: bool,
    health_state: String,
    health_reason: String,
    rollout_gate: String,
    processed_case_count: usize,
    command_count: usize,
    command_name_counts: BTreeMap<String, usize>,
    operation_counts: BTreeMap<String, usize>,
    outcome_counts: BTreeMap<String, usize>,
    status_code_counts: BTreeMap<String, usize>,
    total_run_count: u64,
    cycle_reports: usize,
    invalid_cycle_reports: usize,
    last_reason_codes: Vec<String>,
    reason_code_counts: BTreeMap<String, usize>,
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct VoiceStatusInspectReport {
    state_path: String,
    events_log_path: String,
    events_log_present: bool,
    health_state: String,
    health_reason: String,
    rollout_gate: String,
    processed_case_count: usize,
    interaction_count: usize,
    mode_counts: BTreeMap<String, usize>,
    speaker_counts: BTreeMap<String, usize>,
    outcome_counts: BTreeMap<String, usize>,
    status_code_counts: BTreeMap<String, usize>,
    utterance_count: usize,
    total_run_count: u64,
    cycle_reports: usize,
    invalid_cycle_reports: usize,
    last_reason_codes: Vec<String>,
    reason_code_counts: BTreeMap<String, usize>,
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct DeploymentStatusInspectReport {
    state_path: String,
    events_log_path: String,
    events_log_present: bool,
    health_state: String,
    health_reason: String,
    rollout_gate: String,
    processed_case_count: usize,
    rollout_count: usize,
    target_counts: BTreeMap<String, usize>,
    runtime_profile_counts: BTreeMap<String, usize>,
    environment_counts: BTreeMap<String, usize>,
    outcome_counts: BTreeMap<String, usize>,
    status_code_counts: BTreeMap<String, usize>,
    error_code_counts: BTreeMap<String, usize>,
    total_replicas: u64,
    wasm_rollout_count: usize,
    cloud_rollout_count: usize,
    cycle_reports: usize,
    invalid_cycle_reports: usize,
    last_reason_codes: Vec<String>,
    reason_code_counts: BTreeMap<String, usize>,
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct OperatorControlComponentSummaryRow {
    component: String,
    health_state: String,
    health_reason: String,
    rollout_gate: String,
    reason_code: String,
    recommendation: String,
    queue_depth: usize,
    failure_streak: usize,
    state_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OperatorControlComponentInputs {
    state_path: String,
    health_state: String,
    health_reason: String,
    rollout_gate: String,
    reason_code: String,
    recommendation: String,
    queue_depth: usize,
    failure_streak: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct OperatorControlDaemonSummary {
    health_state: String,
    rollout_gate: String,
    reason_code: String,
    recommendation: String,
    profile: String,
    installed: bool,
    running: bool,
    start_attempts: u64,
    stop_attempts: u64,
    diagnostics: usize,
    state_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct OperatorControlReleaseChannelSummary {
    health_state: String,
    rollout_gate: String,
    reason_code: String,
    recommendation: String,
    configured: bool,
    channel: String,
    path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct OperatorControlPolicyPosture {
    pairing_strict_effective: bool,
    pairing_config_strict_mode: bool,
    pairing_allowlist_strict: bool,
    pairing_rules_configured: bool,
    pairing_registry_entries: usize,
    pairing_allowlist_channel_rules: usize,
    provider_subscription_strict: bool,
    gateway_auth_mode: String,
    gateway_remote_profile: String,
    gateway_remote_posture: String,
    gateway_remote_gate: String,
    gateway_remote_risk_level: String,
    gateway_remote_reason_codes: Vec<String>,
    gateway_remote_recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct OperatorControlSummaryReport {
    generated_unix_ms: u64,
    health_state: String,
    rollout_gate: String,
    reason_codes: Vec<String>,
    recommendations: Vec<String>,
    policy_posture: OperatorControlPolicyPosture,
    daemon: OperatorControlDaemonSummary,
    release_channel: OperatorControlReleaseChannelSummary,
    components: Vec<OperatorControlComponentSummaryRow>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct OperatorControlSummaryDiffComponentRow {
    component: String,
    drift_state: String,
    severity: String,
    health_state_before: String,
    health_state_after: String,
    rollout_gate_before: String,
    rollout_gate_after: String,
    reason_code_before: String,
    reason_code_after: String,
    recommendation_before: String,
    recommendation_after: String,
    queue_depth_before: usize,
    queue_depth_after: usize,
    failure_streak_before: usize,
    failure_streak_after: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct OperatorControlSummaryDiffReport {
    generated_unix_ms: u64,
    baseline_generated_unix_ms: u64,
    current_generated_unix_ms: u64,
    drift_state: String,
    risk_level: String,
    health_state_before: String,
    health_state_after: String,
    rollout_gate_before: String,
    rollout_gate_after: String,
    reason_codes_added: Vec<String>,
    reason_codes_removed: Vec<String>,
    recommendations_added: Vec<String>,
    recommendations_removed: Vec<String>,
    changed_components: Vec<OperatorControlSummaryDiffComponentRow>,
    unchanged_component_count: usize,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct PairingAllowlistSummaryFile {
    #[serde(default)]
    strict: bool,
    #[serde(default)]
    channels: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct PairingRegistrySummaryFile {
    #[serde(default)]
    pairings: Vec<serde_json::Value>,
}

pub fn execute_channel_store_admin_command(cli: &Cli) -> Result<()> {
    if let Some(raw_target) = cli.transport_health_inspect.as_deref() {
        let target = parse_transport_health_inspect_target(raw_target)?;
        let rows = collect_transport_health_rows(cli, &target)?;
        if cli.transport_health_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&rows)
                    .context("failed to render transport health json")?
            );
        } else {
            println!("{}", render_transport_health_rows(&rows));
        }
        return Ok(());
    }

    if let Some(repo_slug) = cli.github_status_inspect.as_deref() {
        let report = collect_github_status_report(cli, repo_slug)?;
        if cli.github_status_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .context("failed to render github status json")?
            );
        } else {
            println!("{}", render_github_status_report(&report));
        }
        return Ok(());
    }

    if cli.operator_control_summary {
        let report = collect_operator_control_summary_report(cli)?;
        if let Some(snapshot_path) = cli.operator_control_summary_snapshot_out.as_deref() {
            save_operator_control_summary_snapshot(snapshot_path, &report)?;
        }

        if let Some(compare_path) = cli.operator_control_summary_compare.as_deref() {
            let baseline = load_operator_control_summary_snapshot(compare_path)?;
            let drift = build_operator_control_summary_diff_report(&baseline, &report);
            if cli.operator_control_summary_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&drift)
                        .context("failed to render operator control summary diff json")?
                );
            } else {
                println!("{}", render_operator_control_summary_diff_report(&drift));
            }
        } else if cli.operator_control_summary_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .context("failed to render operator control summary json")?
            );
        } else {
            println!("{}", render_operator_control_summary_report(&report));
        }
        return Ok(());
    }

    if cli.dashboard_status_inspect {
        let report = collect_dashboard_status_report(cli)?;
        if cli.dashboard_status_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .context("failed to render dashboard status json")?
            );
        } else {
            println!("{}", render_dashboard_status_report(&report));
        }
        return Ok(());
    }

    if cli.multi_channel_status_inspect {
        let report = collect_multi_channel_status_report(cli)?;
        if cli.multi_channel_status_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .context("failed to render multi-channel status json")?
            );
        } else {
            println!("{}", render_multi_channel_status_report(&report));
        }
        return Ok(());
    }

    if cli.multi_agent_status_inspect {
        let report = collect_multi_agent_status_report(cli)?;
        if cli.multi_agent_status_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .context("failed to render multi-agent status json")?
            );
        } else {
            println!("{}", render_multi_agent_status_report(&report));
        }
        return Ok(());
    }

    if cli.gateway_status_inspect {
        let report = collect_gateway_status_report(cli)?;
        if cli.gateway_status_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .context("failed to render gateway status json")?
            );
        } else {
            println!("{}", render_gateway_status_report(&report));
        }
        return Ok(());
    }

    if cli.custom_command_status_inspect {
        let report = collect_custom_command_status_report(cli)?;
        if cli.custom_command_status_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .context("failed to render custom-command status json")?
            );
        } else {
            println!("{}", render_custom_command_status_report(&report));
        }
        return Ok(());
    }

    if cli.voice_status_inspect {
        let report = collect_voice_status_report(cli)?;
        if cli.voice_status_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .context("failed to render voice status json")?
            );
        } else {
            println!("{}", render_voice_status_report(&report));
        }
        return Ok(());
    }

    if cli.deployment_status_inspect {
        let report = collect_deployment_status_report(cli)?;
        if cli.deployment_status_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .context("failed to render deployment status json")?
            );
        } else {
            println!("{}", render_deployment_status_report(&report));
        }
        return Ok(());
    }

    if let Some(raw_ref) = cli.channel_store_inspect.as_deref() {
        let channel_ref = ChannelStore::parse_channel_ref(raw_ref)?;
        let store = ChannelStore::open(
            &cli.channel_store_root,
            &channel_ref.transport,
            &channel_ref.channel_id,
        )?;
        let report = store.inspect()?;
        println!(
            "channel store inspect: transport={} channel_id={} dir={} log_records={} context_records={} invalid_log_lines={} invalid_context_lines={} artifact_records={} invalid_artifact_lines={} active_artifacts={} expired_artifacts={} memory_exists={} memory_bytes={}",
            report.transport,
            report.channel_id,
            report.channel_dir.display(),
            report.log_records,
            report.context_records,
            report.invalid_log_lines,
            report.invalid_context_lines,
            report.artifact_records,
            report.invalid_artifact_lines,
            report.active_artifacts,
            report.expired_artifacts,
            report.memory_exists,
            report.memory_bytes,
        );
        return Ok(());
    }

    if let Some(raw_ref) = cli.channel_store_repair.as_deref() {
        let channel_ref = ChannelStore::parse_channel_ref(raw_ref)?;
        let store = ChannelStore::open(
            &cli.channel_store_root,
            &channel_ref.transport,
            &channel_ref.channel_id,
        )?;
        let report = store.repair()?;
        println!(
            "channel store repair: transport={} channel_id={} log_removed_lines={} context_removed_lines={} artifact_expired_removed={} artifact_invalid_removed={} log_backup_path={} context_backup_path={}",
            channel_ref.transport,
            channel_ref.channel_id,
            report.log_removed_lines,
            report.context_removed_lines,
            report.artifact_expired_removed,
            report.artifact_invalid_removed,
            report
                .log_backup_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string()),
            report
                .context_backup_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string()),
        );
        return Ok(());
    }

    Ok(())
}

fn collect_github_status_report(cli: &Cli, repo_slug: &str) -> Result<GithubStatusInspectReport> {
    let (owner, repo) = parse_github_repo_slug(repo_slug)?;
    let normalized_repo = format!("{owner}/{repo}");
    let repo_dir = sanitize_for_path(&format!("{owner}__{repo}"));
    let repo_root = cli.github_state_dir.join(repo_dir);
    let state_path = repo_root.join("state.json");
    let inbound_log_path = repo_root.join("inbound-events.jsonl");
    let outbound_log_path = repo_root.join("outbound-events.jsonl");

    let state = load_github_status_state(&state_path)?;
    let inbound_summary = load_github_inbound_log_summary(&inbound_log_path)?;
    let outbound_summary = load_github_outbound_log_summary(&outbound_log_path)?;
    let classification = state.health.classify();
    let rollout_gate = if classification.state.as_str() == "healthy" {
        "pass"
    } else {
        "hold"
    };

    let mut issue_sessions = state
        .issue_sessions
        .into_iter()
        .map(
            |(issue_number, session)| GithubStatusIssueSessionInspectRow {
                issue_number,
                session_id: session.session_id,
                last_comment_id: session.last_comment_id,
                last_run_id: session.last_run_id,
                active_run_id: session.active_run_id,
                last_event_key: session.last_event_key,
                last_event_kind: session.last_event_kind,
                last_actor_login: session.last_actor_login,
                last_reason_code: session.last_reason_code,
                last_processed_unix_ms: session.last_processed_unix_ms,
                total_processed_events: session.total_processed_events,
                total_duplicate_events: session.total_duplicate_events,
                total_failed_events: session.total_failed_events,
                total_denied_events: session.total_denied_events,
                total_runs_started: session.total_runs_started,
                total_runs_completed: session.total_runs_completed,
                total_runs_failed: session.total_runs_failed,
            },
        )
        .collect::<Vec<_>>();
    issue_sessions.sort_by(|left, right| left.issue_number.cmp(&right.issue_number));

    Ok(GithubStatusInspectReport {
        repo: normalized_repo,
        state_path: state_path.display().to_string(),
        inbound_log_path: inbound_log_path.display().to_string(),
        outbound_log_path: outbound_log_path.display().to_string(),
        inbound_log_present: inbound_summary.log_present,
        outbound_log_present: outbound_summary.log_present,
        health_state: classification.state.as_str().to_string(),
        health_reason: classification.reason.to_string(),
        rollout_gate: rollout_gate.to_string(),
        last_issue_scan_at: state.last_issue_scan_at,
        processed_event_count: state.processed_event_keys.len(),
        issue_session_count: issue_sessions.len(),
        issue_sessions,
        inbound_records: inbound_summary.records,
        inbound_invalid_records: inbound_summary.invalid_records,
        inbound_kind_counts: inbound_summary.kind_counts,
        outbound_records: outbound_summary.records,
        outbound_invalid_records: outbound_summary.invalid_records,
        outbound_command_counts: outbound_summary.command_counts,
        outbound_status_counts: outbound_summary.status_counts,
        outbound_reason_code_counts: outbound_summary.reason_code_counts,
        outbound_last_reason_codes: outbound_summary.last_reason_codes,
        outbound_last_event_key: outbound_summary.last_event_key,
        health: state.health,
    })
}

fn load_github_status_state(path: &Path) -> Result<GithubStatusStateFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read state file {}", path.display()))?;
    serde_json::from_str::<GithubStatusStateFile>(&raw)
        .with_context(|| format!("failed to parse state file {}", path.display()))
}

fn load_github_inbound_log_summary(path: &Path) -> Result<GithubInboundLogSummary> {
    if !path.exists() {
        return Ok(GithubInboundLogSummary {
            log_present: false,
            ..GithubInboundLogSummary::default()
        });
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut summary = GithubInboundLogSummary {
        log_present: true,
        ..GithubInboundLogSummary::default()
    };
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(record) => {
                summary.records = summary.records.saturating_add(1);
                if let Some(kind) = record.get("kind").and_then(serde_json::Value::as_str) {
                    increment_count(&mut summary.kind_counts, kind);
                }
            }
            Err(_) => {
                summary.invalid_records = summary.invalid_records.saturating_add(1);
            }
        }
    }
    Ok(summary)
}

fn load_github_outbound_log_summary(path: &Path) -> Result<GithubOutboundLogSummary> {
    if !path.exists() {
        return Ok(GithubOutboundLogSummary {
            log_present: false,
            ..GithubOutboundLogSummary::default()
        });
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut summary = GithubOutboundLogSummary {
        log_present: true,
        ..GithubOutboundLogSummary::default()
    };
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(record) => {
                summary.records = summary.records.saturating_add(1);
                if let Some(command) = record.get("command").and_then(serde_json::Value::as_str) {
                    increment_count(&mut summary.command_counts, command);
                }
                if let Some(status) = record.get("status").and_then(serde_json::Value::as_str) {
                    increment_count(&mut summary.status_counts, status);
                }
                if let Some(reason_code) = record
                    .get("reason_code")
                    .and_then(serde_json::Value::as_str)
                {
                    increment_count(&mut summary.reason_code_counts, reason_code);
                    summary.last_reason_codes.push(reason_code.to_string());
                    if summary.last_reason_codes.len() > 5 {
                        summary.last_reason_codes.remove(0);
                    }
                }
                summary.last_event_key = record
                    .get("event_key")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
            }
            Err(_) => {
                summary.invalid_records = summary.invalid_records.saturating_add(1);
            }
        }
    }
    Ok(summary)
}

fn collect_dashboard_status_report(cli: &Cli) -> Result<DashboardStatusInspectReport> {
    let state_path = cli.dashboard_state_dir.join("state.json");
    let events_log_path = cli.dashboard_state_dir.join("runtime-events.jsonl");
    let state = load_dashboard_status_state(&state_path)?;
    let cycle_summary = load_dashboard_cycle_report_summary(&events_log_path)?;
    let classification = state.health.classify();
    let health_reason = if !cycle_summary.last_health_reason.trim().is_empty() {
        cycle_summary.last_health_reason.clone()
    } else {
        classification.reason
    };
    let rollout_gate = if classification.state.as_str() == "healthy" {
        "pass"
    } else {
        "hold"
    };

    Ok(DashboardStatusInspectReport {
        state_path: state_path.display().to_string(),
        events_log_path: events_log_path.display().to_string(),
        events_log_present: cycle_summary.events_log_present,
        health_state: classification.state.as_str().to_string(),
        health_reason,
        rollout_gate: rollout_gate.to_string(),
        processed_case_count: state.processed_case_keys.len(),
        widget_count: state.widget_views.len(),
        control_audit_count: state.control_audit.len(),
        cycle_reports: cycle_summary.cycle_reports,
        invalid_cycle_reports: cycle_summary.invalid_cycle_reports,
        last_reason_codes: cycle_summary.last_reason_codes,
        health: state.health,
    })
}

fn load_dashboard_status_state(path: &Path) -> Result<DashboardStatusStateFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str::<DashboardStatusStateFile>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))
}

fn load_dashboard_cycle_report_summary(path: &Path) -> Result<DashboardCycleReportSummary> {
    if !path.exists() {
        return Ok(DashboardCycleReportSummary {
            events_log_present: false,
            ..DashboardCycleReportSummary::default()
        });
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut summary = DashboardCycleReportSummary {
        events_log_present: true,
        ..DashboardCycleReportSummary::default()
    };
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<DashboardCycleReportLine>(trimmed) {
            Ok(report) => {
                summary.cycle_reports = summary.cycle_reports.saturating_add(1);
                summary.last_reason_codes = report.reason_codes;
                summary.last_health_reason = report.health_reason;
            }
            Err(_) => {
                summary.invalid_cycle_reports = summary.invalid_cycle_reports.saturating_add(1);
            }
        }
    }
    Ok(summary)
}

fn collect_multi_channel_status_report(cli: &Cli) -> Result<MultiChannelStatusInspectReport> {
    let state_path = cli.multi_channel_state_dir.join("state.json");
    let events_log_path = cli.multi_channel_state_dir.join("runtime-events.jsonl");
    let state = load_multi_channel_status_state(&state_path)?;
    let cycle_summary = load_multi_channel_cycle_report_summary(&events_log_path)?;
    let classification = state.health.classify();
    let health_reason = if !cycle_summary.last_health_reason.trim().is_empty() {
        cycle_summary.last_health_reason.clone()
    } else {
        classification.reason
    };
    let rollout_gate = if classification.state.as_str() == "healthy" {
        "pass"
    } else {
        "hold"
    };

    let mut transport_counts = BTreeMap::new();
    for event_key in &state.processed_event_keys {
        let Some((transport, _)) = event_key.split_once(':') else {
            continue;
        };
        increment_count(&mut transport_counts, &transport.to_ascii_lowercase());
    }

    let connectors = match tau_multi_channel::load_multi_channel_live_connectors_status_report(
        &cli.multi_channel_live_connectors_state_path,
    ) {
        Ok(report) if report.state_present => Some(report),
        Ok(_) => None,
        Err(_) => None,
    };

    Ok(MultiChannelStatusInspectReport {
        state_path: state_path.display().to_string(),
        events_log_path: events_log_path.display().to_string(),
        events_log_present: cycle_summary.events_log_present,
        health_state: classification.state.as_str().to_string(),
        health_reason,
        rollout_gate: rollout_gate.to_string(),
        processed_event_count: state.processed_event_keys.len(),
        transport_counts,
        cycle_reports: cycle_summary.cycle_reports,
        invalid_cycle_reports: cycle_summary.invalid_cycle_reports,
        last_reason_codes: cycle_summary.last_reason_codes,
        reason_code_counts: cycle_summary.reason_code_counts,
        health: state.health,
        telemetry: MultiChannelStatusTelemetryInspectReport {
            typing_events_emitted: state.telemetry.typing_events_emitted,
            presence_events_emitted: state.telemetry.presence_events_emitted,
            usage_summary_records: state.telemetry.usage_summary_records,
            usage_response_chars: state.telemetry.usage_response_chars,
            usage_chunks: state.telemetry.usage_chunks,
            usage_estimated_cost_micros: state.telemetry.usage_estimated_cost_micros,
            typing_events_by_transport: state.telemetry.typing_events_by_transport,
            presence_events_by_transport: state.telemetry.presence_events_by_transport,
            usage_summary_records_by_transport: state.telemetry.usage_summary_records_by_transport,
            usage_response_chars_by_transport: state.telemetry.usage_response_chars_by_transport,
            usage_chunks_by_transport: state.telemetry.usage_chunks_by_transport,
            usage_estimated_cost_micros_by_transport: state
                .telemetry
                .usage_estimated_cost_micros_by_transport,
            policy: MultiChannelStatusTelemetryPolicyInspectReport {
                typing_presence_enabled: state.telemetry_policy.typing_presence_enabled,
                usage_summary_enabled: state.telemetry_policy.usage_summary_enabled,
                include_identifiers: state.telemetry_policy.include_identifiers,
                typing_presence_min_response_chars: state
                    .telemetry_policy
                    .typing_presence_min_response_chars,
            },
        },
        connectors,
    })
}

fn collect_multi_agent_status_report(cli: &Cli) -> Result<MultiAgentStatusInspectReport> {
    let state_path = cli.multi_agent_state_dir.join("state.json");
    let events_log_path = cli.multi_agent_state_dir.join("runtime-events.jsonl");
    let state = load_multi_agent_status_state(&state_path)?;
    let cycle_summary = load_multi_agent_cycle_report_summary(&events_log_path)?;
    let classification = state.health.classify();
    let health_reason = if !cycle_summary.last_health_reason.trim().is_empty() {
        cycle_summary.last_health_reason.clone()
    } else {
        classification.reason
    };
    let rollout_gate = if classification.state.as_str() == "healthy" {
        "pass"
    } else {
        "hold"
    };

    let mut phase_counts = BTreeMap::new();
    let mut selected_role_counts = BTreeMap::new();
    let mut category_counts = BTreeMap::new();
    for routed_case in &state.routed_cases {
        if !routed_case.phase.trim().is_empty() {
            increment_count(&mut phase_counts, routed_case.phase.trim());
        }
        if !routed_case.selected_role.trim().is_empty() {
            increment_count(&mut selected_role_counts, routed_case.selected_role.trim());
        }
        if !routed_case.category.trim().is_empty() {
            increment_count(&mut category_counts, routed_case.category.trim());
        }
    }

    Ok(MultiAgentStatusInspectReport {
        state_path: state_path.display().to_string(),
        events_log_path: events_log_path.display().to_string(),
        events_log_present: cycle_summary.events_log_present,
        health_state: classification.state.as_str().to_string(),
        health_reason,
        rollout_gate: rollout_gate.to_string(),
        processed_case_count: state.processed_case_keys.len(),
        routed_case_count: state.routed_cases.len(),
        phase_counts,
        selected_role_counts,
        category_counts,
        cycle_reports: cycle_summary.cycle_reports,
        invalid_cycle_reports: cycle_summary.invalid_cycle_reports,
        last_reason_codes: cycle_summary.last_reason_codes,
        reason_code_counts: cycle_summary.reason_code_counts,
        health: state.health,
    })
}

fn collect_gateway_status_report(cli: &Cli) -> Result<GatewayStatusInspectReport> {
    let state_path = cli.gateway_state_dir.join("state.json");
    let events_log_path = cli.gateway_state_dir.join("runtime-events.jsonl");
    let state = load_gateway_status_state(&state_path)?;
    let cycle_summary = load_gateway_cycle_report_summary(&events_log_path)?;
    let classification = state.health.classify();
    let health_reason = if !cycle_summary.last_health_reason.trim().is_empty() {
        cycle_summary.last_health_reason.clone()
    } else {
        classification.reason
    };
    let fallback_guardrail_gate = if classification.state.as_str() == "healthy" {
        "pass"
    } else {
        "hold"
    };
    let guardrail_gate = match state.guardrail.gate.trim().to_ascii_lowercase().as_str() {
        "pass" => "pass",
        "hold" => "hold",
        _ => fallback_guardrail_gate,
    };
    let guardrail_reason_code = if !state.guardrail.reason_code.trim().is_empty() {
        state.guardrail.reason_code.trim().to_string()
    } else if guardrail_gate == "pass" {
        "guardrail_checks_passing".to_string()
    } else {
        "health_state_not_healthy".to_string()
    };
    let service_status = normalize_gateway_service_status(&state.service.status).to_string();
    let rollout_gate = if service_status == "stopped" {
        "hold".to_string()
    } else {
        guardrail_gate.to_string()
    };
    let rollout_reason_code = if service_status == "stopped" {
        "service_stopped".to_string()
    } else {
        guardrail_reason_code.clone()
    };

    let mut method_counts = BTreeMap::new();
    let mut status_code_counts = BTreeMap::new();
    let mut error_code_counts = BTreeMap::new();
    for request in &state.requests {
        if !request.method.trim().is_empty() {
            increment_count(
                &mut method_counts,
                &request.method.trim().to_ascii_uppercase(),
            );
        }
        if request.status_code > 0 {
            increment_count(&mut status_code_counts, &request.status_code.to_string());
        }
        if !request.error_code.trim().is_empty() {
            increment_count(&mut error_code_counts, request.error_code.trim());
        }
    }

    Ok(GatewayStatusInspectReport {
        state_path: state_path.display().to_string(),
        events_log_path: events_log_path.display().to_string(),
        events_log_present: cycle_summary.events_log_present,
        health_state: classification.state.as_str().to_string(),
        health_reason,
        rollout_gate,
        rollout_reason_code,
        service_status,
        service_last_transition_unix_ms: state.service.last_transition_unix_ms,
        service_last_started_unix_ms: state.service.last_started_unix_ms,
        service_last_stopped_unix_ms: state.service.last_stopped_unix_ms,
        service_last_stop_reason: state.service.last_stop_reason.clone(),
        service_startup_attempts: state.service.startup_attempts,
        service_startup_failure_streak: state.service.startup_failure_streak,
        service_last_startup_error: state.service.last_startup_error.clone(),
        guardrail_gate: guardrail_gate.to_string(),
        guardrail_reason_code,
        processed_case_count: state.processed_case_keys.len(),
        request_count: state.requests.len(),
        method_counts,
        status_code_counts,
        error_code_counts,
        guardrail_failure_streak_threshold: state.guardrail.failure_streak_threshold,
        guardrail_retryable_failures_threshold: state.guardrail.retryable_failures_threshold,
        guardrail_failure_streak: state.guardrail.failure_streak,
        guardrail_last_failed_cases: state.guardrail.last_failed_cases,
        guardrail_last_retryable_failures: state.guardrail.last_retryable_failures,
        cycle_reports: cycle_summary.cycle_reports,
        invalid_cycle_reports: cycle_summary.invalid_cycle_reports,
        last_reason_codes: cycle_summary.last_reason_codes,
        reason_code_counts: cycle_summary.reason_code_counts,
        health: state.health,
    })
}

fn collect_custom_command_status_report(cli: &Cli) -> Result<CustomCommandStatusInspectReport> {
    let state_path = cli.custom_command_state_dir.join("state.json");
    let events_log_path = cli.custom_command_state_dir.join("runtime-events.jsonl");
    let state = load_custom_command_status_state(&state_path)?;
    let cycle_summary = load_custom_command_cycle_report_summary(&events_log_path)?;
    let classification = state.health.classify();
    let health_reason = if !cycle_summary.last_health_reason.trim().is_empty() {
        cycle_summary.last_health_reason.clone()
    } else {
        classification.reason
    };
    let rollout_gate = if classification.state.as_str() == "healthy" {
        "pass"
    } else {
        "hold"
    };

    let mut operation_counts = BTreeMap::new();
    let mut command_name_counts = BTreeMap::new();
    let mut outcome_counts = BTreeMap::new();
    let mut status_code_counts = BTreeMap::new();
    let mut total_run_count = 0_u64;
    for command in &state.commands {
        if !command.command_name.trim().is_empty() {
            increment_count(&mut command_name_counts, command.command_name.trim());
        }
        if !command.operation.trim().is_empty() {
            increment_count(
                &mut operation_counts,
                &command.operation.trim().to_ascii_uppercase(),
            );
        }
        if !command.last_outcome.trim().is_empty() {
            increment_count(&mut outcome_counts, command.last_outcome.trim());
        }
        if command.last_status_code > 0 {
            increment_count(
                &mut status_code_counts,
                &command.last_status_code.to_string(),
            );
        }
        total_run_count = total_run_count.saturating_add(command.run_count);
    }

    Ok(CustomCommandStatusInspectReport {
        state_path: state_path.display().to_string(),
        events_log_path: events_log_path.display().to_string(),
        events_log_present: cycle_summary.events_log_present,
        health_state: classification.state.as_str().to_string(),
        health_reason,
        rollout_gate: rollout_gate.to_string(),
        processed_case_count: state.processed_case_keys.len(),
        command_count: state.commands.len(),
        command_name_counts,
        operation_counts,
        outcome_counts,
        status_code_counts,
        total_run_count,
        cycle_reports: cycle_summary.cycle_reports,
        invalid_cycle_reports: cycle_summary.invalid_cycle_reports,
        last_reason_codes: cycle_summary.last_reason_codes,
        reason_code_counts: cycle_summary.reason_code_counts,
        health: state.health,
    })
}

fn collect_voice_status_report(cli: &Cli) -> Result<VoiceStatusInspectReport> {
    let state_path = cli.voice_state_dir.join("state.json");
    let events_log_path = cli.voice_state_dir.join("runtime-events.jsonl");
    let state = load_voice_status_state(&state_path)?;
    let cycle_summary = load_voice_cycle_report_summary(&events_log_path)?;
    let classification = state.health.classify();
    let health_reason = if !cycle_summary.last_health_reason.trim().is_empty() {
        cycle_summary.last_health_reason.clone()
    } else {
        classification.reason
    };
    let rollout_gate = if classification.state.as_str() == "healthy" {
        "pass"
    } else {
        "hold"
    };

    let mut mode_counts = BTreeMap::new();
    let mut speaker_counts = BTreeMap::new();
    let mut outcome_counts = BTreeMap::new();
    let mut status_code_counts = BTreeMap::new();
    let mut utterance_count = 0usize;
    let mut total_run_count = 0u64;

    for interaction in &state.interactions {
        if !interaction.mode.trim().is_empty() {
            increment_count(&mut mode_counts, interaction.mode.trim());
        }
        if !interaction.speaker_id.trim().is_empty() {
            increment_count(&mut speaker_counts, interaction.speaker_id.trim());
        }
        if !interaction.last_outcome.trim().is_empty() {
            increment_count(&mut outcome_counts, interaction.last_outcome.trim());
        }
        if interaction.last_status_code > 0 {
            increment_count(
                &mut status_code_counts,
                &interaction.last_status_code.to_string(),
            );
        }
        if !interaction.utterance.trim().is_empty() {
            utterance_count = utterance_count.saturating_add(1);
        }
        total_run_count = total_run_count.saturating_add(interaction.run_count);
    }

    Ok(VoiceStatusInspectReport {
        state_path: state_path.display().to_string(),
        events_log_path: events_log_path.display().to_string(),
        events_log_present: cycle_summary.events_log_present,
        health_state: classification.state.as_str().to_string(),
        health_reason,
        rollout_gate: rollout_gate.to_string(),
        processed_case_count: state.processed_case_keys.len(),
        interaction_count: state.interactions.len(),
        mode_counts,
        speaker_counts,
        outcome_counts,
        status_code_counts,
        utterance_count,
        total_run_count,
        cycle_reports: cycle_summary.cycle_reports,
        invalid_cycle_reports: cycle_summary.invalid_cycle_reports,
        last_reason_codes: cycle_summary.last_reason_codes,
        reason_code_counts: cycle_summary.reason_code_counts,
        health: state.health,
    })
}

fn collect_deployment_status_report(cli: &Cli) -> Result<DeploymentStatusInspectReport> {
    let state_path = cli.deployment_state_dir.join("state.json");
    let events_log_path = cli.deployment_state_dir.join("runtime-events.jsonl");
    let state = load_deployment_status_state(&state_path)?;
    let cycle_summary = load_deployment_cycle_report_summary(&events_log_path)?;
    let classification = state.health.classify();
    let health_reason = if !cycle_summary.last_health_reason.trim().is_empty() {
        cycle_summary.last_health_reason.clone()
    } else {
        classification.reason
    };
    let rollout_gate = if classification.state.as_str() == "healthy" {
        "pass"
    } else {
        "hold"
    };

    let mut target_counts = BTreeMap::new();
    let mut runtime_profile_counts = BTreeMap::new();
    let mut environment_counts = BTreeMap::new();
    let mut outcome_counts = BTreeMap::new();
    let mut status_code_counts = BTreeMap::new();
    let mut error_code_counts = BTreeMap::new();
    let mut total_replicas = 0_u64;
    let mut wasm_rollout_count = 0usize;
    let mut cloud_rollout_count = 0usize;
    for rollout in &state.rollouts {
        let deploy_target = rollout.deploy_target.trim().to_ascii_lowercase();
        if !deploy_target.is_empty() {
            increment_count(&mut target_counts, &deploy_target);
            if deploy_target == "wasm" {
                wasm_rollout_count = wasm_rollout_count.saturating_add(1);
            } else {
                cloud_rollout_count = cloud_rollout_count.saturating_add(1);
            }
        }
        if !rollout.runtime_profile.trim().is_empty() {
            increment_count(
                &mut runtime_profile_counts,
                &rollout.runtime_profile.trim().to_ascii_lowercase(),
            );
        }
        if !rollout.environment.trim().is_empty() {
            increment_count(
                &mut environment_counts,
                &rollout.environment.trim().to_ascii_lowercase(),
            );
        }
        if !rollout.outcome.trim().is_empty() {
            increment_count(&mut outcome_counts, rollout.outcome.trim());
        }
        if rollout.status_code > 0 {
            increment_count(&mut status_code_counts, &rollout.status_code.to_string());
        }
        if !rollout.error_code.trim().is_empty() {
            increment_count(&mut error_code_counts, rollout.error_code.trim());
        }
        total_replicas = total_replicas.saturating_add(u64::from(rollout.replicas));
    }

    Ok(DeploymentStatusInspectReport {
        state_path: state_path.display().to_string(),
        events_log_path: events_log_path.display().to_string(),
        events_log_present: cycle_summary.events_log_present,
        health_state: classification.state.as_str().to_string(),
        health_reason,
        rollout_gate: rollout_gate.to_string(),
        processed_case_count: state.processed_case_keys.len(),
        rollout_count: state.rollouts.len(),
        target_counts,
        runtime_profile_counts,
        environment_counts,
        outcome_counts,
        status_code_counts,
        error_code_counts,
        total_replicas,
        wasm_rollout_count,
        cloud_rollout_count,
        cycle_reports: cycle_summary.cycle_reports,
        invalid_cycle_reports: cycle_summary.invalid_cycle_reports,
        last_reason_codes: cycle_summary.last_reason_codes,
        reason_code_counts: cycle_summary.reason_code_counts,
        health: state.health,
    })
}

fn collect_operator_control_summary_report(cli: &Cli) -> Result<OperatorControlSummaryReport> {
    let components = vec![
        collect_operator_dashboard_component(cli),
        collect_operator_multi_channel_component(cli),
        collect_operator_multi_agent_component(cli),
        collect_operator_gateway_component(cli),
        collect_operator_deployment_component(cli),
        collect_operator_custom_command_component(cli),
        collect_operator_voice_component(cli),
    ];
    let daemon = collect_operator_daemon_summary(cli);
    let release_channel = collect_operator_release_channel_summary();
    let policy_posture = collect_operator_policy_posture(cli);

    let mut rollout_gate = "pass".to_string();
    let mut health_rank = 0_u8;
    let mut reason_codes = Vec::new();
    let mut recommendations = Vec::new();

    for component in &components {
        health_rank = health_rank.max(operator_health_state_rank(&component.health_state));
        if component.rollout_gate == "hold" {
            rollout_gate = "hold".to_string();
            push_unique_string(
                &mut reason_codes,
                format!("{}:{}", component.component, component.reason_code),
            );
            push_unique_string(&mut recommendations, component.recommendation.clone());
        }
    }

    health_rank = health_rank.max(operator_health_state_rank(&daemon.health_state));
    if daemon.rollout_gate == "hold" {
        rollout_gate = "hold".to_string();
        push_unique_string(&mut reason_codes, format!("daemon:{}", daemon.reason_code));
        push_unique_string(&mut recommendations, daemon.recommendation.clone());
    }

    health_rank = health_rank.max(operator_health_state_rank(&release_channel.health_state));
    if release_channel.rollout_gate == "hold" {
        rollout_gate = "hold".to_string();
        push_unique_string(
            &mut reason_codes,
            format!("release-channel:{}", release_channel.reason_code),
        );
        push_unique_string(&mut recommendations, release_channel.recommendation.clone());
    }

    if policy_posture.gateway_remote_gate == "hold" {
        rollout_gate = "hold".to_string();
        health_rank = health_rank.max(1);
        let posture_reason = policy_posture
            .gateway_remote_reason_codes
            .first()
            .cloned()
            .unwrap_or_else(|| "remote_profile_hold".to_string());
        push_unique_string(
            &mut reason_codes,
            format!("gateway-remote-profile:{posture_reason}"),
        );
        for recommendation in &policy_posture.gateway_remote_recommendations {
            push_unique_string(&mut recommendations, recommendation.clone());
        }
    }

    if reason_codes.is_empty() {
        reason_codes.push("all_checks_passing".to_string());
    }
    if recommendations.is_empty() {
        recommendations.push("no_immediate_action_required".to_string());
    }

    Ok(OperatorControlSummaryReport {
        generated_unix_ms: current_unix_timestamp_ms(),
        health_state: operator_health_state_label(health_rank).to_string(),
        rollout_gate,
        reason_codes,
        recommendations,
        policy_posture,
        daemon,
        release_channel,
        components,
    })
}

fn save_operator_control_summary_snapshot(
    path: &Path,
    report: &OperatorControlSummaryReport,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create snapshot directory {}", parent.display())
            })?;
        }
    }
    let payload = serde_json::to_string_pretty(report)
        .context("failed to serialize operator control summary snapshot")?;
    std::fs::write(path, payload).with_context(|| {
        format!(
            "failed to write operator control summary snapshot {}",
            path.display()
        )
    })
}

fn load_operator_control_summary_snapshot(path: &Path) -> Result<OperatorControlSummaryReport> {
    let payload = std::fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read operator control summary snapshot {}",
            path.display()
        )
    })?;
    serde_json::from_str::<OperatorControlSummaryReport>(&payload).with_context(|| {
        format!(
            "failed to parse operator control summary snapshot {}",
            path.display()
        )
    })
}

fn component_drift_rank(
    before: &OperatorControlComponentSummaryRow,
    after: &OperatorControlComponentSummaryRow,
) -> i8 {
    let before_health = operator_health_state_rank(&before.health_state) as i8;
    let after_health = operator_health_state_rank(&after.health_state) as i8;
    let health_delta = after_health - before_health;
    let before_gate = if before.rollout_gate == "hold" { 1 } else { 0 };
    let after_gate = if after.rollout_gate == "hold" { 1 } else { 0 };
    let gate_delta = after_gate - before_gate;

    if health_delta > 0 || gate_delta > 0 {
        1
    } else if health_delta < 0 || gate_delta < 0 {
        -1
    } else {
        0
    }
}

fn classify_component_drift_state(
    before: &OperatorControlComponentSummaryRow,
    after: &OperatorControlComponentSummaryRow,
) -> (&'static str, &'static str) {
    let changed = before.health_state != after.health_state
        || before.rollout_gate != after.rollout_gate
        || before.reason_code != after.reason_code
        || before.recommendation != after.recommendation
        || before.queue_depth != after.queue_depth
        || before.failure_streak != after.failure_streak;

    if !changed {
        return ("stable", "none");
    }

    match component_drift_rank(before, after) {
        1 => ("regressed", "high"),
        -1 => ("improved", "low"),
        _ => ("changed", "medium"),
    }
}

fn component_snapshot_placeholder(name: &str) -> OperatorControlComponentSummaryRow {
    OperatorControlComponentSummaryRow {
        component: name.to_string(),
        state_path: "snapshot_missing".to_string(),
        health_state: "failing".to_string(),
        health_reason: "component snapshot missing".to_string(),
        rollout_gate: "hold".to_string(),
        reason_code: "snapshot_missing".to_string(),
        recommendation: "capture a complete baseline snapshot and rerun compare".to_string(),
        queue_depth: 0,
        failure_streak: 0,
    }
}

fn vec_delta(before: &[String], after: &[String]) -> (Vec<String>, Vec<String>) {
    let before_set: BTreeSet<String> = before.iter().cloned().collect();
    let after_set: BTreeSet<String> = after.iter().cloned().collect();
    let added = after_set
        .difference(&before_set)
        .cloned()
        .collect::<Vec<String>>();
    let removed = before_set
        .difference(&after_set)
        .cloned()
        .collect::<Vec<String>>();
    (added, removed)
}

fn build_operator_control_summary_diff_report(
    baseline: &OperatorControlSummaryReport,
    current: &OperatorControlSummaryReport,
) -> OperatorControlSummaryDiffReport {
    let mut baseline_components = BTreeMap::new();
    for component in &baseline.components {
        baseline_components.insert(component.component.clone(), component.clone());
    }

    let mut current_components = BTreeMap::new();
    for component in &current.components {
        current_components.insert(component.component.clone(), component.clone());
    }

    let mut component_names: BTreeSet<String> = BTreeSet::new();
    component_names.extend(baseline_components.keys().cloned());
    component_names.extend(current_components.keys().cloned());

    let mut changed_components = Vec::new();
    let mut unchanged_component_count = 0usize;
    for name in component_names {
        let before = baseline_components
            .get(&name)
            .cloned()
            .unwrap_or_else(|| component_snapshot_placeholder(&name));
        let after = current_components
            .get(&name)
            .cloned()
            .unwrap_or_else(|| component_snapshot_placeholder(&name));
        let (drift_state, severity) = classify_component_drift_state(&before, &after);
        if drift_state == "stable" {
            unchanged_component_count = unchanged_component_count.saturating_add(1);
            continue;
        }
        changed_components.push(OperatorControlSummaryDiffComponentRow {
            component: name,
            drift_state: drift_state.to_string(),
            severity: severity.to_string(),
            health_state_before: before.health_state,
            health_state_after: after.health_state,
            rollout_gate_before: before.rollout_gate,
            rollout_gate_after: after.rollout_gate,
            reason_code_before: before.reason_code,
            reason_code_after: after.reason_code,
            recommendation_before: before.recommendation,
            recommendation_after: after.recommendation,
            queue_depth_before: before.queue_depth,
            queue_depth_after: after.queue_depth,
            failure_streak_before: before.failure_streak,
            failure_streak_after: after.failure_streak,
        });
    }

    let (reason_codes_added, reason_codes_removed) =
        vec_delta(&baseline.reason_codes, &current.reason_codes);
    let (recommendations_added, recommendations_removed) =
        vec_delta(&baseline.recommendations, &current.recommendations);

    let health_drift = operator_health_state_rank(&current.health_state) as i8
        - operator_health_state_rank(&baseline.health_state) as i8;
    let gate_before = if baseline.rollout_gate == "hold" {
        1
    } else {
        0
    };
    let gate_after = if current.rollout_gate == "hold" { 1 } else { 0 };
    let gate_drift = gate_after - gate_before;
    let drift_state = if health_drift > 0 || gate_drift > 0 {
        "regressed"
    } else if health_drift < 0 || gate_drift < 0 {
        "improved"
    } else if changed_components.is_empty()
        && reason_codes_added.is_empty()
        && reason_codes_removed.is_empty()
        && recommendations_added.is_empty()
        && recommendations_removed.is_empty()
    {
        "stable"
    } else {
        "changed"
    };

    let risk_level = if drift_state == "regressed" && current.rollout_gate == "hold" {
        "high"
    } else if drift_state == "regressed" || current.health_state == "degraded" {
        "moderate"
    } else {
        "low"
    };

    OperatorControlSummaryDiffReport {
        generated_unix_ms: current_unix_timestamp_ms(),
        baseline_generated_unix_ms: baseline.generated_unix_ms,
        current_generated_unix_ms: current.generated_unix_ms,
        drift_state: drift_state.to_string(),
        risk_level: risk_level.to_string(),
        health_state_before: baseline.health_state.clone(),
        health_state_after: current.health_state.clone(),
        rollout_gate_before: baseline.rollout_gate.clone(),
        rollout_gate_after: current.rollout_gate.clone(),
        reason_codes_added,
        reason_codes_removed,
        recommendations_added,
        recommendations_removed,
        changed_components,
        unchanged_component_count,
    }
}

fn collect_operator_dashboard_component(cli: &Cli) -> OperatorControlComponentSummaryRow {
    let state_path = cli.dashboard_state_dir.join("state.json");
    match collect_dashboard_status_report(cli) {
        Ok(report) => build_operator_component_row(
            "dashboard",
            OperatorControlComponentInputs {
                state_path: report.state_path,
                health_state: report.health_state,
                health_reason: report.health_reason,
                rollout_gate: report.rollout_gate,
                reason_code: latest_reason_code_or_fallback(
                    &report.last_reason_codes,
                    "dashboard_status",
                ),
                recommendation: report.health.classify().recommendation.to_string(),
                queue_depth: report.health.queue_depth,
                failure_streak: report.health.failure_streak,
            },
        ),
        Err(error) => operator_component_unavailable("dashboard", &state_path, &error),
    }
}

fn collect_operator_multi_channel_component(cli: &Cli) -> OperatorControlComponentSummaryRow {
    let state_path = cli.multi_channel_state_dir.join("state.json");
    match collect_multi_channel_status_report(cli) {
        Ok(report) => build_operator_component_row(
            "multi-channel",
            OperatorControlComponentInputs {
                state_path: report.state_path,
                health_state: report.health_state,
                health_reason: report.health_reason,
                rollout_gate: report.rollout_gate,
                reason_code: latest_reason_code_or_fallback(
                    &report.last_reason_codes,
                    "multi_channel_status",
                ),
                recommendation: report.health.classify().recommendation.to_string(),
                queue_depth: report.health.queue_depth,
                failure_streak: report.health.failure_streak,
            },
        ),
        Err(error) => operator_component_unavailable("multi-channel", &state_path, &error),
    }
}

fn collect_operator_multi_agent_component(cli: &Cli) -> OperatorControlComponentSummaryRow {
    let state_path = cli.multi_agent_state_dir.join("state.json");
    match collect_multi_agent_status_report(cli) {
        Ok(report) => build_operator_component_row(
            "multi-agent",
            OperatorControlComponentInputs {
                state_path: report.state_path,
                health_state: report.health_state,
                health_reason: report.health_reason,
                rollout_gate: report.rollout_gate,
                reason_code: latest_reason_code_or_fallback(
                    &report.last_reason_codes,
                    "multi_agent_status",
                ),
                recommendation: report.health.classify().recommendation.to_string(),
                queue_depth: report.health.queue_depth,
                failure_streak: report.health.failure_streak,
            },
        ),
        Err(error) => operator_component_unavailable("multi-agent", &state_path, &error),
    }
}

fn collect_operator_gateway_component(cli: &Cli) -> OperatorControlComponentSummaryRow {
    let state_path = cli.gateway_state_dir.join("state.json");
    match collect_gateway_status_report(cli) {
        Ok(report) => {
            let recommendation = if report.rollout_reason_code == "service_stopped" {
                "start gateway service mode or clear stop reason before resuming traffic"
            } else {
                report.health.classify().recommendation
            };
            build_operator_component_row(
                "gateway",
                OperatorControlComponentInputs {
                    state_path: report.state_path,
                    health_state: report.health_state,
                    health_reason: report.health_reason,
                    rollout_gate: report.rollout_gate,
                    reason_code: report.rollout_reason_code,
                    recommendation: recommendation.to_string(),
                    queue_depth: report.health.queue_depth,
                    failure_streak: report.health.failure_streak,
                },
            )
        }
        Err(error) => operator_component_unavailable("gateway", &state_path, &error),
    }
}

fn collect_operator_deployment_component(cli: &Cli) -> OperatorControlComponentSummaryRow {
    let state_path = cli.deployment_state_dir.join("state.json");
    match collect_deployment_status_report(cli) {
        Ok(report) => build_operator_component_row(
            "deployment",
            OperatorControlComponentInputs {
                state_path: report.state_path,
                health_state: report.health_state,
                health_reason: report.health_reason,
                rollout_gate: report.rollout_gate,
                reason_code: latest_reason_code_or_fallback(
                    &report.last_reason_codes,
                    "deployment_status",
                ),
                recommendation: report.health.classify().recommendation.to_string(),
                queue_depth: report.health.queue_depth,
                failure_streak: report.health.failure_streak,
            },
        ),
        Err(error) => operator_component_unavailable("deployment", &state_path, &error),
    }
}

fn collect_operator_custom_command_component(cli: &Cli) -> OperatorControlComponentSummaryRow {
    let state_path = cli.custom_command_state_dir.join("state.json");
    match collect_custom_command_status_report(cli) {
        Ok(report) => build_operator_component_row(
            "custom-command",
            OperatorControlComponentInputs {
                state_path: report.state_path,
                health_state: report.health_state,
                health_reason: report.health_reason,
                rollout_gate: report.rollout_gate,
                reason_code: latest_reason_code_or_fallback(
                    &report.last_reason_codes,
                    "custom_command_status",
                ),
                recommendation: report.health.classify().recommendation.to_string(),
                queue_depth: report.health.queue_depth,
                failure_streak: report.health.failure_streak,
            },
        ),
        Err(error) => operator_component_unavailable("custom-command", &state_path, &error),
    }
}

fn collect_operator_voice_component(cli: &Cli) -> OperatorControlComponentSummaryRow {
    let state_path = cli.voice_state_dir.join("state.json");
    match collect_voice_status_report(cli) {
        Ok(report) => build_operator_component_row(
            "voice",
            OperatorControlComponentInputs {
                state_path: report.state_path,
                health_state: report.health_state,
                health_reason: report.health_reason,
                rollout_gate: report.rollout_gate,
                reason_code: latest_reason_code_or_fallback(
                    &report.last_reason_codes,
                    "voice_status",
                ),
                recommendation: report.health.classify().recommendation.to_string(),
                queue_depth: report.health.queue_depth,
                failure_streak: report.health.failure_streak,
            },
        ),
        Err(error) => operator_component_unavailable("voice", &state_path, &error),
    }
}

fn collect_operator_daemon_summary(cli: &Cli) -> OperatorControlDaemonSummary {
    let config = TauDaemonConfig {
        state_dir: cli.daemon_state_dir.clone(),
        profile: cli.daemon_profile,
    };
    match inspect_tau_daemon(&config) {
        Ok(report) => {
            let (health_state, rollout_gate, reason_code, recommendation) = if report.running {
                (
                    "healthy".to_string(),
                    "pass".to_string(),
                    "daemon_running".to_string(),
                    "no immediate action required".to_string(),
                )
            } else if report.installed {
                (
                    "degraded".to_string(),
                    "hold".to_string(),
                    "daemon_not_running".to_string(),
                    "start daemon with --daemon-start to restore background processing".to_string(),
                )
            } else {
                (
                    "degraded".to_string(),
                    "hold".to_string(),
                    "daemon_not_installed".to_string(),
                    "install daemon with --daemon-install if background lifecycle management is required".to_string(),
                )
            };
            OperatorControlDaemonSummary {
                health_state,
                rollout_gate,
                reason_code,
                recommendation,
                profile: report.profile,
                installed: report.installed,
                running: report.running,
                start_attempts: report.start_attempts,
                stop_attempts: report.stop_attempts,
                diagnostics: report.diagnostics.len(),
                state_path: report.state_path,
            }
        }
        Err(error) => OperatorControlDaemonSummary {
            health_state: "failing".to_string(),
            rollout_gate: "hold".to_string(),
            reason_code: "daemon_status_unavailable".to_string(),
            recommendation: "inspect --daemon-state-dir permissions and rerun --daemon-status"
                .to_string(),
            profile: cli.daemon_profile.as_str().to_string(),
            installed: false,
            running: false,
            start_attempts: 0,
            stop_attempts: 0,
            diagnostics: 1,
            state_path: format!("{} ({error})", cli.daemon_state_dir.display()),
        },
    }
}

fn collect_operator_release_channel_summary() -> OperatorControlReleaseChannelSummary {
    match default_release_channel_path() {
        Ok(path) => match load_release_channel_store(&path) {
            Ok(Some(channel)) => OperatorControlReleaseChannelSummary {
                health_state: "healthy".to_string(),
                rollout_gate: "pass".to_string(),
                reason_code: "release_channel_loaded".to_string(),
                recommendation: "no immediate action required".to_string(),
                configured: true,
                channel: channel.as_str().to_string(),
                path: path.display().to_string(),
            },
            Ok(None) => OperatorControlReleaseChannelSummary {
                health_state: "degraded".to_string(),
                rollout_gate: "hold".to_string(),
                reason_code: "release_channel_missing".to_string(),
                recommendation:
                    "set a release channel with '/release-channel set <stable|beta|dev>'"
                        .to_string(),
                configured: false,
                channel: "unknown".to_string(),
                path: path.display().to_string(),
            },
            Err(error) => OperatorControlReleaseChannelSummary {
                health_state: "failing".to_string(),
                rollout_gate: "hold".to_string(),
                reason_code: "release_channel_load_failed".to_string(),
                recommendation:
                    "repair .tau/release-channel.json or rerun '/release-channel set ...'"
                        .to_string(),
                configured: false,
                channel: "unknown".to_string(),
                path: format!("{} ({error})", path.display()),
            },
        },
        Err(error) => OperatorControlReleaseChannelSummary {
            health_state: "failing".to_string(),
            rollout_gate: "hold".to_string(),
            reason_code: "release_channel_path_unavailable".to_string(),
            recommendation: "run from a writable workspace root to resolve .tau paths".to_string(),
            configured: false,
            channel: "unknown".to_string(),
            path: format!("unknown ({error})"),
        },
    }
}

fn collect_operator_policy_posture(cli: &Cli) -> OperatorControlPolicyPosture {
    let pairing_policy = pairing_policy_for_state_dir(&cli.channel_store_root);
    let (pairing_allowlist_strict, pairing_allowlist_channel_rules) =
        load_pairing_allowlist_posture(&pairing_policy.allowlist_path);
    let pairing_registry_entries = load_pairing_registry_entry_count(&pairing_policy.registry_path);
    let pairing_rules_configured =
        pairing_allowlist_channel_rules > 0 || pairing_registry_entries > 0;
    let pairing_strict_effective =
        pairing_policy.strict_mode || pairing_allowlist_strict || pairing_rules_configured;

    let remote_profile = match tau_cli::gateway_remote_profile::evaluate_gateway_remote_profile(cli)
    {
        Ok(report) => report,
        Err(_) => tau_cli::gateway_remote_profile::GatewayRemoteProfileReport {
            profile: cli.gateway_remote_profile.as_str().to_string(),
            posture: "unknown".to_string(),
            gate: "hold".to_string(),
            risk_level: "high".to_string(),
            server_enabled: cli.gateway_openresponses_server,
            bind: cli.gateway_openresponses_bind.clone(),
            bind_ip: "unknown".to_string(),
            loopback_bind: false,
            auth_mode: cli.gateway_openresponses_auth_mode.as_str().to_string(),
            auth_token_configured: false,
            auth_password_configured: false,
            remote_enabled: !matches!(
                cli.gateway_remote_profile,
                CliGatewayRemoteProfile::LocalOnly
            ),
            reason_codes: vec!["remote_profile_evaluation_failed".to_string()],
            recommendations: vec![
                "run --gateway-remote-profile-inspect to inspect posture diagnostics".to_string(),
            ],
        },
    };

    OperatorControlPolicyPosture {
        pairing_strict_effective,
        pairing_config_strict_mode: pairing_policy.strict_mode,
        pairing_allowlist_strict,
        pairing_rules_configured,
        pairing_registry_entries,
        pairing_allowlist_channel_rules,
        provider_subscription_strict: cli.provider_subscription_strict,
        gateway_auth_mode: cli.gateway_openresponses_auth_mode.as_str().to_string(),
        gateway_remote_profile: remote_profile.profile,
        gateway_remote_posture: remote_profile.posture,
        gateway_remote_gate: remote_profile.gate,
        gateway_remote_risk_level: remote_profile.risk_level,
        gateway_remote_reason_codes: remote_profile.reason_codes,
        gateway_remote_recommendations: remote_profile.recommendations,
    }
}

fn load_pairing_allowlist_posture(path: &Path) -> (bool, usize) {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return (false, 0),
    };
    let parsed = match serde_json::from_str::<PairingAllowlistSummaryFile>(&raw) {
        Ok(parsed) => parsed,
        Err(_) => return (false, 0),
    };
    let rules = parsed
        .channels
        .values()
        .map(|actors| actors.len())
        .sum::<usize>();
    (parsed.strict, rules)
}

fn load_pairing_registry_entry_count(path: &Path) -> usize {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return 0,
    };
    match serde_json::from_str::<PairingRegistrySummaryFile>(&raw) {
        Ok(parsed) => parsed.pairings.len(),
        Err(_) => 0,
    }
}

fn operator_component_unavailable(
    component: &str,
    state_path: &Path,
    error: &anyhow::Error,
) -> OperatorControlComponentSummaryRow {
    build_operator_component_row(
        component,
        OperatorControlComponentInputs {
            state_path: state_path.display().to_string(),
            health_state: "failing".to_string(),
            health_reason: format!("status unavailable: {error}"),
            rollout_gate: "hold".to_string(),
            reason_code: "state_unavailable".to_string(),
            recommendation: "bootstrap or repair component state, then rerun operator summary"
                .to_string(),
            queue_depth: 0,
            failure_streak: 0,
        },
    )
}

fn build_operator_component_row(
    component: &str,
    inputs: OperatorControlComponentInputs,
) -> OperatorControlComponentSummaryRow {
    OperatorControlComponentSummaryRow {
        component: component.to_string(),
        health_state: inputs.health_state,
        health_reason: inputs.health_reason,
        rollout_gate: inputs.rollout_gate,
        reason_code: inputs.reason_code,
        recommendation: inputs.recommendation,
        queue_depth: inputs.queue_depth,
        failure_streak: inputs.failure_streak,
        state_path: inputs.state_path,
    }
}

fn latest_reason_code_or_fallback(reason_codes: &[String], fallback: &str) -> String {
    reason_codes
        .iter()
        .rev()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| fallback.to_string())
}

fn operator_health_state_rank(state: &str) -> u8 {
    if state.eq_ignore_ascii_case("healthy") {
        return 0;
    }
    if state.eq_ignore_ascii_case("degraded") {
        return 1;
    }
    2
}

fn operator_health_state_label(rank: u8) -> &'static str {
    match rank {
        0 => "healthy",
        1 => "degraded",
        _ => "failing",
    }
}

fn push_unique_string(list: &mut Vec<String>, value: impl Into<String>) {
    let value = value.into();
    if value.trim().is_empty() {
        return;
    }
    if list.iter().any(|existing| existing == &value) {
        return;
    }
    list.push(value);
}

fn load_multi_agent_status_state(path: &Path) -> Result<MultiAgentStatusStateFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str::<MultiAgentStatusStateFile>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))
}

fn load_gateway_status_state(path: &Path) -> Result<GatewayStatusStateFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str::<GatewayStatusStateFile>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))
}

fn load_custom_command_status_state(path: &Path) -> Result<CustomCommandStatusStateFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str::<CustomCommandStatusStateFile>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))
}

fn load_voice_status_state(path: &Path) -> Result<VoiceStatusStateFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str::<VoiceStatusStateFile>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))
}

fn load_deployment_status_state(path: &Path) -> Result<DeploymentStatusStateFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str::<DeploymentStatusStateFile>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))
}

fn load_multi_channel_status_state(path: &Path) -> Result<MultiChannelStatusStateFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str::<MultiChannelStatusStateFile>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))
}

fn load_multi_channel_cycle_report_summary(path: &Path) -> Result<MultiChannelCycleReportSummary> {
    if !path.exists() {
        return Ok(MultiChannelCycleReportSummary {
            events_log_present: false,
            ..MultiChannelCycleReportSummary::default()
        });
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut summary = MultiChannelCycleReportSummary {
        events_log_present: true,
        ..MultiChannelCycleReportSummary::default()
    };
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<MultiChannelCycleReportLine>(trimmed) {
            Ok(report) => {
                summary.cycle_reports = summary.cycle_reports.saturating_add(1);
                summary.last_reason_codes = report.reason_codes.clone();
                summary.last_health_reason = report.health_reason;
                for reason_code in report.reason_codes {
                    increment_count(&mut summary.reason_code_counts, reason_code.trim());
                }
            }
            Err(_) => {
                summary.invalid_cycle_reports = summary.invalid_cycle_reports.saturating_add(1);
            }
        }
    }
    Ok(summary)
}

fn load_multi_agent_cycle_report_summary(path: &Path) -> Result<MultiAgentCycleReportSummary> {
    if !path.exists() {
        return Ok(MultiAgentCycleReportSummary {
            events_log_present: false,
            ..MultiAgentCycleReportSummary::default()
        });
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut summary = MultiAgentCycleReportSummary {
        events_log_present: true,
        ..MultiAgentCycleReportSummary::default()
    };
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<MultiAgentCycleReportLine>(trimmed) {
            Ok(report) => {
                summary.cycle_reports = summary.cycle_reports.saturating_add(1);
                summary.last_reason_codes = report.reason_codes.clone();
                summary.last_health_reason = report.health_reason;
                for reason_code in report.reason_codes {
                    increment_count(&mut summary.reason_code_counts, reason_code.trim());
                }
            }
            Err(_) => {
                summary.invalid_cycle_reports = summary.invalid_cycle_reports.saturating_add(1);
            }
        }
    }
    Ok(summary)
}

fn load_gateway_cycle_report_summary(path: &Path) -> Result<GatewayCycleReportSummary> {
    if !path.exists() {
        return Ok(GatewayCycleReportSummary {
            events_log_present: false,
            ..GatewayCycleReportSummary::default()
        });
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut summary = GatewayCycleReportSummary {
        events_log_present: true,
        ..GatewayCycleReportSummary::default()
    };
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<GatewayCycleReportLine>(trimmed) {
            Ok(report) => {
                summary.cycle_reports = summary.cycle_reports.saturating_add(1);
                summary.last_reason_codes = report.reason_codes.clone();
                summary.last_health_reason = report.health_reason;
                for reason_code in report.reason_codes {
                    increment_count(&mut summary.reason_code_counts, reason_code.trim());
                }
            }
            Err(_) => {
                summary.invalid_cycle_reports = summary.invalid_cycle_reports.saturating_add(1);
            }
        }
    }
    Ok(summary)
}

fn load_custom_command_cycle_report_summary(
    path: &Path,
) -> Result<CustomCommandCycleReportSummary> {
    if !path.exists() {
        return Ok(CustomCommandCycleReportSummary {
            events_log_present: false,
            ..CustomCommandCycleReportSummary::default()
        });
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut summary = CustomCommandCycleReportSummary {
        events_log_present: true,
        ..CustomCommandCycleReportSummary::default()
    };
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<CustomCommandCycleReportLine>(trimmed) {
            Ok(report) => {
                summary.cycle_reports = summary.cycle_reports.saturating_add(1);
                summary.last_reason_codes = report.reason_codes.clone();
                summary.last_health_reason = report.health_reason;
                for reason_code in report.reason_codes {
                    increment_count(&mut summary.reason_code_counts, reason_code.trim());
                }
            }
            Err(_) => {
                summary.invalid_cycle_reports = summary.invalid_cycle_reports.saturating_add(1);
            }
        }
    }
    Ok(summary)
}

fn load_voice_cycle_report_summary(path: &Path) -> Result<VoiceCycleReportSummary> {
    if !path.exists() {
        return Ok(VoiceCycleReportSummary {
            events_log_present: false,
            ..VoiceCycleReportSummary::default()
        });
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut summary = VoiceCycleReportSummary {
        events_log_present: true,
        ..VoiceCycleReportSummary::default()
    };
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<VoiceCycleReportLine>(trimmed) {
            Ok(report) => {
                summary.cycle_reports = summary.cycle_reports.saturating_add(1);
                summary.last_reason_codes = report.reason_codes.clone();
                summary.last_health_reason = report.health_reason;
                for reason_code in report.reason_codes {
                    increment_count(&mut summary.reason_code_counts, reason_code.trim());
                }
            }
            Err(_) => {
                summary.invalid_cycle_reports = summary.invalid_cycle_reports.saturating_add(1);
            }
        }
    }
    Ok(summary)
}

fn load_deployment_cycle_report_summary(path: &Path) -> Result<DeploymentCycleReportSummary> {
    if !path.exists() {
        return Ok(DeploymentCycleReportSummary {
            events_log_present: false,
            ..DeploymentCycleReportSummary::default()
        });
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut summary = DeploymentCycleReportSummary {
        events_log_present: true,
        ..DeploymentCycleReportSummary::default()
    };
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<DeploymentCycleReportLine>(trimmed) {
            Ok(report) => {
                summary.cycle_reports = summary.cycle_reports.saturating_add(1);
                summary.last_reason_codes = report.reason_codes.clone();
                summary.last_health_reason = report.health_reason;
                for reason_code in report.reason_codes {
                    increment_count(&mut summary.reason_code_counts, reason_code.trim());
                }
            }
            Err(_) => {
                summary.invalid_cycle_reports = summary.invalid_cycle_reports.saturating_add(1);
            }
        }
    }
    Ok(summary)
}

fn increment_count(map: &mut BTreeMap<String, usize>, raw: &str) {
    let key = raw.trim();
    if key.is_empty() {
        return;
    }
    let counter = map.entry(key.to_string()).or_insert(0);
    *counter = counter.saturating_add(1);
}

#[cfg(test)]
mod tests;
