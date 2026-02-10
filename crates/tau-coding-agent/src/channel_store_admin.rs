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
struct MultiAgentStatusStateFile {
    #[serde(default)]
    processed_case_keys: Vec<String>,
    #[serde(default)]
    routed_cases: Vec<MultiAgentStatusRoutedCase>,
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
struct DashboardCycleReportLine {
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DashboardCycleReportSummary {
    events_log_present: bool,
    cycle_reports: usize,
    invalid_cycle_reports: usize,
    last_reason_codes: Vec<String>,
    last_health_reason: String,
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
            "invalid --transport-health-inspect '{}', expected slack, github, github:owner/repo, multi-channel, multi-agent, memory, or dashboard",
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

    let Some((transport, repo_slug)) = trimmed.split_once(':') else {
        bail!(
            "invalid --transport-health-inspect '{}', expected slack, github, github:owner/repo, multi-channel, multi-agent, memory, or dashboard",
            raw
        );
    };
    if !transport.eq_ignore_ascii_case("github") {
        bail!(
            "invalid --transport-health-inspect '{}', expected slack, github, github:owner/repo, multi-channel, multi-agent, memory, or dashboard",
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

fn load_multi_agent_status_state(path: &Path) -> Result<MultiAgentStatusStateFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str::<MultiAgentStatusStateFile>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))
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

    use clap::Parser;
    use tempfile::tempdir;

    use super::{
        collect_dashboard_status_report, collect_multi_agent_status_report,
        collect_transport_health_rows, parse_transport_health_inspect_target,
        render_dashboard_status_report, render_multi_agent_status_report,
        render_transport_health_row, render_transport_health_rows, TransportHealthInspectRow,
        TransportHealthInspectTarget,
    };
    use crate::transport_health::TransportHealthState;
    use crate::Cli;
    use crate::TransportHealthSnapshot;

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
        let github_repo_dir = github_root.join("owner__repo");
        std::fs::create_dir_all(&github_repo_dir).expect("create github repo dir");
        std::fs::create_dir_all(&slack_root).expect("create slack dir");
        std::fs::create_dir_all(&multi_channel_root).expect("create multi-channel dir");
        std::fs::create_dir_all(&multi_agent_root).expect("create multi-agent dir");
        std::fs::create_dir_all(&memory_root).expect("create memory dir");
        std::fs::create_dir_all(&dashboard_root).expect("create dashboard dir");

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

        let mut cli = Cli::parse_from(["tau-rs"]);
        cli.github_state_dir = github_root;
        cli.slack_state_dir = slack_root;
        cli.multi_channel_state_dir = multi_channel_root;
        cli.multi_agent_state_dir = multi_agent_root;
        cli.memory_state_dir = memory_root;
        cli.dashboard_state_dir = dashboard_root;

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

        let rendered = render_transport_health_rows(&[
            github_rows[0].clone(),
            slack_rows[0].clone(),
            multi_channel_rows[0].clone(),
            multi_agent_rows[0].clone(),
            memory_rows[0].clone(),
            dashboard_rows[0].clone(),
        ]);
        assert!(rendered.contains("transport=github"));
        assert!(rendered.contains("transport=slack"));
        assert!(rendered.contains("transport=multi-channel"));
        assert!(rendered.contains("transport=multi-agent"));
        assert!(rendered.contains("transport=memory"));
        assert!(rendered.contains("transport=dashboard"));
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

        let mut cli = Cli::parse_from(["tau-rs"]);
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

        let mut cli = Cli::parse_from(["tau-rs"]);
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

        let mut cli = Cli::parse_from(["tau-rs"]);
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

        let mut cli = Cli::parse_from(["tau-rs"]);
        cli.dashboard_state_dir = PathBuf::from(&dashboard_root);

        let rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Dashboard)
            .expect("collect dashboard row");
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

        let mut cli = Cli::parse_from(["tau-rs"]);
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

        let mut cli = Cli::parse_from(["tau-rs"]);
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

        let mut cli = Cli::parse_from(["tau-rs"]);
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

        let mut cli = Cli::parse_from(["tau-rs"]);
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
}
