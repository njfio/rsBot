use std::{path::Path, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use tau_agent_core::{Agent, AgentConfig, AgentEvent};
use tau_ai::{LlmClient, Message, MessageRole};
use tau_events::{
    dry_run_events, enforce_events_dry_run_gate,
    ingest_webhook_immediate_event as ingest_webhook_immediate_event_core, inspect_events,
    run_event_scheduler as run_core_event_scheduler, simulate_events, validate_events_definitions,
    EventRunner, EventSchedulerConfig as CoreEventSchedulerConfig, EventTemplateSchedule,
    EventsDryRunConfig, EventsDryRunGateConfig, EventsInspectConfig, EventsSimulateConfig,
    EventsTemplateConfig, EventsValidateConfig,
};

use crate::tools::ToolPolicy;
use crate::{
    channel_store::ChannelLogEntry, current_unix_timestamp_ms, run_prompt_with_cancellation, Cli,
    CliEventTemplateSchedule, PromptRunStatus, RenderOptions, SessionRuntime,
};
use tau_session::SessionStore;

pub(crate) use tau_events::{
    EventDefinition, EventWebhookIngestConfig, EventsDryRunReport, EventsInspectReport,
    EventsSimulateReport, EventsValidateReport, WebhookSignatureAlgorithm,
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

pub(crate) fn execute_events_inspect_command(cli: &Cli) -> Result<()> {
    let report = inspect_events(
        &EventsInspectConfig {
            events_dir: cli.events_dir.clone(),
            state_path: cli.events_state_path.clone(),
            queue_limit: cli.events_queue_limit.max(1),
            stale_immediate_max_age_seconds: cli.events_stale_immediate_max_age_seconds,
        },
        current_unix_timestamp_ms(),
    )?;

    if cli.events_inspect_json {
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

pub(crate) fn execute_events_validate_command(cli: &Cli) -> Result<()> {
    let report = validate_events_definitions(
        &EventsValidateConfig {
            events_dir: cli.events_dir.clone(),
            state_path: cli.events_state_path.clone(),
        },
        current_unix_timestamp_ms(),
    )?;

    if cli.events_validate_json {
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

pub(crate) fn execute_events_template_write_command(cli: &Cli) -> Result<()> {
    let target_path = cli
        .events_template_write
        .as_ref()
        .ok_or_else(|| anyhow!("--events-template-write is required"))?;

    let now_unix_ms = current_unix_timestamp_ms();
    let schedule = match cli.events_template_schedule {
        CliEventTemplateSchedule::Immediate => EventTemplateSchedule::Immediate,
        CliEventTemplateSchedule::At => EventTemplateSchedule::At,
        CliEventTemplateSchedule::Periodic => EventTemplateSchedule::Periodic,
    };

    let at_unix_ms = if matches!(cli.events_template_schedule, CliEventTemplateSchedule::At) {
        Some(
            cli.events_template_at_unix_ms
                .unwrap_or_else(|| now_unix_ms.saturating_add(300_000)),
        )
    } else {
        None
    };

    let cron = if matches!(
        cli.events_template_schedule,
        CliEventTemplateSchedule::Periodic
    ) {
        Some(
            cli.events_template_cron
                .clone()
                .unwrap_or_else(|| "0 0/15 * * * * *".to_string()),
        )
    } else {
        None
    };

    let config = EventsTemplateConfig {
        target_path: target_path.to_path_buf(),
        overwrite: cli.events_template_overwrite,
        schedule,
        channel: cli
            .events_template_channel
            .clone()
            .unwrap_or_else(|| "slack/C123".to_string()),
        prompt: cli.events_template_prompt.clone().unwrap_or_default(),
        event_id: cli.events_template_id.clone(),
        at_unix_ms,
        cron,
        timezone: Some(cli.events_template_timezone.clone()),
    };

    let report = tau_events::write_event_template(&config, now_unix_ms)?;

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

pub(crate) fn execute_events_simulate_command(cli: &Cli) -> Result<()> {
    let report = simulate_events(
        &EventsSimulateConfig {
            events_dir: cli.events_dir.clone(),
            state_path: cli.events_state_path.clone(),
            horizon_seconds: cli.events_simulate_horizon_seconds,
            stale_immediate_max_age_seconds: cli.events_stale_immediate_max_age_seconds,
        },
        current_unix_timestamp_ms(),
    )?;

    if cli.events_simulate_json {
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

pub(crate) fn execute_events_dry_run_command(cli: &Cli) -> Result<()> {
    let report = dry_run_events(
        &EventsDryRunConfig {
            events_dir: cli.events_dir.clone(),
            state_path: cli.events_state_path.clone(),
            queue_limit: cli.events_queue_limit.max(1),
            stale_immediate_max_age_seconds: cli.events_stale_immediate_max_age_seconds,
        },
        current_unix_timestamp_ms(),
    )?;

    if cli.events_dry_run_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render events dry run json")?
        );
    } else {
        println!("{}", render_events_dry_run_report(&report));
    }

    let max_error_rows = if cli.events_dry_run_strict {
        Some(0)
    } else {
        cli.events_dry_run_max_error_rows
            .map(|value| value as usize)
    };
    let gate_config = EventsDryRunGateConfig {
        max_error_rows,
        max_execute_rows: cli
            .events_dry_run_max_execute_rows
            .map(|value| value as usize),
    };
    enforce_events_dry_run_gate(&report, &gate_config)?;
    Ok(())
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

pub(crate) fn ingest_webhook_immediate_event(config: &EventWebhookIngestConfig) -> Result<()> {
    ingest_webhook_immediate_event_core(config)
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

fn render_events_inspect_report(report: &EventsInspectReport) -> String {
    format!(
        "events inspect: events_dir={} state_path={} now_unix_ms={} discovered_events={} malformed_events={} enabled_events={} disabled_events={} due_now_events={} queued_now_events={} not_due_events={} stale_immediate_events={} due_eval_failed_events={} schedule_immediate_events={} schedule_at_events={} schedule_periodic_events={} periodic_with_last_run_state={} periodic_missing_last_run_state={} queue_limit={} stale_immediate_max_age_seconds={}",
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
