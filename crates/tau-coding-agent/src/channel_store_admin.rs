use super::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
enum TransportHealthInspectTarget {
    Slack,
    GithubAll,
    GithubRepo { owner: String, repo: String },
    MultiChannel,
    MultiAgent,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    connectors:
        Option<crate::multi_channel_live_connectors::MultiChannelLiveConnectorsStatusReport>,
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
            "invalid --transport-health-inspect '{}', expected slack, github, github:owner/repo, multi-channel, multi-agent, memory, dashboard, gateway, deployment, custom-command, or voice",
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
            "invalid --transport-health-inspect '{}', expected slack, github, github:owner/repo, multi-channel, multi-agent, memory, dashboard, gateway, deployment, custom-command, or voice",
            raw
        );
    };
    if !transport.eq_ignore_ascii_case("github") {
        bail!(
            "invalid --transport-health-inspect '{}', expected slack, github, github:owner/repo, multi-channel, multi-agent, memory, dashboard, gateway, deployment, custom-command, or voice",
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
    let connector_summary = report
        .connectors
        .as_ref()
        .map(|connectors| {
            let mut channel_rows = Vec::new();
            for (channel, state) in &connectors.channels {
                channel_rows.push(format!(
                    "{}:{}:{}:{}:{}",
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
                    state.events_ingested,
                    state.duplicates_skipped
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
        "multi-channel status inspect: state_path={} events_log_path={} events_log_present={} health_state={} health_reason={} rollout_gate={} processed_event_count={} transport_counts={} cycle_reports={} invalid_cycle_reports={} last_reason_codes={} reason_code_counts={} queue_depth={} failure_streak={} last_cycle_failed={} last_cycle_completed={} connectors={}",
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
        collect_custom_command_status_report, collect_dashboard_status_report,
        collect_deployment_status_report, collect_gateway_status_report,
        collect_multi_agent_status_report, collect_multi_channel_status_report,
        collect_transport_health_rows, collect_voice_status_report,
        parse_transport_health_inspect_target, render_custom_command_status_report,
        render_dashboard_status_report, render_deployment_status_report,
        render_gateway_status_report, render_multi_agent_status_report,
        render_multi_channel_status_report, render_transport_health_row,
        render_transport_health_rows, render_voice_status_report, TransportHealthInspectRow,
        TransportHealthInspectTarget,
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

        let rendered = render_multi_channel_status_report(&report);
        assert!(rendered.contains("multi-channel status inspect:"));
        assert!(rendered.contains("rollout_gate=pass"));
        assert!(rendered.contains("processed_event_count=4"));
        assert!(rendered.contains("transport_counts=discord:1,telegram:2,whatsapp:1"));
        assert!(rendered.contains(
            "reason_code_counts=duplicate_events_skipped:1,events_applied:1,healthy_cycle:2"
        ));
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
}
