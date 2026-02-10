use super::*;
use std::collections::{BTreeMap, BTreeSet};

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
    connectors:
        Option<crate::multi_channel_live_connectors::MultiChannelLiveConnectorsStatusReport>,
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

pub(crate) fn execute_channel_store_admin_command(cli: &Cli) -> Result<()> {
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

fn parse_transport_health_inspect_target(raw: &str) -> Result<TransportHealthInspectTarget> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!(
            "invalid --transport-health-inspect '{}', expected slack, github, github:owner/repo, multi-channel, multi-agent, browser-automation, memory, dashboard, gateway, deployment, custom-command, or voice",
            raw
        );
    }
    if trimmed.eq_ignore_ascii_case("slack") {
        return Ok(TransportHealthInspectTarget::Slack);
    }
    if trimmed.eq_ignore_ascii_case("github") {
        return Ok(TransportHealthInspectTarget::GithubAll);
    }
    if trimmed.eq_ignore_ascii_case("multi-channel") || trimmed.eq_ignore_ascii_case("multichannel")
    {
        return Ok(TransportHealthInspectTarget::MultiChannel);
    }
    if trimmed.eq_ignore_ascii_case("multi-agent") || trimmed.eq_ignore_ascii_case("multiagent") {
        return Ok(TransportHealthInspectTarget::MultiAgent);
    }
    if trimmed.eq_ignore_ascii_case("browser-automation")
        || trimmed.eq_ignore_ascii_case("browserautomation")
        || trimmed.eq_ignore_ascii_case("browser")
    {
        return Ok(TransportHealthInspectTarget::BrowserAutomation);
    }
    if trimmed.eq_ignore_ascii_case("memory") {
        return Ok(TransportHealthInspectTarget::Memory);
    }
    if trimmed.eq_ignore_ascii_case("dashboard") {
        return Ok(TransportHealthInspectTarget::Dashboard);
    }
    if trimmed.eq_ignore_ascii_case("gateway") {
        return Ok(TransportHealthInspectTarget::Gateway);
    }
    if trimmed.eq_ignore_ascii_case("deployment") {
        return Ok(TransportHealthInspectTarget::Deployment);
    }
    if trimmed.eq_ignore_ascii_case("custom-command")
        || trimmed.eq_ignore_ascii_case("customcommand")
    {
        return Ok(TransportHealthInspectTarget::CustomCommand);
    }
    if trimmed.eq_ignore_ascii_case("voice") {
        return Ok(TransportHealthInspectTarget::Voice);
    }

    let Some((transport, repo_slug)) = trimmed.split_once(':') else {
        bail!(
            "invalid --transport-health-inspect '{}', expected slack, github, github:owner/repo, multi-channel, multi-agent, browser-automation, memory, dashboard, gateway, deployment, custom-command, or voice",
            raw
        );
    };
    if !transport.eq_ignore_ascii_case("github") {
        bail!(
            "invalid --transport-health-inspect '{}', expected slack, github, github:owner/repo, multi-channel, multi-agent, browser-automation, memory, dashboard, gateway, deployment, custom-command, or voice",
            raw
        );
    }

    let (owner, repo) = repo_slug
        .split_once('/')
        .ok_or_else(|| anyhow!("invalid github target '{}', expected owner/repo", repo_slug))?;
    let owner = owner.trim();
    let repo = repo.trim();
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        bail!("invalid github target '{}', expected owner/repo", repo_slug);
    }

    Ok(TransportHealthInspectTarget::GithubRepo {
        owner: owner.to_string(),
        repo: repo.to_string(),
    })
}

fn parse_github_repo_slug(raw: &str) -> Result<(String, String)> {
    let trimmed = raw.trim();
    let (owner, repo) = trimmed.split_once('/').ok_or_else(|| {
        anyhow!(
            "invalid --github-status-inspect target '{}', expected owner/repo",
            raw
        )
    })?;
    let owner = owner.trim();
    let repo = repo.trim();
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        bail!(
            "invalid --github-status-inspect target '{}', expected owner/repo",
            raw
        );
    }
    Ok((owner.to_string(), repo.to_string()))
}

fn collect_transport_health_rows(
    cli: &Cli,
    target: &TransportHealthInspectTarget,
) -> Result<Vec<TransportHealthInspectRow>> {
    match target {
        TransportHealthInspectTarget::Slack => Ok(vec![collect_slack_transport_health_row(cli)?]),
        TransportHealthInspectTarget::GithubAll => collect_all_github_transport_health_rows(cli),
        TransportHealthInspectTarget::GithubRepo { owner, repo } => {
            Ok(vec![collect_github_transport_health_row(cli, owner, repo)?])
        }
        TransportHealthInspectTarget::MultiChannel => {
            Ok(vec![collect_multi_channel_transport_health_row(cli)?])
        }
        TransportHealthInspectTarget::MultiAgent => {
            Ok(vec![collect_multi_agent_transport_health_row(cli)?])
        }
        TransportHealthInspectTarget::BrowserAutomation => {
            Ok(vec![collect_browser_automation_transport_health_row(cli)?])
        }
        TransportHealthInspectTarget::Memory => Ok(vec![collect_memory_transport_health_row(cli)?]),
        TransportHealthInspectTarget::Dashboard => {
            Ok(vec![collect_dashboard_transport_health_row(cli)?])
        }
        TransportHealthInspectTarget::Gateway => {
            Ok(vec![collect_gateway_transport_health_row(cli)?])
        }
        TransportHealthInspectTarget::Deployment => {
            Ok(vec![collect_deployment_transport_health_row(cli)?])
        }
        TransportHealthInspectTarget::CustomCommand => {
            Ok(vec![collect_custom_command_transport_health_row(cli)?])
        }
        TransportHealthInspectTarget::Voice => Ok(vec![collect_voice_transport_health_row(cli)?]),
    }
}

fn collect_slack_transport_health_row(cli: &Cli) -> Result<TransportHealthInspectRow> {
    let state_path = cli.slack_state_dir.join("state.json");
    let health = load_transport_health_snapshot(&state_path)?;
    Ok(TransportHealthInspectRow {
        transport: "slack".to_string(),
        target: "slack".to_string(),
        state_path: state_path.display().to_string(),
        health,
    })
}

fn collect_github_transport_health_row(
    cli: &Cli,
    owner: &str,
    repo: &str,
) -> Result<TransportHealthInspectRow> {
    let repo_slug = format!("{owner}/{repo}");
    let repo_dir = sanitize_for_path(&format!("{owner}__{repo}"));
    let state_path = cli.github_state_dir.join(repo_dir).join("state.json");
    let health = load_transport_health_snapshot(&state_path)?;
    Ok(TransportHealthInspectRow {
        transport: "github".to_string(),
        target: repo_slug,
        state_path: state_path.display().to_string(),
        health,
    })
}

