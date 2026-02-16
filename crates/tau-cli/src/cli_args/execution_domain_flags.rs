use std::path::PathBuf;

use clap::{ArgAction, Args};

use crate::CliEventTemplateSchedule;

/// Execution-domain event inspection/validation/simulation/template flags.
#[derive(Debug, Args)]
pub struct CliExecutionDomainFlags {
    #[arg(
        long = "events-inspect",
        env = "TAU_EVENTS_INSPECT",
        default_value_t = false,
        conflicts_with = "events_validate",
        conflicts_with = "events_simulate",
        conflicts_with = "events_dry_run",
        conflicts_with = "events_template_write",
        conflicts_with = "events_runner",
        conflicts_with = "event_webhook_ingest_file",
        help = "Inspect scheduled events state and due/queue diagnostics, then exit"
    )]
    pub events_inspect: bool,

    #[arg(
        long = "events-inspect-json",
        env = "TAU_EVENTS_INSPECT_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "events_inspect",
        help = "Emit --events-inspect output as pretty JSON"
    )]
    pub events_inspect_json: bool,

    #[arg(
        long = "events-validate",
        env = "TAU_EVENTS_VALIDATE",
        default_value_t = false,
        conflicts_with = "events_inspect",
        conflicts_with = "events_simulate",
        conflicts_with = "events_dry_run",
        conflicts_with = "events_template_write",
        conflicts_with = "events_runner",
        conflicts_with = "event_webhook_ingest_file",
        help = "Validate scheduled event definition files and exit non-zero on invalid entries"
    )]
    pub events_validate: bool,

    #[arg(
        long = "events-validate-json",
        env = "TAU_EVENTS_VALIDATE_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "events_validate",
        help = "Emit --events-validate output as pretty JSON"
    )]
    pub events_validate_json: bool,

    #[arg(
        long = "events-simulate",
        env = "TAU_EVENTS_SIMULATE",
        default_value_t = false,
        conflicts_with = "events_inspect",
        conflicts_with = "events_validate",
        conflicts_with = "events_dry_run",
        conflicts_with = "events_template_write",
        conflicts_with = "events_runner",
        conflicts_with = "event_webhook_ingest_file",
        help = "Simulate next event due timings and horizon posture, then exit"
    )]
    pub events_simulate: bool,

    #[arg(
        long = "events-simulate-json",
        env = "TAU_EVENTS_SIMULATE_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "events_simulate",
        help = "Emit --events-simulate output as pretty JSON"
    )]
    pub events_simulate_json: bool,

    #[arg(
        long = "events-simulate-horizon-seconds",
        env = "TAU_EVENTS_SIMULATE_HORIZON_SECONDS",
        default_value_t = 3_600,
        requires = "events_simulate",
        help = "Horizon window used to classify event next-due timing"
    )]
    pub events_simulate_horizon_seconds: u64,

    #[arg(
        long = "events-dry-run",
        env = "TAU_EVENTS_DRY_RUN",
        default_value_t = false,
        conflicts_with = "events_inspect",
        conflicts_with = "events_validate",
        conflicts_with = "events_simulate",
        conflicts_with = "events_template_write",
        conflicts_with = "events_runner",
        conflicts_with = "event_webhook_ingest_file",
        help = "Preview which events would execute now without mutating state or files"
    )]
    pub events_dry_run: bool,

    #[arg(
        long = "events-dry-run-json",
        env = "TAU_EVENTS_DRY_RUN_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "events_dry_run",
        help = "Emit --events-dry-run output as pretty JSON"
    )]
    pub events_dry_run_json: bool,

    #[arg(
        long = "events-dry-run-strict",
        env = "TAU_EVENTS_DRY_RUN_STRICT",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "events_dry_run",
        help = "Exit non-zero when --events-dry-run reports malformed or invalid definitions"
    )]
    pub events_dry_run_strict: bool,

    #[arg(
        long = "events-dry-run-max-error-rows",
        env = "TAU_EVENTS_DRY_RUN_MAX_ERROR_ROWS",
        requires = "events_dry_run",
        value_name = "count",
        help = "Fail dry-run when error row count exceeds this threshold"
    )]
    pub events_dry_run_max_error_rows: Option<u64>,

    #[arg(
        long = "events-dry-run-max-execute-rows",
        env = "TAU_EVENTS_DRY_RUN_MAX_EXECUTE_ROWS",
        requires = "events_dry_run",
        value_name = "count",
        help = "Fail dry-run when execute row count exceeds this threshold"
    )]
    pub events_dry_run_max_execute_rows: Option<u64>,

    #[arg(
        long = "events-template-write",
        env = "TAU_EVENTS_TEMPLATE_WRITE",
        value_name = "PATH",
        conflicts_with = "events_inspect",
        conflicts_with = "events_validate",
        conflicts_with = "events_simulate",
        conflicts_with = "events_dry_run",
        conflicts_with = "events_runner",
        conflicts_with = "event_webhook_ingest_file",
        help = "Write a schedule-specific event template JSON file and exit"
    )]
    pub events_template_write: Option<PathBuf>,

    #[arg(
        long = "events-template-schedule",
        env = "TAU_EVENTS_TEMPLATE_SCHEDULE",
        value_enum,
        default_value_t = CliEventTemplateSchedule::Immediate,
        requires = "events_template_write",
        help = "Schedule variant for --events-template-write: immediate, at, periodic"
    )]
    pub events_template_schedule: CliEventTemplateSchedule,

    #[arg(
        long = "events-template-overwrite",
        env = "TAU_EVENTS_TEMPLATE_OVERWRITE",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "events_template_write",
        help = "Allow overwriting an existing template file path"
    )]
    pub events_template_overwrite: bool,

    #[arg(
        long = "events-template-id",
        env = "TAU_EVENTS_TEMPLATE_ID",
        requires = "events_template_write",
        help = "Optional event id override for generated template"
    )]
    pub events_template_id: Option<String>,

    #[arg(
        long = "events-template-channel",
        env = "TAU_EVENTS_TEMPLATE_CHANNEL",
        requires = "events_template_write",
        value_name = "transport/channel_id",
        help = "Optional channel ref override for generated template"
    )]
    pub events_template_channel: Option<String>,

    #[arg(
        long = "events-template-prompt",
        env = "TAU_EVENTS_TEMPLATE_PROMPT",
        requires = "events_template_write",
        help = "Optional prompt override for generated template"
    )]
    pub events_template_prompt: Option<String>,

    #[arg(
        long = "events-template-at-unix-ms",
        env = "TAU_EVENTS_TEMPLATE_AT_UNIX_MS",
        requires = "events_template_write",
        help = "Optional unix timestamp (ms) used for --events-template-schedule at"
    )]
    pub events_template_at_unix_ms: Option<u64>,

    #[arg(
        long = "events-template-cron",
        env = "TAU_EVENTS_TEMPLATE_CRON",
        requires = "events_template_write",
        help = "Optional cron override used for --events-template-schedule periodic"
    )]
    pub events_template_cron: Option<String>,

    #[arg(
        long = "events-template-timezone",
        env = "TAU_EVENTS_TEMPLATE_TIMEZONE",
        default_value = "UTC",
        requires = "events_template_write",
        help = "Timezone used for --events-template-schedule periodic"
    )]
    pub events_template_timezone: String,
}
