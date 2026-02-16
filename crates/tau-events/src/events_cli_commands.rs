use anyhow::{anyhow, bail, Context, Result};
use tau_cli::{Cli, CliEventTemplateSchedule};
use tau_core::current_unix_timestamp_ms;

use crate::{
    dry_run_events, enforce_events_dry_run_gate, inspect_events, simulate_events,
    validate_events_definitions, write_event_template, EventTemplateSchedule, EventsDryRunConfig,
    EventsDryRunGateConfig, EventsDryRunReport, EventsInspectConfig, EventsInspectReport,
    EventsSimulateConfig, EventsSimulateReport, EventsTemplateConfig, EventsValidateConfig,
    EventsValidateReport,
};

/// Execute events inspect mode and print either JSON or text report.
pub fn execute_events_inspect_command(cli: &Cli) -> Result<()> {
    let report = inspect_events(
        &EventsInspectConfig {
            events_dir: cli.events_dir.clone(),
            state_path: cli.events_state_path.clone(),
            queue_limit: cli.events_queue_limit.max(1),
            stale_immediate_max_age_seconds: cli.events_stale_immediate_max_age_seconds,
        },
        current_unix_timestamp_ms(),
    )?;

    if cli.execution_domain.events_inspect_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render events inspect json")?
        );
    } else {
        println!("{}", render_events_inspect_report(&report));
    }
    Ok(())
}

/// Execute events validate mode and fail when invalid definition files are found.
pub fn execute_events_validate_command(cli: &Cli) -> Result<()> {
    let report = validate_events_definitions(
        &EventsValidateConfig {
            events_dir: cli.events_dir.clone(),
            state_path: cli.events_state_path.clone(),
        },
        current_unix_timestamp_ms(),
    )?;

    if cli.execution_domain.events_validate_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render events validate json")?
        );
    } else {
        println!("{}", render_events_validate_report(&report));
    }

    if report.failed_files > 0 {
        bail!(
            "events validate failed: failed_files={} invalid_files={} malformed_files={}",
            report.failed_files,
            report.invalid_files,
            report.malformed_files
        );
    }
    Ok(())
}

/// Execute events template-write mode and emit generated template metadata.
pub fn execute_events_template_write_command(cli: &Cli) -> Result<()> {
    let target_path = cli
        .execution_domain
        .events_template_write
        .as_ref()
        .ok_or_else(|| anyhow!("--events-template-write is required"))?;

    let now_unix_ms = current_unix_timestamp_ms();
    let schedule = match cli.execution_domain.events_template_schedule {
        CliEventTemplateSchedule::Immediate => EventTemplateSchedule::Immediate,
        CliEventTemplateSchedule::At => EventTemplateSchedule::At,
        CliEventTemplateSchedule::Periodic => EventTemplateSchedule::Periodic,
    };

    let at_unix_ms = if matches!(
        cli.execution_domain.events_template_schedule,
        CliEventTemplateSchedule::At
    ) {
        Some(
            cli.execution_domain
                .events_template_at_unix_ms
                .unwrap_or_else(|| now_unix_ms.saturating_add(300_000)),
        )
    } else {
        None
    };

    let cron = if matches!(
        cli.execution_domain.events_template_schedule,
        CliEventTemplateSchedule::Periodic
    ) {
        Some(
            cli.execution_domain
                .events_template_cron
                .clone()
                .unwrap_or_else(|| "0 0/15 * * * * *".to_string()),
        )
    } else {
        None
    };

    let config = EventsTemplateConfig {
        target_path: target_path.to_path_buf(),
        overwrite: cli.execution_domain.events_template_overwrite,
        schedule,
        channel: cli
            .execution_domain
            .events_template_channel
            .clone()
            .unwrap_or_else(|| "slack/C123".to_string()),
        prompt: cli
            .execution_domain
            .events_template_prompt
            .clone()
            .unwrap_or_default(),
        event_id: cli.execution_domain.events_template_id.clone(),
        at_unix_ms,
        cron,
        timezone: Some(cli.execution_domain.events_template_timezone.clone()),
    };

    let report = write_event_template(&config, now_unix_ms)?;

    println!(
        "events template write: path={} schedule={} event_id={} channel={} overwrite={}",
        report.path.display(),
        report.schedule,
        report.event_id,
        report.channel,
        report.overwrite,
    );
    Ok(())
}

/// Execute events simulation mode for due/horizon posture inspection.
pub fn execute_events_simulate_command(cli: &Cli) -> Result<()> {
    let report = simulate_events(
        &EventsSimulateConfig {
            events_dir: cli.events_dir.clone(),
            state_path: cli.events_state_path.clone(),
            horizon_seconds: cli.execution_domain.events_simulate_horizon_seconds,
            stale_immediate_max_age_seconds: cli.events_stale_immediate_max_age_seconds,
        },
        current_unix_timestamp_ms(),
    )?;

    if cli.execution_domain.events_simulate_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render events simulate json")?
        );
    } else {
        println!("{}", render_events_simulate_report(&report));
    }
    Ok(())
}