fn collect_all_github_transport_health_rows(cli: &Cli) -> Result<Vec<TransportHealthInspectRow>> {
    if !cli.github_state_dir.exists() {
        bail!(
            "github state directory does not exist: {}",
            cli.github_state_dir.display()
        );
    }

    let mut rows = Vec::new();
    for entry_result in std::fs::read_dir(&cli.github_state_dir)
        .with_context(|| format!("failed to read {}", cli.github_state_dir.display()))?
    {
        let entry = entry_result
            .with_context(|| format!("failed to read {}", cli.github_state_dir.display()))?;
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }
        let state_path = entry_path.join("state.json");
        if !state_path.is_file() {
            continue;
        }
        let Some(repo_dir_name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let health = load_transport_health_snapshot(&state_path)?;
        rows.push(TransportHealthInspectRow {
            transport: "github".to_string(),
            target: decode_repo_target_from_dir_name(&repo_dir_name),
            state_path: state_path.display().to_string(),
            health,
        });
    }

    rows.sort_by(|left, right| left.target.cmp(&right.target));
    if rows.is_empty() {
        bail!(
            "no github state files found under {}",
            cli.github_state_dir.display()
        );
    }
    Ok(rows)
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

fn collect_multi_channel_transport_health_row(cli: &Cli) -> Result<TransportHealthInspectRow> {
    let state_path = cli.multi_channel_state_dir.join("state.json");
    let health = load_transport_health_snapshot(&state_path)?;
    Ok(TransportHealthInspectRow {
        transport: "multi-channel".to_string(),
        target: "telegram/discord/whatsapp".to_string(),
        state_path: state_path.display().to_string(),
        health,
    })
}

fn collect_multi_agent_transport_health_row(cli: &Cli) -> Result<TransportHealthInspectRow> {
    let state_path = cli.multi_agent_state_dir.join("state.json");
    let health = load_transport_health_snapshot(&state_path)?;
    Ok(TransportHealthInspectRow {
        transport: "multi-agent".to_string(),
        target: "orchestrator-router".to_string(),
        state_path: state_path.display().to_string(),
        health,
    })
}

fn collect_browser_automation_transport_health_row(cli: &Cli) -> Result<TransportHealthInspectRow> {
    let state_path = cli.browser_automation_state_dir.join("state.json");
    let health = load_transport_health_snapshot(&state_path)?;
    Ok(TransportHealthInspectRow {
        transport: "browser-automation".to_string(),
        target: "fixture-runtime".to_string(),
        state_path: state_path.display().to_string(),
        health,
    })
}

fn collect_memory_transport_health_row(cli: &Cli) -> Result<TransportHealthInspectRow> {
    let state_path = cli.memory_state_dir.join("state.json");
    let health = load_transport_health_snapshot(&state_path)?;
    Ok(TransportHealthInspectRow {
        transport: "memory".to_string(),
        target: "semantic-memory".to_string(),
        state_path: state_path.display().to_string(),
        health,
    })
}

fn collect_dashboard_transport_health_row(cli: &Cli) -> Result<TransportHealthInspectRow> {
    let state_path = cli.dashboard_state_dir.join("state.json");
    let health = load_transport_health_snapshot(&state_path)?;
    Ok(TransportHealthInspectRow {
        transport: "dashboard".to_string(),
        target: "operator-control-plane".to_string(),
        state_path: state_path.display().to_string(),
        health,
    })
}

fn collect_gateway_transport_health_row(cli: &Cli) -> Result<TransportHealthInspectRow> {
    let state_path = cli.gateway_state_dir.join("state.json");
    let health = load_transport_health_snapshot(&state_path)?;
    Ok(TransportHealthInspectRow {
        transport: "gateway".to_string(),
        target: "gateway-service".to_string(),
        state_path: state_path.display().to_string(),
        health,
    })
}

fn collect_deployment_transport_health_row(cli: &Cli) -> Result<TransportHealthInspectRow> {
    let state_path = cli.deployment_state_dir.join("state.json");
    let health = load_transport_health_snapshot(&state_path)?;
    Ok(TransportHealthInspectRow {
        transport: "deployment".to_string(),
        target: "cloud-and-wasm-runtime".to_string(),
        state_path: state_path.display().to_string(),
        health,
    })
}

fn collect_custom_command_transport_health_row(cli: &Cli) -> Result<TransportHealthInspectRow> {
    let state_path = cli.custom_command_state_dir.join("state.json");
    let health = load_transport_health_snapshot(&state_path)?;
    Ok(TransportHealthInspectRow {
        transport: "custom-command".to_string(),
        target: "no-code-command-registry".to_string(),
        state_path: state_path.display().to_string(),
        health,
    })
}

fn collect_voice_transport_health_row(cli: &Cli) -> Result<TransportHealthInspectRow> {
    let state_path = cli.voice_state_dir.join("state.json");
    let health = load_transport_health_snapshot(&state_path)?;
    Ok(TransportHealthInspectRow {
        transport: "voice".to_string(),
        target: "wake-word-pipeline".to_string(),
        state_path: state_path.display().to_string(),
        health,
    })
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

    let connectors =
        match crate::multi_channel_live_connectors::load_multi_channel_live_connectors_status_report(
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
    let config = crate::daemon_runtime::TauDaemonConfig {
        state_dir: cli.daemon_state_dir.clone(),
        profile: cli.daemon_profile,
    };
    match crate::daemon_runtime::inspect_tau_daemon(&config) {
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

    let remote_profile = match crate::gateway_remote_profile::evaluate_gateway_remote_profile(cli) {
        Ok(report) => report,
        Err(_) => crate::gateway_remote_profile::GatewayRemoteProfileReport {
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

fn load_transport_health_snapshot(state_path: &Path) -> Result<TransportHealthSnapshot> {
    let raw = std::fs::read_to_string(state_path)
        .with_context(|| format!("failed to read state file {}", state_path.display()))?;
    let parsed = serde_json::from_str::<TransportHealthStateFile>(&raw)
        .with_context(|| format!("failed to parse state file {}", state_path.display()))?;
    Ok(parsed.health)
}

fn decode_repo_target_from_dir_name(dir_name: &str) -> String {
    let Some((owner, repo)) = dir_name.split_once("__") else {
        return dir_name.to_string();
    };
    if owner.is_empty() || repo.is_empty() {
        return dir_name.to_string();
    }
    format!("{owner}/{repo}")
}

fn render_operator_control_summary_report(report: &OperatorControlSummaryReport) -> String {
    let mut lines = vec![
        format!(
            "operator control summary: generated_unix_ms={} health_state={} rollout_gate={} reason_codes={} recommendations={}",
            report.generated_unix_ms,
            report.health_state,
            report.rollout_gate,
            render_string_vec(&report.reason_codes),
            render_string_vec(&report.recommendations),
        ),
        format!(
            "operator control policy posture: pairing_strict_effective={} pairing_config_strict_mode={} pairing_allowlist_strict={} pairing_rules_configured={} pairing_registry_entries={} pairing_allowlist_channel_rules={} provider_subscription_strict={} gateway_auth_mode={} gateway_remote_profile={} gateway_remote_posture={} gateway_remote_gate={} gateway_remote_risk_level={} gateway_remote_reason_codes={} gateway_remote_recommendations={}",
            report.policy_posture.pairing_strict_effective,
            report.policy_posture.pairing_config_strict_mode,
            report.policy_posture.pairing_allowlist_strict,
            report.policy_posture.pairing_rules_configured,
            report.policy_posture.pairing_registry_entries,
            report.policy_posture.pairing_allowlist_channel_rules,
            report.policy_posture.provider_subscription_strict,
            report.policy_posture.gateway_auth_mode,
            report.policy_posture.gateway_remote_profile,
            report.policy_posture.gateway_remote_posture,
            report.policy_posture.gateway_remote_gate,
            report.policy_posture.gateway_remote_risk_level,
            render_string_vec(&report.policy_posture.gateway_remote_reason_codes),
            render_string_vec(&report.policy_posture.gateway_remote_recommendations),
        ),
        format!(
            "operator control daemon: health_state={} rollout_gate={} reason_code={} recommendation={} profile={} installed={} running={} start_attempts={} stop_attempts={} diagnostics={} state_path={}",
            report.daemon.health_state,
            report.daemon.rollout_gate,
            report.daemon.reason_code,
            report.daemon.recommendation,
            report.daemon.profile,
            report.daemon.installed,
            report.daemon.running,
            report.daemon.start_attempts,
            report.daemon.stop_attempts,
            report.daemon.diagnostics,
            report.daemon.state_path,
        ),
        format!(
            "operator control release channel: health_state={} rollout_gate={} reason_code={} recommendation={} configured={} channel={} path={}",
            report.release_channel.health_state,
            report.release_channel.rollout_gate,
            report.release_channel.reason_code,
            report.release_channel.recommendation,
            report.release_channel.configured,
            report.release_channel.channel,
            report.release_channel.path,
        ),
    ];

    for component in &report.components {
        lines.push(format!(
            "operator control component: component={} health_state={} rollout_gate={} reason_code={} health_reason={} recommendation={} queue_depth={} failure_streak={} state_path={}",
            component.component,
            component.health_state,
            component.rollout_gate,
            component.reason_code,
            component.health_reason,
            component.recommendation,
            component.queue_depth,
            component.failure_streak,
            component.state_path,
        ));
    }

    lines.join("\n")
}

fn render_operator_control_summary_diff_report(
    report: &OperatorControlSummaryDiffReport,
) -> String {
    let mut lines = vec![format!(
        "operator control summary diff: generated_unix_ms={} baseline_generated_unix_ms={} current_generated_unix_ms={} drift_state={} risk_level={} health_state_before={} health_state_after={} rollout_gate_before={} rollout_gate_after={} reason_codes_added={} reason_codes_removed={} recommendations_added={} recommendations_removed={} changed_components={} unchanged_components={}",
        report.generated_unix_ms,
        report.baseline_generated_unix_ms,
        report.current_generated_unix_ms,
        report.drift_state,
        report.risk_level,
        report.health_state_before,
        report.health_state_after,
        report.rollout_gate_before,
        report.rollout_gate_after,
        render_string_vec(&report.reason_codes_added),
        render_string_vec(&report.reason_codes_removed),
        render_string_vec(&report.recommendations_added),
        render_string_vec(&report.recommendations_removed),
        report.changed_components.len(),
        report.unchanged_component_count,
    )];

    for component in &report.changed_components {
        lines.push(format!(
            "operator control summary diff component: component={} drift_state={} severity={} health_state_before={} health_state_after={} rollout_gate_before={} rollout_gate_after={} reason_code_before={} reason_code_after={} recommendation_before={} recommendation_after={} queue_depth_before={} queue_depth_after={} failure_streak_before={} failure_streak_after={}",
            component.component,
            component.drift_state,
            component.severity,
            component.health_state_before,
            component.health_state_after,
            component.rollout_gate_before,
            component.rollout_gate_after,
            component.reason_code_before,
            component.reason_code_after,
            component.recommendation_before,
            component.recommendation_after,
            component.queue_depth_before,
            component.queue_depth_after,
            component.failure_streak_before,
            component.failure_streak_after,
        ));
    }

    lines.join("\n")
}

fn render_transport_health_rows(rows: &[TransportHealthInspectRow]) -> String {
    rows.iter()
        .map(render_transport_health_row)
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_transport_health_row(row: &TransportHealthInspectRow) -> String {
    format!(
        "transport health inspect: transport={} target={} state_path={} updated_unix_ms={} cycle_duration_ms={} queue_depth={} active_runs={} failure_streak={} last_cycle_discovered={} last_cycle_processed={} last_cycle_completed={} last_cycle_failed={} last_cycle_duplicates={}",
        row.transport,
        row.target,
        row.state_path,
        row.health.updated_unix_ms,
        row.health.cycle_duration_ms,
        row.health.queue_depth,
        row.health.active_runs,
        row.health.failure_streak,
        row.health.last_cycle_discovered,
        row.health.last_cycle_processed,
        row.health.last_cycle_completed,
        row.health.last_cycle_failed,
        row.health.last_cycle_duplicates,
    )
}

fn render_github_status_report(report: &GithubStatusInspectReport) -> String {
    let reason_codes = if report.outbound_last_reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.outbound_last_reason_codes.join(",")
    };
    let outbound_reasons = if report.outbound_reason_code_counts.is_empty() {
        "none".to_string()
    } else {
        report
            .outbound_reason_code_counts
            .iter()
            .map(|(reason, count)| format!("{reason}:{count}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    let outbound_commands = if report.outbound_command_counts.is_empty() {
        "none".to_string()
    } else {
        report
            .outbound_command_counts
            .iter()
            .map(|(command, count)| format!("{command}:{count}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    let inbound_kinds = if report.inbound_kind_counts.is_empty() {
        "none".to_string()
    } else {
        report
            .inbound_kind_counts
            .iter()
            .map(|(kind, count)| format!("{kind}:{count}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    format!(
        "github status inspect: repo={} state_path={} inbound_log_path={} outbound_log_path={} health_state={} health_reason={} rollout_gate={} processed_event_count={} issue_session_count={} inbound_records={} inbound_invalid_records={} inbound_kind_counts={} outbound_records={} outbound_invalid_records={} outbound_command_counts={} outbound_reason_code_counts={} outbound_last_reason_codes={} outbound_last_event_key={}",
        report.repo,
        report.state_path,
        report.inbound_log_path,
        report.outbound_log_path,
        report.health_state,
        report.health_reason,
        report.rollout_gate,
        report.processed_event_count,
        report.issue_session_count,
        report.inbound_records,
        report.inbound_invalid_records,
        inbound_kinds,
        report.outbound_records,
        report.outbound_invalid_records,
        outbound_commands,
        outbound_reasons,
        reason_codes,
        report
            .outbound_last_event_key
            .as_deref()
            .unwrap_or("none"),
    )
}

fn render_dashboard_status_report(report: &DashboardStatusInspectReport) -> String {
    let reason_codes = if report.last_reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.last_reason_codes.join(",")
    };
    format!(
        "dashboard status inspect: state_path={} events_log_path={} events_log_present={} health_state={} health_reason={} rollout_gate={} processed_case_count={} widget_count={} control_audit_count={} cycle_reports={} invalid_cycle_reports={} last_reason_codes={} queue_depth={} failure_streak={} last_cycle_failed={} last_cycle_completed={}",
        report.state_path,
        report.events_log_path,
        report.events_log_present,
        report.health_state,
        report.health_reason,
        report.rollout_gate,
        report.processed_case_count,
        report.widget_count,
        report.control_audit_count,
        report.cycle_reports,
        report.invalid_cycle_reports,
        reason_codes,
        report.health.queue_depth,
        report.health.failure_streak,
        report.health.last_cycle_failed,
        report.health.last_cycle_completed,
    )
}

fn render_multi_channel_status_report(report: &MultiChannelStatusInspectReport) -> String {
    let reason_codes = if report.last_reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.last_reason_codes.join(",")
    };
    let telemetry_summary = format!(
        "typing_events={} presence_events={} usage_records={} usage_chars={} usage_chunks={} usage_cost_micros={} typing_by_transport={} presence_by_transport={} usage_records_by_transport={} usage_chars_by_transport={} usage_chunks_by_transport={} usage_cost_micros_by_transport={} policy=typing_presence:{}|usage_summary:{}|include_identifiers:{}|min_response_chars:{}",
        report.telemetry.typing_events_emitted,
        report.telemetry.presence_events_emitted,
        report.telemetry.usage_summary_records,
        report.telemetry.usage_response_chars,
        report.telemetry.usage_chunks,
        report.telemetry.usage_estimated_cost_micros,
        render_counter_map(&report.telemetry.typing_events_by_transport),
        render_counter_map(&report.telemetry.presence_events_by_transport),
        render_counter_map(&report.telemetry.usage_summary_records_by_transport),
        render_counter_map(&report.telemetry.usage_response_chars_by_transport),
        render_counter_map(&report.telemetry.usage_chunks_by_transport),
        render_u64_counter_map(&report.telemetry.usage_estimated_cost_micros_by_transport),
        report.telemetry.policy.typing_presence_enabled,
        report.telemetry.policy.usage_summary_enabled,
        report.telemetry.policy.include_identifiers,
        report.telemetry.policy.typing_presence_min_response_chars,
    );
    let connector_summary = report
        .connectors
        .as_ref()
        .map(|connectors| {
            let now_unix_ms = current_unix_timestamp_ms();
            let mut channel_rows = Vec::new();
            for (channel, state) in &connectors.channels {
                let operator_guidance = if state.breaker_state == "open"
                    && state.breaker_open_until_unix_ms > now_unix_ms
                {
                    format!(
                        "wait_for_breaker_recovery_until:{}",
                        state.breaker_open_until_unix_ms
                    )
                } else if state.liveness == "degraded" {
                    "inspect_provider_errors_and_credentials".to_string()
                } else {
                    "none".to_string()
                };
                channel_rows.push(format!(
                    "{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                    channel,
                    if state.mode.trim().is_empty() {
                        "unknown"
                    } else {
                        state.mode.as_str()
                    },
                    if state.liveness.trim().is_empty() {
                        "unknown"
                    } else {
                        state.liveness.as_str()
                    },
                    if state.breaker_state.trim().is_empty() {
                        "unknown"
                    } else {
                        state.breaker_state.as_str()
                    },
                    state.events_ingested,
                    state.duplicates_skipped,
                    state.retry_budget_remaining,
                    state.breaker_open_until_unix_ms,
                    if state.breaker_last_open_reason.trim().is_empty() {
                        "none"
                    } else {
                        state.breaker_last_open_reason.as_str()
                    },
                    state.breaker_open_count,
                    operator_guidance
                ));
            }
            channel_rows.sort();
            format!(
                "state_present={} processed_event_count={} channels={}",
                connectors.state_present,
                connectors.processed_event_count,
                if channel_rows.is_empty() {
                    "none".to_string()
                } else {
                    channel_rows.join(",")
                }
            )
        })
        .unwrap_or_else(|| "none".to_string());
    format!(
        "multi-channel status inspect: state_path={} events_log_path={} events_log_present={} health_state={} health_reason={} rollout_gate={} processed_event_count={} transport_counts={} cycle_reports={} invalid_cycle_reports={} last_reason_codes={} reason_code_counts={} queue_depth={} failure_streak={} last_cycle_failed={} last_cycle_completed={} telemetry={} connectors={}",
        report.state_path,
        report.events_log_path,
        report.events_log_present,
        report.health_state,
        report.health_reason,
        report.rollout_gate,
        report.processed_event_count,
        render_counter_map(&report.transport_counts),
        report.cycle_reports,
        report.invalid_cycle_reports,
        reason_codes,
        render_counter_map(&report.reason_code_counts),
        report.health.queue_depth,
        report.health.failure_streak,
        report.health.last_cycle_failed,
        report.health.last_cycle_completed,
        telemetry_summary,
        connector_summary,
    )
}

fn render_multi_agent_status_report(report: &MultiAgentStatusInspectReport) -> String {
    let reason_codes = if report.last_reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.last_reason_codes.join(",")
    };
    format!(
        "multi-agent status inspect: state_path={} events_log_path={} events_log_present={} health_state={} health_reason={} rollout_gate={} processed_case_count={} routed_case_count={} phase_counts={} selected_role_counts={} category_counts={} cycle_reports={} invalid_cycle_reports={} last_reason_codes={} reason_code_counts={} queue_depth={} failure_streak={} last_cycle_failed={} last_cycle_completed={}",
        report.state_path,
        report.events_log_path,
        report.events_log_present,
        report.health_state,
        report.health_reason,
        report.rollout_gate,
        report.processed_case_count,
        report.routed_case_count,
        render_counter_map(&report.phase_counts),
        render_counter_map(&report.selected_role_counts),
        render_counter_map(&report.category_counts),
        report.cycle_reports,
        report.invalid_cycle_reports,
        reason_codes,
        render_counter_map(&report.reason_code_counts),
        report.health.queue_depth,
        report.health.failure_streak,
        report.health.last_cycle_failed,
        report.health.last_cycle_completed,
    )
}

fn render_gateway_status_report(report: &GatewayStatusInspectReport) -> String {
    let reason_codes = if report.last_reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.last_reason_codes.join(",")
    };
    let service_last_startup_error = if report.service_last_startup_error.trim().is_empty() {
        "none".to_string()
    } else {
        report.service_last_startup_error.clone()
    };
    let service_last_stop_reason = if report.service_last_stop_reason.trim().is_empty() {
        "none".to_string()
    } else {
        report.service_last_stop_reason.clone()
    };
    format!(
        "gateway status inspect: state_path={} events_log_path={} events_log_present={} health_state={} health_reason={} rollout_gate={} rollout_reason_code={} service_status={} service_last_transition_unix_ms={} service_last_started_unix_ms={} service_last_stopped_unix_ms={} service_last_stop_reason={} service_startup_attempts={} service_startup_failure_streak={} service_last_startup_error={} guardrail_gate={} guardrail_reason_code={} processed_case_count={} request_count={} method_counts={} status_code_counts={} error_code_counts={} guardrail_failure_streak_threshold={} guardrail_retryable_failures_threshold={} guardrail_failure_streak={} guardrail_last_failed_cases={} guardrail_last_retryable_failures={} cycle_reports={} invalid_cycle_reports={} last_reason_codes={} reason_code_counts={} queue_depth={} failure_streak={} last_cycle_failed={} last_cycle_completed={}",
        report.state_path,
        report.events_log_path,
        report.events_log_present,
        report.health_state,
        report.health_reason,
        report.rollout_gate,
        report.rollout_reason_code,
        report.service_status,
        report.service_last_transition_unix_ms,
        report.service_last_started_unix_ms,
        report.service_last_stopped_unix_ms,
        service_last_stop_reason,
        report.service_startup_attempts,
        report.service_startup_failure_streak,
        service_last_startup_error,
        report.guardrail_gate,
        report.guardrail_reason_code,
        report.processed_case_count,
        report.request_count,
        render_counter_map(&report.method_counts),
        render_counter_map(&report.status_code_counts),
        render_counter_map(&report.error_code_counts),
        report.guardrail_failure_streak_threshold,
        report.guardrail_retryable_failures_threshold,
        report.guardrail_failure_streak,
        report.guardrail_last_failed_cases,
        report.guardrail_last_retryable_failures,
        report.cycle_reports,
        report.invalid_cycle_reports,
        reason_codes,
        render_counter_map(&report.reason_code_counts),
        report.health.queue_depth,
        report.health.failure_streak,
        report.health.last_cycle_failed,
        report.health.last_cycle_completed,
    )
}

fn render_custom_command_status_report(report: &CustomCommandStatusInspectReport) -> String {
    let reason_codes = if report.last_reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.last_reason_codes.join(",")
    };
    format!(
        "custom-command status inspect: state_path={} events_log_path={} events_log_present={} health_state={} health_reason={} rollout_gate={} processed_case_count={} command_count={} command_name_counts={} operation_counts={} outcome_counts={} status_code_counts={} total_run_count={} cycle_reports={} invalid_cycle_reports={} last_reason_codes={} reason_code_counts={} queue_depth={} failure_streak={} last_cycle_failed={} last_cycle_completed={}",
        report.state_path,
        report.events_log_path,
        report.events_log_present,
        report.health_state,
        report.health_reason,
        report.rollout_gate,
        report.processed_case_count,
        report.command_count,
        render_counter_map(&report.command_name_counts),
        render_counter_map(&report.operation_counts),
        render_counter_map(&report.outcome_counts),
        render_counter_map(&report.status_code_counts),
        report.total_run_count,
        report.cycle_reports,
        report.invalid_cycle_reports,
        reason_codes,
        render_counter_map(&report.reason_code_counts),
        report.health.queue_depth,
        report.health.failure_streak,
        report.health.last_cycle_failed,
        report.health.last_cycle_completed,
    )
}

fn render_voice_status_report(report: &VoiceStatusInspectReport) -> String {
    let reason_codes = if report.last_reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.last_reason_codes.join(",")
    };
    format!(
        "voice status inspect: state_path={} events_log_path={} events_log_present={} health_state={} health_reason={} rollout_gate={} processed_case_count={} interaction_count={} mode_counts={} speaker_counts={} outcome_counts={} status_code_counts={} utterance_count={} total_run_count={} cycle_reports={} invalid_cycle_reports={} last_reason_codes={} reason_code_counts={} queue_depth={} failure_streak={} last_cycle_failed={} last_cycle_completed={}",
        report.state_path,
        report.events_log_path,
        report.events_log_present,
        report.health_state,
        report.health_reason,
        report.rollout_gate,
        report.processed_case_count,
        report.interaction_count,
        render_counter_map(&report.mode_counts),
        render_counter_map(&report.speaker_counts),
        render_counter_map(&report.outcome_counts),
        render_counter_map(&report.status_code_counts),
        report.utterance_count,
        report.total_run_count,
        report.cycle_reports,
        report.invalid_cycle_reports,
        reason_codes,
        render_counter_map(&report.reason_code_counts),
        report.health.queue_depth,
        report.health.failure_streak,
        report.health.last_cycle_failed,
        report.health.last_cycle_completed,
    )
}

fn render_deployment_status_report(report: &DeploymentStatusInspectReport) -> String {
    let reason_codes = if report.last_reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.last_reason_codes.join(",")
    };
    format!(
        "deployment status inspect: state_path={} events_log_path={} events_log_present={} health_state={} health_reason={} rollout_gate={} processed_case_count={} rollout_count={} target_counts={} runtime_profile_counts={} environment_counts={} outcome_counts={} status_code_counts={} error_code_counts={} total_replicas={} wasm_rollout_count={} cloud_rollout_count={} cycle_reports={} invalid_cycle_reports={} last_reason_codes={} reason_code_counts={} queue_depth={} failure_streak={} last_cycle_failed={} last_cycle_completed={}",
        report.state_path,
        report.events_log_path,
        report.events_log_present,
        report.health_state,
        report.health_reason,
        report.rollout_gate,
        report.processed_case_count,
        report.rollout_count,
        render_counter_map(&report.target_counts),
        render_counter_map(&report.runtime_profile_counts),
        render_counter_map(&report.environment_counts),
        render_counter_map(&report.outcome_counts),
        render_counter_map(&report.status_code_counts),
        render_counter_map(&report.error_code_counts),
        report.total_replicas,
        report.wasm_rollout_count,
        report.cloud_rollout_count,
        report.cycle_reports,
        report.invalid_cycle_reports,
        reason_codes,
        render_counter_map(&report.reason_code_counts),
        report.health.queue_depth,
        report.health.failure_streak,
        report.health.last_cycle_failed,
        report.health.last_cycle_completed,
    )
}

fn render_counter_map(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    counts
        .iter()
        .map(|(key, value)| format!("{key}:{value}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn render_u64_counter_map(counts: &BTreeMap<String, u64>) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    counts
        .iter()
        .map(|(key, value)| format!("{key}:{value}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn render_string_vec(values: &[String]) -> String {
    if values.is_empty() {
        return "none".to_string();
    }
    values.join(",")
}

fn sanitize_for_path(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::thread;

    use clap::Parser;
    use tempfile::tempdir;

    use super::{
        build_operator_control_summary_diff_report, collect_custom_command_status_report,
        collect_dashboard_status_report, collect_deployment_status_report,
        collect_gateway_status_report, collect_github_status_report,
        collect_multi_agent_status_report, collect_multi_channel_status_report,
        collect_operator_control_summary_report, collect_transport_health_rows,
        collect_voice_status_report, load_operator_control_summary_snapshot,
        operator_health_state_rank, parse_transport_health_inspect_target,
        render_custom_command_status_report, render_dashboard_status_report,
        render_deployment_status_report, render_gateway_status_report, render_github_status_report,
        render_multi_agent_status_report, render_multi_channel_status_report,
        render_operator_control_summary_diff_report, render_operator_control_summary_report,
        render_transport_health_row, render_transport_health_rows, render_voice_status_report,
        save_operator_control_summary_snapshot, OperatorControlComponentSummaryRow,
        OperatorControlDaemonSummary, OperatorControlPolicyPosture,
        OperatorControlReleaseChannelSummary, OperatorControlSummaryReport,
        TransportHealthInspectRow, TransportHealthInspectTarget,
    };
    use crate::transport_health::TransportHealthState;
    use crate::Cli;
    use crate::TransportHealthSnapshot;

    fn parse_cli(args: &[&str]) -> Cli {
        let owned_args = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        thread::Builder::new()
            .name("tau-cli-parse".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || Cli::parse_from(owned_args))
            .expect("spawn cli parse thread")
            .join()
            .expect("join cli parse thread")
    }

    struct OperatorSummaryFixtureInput<'a> {
        generated_unix_ms: u64,
        health_state: &'a str,
        rollout_gate: &'a str,
        reason_code: &'a str,
        recommendation: &'a str,
        component_health_state: &'a str,
        component_rollout_gate: &'a str,
        queue_depth: usize,
        failure_streak: usize,
    }

    fn operator_summary_fixture(
        input: OperatorSummaryFixtureInput<'_>,
    ) -> OperatorControlSummaryReport {
        OperatorControlSummaryReport {
            generated_unix_ms: input.generated_unix_ms,
            health_state: input.health_state.to_string(),
            rollout_gate: input.rollout_gate.to_string(),
            reason_codes: vec![input.reason_code.to_string()],
            recommendations: vec![input.recommendation.to_string()],
            policy_posture: OperatorControlPolicyPosture {
                pairing_strict_effective: false,
                pairing_config_strict_mode: false,
                pairing_allowlist_strict: false,
                pairing_rules_configured: false,
                pairing_registry_entries: 0,
                pairing_allowlist_channel_rules: 0,
                provider_subscription_strict: false,
                gateway_auth_mode: "token".to_string(),
                gateway_remote_profile: "local-only".to_string(),
                gateway_remote_posture: "local-only".to_string(),
                gateway_remote_gate: "pass".to_string(),
                gateway_remote_risk_level: "low".to_string(),
                gateway_remote_reason_codes: vec!["local_only_profile".to_string()],
                gateway_remote_recommendations: vec!["no_immediate_action_required".to_string()],
            },
            daemon: OperatorControlDaemonSummary {
                health_state: "healthy".to_string(),
                rollout_gate: "pass".to_string(),
                reason_code: "daemon_running".to_string(),
                recommendation: "no_immediate_action_required".to_string(),
                profile: "none".to_string(),
                installed: false,
                running: false,
                start_attempts: 0,
                stop_attempts: 0,
                diagnostics: 0,
                state_path: ".tau/daemon/state.json".to_string(),
            },
            release_channel: OperatorControlReleaseChannelSummary {
                health_state: "healthy".to_string(),
                rollout_gate: "pass".to_string(),
                reason_code: "configured".to_string(),
                recommendation: "no_immediate_action_required".to_string(),
                configured: true,
                channel: "stable".to_string(),
                path: ".tau/release-channel.json".to_string(),
            },
            components: vec![OperatorControlComponentSummaryRow {
                component: "gateway".to_string(),
                state_path: ".tau/gateway/state.json".to_string(),
                health_state: input.component_health_state.to_string(),
                health_reason: "fixture".to_string(),
                rollout_gate: input.component_rollout_gate.to_string(),
                reason_code: input.reason_code.to_string(),
                recommendation: input.recommendation.to_string(),
                queue_depth: input.queue_depth,
                failure_streak: input.failure_streak,
            }],
        }
    }

    #[test]
    fn unit_parse_transport_health_inspect_target_accepts_supported_values() {
        assert_eq!(
            parse_transport_health_inspect_target("slack").expect("slack"),
            TransportHealthInspectTarget::Slack
        );
        assert_eq!(
            parse_transport_health_inspect_target("github").expect("github"),
            TransportHealthInspectTarget::GithubAll
        );
        assert_eq!(
            parse_transport_health_inspect_target("github:owner/repo").expect("github owner/repo"),
            TransportHealthInspectTarget::GithubRepo {
                owner: "owner".to_string(),
                repo: "repo".to_string(),
            }
        );
        assert_eq!(
            parse_transport_health_inspect_target("multi-channel").expect("multi-channel"),
            TransportHealthInspectTarget::MultiChannel
        );
        assert_eq!(
            parse_transport_health_inspect_target("multichannel").expect("multichannel"),
            TransportHealthInspectTarget::MultiChannel
        );
        assert_eq!(
            parse_transport_health_inspect_target("multi-agent").expect("multi-agent"),
            TransportHealthInspectTarget::MultiAgent
        );
        assert_eq!(
            parse_transport_health_inspect_target("multiagent").expect("multiagent"),
            TransportHealthInspectTarget::MultiAgent
        );
        assert_eq!(
            parse_transport_health_inspect_target("browser-automation")
                .expect("browser-automation"),
            TransportHealthInspectTarget::BrowserAutomation
        );
        assert_eq!(
            parse_transport_health_inspect_target("browserautomation").expect("browserautomation"),
            TransportHealthInspectTarget::BrowserAutomation
        );
        assert_eq!(
            parse_transport_health_inspect_target("browser").expect("browser"),
            TransportHealthInspectTarget::BrowserAutomation
        );
        assert_eq!(
            parse_transport_health_inspect_target("memory").expect("memory"),
            TransportHealthInspectTarget::Memory
        );
        assert_eq!(
            parse_transport_health_inspect_target("dashboard").expect("dashboard"),
            TransportHealthInspectTarget::Dashboard
        );
        assert_eq!(
            parse_transport_health_inspect_target("gateway").expect("gateway"),
            TransportHealthInspectTarget::Gateway
        );
        assert_eq!(
            parse_transport_health_inspect_target("deployment").expect("deployment"),
            TransportHealthInspectTarget::Deployment
        );
        assert_eq!(
            parse_transport_health_inspect_target("custom-command").expect("custom-command"),
            TransportHealthInspectTarget::CustomCommand
        );
        assert_eq!(
            parse_transport_health_inspect_target("customcommand").expect("customcommand"),
            TransportHealthInspectTarget::CustomCommand
        );
        assert_eq!(
            parse_transport_health_inspect_target("voice").expect("voice"),
            TransportHealthInspectTarget::Voice
        );
    }

    #[test]
    fn unit_render_transport_health_row_formats_expected_fields() {
        let row = TransportHealthInspectRow {
            transport: "github".to_string(),
            target: "owner/repo".to_string(),
            state_path: "/tmp/state.json".to_string(),
            health: TransportHealthSnapshot {
                updated_unix_ms: 123,
                cycle_duration_ms: 88,
                queue_depth: 3,
                active_runs: 1,
                failure_streak: 0,
                last_cycle_discovered: 4,
                last_cycle_processed: 3,
                last_cycle_completed: 2,
                last_cycle_failed: 1,
                last_cycle_duplicates: 1,
            },
        };
        let rendered = render_transport_health_row(&row);
        assert!(rendered.contains("transport=github"));
        assert!(rendered.contains("target=owner/repo"));
        assert!(rendered.contains("cycle_duration_ms=88"));
        assert!(rendered.contains("last_cycle_failed=1"));
    }

    #[test]
    fn functional_collect_transport_health_rows_reads_github_and_slack_states() {
        let temp = tempdir().expect("tempdir");
        let github_root = temp.path().join("github");
        let slack_root = temp.path().join("slack");
        let multi_channel_root = temp.path().join("multi-channel");
        let multi_agent_root = temp.path().join("multi-agent");
        let browser_automation_root = temp.path().join("browser-automation");
        let memory_root = temp.path().join("memory");
        let dashboard_root = temp.path().join("dashboard");
        let gateway_root = temp.path().join("gateway");
        let deployment_root = temp.path().join("deployment");
        let custom_command_root = temp.path().join("custom-command");
        let voice_root = temp.path().join("voice");
        let github_repo_dir = github_root.join("owner__repo");
        std::fs::create_dir_all(&github_repo_dir).expect("create github repo dir");
        std::fs::create_dir_all(&slack_root).expect("create slack dir");
        std::fs::create_dir_all(&multi_channel_root).expect("create multi-channel dir");
        std::fs::create_dir_all(&multi_agent_root).expect("create multi-agent dir");
        std::fs::create_dir_all(&browser_automation_root).expect("create browser-automation dir");
        std::fs::create_dir_all(&memory_root).expect("create memory dir");
        std::fs::create_dir_all(&dashboard_root).expect("create dashboard dir");
        std::fs::create_dir_all(&gateway_root).expect("create gateway dir");
        std::fs::create_dir_all(&deployment_root).expect("create deployment dir");
        std::fs::create_dir_all(&custom_command_root).expect("create custom-command dir");
        std::fs::create_dir_all(&voice_root).expect("create voice dir");

        std::fs::write(
            github_repo_dir.join("state.json"),
            r#"{
  "schema_version": 1,
  "last_issue_scan_at": "2026-01-01T00:00:00Z",
  "processed_event_keys": [],
  "issue_sessions": {},
  "health": {
    "updated_unix_ms": 100,
    "cycle_duration_ms": 25,
    "queue_depth": 0,
    "active_runs": 1,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write github state");

        std::fs::write(
            slack_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_event_keys": [],
  "health": {
    "updated_unix_ms": 200,
    "cycle_duration_ms": 50,
    "queue_depth": 2,
    "active_runs": 1,
    "failure_streak": 1,
    "last_cycle_discovered": 4,
    "last_cycle_processed": 3,
    "last_cycle_completed": 1,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 1
  }
}
"#,
        )
        .expect("write slack state");

        std::fs::write(
            multi_channel_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_event_keys": [],
  "health": {
    "updated_unix_ms": 300,
    "cycle_duration_ms": 15,
    "queue_depth": 1,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 3,
    "last_cycle_processed": 3,
    "last_cycle_completed": 3,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write multi-channel state");

        std::fs::write(
            multi_agent_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "routed_cases": [],
  "health": {
    "updated_unix_ms": 350,
    "cycle_duration_ms": 19,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write multi-agent state");

        std::fs::write(
            browser_automation_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "health": {
    "updated_unix_ms": 375,
    "cycle_duration_ms": 28,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write browser-automation state");

        std::fs::write(
            memory_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "entries": [],
  "health": {
    "updated_unix_ms": 400,
    "cycle_duration_ms": 32,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 5,
    "last_cycle_processed": 5,
    "last_cycle_completed": 5,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 1
  }
}
"#,
        )
        .expect("write memory state");

        std::fs::write(
            dashboard_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "widget_views": [],
  "control_audit": [],
  "health": {
    "updated_unix_ms": 500,
    "cycle_duration_ms": 40,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write dashboard state");

        std::fs::write(
            gateway_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "requests": [],
  "health": {
    "updated_unix_ms": 550,
    "cycle_duration_ms": 21,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write gateway state");

        std::fs::write(
            deployment_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "rollouts": [],
  "health": {
    "updated_unix_ms": 560,
    "cycle_duration_ms": 17,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 3,
    "last_cycle_processed": 3,
    "last_cycle_completed": 3,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write deployment state");

        std::fs::write(
            custom_command_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "commands": [],
  "health": {
    "updated_unix_ms": 580,
    "cycle_duration_ms": 23,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 4,
    "last_cycle_processed": 4,
    "last_cycle_completed": 4,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write custom-command state");

        std::fs::write(
            voice_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "interactions": [],
  "health": {
    "updated_unix_ms": 590,
    "cycle_duration_ms": 18,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 3,
    "last_cycle_processed": 3,
    "last_cycle_completed": 3,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write voice state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.github_state_dir = github_root;
        cli.slack_state_dir = slack_root;
        cli.multi_channel_state_dir = multi_channel_root;
        cli.multi_agent_state_dir = multi_agent_root;
        cli.browser_automation_state_dir = browser_automation_root;
        cli.memory_state_dir = memory_root;
        cli.dashboard_state_dir = dashboard_root;
        cli.gateway_state_dir = gateway_root;
        cli.deployment_state_dir = deployment_root;
        cli.custom_command_state_dir = custom_command_root;
        cli.voice_state_dir = voice_root;

        let github_rows =
            collect_transport_health_rows(&cli, &TransportHealthInspectTarget::GithubAll)
                .expect("collect github rows");
        assert_eq!(github_rows.len(), 1);
        assert_eq!(github_rows[0].transport, "github");
        assert_eq!(github_rows[0].target, "owner/repo");
        assert_eq!(github_rows[0].health.last_cycle_processed, 2);

        let slack_rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Slack)
            .expect("collect slack rows");
        assert_eq!(slack_rows.len(), 1);
        assert_eq!(slack_rows[0].transport, "slack");
        assert_eq!(slack_rows[0].health.queue_depth, 2);

        let multi_channel_rows =
            collect_transport_health_rows(&cli, &TransportHealthInspectTarget::MultiChannel)
                .expect("collect multi-channel rows");
        assert_eq!(multi_channel_rows.len(), 1);
        assert_eq!(multi_channel_rows[0].transport, "multi-channel");
        assert_eq!(multi_channel_rows[0].target, "telegram/discord/whatsapp");
        assert_eq!(multi_channel_rows[0].health.last_cycle_discovered, 3);

        let multi_agent_rows =
            collect_transport_health_rows(&cli, &TransportHealthInspectTarget::MultiAgent)
                .expect("collect multi-agent rows");
        assert_eq!(multi_agent_rows.len(), 1);
        assert_eq!(multi_agent_rows[0].transport, "multi-agent");
        assert_eq!(multi_agent_rows[0].target, "orchestrator-router");
        assert_eq!(multi_agent_rows[0].health.last_cycle_discovered, 2);

        let browser_automation_rows =
            collect_transport_health_rows(&cli, &TransportHealthInspectTarget::BrowserAutomation)
                .expect("collect browser-automation rows");
        assert_eq!(browser_automation_rows.len(), 1);
        assert_eq!(browser_automation_rows[0].transport, "browser-automation");
        assert_eq!(browser_automation_rows[0].target, "fixture-runtime");
        assert_eq!(browser_automation_rows[0].health.last_cycle_discovered, 1);

        let memory_rows =
            collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Memory)
                .expect("collect memory rows");
        assert_eq!(memory_rows.len(), 1);
        assert_eq!(memory_rows[0].transport, "memory");
        assert_eq!(memory_rows[0].target, "semantic-memory");
        assert_eq!(memory_rows[0].health.last_cycle_discovered, 5);

        let dashboard_rows =
            collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Dashboard)
                .expect("collect dashboard rows");
        assert_eq!(dashboard_rows.len(), 1);
        assert_eq!(dashboard_rows[0].transport, "dashboard");
        assert_eq!(dashboard_rows[0].target, "operator-control-plane");
        assert_eq!(dashboard_rows[0].health.last_cycle_discovered, 2);

        let gateway_rows =
            collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Gateway)
                .expect("collect gateway rows");
        assert_eq!(gateway_rows.len(), 1);
        assert_eq!(gateway_rows[0].transport, "gateway");
        assert_eq!(gateway_rows[0].target, "gateway-service");
        assert_eq!(gateway_rows[0].health.last_cycle_discovered, 2);

        let deployment_rows =
            collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Deployment)
                .expect("collect deployment rows");
        assert_eq!(deployment_rows.len(), 1);
        assert_eq!(deployment_rows[0].transport, "deployment");
        assert_eq!(deployment_rows[0].target, "cloud-and-wasm-runtime");
        assert_eq!(deployment_rows[0].health.last_cycle_discovered, 3);

        let custom_command_rows =
            collect_transport_health_rows(&cli, &TransportHealthInspectTarget::CustomCommand)
                .expect("collect custom-command rows");
        assert_eq!(custom_command_rows.len(), 1);
        assert_eq!(custom_command_rows[0].transport, "custom-command");
        assert_eq!(custom_command_rows[0].target, "no-code-command-registry");
        assert_eq!(custom_command_rows[0].health.last_cycle_discovered, 4);

        let voice_rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Voice)
            .expect("collect voice rows");
        assert_eq!(voice_rows.len(), 1);
        assert_eq!(voice_rows[0].transport, "voice");
        assert_eq!(voice_rows[0].target, "wake-word-pipeline");
        assert_eq!(voice_rows[0].health.last_cycle_discovered, 3);

        let rendered = render_transport_health_rows(&[
            github_rows[0].clone(),
            slack_rows[0].clone(),
            multi_channel_rows[0].clone(),
            multi_agent_rows[0].clone(),
            browser_automation_rows[0].clone(),
            memory_rows[0].clone(),
            dashboard_rows[0].clone(),
            gateway_rows[0].clone(),
            deployment_rows[0].clone(),
            custom_command_rows[0].clone(),
            voice_rows[0].clone(),
        ]);
        assert!(rendered.contains("transport=github"));
        assert!(rendered.contains("transport=slack"));
        assert!(rendered.contains("transport=multi-channel"));
        assert!(rendered.contains("transport=multi-agent"));
        assert!(rendered.contains("transport=browser-automation"));
        assert!(rendered.contains("transport=memory"));
        assert!(rendered.contains("transport=dashboard"));
        assert!(rendered.contains("transport=gateway"));
        assert!(rendered.contains("transport=deployment"));
        assert!(rendered.contains("transport=custom-command"));
        assert!(rendered.contains("transport=voice"));
    }

    #[test]
    fn regression_collect_transport_health_rows_defaults_missing_health_fields() {
        let temp = tempdir().expect("tempdir");
        let github_root = temp.path().join("github");
        let github_repo_dir = github_root.join("owner__repo");
        std::fs::create_dir_all(&github_repo_dir).expect("create github repo dir");
        std::fs::write(
            github_repo_dir.join("state.json"),
            r#"{
  "schema_version": 1,
  "last_issue_scan_at": null,
  "processed_event_keys": [],
  "issue_sessions": {}
}
"#,
        )
        .expect("write legacy github state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.github_state_dir = PathBuf::from(&github_root);

        let rows = collect_transport_health_rows(
            &cli,
            &TransportHealthInspectTarget::GithubRepo {
                owner: "owner".to_string(),
                repo: "repo".to_string(),
            },
        )
        .expect("collect legacy row");
        assert_eq!(rows[0].health, TransportHealthSnapshot::default());
    }

    #[test]
    fn regression_collect_transport_health_rows_defaults_missing_health_fields_for_memory() {
        let temp = tempdir().expect("tempdir");
        let memory_root = temp.path().join("memory");
        std::fs::create_dir_all(&memory_root).expect("create memory dir");
        std::fs::write(
            memory_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "entries": []
}
"#,
        )
        .expect("write legacy memory state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.memory_state_dir = PathBuf::from(&memory_root);

        let rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Memory)
            .expect("collect memory row");
        assert_eq!(rows[0].health, TransportHealthSnapshot::default());
    }

    #[test]
    fn regression_collect_transport_health_rows_defaults_missing_health_fields_for_multi_agent() {
        let temp = tempdir().expect("tempdir");
        let multi_agent_root = temp.path().join("multi-agent");
        std::fs::create_dir_all(&multi_agent_root).expect("create multi-agent dir");
        std::fs::write(
            multi_agent_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "routed_cases": []
}
"#,
        )
        .expect("write legacy multi-agent state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.multi_agent_state_dir = PathBuf::from(&multi_agent_root);

        let rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::MultiAgent)
            .expect("collect multi-agent row");
        assert_eq!(rows[0].health, TransportHealthSnapshot::default());
    }

    #[test]
    fn regression_collect_transport_health_rows_defaults_missing_health_fields_for_browser_automation(
    ) {
        let temp = tempdir().expect("tempdir");
        let browser_automation_root = temp.path().join("browser-automation");
        std::fs::create_dir_all(&browser_automation_root).expect("create browser-automation dir");
        std::fs::write(
            browser_automation_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": []
}
"#,
        )
        .expect("write legacy browser-automation state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.browser_automation_state_dir = PathBuf::from(&browser_automation_root);

        let rows =
            collect_transport_health_rows(&cli, &TransportHealthInspectTarget::BrowserAutomation)
                .expect("collect browser-automation row");
        assert_eq!(rows[0].health, TransportHealthSnapshot::default());
    }

    #[test]
    fn regression_collect_transport_health_rows_defaults_missing_health_fields_for_dashboard() {
        let temp = tempdir().expect("tempdir");
        let dashboard_root = temp.path().join("dashboard");
        std::fs::create_dir_all(&dashboard_root).expect("create dashboard dir");
        std::fs::write(
            dashboard_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "widget_views": [],
  "control_audit": []
}
"#,
        )
        .expect("write legacy dashboard state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.dashboard_state_dir = PathBuf::from(&dashboard_root);

        let rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Dashboard)
            .expect("collect dashboard row");
        assert_eq!(rows[0].health, TransportHealthSnapshot::default());
    }

    #[test]
    fn regression_collect_transport_health_rows_defaults_missing_health_fields_for_gateway() {
        let temp = tempdir().expect("tempdir");
        let gateway_root = temp.path().join("gateway");
        std::fs::create_dir_all(&gateway_root).expect("create gateway dir");
        std::fs::write(
            gateway_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "requests": []
}
"#,
        )
        .expect("write legacy gateway state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.gateway_state_dir = PathBuf::from(&gateway_root);

        let rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Gateway)
            .expect("collect gateway row");
        assert_eq!(rows[0].health, TransportHealthSnapshot::default());
    }

    #[test]
    fn regression_collect_transport_health_rows_defaults_missing_health_fields_for_custom_command()
    {
        let temp = tempdir().expect("tempdir");
        let custom_command_root = temp.path().join("custom-command");
        std::fs::create_dir_all(&custom_command_root).expect("create custom-command dir");
        std::fs::write(
            custom_command_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "commands": []
}
"#,
        )
        .expect("write legacy custom-command state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.custom_command_state_dir = PathBuf::from(&custom_command_root);

        let rows =
            collect_transport_health_rows(&cli, &TransportHealthInspectTarget::CustomCommand)
                .expect("collect custom-command row");
        assert_eq!(rows[0].health, TransportHealthSnapshot::default());
    }

    #[test]
    fn regression_collect_transport_health_rows_defaults_missing_health_fields_for_deployment() {
        let temp = tempdir().expect("tempdir");
        let deployment_root = temp.path().join("deployment");
        std::fs::create_dir_all(&deployment_root).expect("create deployment dir");
        std::fs::write(
            deployment_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "rollouts": []
}
"#,
        )
        .expect("write legacy deployment state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.deployment_state_dir = PathBuf::from(&deployment_root);

        let rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Deployment)
            .expect("collect deployment row");
        assert_eq!(rows[0].health, TransportHealthSnapshot::default());
    }

    #[test]
    fn regression_collect_transport_health_rows_defaults_missing_health_fields_for_voice() {
        let temp = tempdir().expect("tempdir");
        let voice_root = temp.path().join("voice");
        std::fs::create_dir_all(&voice_root).expect("create voice dir");
        std::fs::write(
            voice_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "interactions": []
}
"#,
        )
        .expect("write legacy voice state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.voice_state_dir = PathBuf::from(&voice_root);

        let rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Voice)
            .expect("collect voice row");
        assert_eq!(rows[0].health, TransportHealthSnapshot::default());
    }

    #[test]
    fn functional_collect_github_status_report_reads_state_and_logs() {
        let temp = tempdir().expect("tempdir");
        let github_root = temp.path().join("github");
        let repo_root = github_root.join("owner__repo");
        std::fs::create_dir_all(&repo_root).expect("create github repo dir");

        std::fs::write(
            repo_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "last_issue_scan_at": "2026-01-01T00:00:00Z",
  "processed_event_keys": ["issue-comment-created:100","issue-comment-created:101"],
  "issue_sessions": {
    "7": {
      "session_id": "issue-7",
      "last_comment_id": 901,
      "last_run_id": "run-7",
      "active_run_id": null,
      "last_event_key": "issue-comment-created:101",
      "last_event_kind": "issue_comment_created",
      "last_actor_login": "alice",
      "last_reason_code": "command_processed",
      "last_processed_unix_ms": 1704067200000,
      "total_processed_events": 4,
      "total_duplicate_events": 1,
      "total_failed_events": 1,
      "total_denied_events": 1,
      "total_runs_started": 3,
      "total_runs_completed": 3,
      "total_runs_failed": 1
    }
  },
  "health": {
    "updated_unix_ms": 200,
    "cycle_duration_ms": 20,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 3,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 1
  }
}
"#,
        )
        .expect("write state");
        std::fs::write(
            repo_root.join("inbound-events.jsonl"),
            r#"{"kind":"issue_comment_created","event_key":"issue-comment-created:100"}
{"kind":"issue_comment_edited","event_key":"issue-comment-edited:101"}
{"kind":"issue_comment_created"}
"#,
        )
        .expect("write inbound");
        std::fs::write(
            repo_root.join("outbound-events.jsonl"),
            r#"{"event_key":"issue-comment-created:100","command":"chat-status","status":"reported","reason_code":"command_processed"}
{"event_key":"issue-comment-created:101","command":"chat-replay","status":"reported","reason_code":"command_processed"}
{"event_key":"issue-comment-created:101","command":"authorization","status":"denied","reason_code":"pairing_denied"}
"#,
        )
        .expect("write outbound");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.github_state_dir = github_root;
        let report =
            collect_github_status_report(&cli, "owner/repo").expect("collect github status report");
        assert_eq!(report.repo, "owner/repo");
        assert_eq!(report.health_state, TransportHealthState::Healthy.as_str());
        assert_eq!(report.rollout_gate, "pass");
        assert_eq!(report.processed_event_count, 2);
        assert_eq!(report.issue_session_count, 1);
        assert_eq!(report.inbound_records, 3);
        assert_eq!(report.outbound_records, 3);
        assert_eq!(report.outbound_command_counts.get("chat-status"), Some(&1));
        assert_eq!(report.outbound_command_counts.get("chat-replay"), Some(&1));
        assert_eq!(report.outbound_status_counts.get("reported"), Some(&2));
        assert_eq!(
            report.outbound_reason_code_counts.get("command_processed"),
            Some(&2)
        );
        assert_eq!(
            report.outbound_reason_code_counts.get("pairing_denied"),
            Some(&1)
        );
        assert_eq!(
            report.outbound_last_event_key.as_deref(),
            Some("issue-comment-created:101")
        );
        let rendered = render_github_status_report(&report);
        assert!(rendered.contains("github status inspect: repo=owner/repo"));
        assert!(rendered
            .contains("outbound_command_counts=authorization:1,chat-replay:1,chat-status:1"));
        assert!(
            rendered.contains("outbound_reason_code_counts=command_processed:2,pairing_denied:1")
        );
    }

    #[test]
    fn regression_collect_github_status_report_handles_missing_logs() {
        let temp = tempdir().expect("tempdir");
        let github_root = temp.path().join("github");
        let repo_root = github_root.join("owner__repo");
        std::fs::create_dir_all(&repo_root).expect("create github repo dir");
        std::fs::write(
            repo_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_event_keys": [],
  "issue_sessions": {},
  "health": {
    "updated_unix_ms": 0,
    "cycle_duration_ms": 0,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 0,
    "last_cycle_processed": 0,
    "last_cycle_completed": 0,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.github_state_dir = github_root;
        let report =
            collect_github_status_report(&cli, "owner/repo").expect("collect github status report");
        assert!(!report.inbound_log_present);
        assert!(!report.outbound_log_present);
        assert_eq!(report.inbound_records, 0);
        assert_eq!(report.outbound_records, 0);
        assert!(report.outbound_reason_code_counts.is_empty());
        assert!(report.outbound_last_reason_codes.is_empty());
        assert_eq!(report.health_state, TransportHealthState::Healthy.as_str());
    }

    #[test]
    fn functional_collect_dashboard_status_report_reads_state_and_cycle_reports() {
        let temp = tempdir().expect("tempdir");
        let dashboard_root = temp.path().join("dashboard");
        std::fs::create_dir_all(&dashboard_root).expect("create dashboard dir");
        std::fs::write(
            dashboard_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": ["snapshot:s1", "control:c1"],
  "widget_views": [{"widget_id":"health-summary"}],
  "control_audit": [{"case_id":"c1"}],
  "health": {
    "updated_unix_ms": 600,
    "cycle_duration_ms": 25,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write dashboard state");
        std::fs::write(
            dashboard_root.join("runtime-events.jsonl"),
            r#"{"reason_codes":["widget_views_updated"],"health_reason":"no recent transport failures observed"}
invalid-json-line
{"reason_codes":["widget_views_updated","control_actions_applied"],"health_reason":"no recent transport failures observed"}
"#,
        )
        .expect("write runtime events");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.dashboard_state_dir = dashboard_root;

        let report = collect_dashboard_status_report(&cli).expect("collect status report");
        assert_eq!(report.health_state, TransportHealthState::Healthy.as_str());
        assert_eq!(report.rollout_gate, "pass");
        assert_eq!(report.processed_case_count, 2);
        assert_eq!(report.widget_count, 1);
        assert_eq!(report.control_audit_count, 1);
        assert_eq!(report.cycle_reports, 2);
        assert_eq!(report.invalid_cycle_reports, 1);
        assert_eq!(
            report.last_reason_codes,
            vec![
                "widget_views_updated".to_string(),
                "control_actions_applied".to_string()
            ]
        );
        let rendered = render_dashboard_status_report(&report);
        assert!(rendered.contains("dashboard status inspect:"));
        assert!(rendered.contains("rollout_gate=pass"));
        assert!(rendered.contains("last_reason_codes=widget_views_updated,control_actions_applied"));
    }

    #[test]
    fn regression_collect_dashboard_status_report_handles_missing_events_log() {
        let temp = tempdir().expect("tempdir");
        let dashboard_root = temp.path().join("dashboard");
        std::fs::create_dir_all(&dashboard_root).expect("create dashboard dir");
        std::fs::write(
            dashboard_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "widget_views": [],
  "control_audit": [],
  "health": {
    "updated_unix_ms": 700,
    "cycle_duration_ms": 32,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 1,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 0,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write dashboard state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.dashboard_state_dir = dashboard_root;

        let report = collect_dashboard_status_report(&cli).expect("collect status report");
        assert!(!report.events_log_present);
        assert_eq!(report.cycle_reports, 0);
        assert_eq!(report.invalid_cycle_reports, 0);
        assert!(report.last_reason_codes.is_empty());
        assert_eq!(report.health_state, TransportHealthState::Degraded.as_str());
        assert_eq!(report.rollout_gate, "hold");
    }

    #[test]
    fn functional_collect_multi_channel_status_report_reads_state_and_cycle_reports() {
        let temp = tempdir().expect("tempdir");
        let multi_channel_root = temp.path().join("multi-channel");
        std::fs::create_dir_all(&multi_channel_root).expect("create multi-channel dir");
        std::fs::write(
            multi_channel_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_event_keys": ["telegram:tg-1", "discord:dc-1", "whatsapp:wa-1", "telegram:tg-2"],
  "health": {
    "updated_unix_ms": 735,
    "cycle_duration_ms": 14,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 4,
    "last_cycle_processed": 4,
    "last_cycle_completed": 4,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  },
  "telemetry": {
    "typing_events_emitted": 6,
    "presence_events_emitted": 6,
    "usage_summary_records": 4,
    "usage_response_chars": 222,
    "usage_chunks": 7,
    "usage_estimated_cost_micros": 1200,
    "typing_events_by_transport": {"telegram": 4, "discord": 2},
    "presence_events_by_transport": {"telegram": 4, "discord": 2},
    "usage_summary_records_by_transport": {"telegram": 2, "discord": 1, "whatsapp": 1},
    "usage_response_chars_by_transport": {"telegram": 120, "discord": 60, "whatsapp": 42},
    "usage_chunks_by_transport": {"telegram": 3, "discord": 2, "whatsapp": 2},
    "usage_estimated_cost_micros_by_transport": {"telegram": 700, "discord": 300, "whatsapp": 200}
  },
  "telemetry_policy": {
    "typing_presence_enabled": true,
    "usage_summary_enabled": true,
    "include_identifiers": false,
    "typing_presence_min_response_chars": 90
  }
}
"#,
        )
        .expect("write multi-channel state");
        std::fs::write(
            multi_channel_root.join("runtime-events.jsonl"),
            r#"{"reason_codes":["healthy_cycle","events_applied"],"health_reason":"no recent transport failures observed"}
invalid-json-line
{"reason_codes":["healthy_cycle","duplicate_events_skipped"],"health_reason":"no recent transport failures observed"}
"#,
        )
        .expect("write multi-channel events");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.multi_channel_state_dir = multi_channel_root;

        let report =
            collect_multi_channel_status_report(&cli).expect("collect multi-channel status");
        assert_eq!(report.health_state, TransportHealthState::Healthy.as_str());
        assert_eq!(report.rollout_gate, "pass");
        assert_eq!(report.processed_event_count, 4);
        assert_eq!(report.transport_counts.get("telegram"), Some(&2));
        assert_eq!(report.transport_counts.get("discord"), Some(&1));
        assert_eq!(report.transport_counts.get("whatsapp"), Some(&1));
        assert_eq!(report.cycle_reports, 2);
        assert_eq!(report.invalid_cycle_reports, 1);
        assert_eq!(
            report.last_reason_codes,
            vec![
                "healthy_cycle".to_string(),
                "duplicate_events_skipped".to_string()
            ]
        );
        assert_eq!(report.reason_code_counts.get("healthy_cycle"), Some(&2));
        assert_eq!(report.reason_code_counts.get("events_applied"), Some(&1));
        assert_eq!(
            report.reason_code_counts.get("duplicate_events_skipped"),
            Some(&1)
        );
        assert_eq!(report.telemetry.typing_events_emitted, 6);
        assert_eq!(report.telemetry.presence_events_emitted, 6);
        assert_eq!(report.telemetry.usage_summary_records, 4);
        assert_eq!(report.telemetry.usage_response_chars, 222);
        assert_eq!(report.telemetry.usage_chunks, 7);
        assert_eq!(report.telemetry.usage_estimated_cost_micros, 1200);
        assert_eq!(
            report.telemetry.typing_events_by_transport.get("telegram"),
            Some(&4)
        );
        assert_eq!(
            report
                .telemetry
                .usage_estimated_cost_micros_by_transport
                .get("discord"),
            Some(&300)
        );
        assert!(report.telemetry.policy.typing_presence_enabled);
        assert!(report.telemetry.policy.usage_summary_enabled);
        assert!(!report.telemetry.policy.include_identifiers);
        assert_eq!(
            report.telemetry.policy.typing_presence_min_response_chars,
            90
        );

        let rendered = render_multi_channel_status_report(&report);
        assert!(rendered.contains("multi-channel status inspect:"));
        assert!(rendered.contains("rollout_gate=pass"));
        assert!(rendered.contains("processed_event_count=4"));
        assert!(rendered.contains("transport_counts=discord:1,telegram:2,whatsapp:1"));
        assert!(rendered.contains("typing_events=6"));
        assert!(rendered.contains("usage_records=4"));
        assert!(rendered
            .contains("usage_cost_micros_by_transport=discord:300,telegram:700,whatsapp:200"));
        assert!(rendered.contains("policy=typing_presence:true|usage_summary:true|include_identifiers:false|min_response_chars:90"));
        assert!(rendered.contains(
            "reason_code_counts=duplicate_events_skipped:1,events_applied:1,healthy_cycle:2"
        ));
    }

    #[test]
    fn functional_render_multi_channel_status_report_includes_connector_breaker_guidance() {
        let temp = tempdir().expect("tempdir");
        let multi_channel_root = temp.path().join("multi-channel");
        std::fs::create_dir_all(&multi_channel_root).expect("create multi-channel dir");
        std::fs::write(
            multi_channel_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_event_keys": ["telegram:tg-1"],
  "health": {
    "updated_unix_ms": 910,
    "cycle_duration_ms": 9,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write multi-channel state");
        std::fs::write(
            multi_channel_root.join("runtime-events.jsonl"),
            r#"{"reason_codes":["healthy_cycle"],"health_reason":"no recent transport failures observed"}"#,
        )
        .expect("write multi-channel events");

        let connectors_state_path = temp.path().join("connectors-state.json");
        let breaker_open_until_unix_ms = crate::current_unix_timestamp_ms().saturating_add(60_000);
        let connectors_state = format!(
            r#"{{
  "schema_version": 1,
  "processed_event_keys": ["telegram:tg-1"],
  "channels": {{
    "telegram": {{
      "mode": "polling",
      "liveness": "open",
      "events_ingested": 3,
      "duplicates_skipped": 1,
      "retry_budget_remaining": 0,
      "breaker_state": "open",
      "breaker_open_until_unix_ms": {breaker_open_until_unix_ms},
      "breaker_last_open_reason": "provider_unavailable",
      "breaker_open_count": 2
    }}
  }}
}}"#
        );
        std::fs::write(&connectors_state_path, connectors_state).expect("write connector state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.multi_channel_state_dir = multi_channel_root;
        cli.multi_channel_live_connectors_state_path = connectors_state_path;

        let report =
            collect_multi_channel_status_report(&cli).expect("collect multi-channel status");
        let rendered = render_multi_channel_status_report(&report);
        assert!(rendered.contains("connectors=state_present=true processed_event_count=1"));
        assert!(rendered.contains("channels=telegram:polling:open:open:3:1:0:"));
        assert!(rendered.contains(":provider_unavailable:2:wait_for_breaker_recovery_until:"));
    }

    #[test]
    fn regression_collect_multi_channel_status_report_handles_missing_events_log() {
        let temp = tempdir().expect("tempdir");
        let multi_channel_root = temp.path().join("multi-channel");
        std::fs::create_dir_all(&multi_channel_root).expect("create multi-channel dir");
        std::fs::write(
            multi_channel_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_event_keys": [],
  "health": {
    "updated_unix_ms": 736,
    "cycle_duration_ms": 19,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 2,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 0,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write multi-channel state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.multi_channel_state_dir = multi_channel_root;

        let report =
            collect_multi_channel_status_report(&cli).expect("collect multi-channel status");
        assert!(!report.events_log_present);
        assert_eq!(report.cycle_reports, 0);
        assert_eq!(report.invalid_cycle_reports, 0);
        assert!(report.last_reason_codes.is_empty());
        assert!(report.reason_code_counts.is_empty());
        assert_eq!(report.telemetry.typing_events_emitted, 0);
        assert_eq!(report.telemetry.presence_events_emitted, 0);
        assert_eq!(report.telemetry.usage_summary_records, 0);
        assert!(report.telemetry.policy.typing_presence_enabled);
        assert!(report.telemetry.policy.usage_summary_enabled);
        assert!(!report.telemetry.policy.include_identifiers);
        assert_eq!(
            report.telemetry.policy.typing_presence_min_response_chars,
            120
        );
        assert_eq!(report.health_state, TransportHealthState::Degraded.as_str());
        assert_eq!(report.rollout_gate, "hold");
    }

    #[test]
    fn functional_collect_multi_agent_status_report_reads_state_and_cycle_reports() {
        let temp = tempdir().expect("tempdir");
        let multi_agent_root = temp.path().join("multi-agent");
        std::fs::create_dir_all(&multi_agent_root).expect("create multi-agent dir");
        std::fs::write(
            multi_agent_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": ["planner:planner-success", "review:review-success"],
  "routed_cases": [
    {
      "case_key": "planner:planner-success",
      "case_id": "planner-success",
      "phase": "planner",
      "selected_role": "planner",
      "attempted_roles": ["planner"],
      "category": "planning",
      "updated_unix_ms": 1
    },
    {
      "case_key": "review:review-success",
      "case_id": "review-success",
      "phase": "review",
      "selected_role": "reviewer",
      "attempted_roles": ["reviewer"],
      "category": "review",
      "updated_unix_ms": 2
    }
  ],
  "health": {
    "updated_unix_ms": 710,
    "cycle_duration_ms": 17,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write multi-agent state");
        std::fs::write(
            multi_agent_root.join("runtime-events.jsonl"),
            r#"{"reason_codes":["routed_cases_updated","retry_attempted"],"health_reason":"no recent transport failures observed"}
invalid-json-line
{"reason_codes":["routed_cases_updated"],"health_reason":"no recent transport failures observed"}
"#,
        )
        .expect("write multi-agent events");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.multi_agent_state_dir = multi_agent_root;

        let report = collect_multi_agent_status_report(&cli).expect("collect status report");
        assert_eq!(report.health_state, TransportHealthState::Healthy.as_str());
        assert_eq!(report.rollout_gate, "pass");
        assert_eq!(report.processed_case_count, 2);
        assert_eq!(report.routed_case_count, 2);
        assert_eq!(report.phase_counts.get("planner"), Some(&1));
        assert_eq!(report.phase_counts.get("review"), Some(&1));
        assert_eq!(report.selected_role_counts.get("planner"), Some(&1));
        assert_eq!(report.selected_role_counts.get("reviewer"), Some(&1));
        assert_eq!(report.category_counts.get("planning"), Some(&1));
        assert_eq!(report.category_counts.get("review"), Some(&1));
        assert_eq!(report.cycle_reports, 2);
        assert_eq!(report.invalid_cycle_reports, 1);
        assert_eq!(
            report.last_reason_codes,
            vec!["routed_cases_updated".to_string()]
        );
        assert_eq!(
            report.reason_code_counts.get("routed_cases_updated"),
            Some(&2)
        );
        assert_eq!(report.reason_code_counts.get("retry_attempted"), Some(&1));
        let rendered = render_multi_agent_status_report(&report);
        assert!(rendered.contains("multi-agent status inspect:"));
        assert!(rendered.contains("rollout_gate=pass"));
        assert!(rendered.contains("phase_counts=planner:1,review:1"));
        assert!(rendered.contains("reason_code_counts=retry_attempted:1,routed_cases_updated:2"));
    }

    #[test]
    fn regression_collect_multi_agent_status_report_handles_missing_events_log() {
        let temp = tempdir().expect("tempdir");
        let multi_agent_root = temp.path().join("multi-agent");
        std::fs::create_dir_all(&multi_agent_root).expect("create multi-agent dir");
        std::fs::write(
            multi_agent_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "routed_cases": [],
  "health": {
    "updated_unix_ms": 711,
    "cycle_duration_ms": 22,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 2,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 0,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write multi-agent state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.multi_agent_state_dir = multi_agent_root;

        let report = collect_multi_agent_status_report(&cli).expect("collect status report");
        assert!(!report.events_log_present);
        assert_eq!(report.cycle_reports, 0);
        assert_eq!(report.invalid_cycle_reports, 0);
        assert!(report.last_reason_codes.is_empty());
        assert!(report.reason_code_counts.is_empty());
        assert_eq!(report.health_state, TransportHealthState::Degraded.as_str());
        assert_eq!(report.rollout_gate, "hold");
    }

    #[test]
    fn functional_collect_gateway_status_report_reads_state_and_cycle_reports() {
        let temp = tempdir().expect("tempdir");
        let gateway_root = temp.path().join("gateway");
        std::fs::create_dir_all(&gateway_root).expect("create gateway dir");
        std::fs::write(
            gateway_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": ["POST:/v1/tasks:gateway-success", "GET:/v1/tasks/42:gateway-retryable"],
  "requests": [
    {
      "case_key": "POST:/v1/tasks:gateway-success",
      "case_id": "gateway-success",
      "method": "POST",
      "endpoint": "/v1/tasks",
      "actor_id": "ops-bot",
      "status_code": 201,
      "outcome": "success",
      "error_code": "",
      "response_body": {"status":"accepted"},
      "updated_unix_ms": 1
    },
    {
      "case_key": "GET:/v1/tasks/42:gateway-retryable",
      "case_id": "gateway-retryable",
      "method": "get",
      "endpoint": "/v1/tasks/42",
      "actor_id": "ops-bot",
      "status_code": 503,
      "outcome": "retryable_failure",
      "error_code": "gateway_backend_unavailable",
      "response_body": {"status":"retryable"},
      "updated_unix_ms": 2
    }
  ],
  "health": {
    "updated_unix_ms": 740,
    "cycle_duration_ms": 18,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  },
  "guardrail": {
    "gate": "pass",
    "reason_code": "guardrail_checks_passing",
    "failure_streak_threshold": 2,
    "retryable_failures_threshold": 2,
    "failure_streak": 0,
    "last_failed_cases": 0,
    "last_retryable_failures": 1,
    "updated_unix_ms": 750
  }
}
"#,
        )
        .expect("write gateway state");
        std::fs::write(
            gateway_root.join("runtime-events.jsonl"),
            r#"{"reason_codes":["healthy_cycle","retry_attempted"],"health_reason":"no recent transport failures observed"}
invalid-json-line
{"reason_codes":["healthy_cycle","duplicate_cases_skipped"],"health_reason":"no recent transport failures observed"}
"#,
        )
        .expect("write gateway events");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.gateway_state_dir = gateway_root;

        let report = collect_gateway_status_report(&cli).expect("collect gateway status report");
        assert_eq!(report.health_state, TransportHealthState::Healthy.as_str());
        assert_eq!(report.rollout_gate, "pass");
        assert_eq!(report.rollout_reason_code, "guardrail_checks_passing");
        assert_eq!(report.service_status, "running");
        assert_eq!(report.guardrail_gate, "pass");
        assert_eq!(report.guardrail_reason_code, "guardrail_checks_passing");
        assert_eq!(report.processed_case_count, 2);
        assert_eq!(report.request_count, 2);
        assert_eq!(report.method_counts.get("GET"), Some(&1));
        assert_eq!(report.method_counts.get("POST"), Some(&1));
        assert_eq!(report.status_code_counts.get("201"), Some(&1));
        assert_eq!(report.status_code_counts.get("503"), Some(&1));
        assert_eq!(report.guardrail_failure_streak_threshold, 2);
        assert_eq!(report.guardrail_retryable_failures_threshold, 2);
        assert_eq!(report.guardrail_failure_streak, 0);
        assert_eq!(report.guardrail_last_failed_cases, 0);
        assert_eq!(report.guardrail_last_retryable_failures, 1);
        assert_eq!(
            report.error_code_counts.get("gateway_backend_unavailable"),
            Some(&1)
        );
        assert_eq!(report.cycle_reports, 2);
        assert_eq!(report.invalid_cycle_reports, 1);
        assert_eq!(
            report.last_reason_codes,
            vec![
                "healthy_cycle".to_string(),
                "duplicate_cases_skipped".to_string()
            ]
        );
        assert_eq!(report.reason_code_counts.get("healthy_cycle"), Some(&2));
        assert_eq!(report.reason_code_counts.get("retry_attempted"), Some(&1));
        assert_eq!(
            report.reason_code_counts.get("duplicate_cases_skipped"),
            Some(&1)
        );

        let rendered = render_gateway_status_report(&report);
        assert!(rendered.contains("gateway status inspect:"));
        assert!(rendered.contains("rollout_gate=pass"));
        assert!(rendered.contains("rollout_reason_code=guardrail_checks_passing"));
        assert!(rendered.contains("method_counts=GET:1,POST:1"));
        assert!(rendered.contains("status_code_counts=201:1,503:1"));
        assert!(rendered.contains("guardrail_failure_streak_threshold=2"));
        assert!(rendered.contains("guardrail_retryable_failures_threshold=2"));
        assert!(rendered.contains(
            "reason_code_counts=duplicate_cases_skipped:1,healthy_cycle:2,retry_attempted:1"
        ));
    }

    #[test]
    fn regression_collect_gateway_status_report_handles_missing_events_log() {
        let temp = tempdir().expect("tempdir");
        let gateway_root = temp.path().join("gateway");
        std::fs::create_dir_all(&gateway_root).expect("create gateway dir");
        std::fs::write(
            gateway_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "requests": [],
  "health": {
    "updated_unix_ms": 741,
    "cycle_duration_ms": 20,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 2,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 0,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write gateway state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.gateway_state_dir = gateway_root;

        let report = collect_gateway_status_report(&cli).expect("collect gateway status report");
        assert!(!report.events_log_present);
        assert_eq!(report.cycle_reports, 0);
        assert_eq!(report.invalid_cycle_reports, 0);
        assert!(report.last_reason_codes.is_empty());
        assert!(report.reason_code_counts.is_empty());
        assert_eq!(report.health_state, TransportHealthState::Degraded.as_str());
        assert_eq!(report.rollout_gate, "hold");
        assert_eq!(report.rollout_reason_code, "health_state_not_healthy");
        assert_eq!(report.service_status, "running");
        assert_eq!(report.guardrail_gate, "hold");
        assert_eq!(report.guardrail_reason_code, "health_state_not_healthy");
        assert_eq!(report.guardrail_failure_streak_threshold, 0);
        assert_eq!(report.guardrail_retryable_failures_threshold, 0);
        assert_eq!(report.guardrail_failure_streak, 0);
        assert_eq!(report.guardrail_last_failed_cases, 0);
        assert_eq!(report.guardrail_last_retryable_failures, 0);
    }

    #[test]
    fn integration_collect_gateway_status_report_service_stop_forces_rollout_hold() {
        let temp = tempdir().expect("tempdir");
        let gateway_root = temp.path().join("gateway");
        std::fs::create_dir_all(&gateway_root).expect("create gateway dir");
        std::fs::write(
            gateway_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "requests": [],
  "health": {
    "updated_unix_ms": 901,
    "cycle_duration_ms": 11,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  },
  "guardrail": {
    "gate": "pass",
    "reason_code": "guardrail_checks_passing",
    "failure_streak_threshold": 2,
    "retryable_failures_threshold": 2,
    "failure_streak": 0,
    "last_failed_cases": 0,
    "last_retryable_failures": 0,
    "updated_unix_ms": 902
  },
  "service": {
    "status": "stopped",
    "startup_attempts": 3,
    "startup_failure_streak": 0,
    "last_startup_error": "",
    "last_started_unix_ms": 850,
    "last_stopped_unix_ms": 900,
    "last_transition_unix_ms": 900,
    "last_stop_reason": "maintenance_window"
  }
}
"#,
        )
        .expect("write gateway state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.gateway_state_dir = gateway_root;

        let report = collect_gateway_status_report(&cli).expect("collect gateway status report");
        assert_eq!(report.service_status, "stopped");
        assert_eq!(report.rollout_gate, "hold");
        assert_eq!(report.rollout_reason_code, "service_stopped");
        assert_eq!(report.guardrail_gate, "pass");
        assert_eq!(report.guardrail_reason_code, "guardrail_checks_passing");
        assert_eq!(report.service_last_stop_reason, "maintenance_window");
        assert_eq!(report.service_startup_attempts, 3);

        let rendered = render_gateway_status_report(&report);
        assert!(rendered.contains("service_status=stopped"));
        assert!(rendered.contains("rollout_reason_code=service_stopped"));
        assert!(rendered.contains("guardrail_gate=pass"));
    }

    #[test]
    fn functional_collect_custom_command_status_report_reads_state_and_cycle_reports() {
        let temp = tempdir().expect("tempdir");
        let custom_command_root = temp.path().join("custom-command");
        std::fs::create_dir_all(&custom_command_root).expect("create custom-command dir");
        std::fs::write(
            custom_command_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": ["CREATE:deploy_release:create-1", "RUN:deploy_release:run-1"],
  "commands": [
    {
      "case_key": "CREATE:deploy_release:create-1",
      "case_id": "create-1",
      "command_name": "deploy_release",
      "template": "deploy {{env}}",
      "operation": "CREATE",
      "last_status_code": 201,
      "last_outcome": "success",
      "run_count": 2,
      "updated_unix_ms": 10
    },
    {
      "case_key": "UPDATE:triage_alerts:update-1",
      "case_id": "update-1",
      "command_name": "triage_alerts",
      "template": "triage {{severity}}",
      "operation": "UPDATE",
      "last_status_code": 200,
      "last_outcome": "success",
      "run_count": 1,
      "updated_unix_ms": 20
    }
  ],
  "health": {
    "updated_unix_ms": 750,
    "cycle_duration_ms": 15,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write custom-command state");
        std::fs::write(
            custom_command_root.join("runtime-events.jsonl"),
            r#"{"reason_codes":["command_registry_mutated","command_runs_recorded"],"health_reason":"no recent transport failures observed"}
invalid-json-line
{"reason_codes":["healthy_cycle","duplicate_cases_skipped"],"health_reason":"no recent transport failures observed"}
"#,
        )
        .expect("write custom-command events");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.custom_command_state_dir = custom_command_root;

        let report = collect_custom_command_status_report(&cli)
            .expect("collect custom-command status report");
        assert_eq!(report.health_state, TransportHealthState::Healthy.as_str());
        assert_eq!(report.rollout_gate, "pass");
        assert_eq!(report.processed_case_count, 2);
        assert_eq!(report.command_count, 2);
        assert_eq!(report.command_name_counts.get("deploy_release"), Some(&1));
        assert_eq!(report.command_name_counts.get("triage_alerts"), Some(&1));
        assert_eq!(report.operation_counts.get("CREATE"), Some(&1));
        assert_eq!(report.operation_counts.get("UPDATE"), Some(&1));
        assert_eq!(report.outcome_counts.get("success"), Some(&2));
        assert_eq!(report.status_code_counts.get("200"), Some(&1));
        assert_eq!(report.status_code_counts.get("201"), Some(&1));
        assert_eq!(report.total_run_count, 3);
        assert_eq!(report.cycle_reports, 2);
        assert_eq!(report.invalid_cycle_reports, 1);
        assert_eq!(
            report.last_reason_codes,
            vec![
                "healthy_cycle".to_string(),
                "duplicate_cases_skipped".to_string()
            ]
        );
        assert_eq!(report.reason_code_counts.get("healthy_cycle"), Some(&1));
        assert_eq!(
            report.reason_code_counts.get("command_registry_mutated"),
            Some(&1)
        );
        assert_eq!(
            report.reason_code_counts.get("command_runs_recorded"),
            Some(&1)
        );
        assert_eq!(
            report.reason_code_counts.get("duplicate_cases_skipped"),
            Some(&1)
        );

        let rendered = render_custom_command_status_report(&report);
        assert!(rendered.contains("custom-command status inspect:"));
        assert!(rendered.contains("rollout_gate=pass"));
        assert!(rendered.contains("command_name_counts=deploy_release:1,triage_alerts:1"));
        assert!(rendered.contains("operation_counts=CREATE:1,UPDATE:1"));
        assert!(rendered.contains("outcome_counts=success:2"));
        assert!(rendered.contains("status_code_counts=200:1,201:1"));
        assert!(rendered.contains("total_run_count=3"));
        assert!(rendered.contains(
            "reason_code_counts=command_registry_mutated:1,command_runs_recorded:1,duplicate_cases_skipped:1,healthy_cycle:1"
        ));
    }

    #[test]
    fn regression_collect_custom_command_status_report_handles_missing_events_log() {
        let temp = tempdir().expect("tempdir");
        let custom_command_root = temp.path().join("custom-command");
        std::fs::create_dir_all(&custom_command_root).expect("create custom-command dir");
        std::fs::write(
            custom_command_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "commands": [],
  "health": {
    "updated_unix_ms": 751,
    "cycle_duration_ms": 17,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 2,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 0,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write custom-command state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.custom_command_state_dir = custom_command_root;

        let report = collect_custom_command_status_report(&cli)
            .expect("collect custom-command status report");
        assert!(!report.events_log_present);
        assert_eq!(report.cycle_reports, 0);
        assert_eq!(report.invalid_cycle_reports, 0);
        assert!(report.last_reason_codes.is_empty());
        assert!(report.reason_code_counts.is_empty());
        assert_eq!(report.health_state, TransportHealthState::Degraded.as_str());
        assert_eq!(report.rollout_gate, "hold");
    }

    #[test]
    fn functional_collect_voice_status_report_reads_state_and_cycle_reports() {
        let temp = tempdir().expect("tempdir");
        let voice_root = temp.path().join("voice");
        std::fs::create_dir_all(&voice_root).expect("create voice dir");
        std::fs::write(
            voice_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": ["turn:tau:ops-1:voice-success-turn", "wake_word:tau:ops-2:voice-wake"],
  "interactions": [
    {
      "case_key": "turn:tau:ops-1:voice-success-turn",
      "case_id": "voice-success-turn",
      "mode": "turn",
      "wake_word": "tau",
      "locale": "en-US",
      "speaker_id": "ops-1",
      "utterance": "open dashboard",
      "last_status_code": 202,
      "last_outcome": "success",
      "run_count": 2,
      "updated_unix_ms": 10
    },
    {
      "case_key": "wake_word:tau:ops-2:voice-wake",
      "case_id": "voice-wake",
      "mode": "wake_word",
      "wake_word": "tau",
      "locale": "en-US",
      "speaker_id": "ops-2",
      "utterance": "",
      "last_status_code": 202,
      "last_outcome": "success",
      "run_count": 1,
      "updated_unix_ms": 20
    }
  ],
  "health": {
    "updated_unix_ms": 760,
    "cycle_duration_ms": 14,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write voice state");
        std::fs::write(
            voice_root.join("runtime-events.jsonl"),
            r#"{"reason_codes":["turns_handled","wake_word_detected"],"health_reason":"no recent transport failures observed"}
invalid-json-line
{"reason_codes":["healthy_cycle","duplicate_cases_skipped"],"health_reason":"no recent transport failures observed"}
"#,
        )
        .expect("write voice events");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.voice_state_dir = voice_root;

        let report = collect_voice_status_report(&cli).expect("collect voice status report");
        assert_eq!(report.health_state, TransportHealthState::Healthy.as_str());
        assert_eq!(report.rollout_gate, "pass");
        assert_eq!(report.processed_case_count, 2);
        assert_eq!(report.interaction_count, 2);
        assert_eq!(report.mode_counts.get("turn"), Some(&1));
        assert_eq!(report.mode_counts.get("wake_word"), Some(&1));
        assert_eq!(report.speaker_counts.get("ops-1"), Some(&1));
        assert_eq!(report.speaker_counts.get("ops-2"), Some(&1));
        assert_eq!(report.outcome_counts.get("success"), Some(&2));
        assert_eq!(report.status_code_counts.get("202"), Some(&2));
        assert_eq!(report.utterance_count, 1);
        assert_eq!(report.total_run_count, 3);
        assert_eq!(report.cycle_reports, 2);
        assert_eq!(report.invalid_cycle_reports, 1);
        assert_eq!(
            report.last_reason_codes,
            vec![
                "healthy_cycle".to_string(),
                "duplicate_cases_skipped".to_string()
            ]
        );
        assert_eq!(report.reason_code_counts.get("turns_handled"), Some(&1));
        assert_eq!(
            report.reason_code_counts.get("wake_word_detected"),
            Some(&1)
        );
        assert_eq!(report.reason_code_counts.get("healthy_cycle"), Some(&1));
        assert_eq!(
            report.reason_code_counts.get("duplicate_cases_skipped"),
            Some(&1)
        );

        let rendered = render_voice_status_report(&report);
        assert!(rendered.contains("voice status inspect:"));
        assert!(rendered.contains("rollout_gate=pass"));
        assert!(rendered.contains("mode_counts=turn:1,wake_word:1"));
        assert!(rendered.contains("speaker_counts=ops-1:1,ops-2:1"));
        assert!(rendered.contains("outcome_counts=success:2"));
        assert!(rendered.contains("status_code_counts=202:2"));
        assert!(rendered.contains("utterance_count=1"));
        assert!(rendered.contains("total_run_count=3"));
        assert!(rendered.contains(
            "reason_code_counts=duplicate_cases_skipped:1,healthy_cycle:1,turns_handled:1,wake_word_detected:1"
        ));
    }

    #[test]
    fn regression_collect_voice_status_report_handles_missing_events_log() {
        let temp = tempdir().expect("tempdir");
        let voice_root = temp.path().join("voice");
        std::fs::create_dir_all(&voice_root).expect("create voice dir");
        std::fs::write(
            voice_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "interactions": [],
  "health": {
    "updated_unix_ms": 761,
    "cycle_duration_ms": 20,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 2,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 0,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write voice state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.voice_state_dir = voice_root;

        let report = collect_voice_status_report(&cli).expect("collect voice status report");
        assert!(!report.events_log_present);
        assert_eq!(report.cycle_reports, 0);
        assert_eq!(report.invalid_cycle_reports, 0);
        assert!(report.last_reason_codes.is_empty());
        assert!(report.reason_code_counts.is_empty());
        assert_eq!(report.health_state, TransportHealthState::Degraded.as_str());
        assert_eq!(report.rollout_gate, "hold");
    }

    #[test]
    fn functional_collect_deployment_status_report_reads_state_and_cycle_reports() {
        let temp = tempdir().expect("tempdir");
        let deployment_root = temp.path().join("deployment");
        std::fs::create_dir_all(&deployment_root).expect("create deployment dir");
        std::fs::write(
            deployment_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": ["container:staging-container:deployment-container-staging", "wasm:edge-wasm:deployment-wasm-edge"],
  "rollouts": [
    {
      "deploy_target": "container",
      "runtime_profile": "native",
      "environment": "staging",
      "status_code": 202,
      "outcome": "success",
      "error_code": "",
      "replicas": 1
    },
    {
      "deploy_target": "wasm",
      "runtime_profile": "wasm_wasi",
      "environment": "production",
      "status_code": 201,
      "outcome": "success",
      "error_code": "",
      "replicas": 2
    }
  ],
  "health": {
    "updated_unix_ms": 780,
    "cycle_duration_ms": 16,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write deployment state");
        std::fs::write(
            deployment_root.join("runtime-events.jsonl"),
            r#"{"reason_codes":["cloud_rollout_applied","wasm_rollout_applied"],"health_reason":"no recent transport failures observed"}
invalid-json-line
{"reason_codes":["healthy_cycle"],"health_reason":"no recent transport failures observed"}
"#,
        )
        .expect("write deployment events");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.deployment_state_dir = deployment_root;

        let report = collect_deployment_status_report(&cli).expect("collect deployment status");
        assert_eq!(report.health_state, TransportHealthState::Healthy.as_str());
        assert_eq!(report.rollout_gate, "pass");
        assert_eq!(report.processed_case_count, 2);
        assert_eq!(report.rollout_count, 2);
        assert_eq!(report.target_counts.get("container"), Some(&1));
        assert_eq!(report.target_counts.get("wasm"), Some(&1));
        assert_eq!(report.runtime_profile_counts.get("native"), Some(&1));
        assert_eq!(report.runtime_profile_counts.get("wasm_wasi"), Some(&1));
        assert_eq!(report.environment_counts.get("staging"), Some(&1));
        assert_eq!(report.environment_counts.get("production"), Some(&1));
        assert_eq!(report.outcome_counts.get("success"), Some(&2));
        assert_eq!(report.status_code_counts.get("201"), Some(&1));
        assert_eq!(report.status_code_counts.get("202"), Some(&1));
        assert_eq!(report.total_replicas, 3);
        assert_eq!(report.wasm_rollout_count, 1);
        assert_eq!(report.cloud_rollout_count, 1);
        assert_eq!(report.cycle_reports, 2);
        assert_eq!(report.invalid_cycle_reports, 1);
        assert_eq!(report.last_reason_codes, vec!["healthy_cycle".to_string()]);
        assert_eq!(report.reason_code_counts.get("healthy_cycle"), Some(&1));
        assert_eq!(
            report.reason_code_counts.get("cloud_rollout_applied"),
            Some(&1)
        );
        assert_eq!(
            report.reason_code_counts.get("wasm_rollout_applied"),
            Some(&1)
        );

        let rendered = render_deployment_status_report(&report);
        assert!(rendered.contains("deployment status inspect:"));
        assert!(rendered.contains("rollout_gate=pass"));
        assert!(rendered.contains("target_counts=container:1,wasm:1"));
        assert!(rendered.contains("runtime_profile_counts=native:1,wasm_wasi:1"));
        assert!(rendered.contains("total_replicas=3"));
        assert!(rendered.contains("wasm_rollout_count=1"));
        assert!(rendered.contains("cloud_rollout_count=1"));
        assert!(rendered.contains(
            "reason_code_counts=cloud_rollout_applied:1,healthy_cycle:1,wasm_rollout_applied:1"
        ));
    }

    #[test]
    fn regression_collect_deployment_status_report_handles_missing_events_log() {
        let temp = tempdir().expect("tempdir");
        let deployment_root = temp.path().join("deployment");
        std::fs::create_dir_all(&deployment_root).expect("create deployment dir");
        std::fs::write(
            deployment_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "rollouts": [],
  "health": {
    "updated_unix_ms": 781,
    "cycle_duration_ms": 20,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 2,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 0,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write deployment state");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.deployment_state_dir = deployment_root;

        let report = collect_deployment_status_report(&cli).expect("collect deployment status");
        assert!(!report.events_log_present);
        assert_eq!(report.cycle_reports, 0);
        assert_eq!(report.invalid_cycle_reports, 0);
        assert!(report.last_reason_codes.is_empty());
        assert!(report.reason_code_counts.is_empty());
        assert_eq!(report.health_state, TransportHealthState::Degraded.as_str());
        assert_eq!(report.rollout_gate, "hold");
    }

    #[test]
    fn unit_operator_health_state_rank_orders_expected_states() {
        assert_eq!(operator_health_state_rank("healthy"), 0);
        assert_eq!(operator_health_state_rank("degraded"), 1);
        assert_eq!(operator_health_state_rank("failing"), 2);
        assert_eq!(operator_health_state_rank("unknown"), 2);
    }

    #[test]
    fn unit_operator_control_summary_diff_classifies_regression() {
        let baseline = operator_summary_fixture(OperatorSummaryFixtureInput {
            generated_unix_ms: 1,
            health_state: "healthy",
            rollout_gate: "pass",
            reason_code: "all_checks_passing",
            recommendation: "no_immediate_action_required",
            component_health_state: "healthy",
            component_rollout_gate: "pass",
            queue_depth: 0,
            failure_streak: 0,
        });
        let current = operator_summary_fixture(OperatorSummaryFixtureInput {
            generated_unix_ms: 2,
            health_state: "failing",
            rollout_gate: "hold",
            reason_code: "gateway:state_unavailable",
            recommendation: "initialize gateway state",
            component_health_state: "failing",
            component_rollout_gate: "hold",
            queue_depth: 4,
            failure_streak: 2,
        });

        let diff = build_operator_control_summary_diff_report(&baseline, &current);
        assert_eq!(diff.drift_state, "regressed");
        assert_eq!(diff.risk_level, "high");
        assert_eq!(diff.health_state_before, "healthy");
        assert_eq!(diff.health_state_after, "failing");
        assert_eq!(diff.rollout_gate_before, "pass");
        assert_eq!(diff.rollout_gate_after, "hold");
        assert_eq!(diff.changed_components.len(), 1);
        let gateway = &diff.changed_components[0];
        assert_eq!(gateway.component, "gateway");
        assert_eq!(gateway.drift_state, "regressed");
        assert_eq!(gateway.severity, "high");
        assert_eq!(gateway.queue_depth_before, 0);
        assert_eq!(gateway.queue_depth_after, 4);
        assert_eq!(gateway.failure_streak_before, 0);
        assert_eq!(gateway.failure_streak_after, 2);
    }

    #[test]
    fn functional_operator_control_summary_diff_render_includes_component_deltas() {
        let baseline = operator_summary_fixture(OperatorSummaryFixtureInput {
            generated_unix_ms: 10,
            health_state: "healthy",
            rollout_gate: "pass",
            reason_code: "all_checks_passing",
            recommendation: "no_immediate_action_required",
            component_health_state: "healthy",
            component_rollout_gate: "pass",
            queue_depth: 0,
            failure_streak: 0,
        });
        let current = operator_summary_fixture(OperatorSummaryFixtureInput {
            generated_unix_ms: 11,
            health_state: "degraded",
            rollout_gate: "hold",
            reason_code: "gateway:service_stopped",
            recommendation: "start gateway service mode",
            component_health_state: "degraded",
            component_rollout_gate: "hold",
            queue_depth: 1,
            failure_streak: 1,
        });
        let diff = build_operator_control_summary_diff_report(&baseline, &current);
        let rendered = render_operator_control_summary_diff_report(&diff);

        assert!(rendered.contains("operator control summary diff:"));
        assert!(rendered.contains("drift_state=regressed"));
        assert!(rendered.contains("operator control summary diff component: component=gateway"));
        assert!(rendered.contains("reason_code_after=gateway:service_stopped"));
    }

    #[test]
    fn functional_operator_control_summary_render_includes_expected_sections() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli(&["tau-rs"]);
        cli.channel_store_root = temp.path().join("channel-store");
        cli.dashboard_state_dir = temp.path().join("dashboard");
        cli.multi_channel_state_dir = temp.path().join("multi-channel");
        cli.multi_agent_state_dir = temp.path().join("multi-agent");
        cli.gateway_state_dir = temp.path().join("gateway");
        cli.deployment_state_dir = temp.path().join("deployment");
        cli.custom_command_state_dir = temp.path().join("custom-command");
        cli.voice_state_dir = temp.path().join("voice");
        cli.daemon_state_dir = temp.path().join("daemon");

        let report = collect_operator_control_summary_report(&cli).expect("collect summary");
        let rendered = render_operator_control_summary_report(&report);

        assert!(rendered.contains("operator control summary:"));
        assert!(rendered.contains("operator control policy posture:"));
        assert!(rendered.contains("operator control daemon:"));
        assert!(rendered.contains("operator control release channel:"));
        assert!(rendered.contains("operator control component: component=gateway"));
    }

    #[test]
    fn integration_operator_control_summary_reflects_persisted_runtime_state_snapshots() {
        let temp = tempdir().expect("tempdir");
        let dashboard_root = temp.path().join("dashboard");
        let multi_channel_root = temp.path().join("multi-channel");
        let multi_agent_root = temp.path().join("multi-agent");
        let gateway_root = temp.path().join("gateway");
        let deployment_root = temp.path().join("deployment");
        let custom_command_root = temp.path().join("custom-command");
        let voice_root = temp.path().join("voice");
        std::fs::create_dir_all(&dashboard_root).expect("create dashboard dir");
        std::fs::create_dir_all(&multi_channel_root).expect("create multi-channel dir");
        std::fs::create_dir_all(&multi_agent_root).expect("create multi-agent dir");
        std::fs::create_dir_all(&gateway_root).expect("create gateway dir");
        std::fs::create_dir_all(&deployment_root).expect("create deployment dir");
        std::fs::create_dir_all(&custom_command_root).expect("create custom-command dir");
        std::fs::create_dir_all(&voice_root).expect("create voice dir");

        std::fs::write(
            dashboard_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": ["dashboard:1"],
  "widget_views": [],
  "control_audit": [],
  "health": {
    "updated_unix_ms": 100,
    "cycle_duration_ms": 10,
    "queue_depth": 1,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write dashboard state");
        std::fs::write(
            multi_channel_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_event_keys": ["telegram:1"],
  "health": {
    "updated_unix_ms": 101,
    "cycle_duration_ms": 11,
    "queue_depth": 2,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write multi-channel state");
        std::fs::write(
            multi_agent_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": ["multi-agent:1"],
  "routed_cases": [],
  "health": {
    "updated_unix_ms": 102,
    "cycle_duration_ms": 12,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write multi-agent state");
        std::fs::write(
            gateway_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": ["gateway:1"],
  "requests": [],
  "health": {
    "updated_unix_ms": 103,
    "cycle_duration_ms": 13,
    "queue_depth": 3,
    "active_runs": 1,
    "failure_streak": 1,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 0,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write gateway state");
        std::fs::write(
            deployment_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": ["deployment:1"],
  "rollouts": [],
  "health": {
    "updated_unix_ms": 104,
    "cycle_duration_ms": 14,
    "queue_depth": 4,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write deployment state");
        std::fs::write(
            custom_command_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": ["custom-command:1"],
  "commands": [],
  "health": {
    "updated_unix_ms": 105,
    "cycle_duration_ms": 15,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write custom-command state");
        std::fs::write(
            voice_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": ["voice:1"],
  "interactions": [],
  "health": {
    "updated_unix_ms": 106,
    "cycle_duration_ms": 16,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write voice state");

        let daemon_state_dir = temp.path().join("daemon");
        let daemon_config = crate::daemon_runtime::TauDaemonConfig {
            state_dir: daemon_state_dir.clone(),
            profile: crate::CliDaemonProfile::Auto,
        };
        crate::daemon_runtime::install_tau_daemon(&daemon_config).expect("install daemon");
        crate::daemon_runtime::start_tau_daemon(&daemon_config).expect("start daemon");

        let mut cli = parse_cli(&["tau-rs"]);
        cli.channel_store_root = temp.path().join("channel-store");
        cli.dashboard_state_dir = dashboard_root;
        cli.multi_channel_state_dir = multi_channel_root;
        cli.multi_agent_state_dir = multi_agent_root;
        cli.gateway_state_dir = gateway_root;
        cli.deployment_state_dir = deployment_root;
        cli.custom_command_state_dir = custom_command_root;
        cli.voice_state_dir = voice_root;
        cli.daemon_state_dir = daemon_state_dir;

        let report = collect_operator_control_summary_report(&cli).expect("collect summary");
        let gateway_row = report
            .components
            .iter()
            .find(|row| row.component == "gateway")
            .expect("gateway row");
        let deployment_row = report
            .components
            .iter()
            .find(|row| row.component == "deployment")
            .expect("deployment row");
        assert_eq!(gateway_row.queue_depth, 3);
        assert_eq!(gateway_row.failure_streak, 1);
        assert_eq!(deployment_row.queue_depth, 4);
        assert!(report.daemon.running);
        assert_eq!(report.daemon.rollout_gate, "pass");
    }

    #[test]
    fn integration_operator_control_summary_snapshot_roundtrip_and_compare() {
        let temp = tempdir().expect("tempdir");
        let baseline_path = temp.path().join("operator-control-baseline.json");
        let baseline = operator_summary_fixture(OperatorSummaryFixtureInput {
            generated_unix_ms: 100,
            health_state: "healthy",
            rollout_gate: "pass",
            reason_code: "all_checks_passing",
            recommendation: "no_immediate_action_required",
            component_health_state: "healthy",
            component_rollout_gate: "pass",
            queue_depth: 0,
            failure_streak: 0,
        });
        save_operator_control_summary_snapshot(&baseline_path, &baseline).expect("save snapshot");
        let loaded = load_operator_control_summary_snapshot(&baseline_path).expect("load snapshot");
        assert_eq!(loaded, baseline);

        let current = operator_summary_fixture(OperatorSummaryFixtureInput {
            generated_unix_ms: 101,
            health_state: "degraded",
            rollout_gate: "hold",
            reason_code: "gateway:state_unavailable",
            recommendation: "initialize gateway state",
            component_health_state: "degraded",
            component_rollout_gate: "hold",
            queue_depth: 2,
            failure_streak: 1,
        });
        let diff = build_operator_control_summary_diff_report(&loaded, &current);
        assert_eq!(diff.drift_state, "regressed");
        assert_eq!(diff.changed_components.len(), 1);
    }

    #[test]
    fn regression_operator_control_summary_handles_missing_state_files_with_explicit_defaults() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli(&["tau-rs"]);
        cli.channel_store_root = temp.path().join("channel-store");
        cli.dashboard_state_dir = temp.path().join("dashboard");
        cli.multi_channel_state_dir = temp.path().join("multi-channel");
        cli.multi_agent_state_dir = temp.path().join("multi-agent");
        cli.gateway_state_dir = temp.path().join("gateway");
        cli.deployment_state_dir = temp.path().join("deployment");
        cli.custom_command_state_dir = temp.path().join("custom-command");
        cli.voice_state_dir = temp.path().join("voice");
        cli.daemon_state_dir = temp.path().join("daemon");

        let report = collect_operator_control_summary_report(&cli).expect("collect summary");
        assert_eq!(report.rollout_gate, "hold");
        assert!(report
            .components
            .iter()
            .all(|row| row.reason_code == "state_unavailable"));
        assert_eq!(report.health_state, "failing");

        let rendered = render_operator_control_summary_report(&report);
        assert!(rendered.contains("operator control summary:"));
        assert!(rendered.contains("reason_code=state_unavailable"));
    }

    #[test]
    fn regression_operator_control_summary_snapshot_load_fails_closed_for_missing_file() {
        let temp = tempdir().expect("tempdir");
        let missing = temp.path().join("missing-summary.json");
        let error = load_operator_control_summary_snapshot(&missing)
            .expect_err("missing snapshot must fail");
        assert!(error
            .to_string()
            .contains("failed to read operator control summary snapshot"));
    }
}