/// Execute events dry-run mode and enforce configured gate thresholds.
pub fn execute_events_dry_run_command(cli: &Cli) -> Result<()> {
    let report = dry_run_events(
        &EventsDryRunConfig {
            events_dir: cli.events_dir.clone(),
            state_path: cli.events_state_path.clone(),
            queue_limit: cli.events_queue_limit.max(1),
            stale_immediate_max_age_seconds: cli.events_stale_immediate_max_age_seconds,
        },
        current_unix_timestamp_ms(),
    )?;

    if cli.execution_domain.events_dry_run_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render events dry run json")?
        );
    } else {
        println!("{}", render_events_dry_run_report(&report));
    }

    let max_error_rows = if cli.execution_domain.events_dry_run_strict {
        Some(0)
    } else {
        cli.execution_domain
            .events_dry_run_max_error_rows
            .map(|value| value as usize)
    };
    let gate_config = EventsDryRunGateConfig {
        max_error_rows,
        max_execute_rows: cli
            .execution_domain
            .events_dry_run_max_execute_rows
            .map(|value| value as usize),
    };
    enforce_events_dry_run_gate(&report, &gate_config)?;
    Ok(())
}

fn render_events_inspect_report(report: &EventsInspectReport) -> String {
    format!(
        "events inspect: events_dir={} state_path={} now_unix_ms={} discovered_events={} malformed_events={} enabled_events={} disabled_events={} due_now_events={} queued_now_events={} not_due_events={} stale_immediate_events={} due_eval_failed_events={} schedule_immediate_events={} schedule_at_events={} schedule_periodic_events={} periodic_with_last_run_state={} periodic_missing_last_run_state={} execution_history_entries={} execution_history_limit={} executed_history_entries={} failed_history_entries={} skipped_history_entries={} last_execution_unix_ms={} last_execution_reason_code={} queue_limit={} stale_immediate_max_age_seconds={}",
        report.events_dir,
        report.state_path,
        report.now_unix_ms,
        report.discovered_events,
        report.malformed_events,
        report.enabled_events,
        report.disabled_events,
        report.due_now_events,
        report.queued_now_events,
        report.not_due_events,
        report.stale_immediate_events,
        report.due_eval_failed_events,
        report.schedule_immediate_events,
        report.schedule_at_events,
        report.schedule_periodic_events,
        report.periodic_with_last_run_state,
        report.periodic_missing_last_run_state,
        report.execution_history_entries,
        report.execution_history_limit,
        report.executed_history_entries,
        report.failed_history_entries,
        report.skipped_history_entries,
        report
            .last_execution_unix_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        report
            .last_execution_reason_code
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or("none"),
        report.queue_limit,
        report.stale_immediate_max_age_seconds,
    )
}

fn render_events_validate_report(report: &EventsValidateReport) -> String {
    let mut lines = vec![format!(
        "events validate: events_dir={} state_path={} now_unix_ms={} total_files={} valid_files={} invalid_files={} malformed_files={} failed_files={} disabled_files={}",
        report.events_dir,
        report.state_path,
        report.now_unix_ms,
        report.total_files,
        report.valid_files,
        report.invalid_files,
        report.malformed_files,
        report.failed_files,
        report.disabled_files,
    )];

    for diagnostic in &report.diagnostics {
        lines.push(format!(
            "events validate error: path={} event_id={} reason_code={} message={}",
            diagnostic.path,
            diagnostic
                .event_id
                .as_deref()
                .filter(|value| !value.is_empty())
                .unwrap_or("none"),
            diagnostic.reason_code,
            diagnostic.message
        ));
    }

    lines.join("\n")
}

fn render_events_simulate_report(report: &EventsSimulateReport) -> String {
    let mut lines = vec![format!(
        "events simulate: events_dir={} state_path={} now_unix_ms={} horizon_seconds={} total_files={} simulated_rows={} malformed_files={} invalid_rows={} due_now_rows={} within_horizon_rows={}",
        report.events_dir,
        report.state_path,
        report.now_unix_ms,
        report.horizon_seconds,
        report.total_files,
        report.simulated_rows,
        report.malformed_files,
        report.invalid_rows,
        report.due_now_rows,
        report.within_horizon_rows,
    )];

    for row in &report.rows {
        lines.push(format!(
            "events simulate row: path={} event_id={} schedule={} enabled={} next_due_unix_ms={} due_now={} within_horizon={} last_run_unix_ms={} channel={}",
            row.path,
            row.event_id,
            row.schedule,
            row.enabled,
            row.next_due_unix_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.due_now,
            row.within_horizon,
            row.last_run_unix_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.channel,
        ));
    }

    for diagnostic in &report.diagnostics {
        lines.push(format!(
            "events simulate error: path={} event_id={} reason_code={} message={}",
            diagnostic.path,
            diagnostic
                .event_id
                .as_deref()
                .filter(|value| !value.is_empty())
                .unwrap_or("none"),
            diagnostic.reason_code,
            diagnostic.message
        ));
    }

    lines.join("\n")
}

fn render_events_dry_run_report(report: &EventsDryRunReport) -> String {
    let mut lines = vec![format!(
        "events dry run: events_dir={} state_path={} now_unix_ms={} queue_limit={} total_files={} evaluated_rows={} execute_rows={} skipped_rows={} error_rows={} malformed_files={}",
        report.events_dir,
        report.state_path,
        report.now_unix_ms,
        report.queue_limit,
        report.total_files,
        report.evaluated_rows,
        report.execute_rows,
        report.skipped_rows,
        report.error_rows,
        report.malformed_files,
    )];

    for row in &report.rows {
        lines.push(format!(
            "events dry run row: path={} event_id={} schedule={} enabled={} decision={} reason_code={} queue_position={} last_run_unix_ms={} channel={} message={}",
            row.path,
            row.event_id.as_deref().unwrap_or("none"),
            row.schedule.as_deref().unwrap_or("none"),
            row.enabled
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.decision,
            row.reason_code,
            row.queue_position
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.last_run_unix_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.channel.as_deref().unwrap_or("none"),
            row.message.as_deref().unwrap_or("none"),
        ));
    }

    lines.join("\n")
}
