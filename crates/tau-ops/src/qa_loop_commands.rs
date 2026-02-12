use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::Value;
use tau_cli::Cli;
use wait_timeout::ChildExt;

pub const QA_LOOP_USAGE: &str = "usage: /qa-loop [--json] [--config <path>] [--stage-timeout-ms <ms>] [--retry-failures <n>] [--max-output-bytes <bytes>] [--changed-file-limit <n>]";

const QA_LOOP_CONFIG_SCHEMA_VERSION: u32 = 1;
const QA_LOOP_REPORT_SCHEMA_VERSION: u32 = 1;
const QA_LOOP_DEFAULT_STAGE_TIMEOUT_MS: u64 = 120_000;
const QA_LOOP_DEFAULT_RETRY_FAILURES: usize = 0;
const QA_LOOP_DEFAULT_MAX_OUTPUT_BYTES: usize = 16_000;
const QA_LOOP_DEFAULT_CHANGED_FILE_LIMIT: usize = 100;
const QA_LOOP_PREVIEW_CHARS: usize = 160;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QaLoopOutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum QaLoopOutcome {
    Pass,
    Fail,
}

impl QaLoopOutcome {
    fn as_str(self) -> &'static str {
        match self {
            QaLoopOutcome::Pass => "pass",
            QaLoopOutcome::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum QaLoopStageStatus {
    Pass,
    Fail,
    Timeout,
    SpawnError,
}

impl QaLoopStageStatus {
    fn as_str(self) -> &'static str {
        match self {
            QaLoopStageStatus::Pass => "pass",
            QaLoopStageStatus::Fail => "fail",
            QaLoopStageStatus::Timeout => "timeout",
            QaLoopStageStatus::SpawnError => "spawn_error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QaLoopCommandOptions {
    output_format: QaLoopOutputFormat,
    config_path: Option<PathBuf>,
    stage_timeout_ms: Option<u64>,
    retry_failures: Option<usize>,
    max_output_bytes: Option<usize>,
    changed_file_limit: Option<usize>,
}

impl Default for QaLoopCommandOptions {
    fn default() -> Self {
        Self {
            output_format: QaLoopOutputFormat::Text,
            config_path: None,
            stage_timeout_ms: None,
            retry_failures: None,
            max_output_bytes: None,
            changed_file_limit: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) struct QaLoopStageAttemptReport {
    pub(crate) attempt: usize,
    pub(crate) status: QaLoopStageStatus,
    pub(crate) exit_code: Option<i32>,
    pub(crate) duration_ms: u64,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) stdout_total_bytes: usize,
    pub(crate) stderr_total_bytes: usize,
    pub(crate) stdout_truncated: bool,
    pub(crate) stderr_truncated: bool,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) struct QaLoopStageReport {
    pub(crate) name: String,
    pub(crate) command: String,
    pub(crate) timeout_ms: u64,
    pub(crate) retry_failures: usize,
    pub(crate) status: QaLoopStageStatus,
    pub(crate) duration_ms: u64,
    pub(crate) attempts: Vec<QaLoopStageAttemptReport>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) struct QaLoopChangedFile {
    pub(crate) status: String,
    pub(crate) path: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) struct QaLoopChangedFilesSummary {
    pub(crate) available: bool,
    pub(crate) total_changed_files: usize,
    pub(crate) shown_changed_files: usize,
    pub(crate) truncated: bool,
    pub(crate) error: Option<String>,
    pub(crate) files: Vec<QaLoopChangedFile>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) struct QaLoopReport {
    pub(crate) schema_version: u32,
    pub(crate) outcome: QaLoopOutcome,
    pub(crate) config_source: String,
    pub(crate) total_stages: usize,
    pub(crate) completed_stages: usize,
    pub(crate) passed_stages: usize,
    pub(crate) failed_stages: usize,
    pub(crate) total_attempts: usize,
    pub(crate) duration_ms: u64,
    pub(crate) root_cause_stage: Option<String>,
    pub(crate) stages: Vec<QaLoopStageReport>,
    pub(crate) changed_files: QaLoopChangedFilesSummary,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
struct QaLoopStageConfig {
    name: String,
    command: String,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    retry_failures: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
struct QaLoopConfigFile {
    #[serde(default = "qa_loop_config_schema_version")]
    schema_version: u32,
    #[serde(default = "default_qa_loop_stages")]
    stages: Vec<QaLoopStageConfig>,
    #[serde(default = "default_qa_loop_stage_timeout_ms")]
    default_stage_timeout_ms: u64,
    #[serde(default = "default_qa_loop_retry_failures")]
    default_retry_failures: usize,
    #[serde(default = "default_qa_loop_max_output_bytes")]
    max_output_bytes: usize,
    #[serde(default = "default_qa_loop_changed_file_limit")]
    changed_file_limit: usize,
}

impl Default for QaLoopConfigFile {
    fn default() -> Self {
        Self {
            schema_version: QA_LOOP_CONFIG_SCHEMA_VERSION,
            stages: default_qa_loop_stages(),
            default_stage_timeout_ms: QA_LOOP_DEFAULT_STAGE_TIMEOUT_MS,
            default_retry_failures: QA_LOOP_DEFAULT_RETRY_FAILURES,
            max_output_bytes: QA_LOOP_DEFAULT_MAX_OUTPUT_BYTES,
            changed_file_limit: QA_LOOP_DEFAULT_CHANGED_FILE_LIMIT,
        }
    }
}

fn qa_loop_config_schema_version() -> u32 {
    QA_LOOP_CONFIG_SCHEMA_VERSION
}

fn default_qa_loop_stage_timeout_ms() -> u64 {
    QA_LOOP_DEFAULT_STAGE_TIMEOUT_MS
}

fn default_qa_loop_retry_failures() -> usize {
    QA_LOOP_DEFAULT_RETRY_FAILURES
}

fn default_qa_loop_max_output_bytes() -> usize {
    QA_LOOP_DEFAULT_MAX_OUTPUT_BYTES
}

fn default_qa_loop_changed_file_limit() -> usize {
    QA_LOOP_DEFAULT_CHANGED_FILE_LIMIT
}

fn default_qa_loop_stages() -> Vec<QaLoopStageConfig> {
    vec![
        QaLoopStageConfig {
            name: "fmt".to_string(),
            command: "cargo fmt --all -- --check".to_string(),
            timeout_ms: None,
            retry_failures: None,
        },
        QaLoopStageConfig {
            name: "clippy".to_string(),
            command: "cargo clippy --workspace --all-targets -- -D warnings".to_string(),
            timeout_ms: None,
            retry_failures: None,
        },
        QaLoopStageConfig {
            name: "test".to_string(),
            command: "cargo test --workspace -- --test-threads=1".to_string(),
            timeout_ms: None,
            retry_failures: None,
        },
    ]
}

pub(crate) fn parse_qa_loop_command_args(command_args: &str) -> Result<QaLoopCommandOptions> {
    let mut options = QaLoopCommandOptions::default();
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let mut index = 0usize;

    while index < tokens.len() {
        match tokens[index] {
            "--json" => {
                options.output_format = QaLoopOutputFormat::Json;
                index += 1;
            }
            "--config" => {
                index += 1;
                let Some(value) = tokens.get(index) else {
                    bail!("{QA_LOOP_USAGE}");
                };
                options.config_path = Some(PathBuf::from(value));
                index += 1;
            }
            "--stage-timeout-ms" => {
                index += 1;
                let Some(value) = tokens.get(index) else {
                    bail!("{QA_LOOP_USAGE}");
                };
                options.stage_timeout_ms =
                    Some(parse_positive_u64_flag("--stage-timeout-ms", value)?);
                index += 1;
            }
            "--retry-failures" => {
                index += 1;
                let Some(value) = tokens.get(index) else {
                    bail!("{QA_LOOP_USAGE}");
                };
                options.retry_failures =
                    Some(parse_non_negative_usize_flag("--retry-failures", value)?);
                index += 1;
            }
            "--max-output-bytes" => {
                index += 1;
                let Some(value) = tokens.get(index) else {
                    bail!("{QA_LOOP_USAGE}");
                };
                options.max_output_bytes =
                    Some(parse_positive_usize_flag("--max-output-bytes", value)?);
                index += 1;
            }
            "--changed-file-limit" => {
                index += 1;
                let Some(value) = tokens.get(index) else {
                    bail!("{QA_LOOP_USAGE}");
                };
                options.changed_file_limit =
                    Some(parse_positive_usize_flag("--changed-file-limit", value)?);
                index += 1;
            }
            _ => bail!("{QA_LOOP_USAGE}"),
        }
    }

    Ok(options)
}

pub fn execute_qa_loop_cli_command(command_args: &str) -> String {
    let options = match parse_qa_loop_command_args(command_args) {
        Ok(options) => options,
        Err(_) => return QA_LOOP_USAGE.to_string(),
    };

    match execute_qa_loop_with_options(&options) {
        Ok(report) => report,
        Err(error) => format!("qa-loop error: {error}"),
    }
}

pub fn execute_qa_loop_preflight_command(cli: &Cli) -> Result<()> {
    let options = qa_loop_options_from_cli(cli);
    let cwd = std::env::current_dir().context("failed to resolve current working directory")?;
    let report = run_qa_loop(&cwd, &options)?;
    match options.output_format {
        QaLoopOutputFormat::Text => println!("{}", render_qa_loop_report(&report)),
        QaLoopOutputFormat::Json => println!("{}", render_qa_loop_report_json(&report)),
    }

    if report.outcome == QaLoopOutcome::Fail {
        bail!(
            "qa-loop failed: root_cause_stage={}",
            report.root_cause_stage.as_deref().unwrap_or("unknown")
        );
    }

    Ok(())
}

fn qa_loop_options_from_cli(cli: &Cli) -> QaLoopCommandOptions {
    QaLoopCommandOptions {
        output_format: if cli.qa_loop_json {
            QaLoopOutputFormat::Json
        } else {
            QaLoopOutputFormat::Text
        },
        config_path: cli.qa_loop_config.clone(),
        stage_timeout_ms: cli.qa_loop_stage_timeout_ms,
        retry_failures: cli.qa_loop_retry_failures,
        max_output_bytes: cli.qa_loop_max_output_bytes,
        changed_file_limit: cli.qa_loop_changed_file_limit,
    }
}

fn execute_qa_loop_with_options(options: &QaLoopCommandOptions) -> Result<String> {
    let cwd = std::env::current_dir().context("failed to resolve current working directory")?;
    let report = run_qa_loop(&cwd, options)?;
    Ok(match options.output_format {
        QaLoopOutputFormat::Text => render_qa_loop_report(&report),
        QaLoopOutputFormat::Json => render_qa_loop_report_json(&report),
    })
}

fn run_qa_loop(cwd: &Path, options: &QaLoopCommandOptions) -> Result<QaLoopReport> {
    let started_at = Instant::now();
    let (config, config_source) = load_qa_loop_config(options.config_path.as_deref())?;
    validate_qa_loop_config(&config)?;

    let max_output_bytes = options.max_output_bytes.unwrap_or(config.max_output_bytes);
    let changed_file_limit = options
        .changed_file_limit
        .unwrap_or(config.changed_file_limit);

    let mut stage_reports = Vec::new();
    let mut root_cause_stage = None;
    let mut total_attempts = 0usize;
    let mut passed_stages = 0usize;
    let mut failed_stages = 0usize;

    for stage in &config.stages {
        let timeout_ms = options
            .stage_timeout_ms
            .or(stage.timeout_ms)
            .unwrap_or(config.default_stage_timeout_ms);
        let retry_failures = options
            .retry_failures
            .or(stage.retry_failures)
            .unwrap_or(config.default_retry_failures);
        let stage_started = Instant::now();
        let mut attempts = Vec::new();

        for attempt_index in 1..=retry_failures.saturating_add(1) {
            let attempt = run_stage_attempt(
                cwd,
                stage.command.as_str(),
                attempt_index,
                timeout_ms,
                max_output_bytes,
            );
            total_attempts = total_attempts.saturating_add(1);
            let attempt_status = attempt.status;
            attempts.push(attempt);
            if attempt_status == QaLoopStageStatus::Pass {
                break;
            }
        }

        let final_status = attempts
            .last()
            .map(|attempt| attempt.status)
            .unwrap_or(QaLoopStageStatus::SpawnError);
        if final_status == QaLoopStageStatus::Pass {
            passed_stages = passed_stages.saturating_add(1);
        } else {
            failed_stages = failed_stages.saturating_add(1);
            if root_cause_stage.is_none() {
                root_cause_stage = Some(stage.name.clone());
            }
        }

        stage_reports.push(QaLoopStageReport {
            name: stage.name.clone(),
            command: stage.command.clone(),
            timeout_ms,
            retry_failures,
            status: final_status,
            duration_ms: elapsed_ms(stage_started.elapsed()),
            attempts,
        });

        if final_status != QaLoopStageStatus::Pass {
            break;
        }
    }

    let changed_files = collect_changed_files(cwd, changed_file_limit);
    let total_stages = config.stages.len();
    let completed_stages = stage_reports.len();
    let outcome = if failed_stages == 0 && completed_stages == total_stages {
        QaLoopOutcome::Pass
    } else {
        QaLoopOutcome::Fail
    };

    Ok(QaLoopReport {
        schema_version: QA_LOOP_REPORT_SCHEMA_VERSION,
        outcome,
        config_source,
        total_stages,
        completed_stages,
        passed_stages,
        failed_stages,
        total_attempts,
        duration_ms: elapsed_ms(started_at.elapsed()),
        root_cause_stage,
        stages: stage_reports,
        changed_files,
    })
}

fn load_qa_loop_config(config_path: Option<&Path>) -> Result<(QaLoopConfigFile, String)> {
    match config_path {
        Some(path) => {
            let raw = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read qa-loop config {}", path.display()))?;
            let parsed = serde_json::from_str::<QaLoopConfigFile>(&raw)
                .with_context(|| format!("failed to parse qa-loop config {}", path.display()))?;
            Ok((parsed, format!("file:{}", path.display())))
        }
        None => Ok((QaLoopConfigFile::default(), "default".to_string())),
    }
}

fn validate_qa_loop_config(config: &QaLoopConfigFile) -> Result<()> {
    if config.schema_version != QA_LOOP_CONFIG_SCHEMA_VERSION {
        bail!(
            "unsupported qa-loop config schema version {} (expected {})",
            config.schema_version,
            QA_LOOP_CONFIG_SCHEMA_VERSION
        );
    }
    if config.stages.is_empty() {
        bail!("qa-loop config must include at least one stage");
    }
    if config.default_stage_timeout_ms == 0 {
        bail!("qa-loop config default_stage_timeout_ms must be greater than 0");
    }
    if config.max_output_bytes == 0 {
        bail!("qa-loop config max_output_bytes must be greater than 0");
    }
    if config.changed_file_limit == 0 {
        bail!("qa-loop config changed_file_limit must be greater than 0");
    }

    let mut names = HashSet::new();
    for stage in &config.stages {
        let name = stage.name.trim();
        if name.is_empty() {
            bail!("qa-loop stage name must not be empty");
        }
        if !names.insert(name.to_string()) {
            bail!("qa-loop stage names must be unique: '{name}'");
        }
        if stage.command.trim().is_empty() {
            bail!("qa-loop stage '{name}' command must not be empty");
        }
        if matches!(stage.timeout_ms, Some(0)) {
            bail!("qa-loop stage '{name}' timeout_ms must be greater than 0");
        }
    }
    Ok(())
}

fn run_stage_attempt(
    cwd: &Path,
    command_text: &str,
    attempt: usize,
    timeout_ms: u64,
    max_output_bytes: usize,
) -> QaLoopStageAttemptReport {
    let started = Instant::now();
    let mut command = stage_shell_command(command_text);
    command.current_dir(cwd);
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return QaLoopStageAttemptReport {
                attempt,
                status: QaLoopStageStatus::SpawnError,
                exit_code: None,
                duration_ms: elapsed_ms(started.elapsed()),
                stdout: String::new(),
                stderr: String::new(),
                stdout_total_bytes: 0,
                stderr_total_bytes: 0,
                stdout_truncated: false,
                stderr_truncated: false,
                error: Some(format!("failed to spawn stage command: {error}")),
            };
        }
    };

    let timeout = Duration::from_millis(timeout_ms.max(1));
    let wait_result = match child.wait_timeout(timeout) {
        Ok(result) => result,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return QaLoopStageAttemptReport {
                attempt,
                status: QaLoopStageStatus::SpawnError,
                exit_code: None,
                duration_ms: elapsed_ms(started.elapsed()),
                stdout: String::new(),
                stderr: String::new(),
                stdout_total_bytes: 0,
                stderr_total_bytes: 0,
                stdout_truncated: false,
                stderr_truncated: false,
                error: Some(format!("failed to wait for stage command: {error}")),
            };
        }
    };

    match wait_result {
        None => {
            let _ = child.kill();
            let output = child.wait_with_output().ok();
            let (stdout, stdout_total_bytes, stdout_truncated) = output
                .as_ref()
                .map(|value| bounded_output(&value.stdout, max_output_bytes))
                .unwrap_or_else(|| (String::new(), 0, false));
            let (stderr, stderr_total_bytes, stderr_truncated) = output
                .as_ref()
                .map(|value| bounded_output(&value.stderr, max_output_bytes))
                .unwrap_or_else(|| (String::new(), 0, false));
            QaLoopStageAttemptReport {
                attempt,
                status: QaLoopStageStatus::Timeout,
                exit_code: None,
                duration_ms: elapsed_ms(started.elapsed()),
                stdout,
                stderr,
                stdout_total_bytes,
                stderr_total_bytes,
                stdout_truncated,
                stderr_truncated,
                error: Some(format!("stage timed out after {} ms", timeout_ms.max(1))),
            }
        }
        Some(_) => match child.wait_with_output() {
            Ok(output) => {
                stage_attempt_report_from_output(attempt, output, started, max_output_bytes)
            }
            Err(error) => QaLoopStageAttemptReport {
                attempt,
                status: QaLoopStageStatus::SpawnError,
                exit_code: None,
                duration_ms: elapsed_ms(started.elapsed()),
                stdout: String::new(),
                stderr: String::new(),
                stdout_total_bytes: 0,
                stderr_total_bytes: 0,
                stdout_truncated: false,
                stderr_truncated: false,
                error: Some(format!("failed to collect stage output: {error}")),
            },
        },
    }
}

fn stage_attempt_report_from_output(
    attempt: usize,
    output: std::process::Output,
    started: Instant,
    max_output_bytes: usize,
) -> QaLoopStageAttemptReport {
    let (stdout, stdout_total_bytes, stdout_truncated) =
        bounded_output(&output.stdout, max_output_bytes);
    let (stderr, stderr_total_bytes, stderr_truncated) =
        bounded_output(&output.stderr, max_output_bytes);
    let status = if output.status.success() {
        QaLoopStageStatus::Pass
    } else {
        QaLoopStageStatus::Fail
    };
    QaLoopStageAttemptReport {
        attempt,
        status,
        exit_code: output.status.code(),
        duration_ms: elapsed_ms(started.elapsed()),
        stdout,
        stderr,
        stdout_total_bytes,
        stderr_total_bytes,
        stdout_truncated,
        stderr_truncated,
        error: None,
    }
}

fn stage_shell_command(command_text: &str) -> Command {
    #[cfg(windows)]
    {
        let mut command = Command::new("cmd");
        command.arg("/C");
        command.arg(command_text);
        command
    }
    #[cfg(not(windows))]
    {
        let shell = std::env::var("SHELL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "sh".to_string());
        let mut command = Command::new(shell);
        command.arg("-lc");
        command.arg(command_text);
        command
    }
}

fn collect_changed_files(cwd: &Path, changed_file_limit: usize) -> QaLoopChangedFilesSummary {
    let output = Command::new("git")
        .arg("status")
        .arg("--porcelain")
        .current_dir(cwd)
        .output();

    match output {
        Err(error) => QaLoopChangedFilesSummary {
            available: false,
            total_changed_files: 0,
            shown_changed_files: 0,
            truncated: false,
            error: Some(format!("git status unavailable: {error}")),
            files: Vec::new(),
        },
        Ok(output) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                return QaLoopChangedFilesSummary {
                    available: false,
                    total_changed_files: 0,
                    shown_changed_files: 0,
                    truncated: false,
                    error: Some(if stderr.is_empty() {
                        "git status failed".to_string()
                    } else {
                        format!("git status failed: {stderr}")
                    }),
                    files: Vec::new(),
                };
            }

            let raw = String::from_utf8_lossy(&output.stdout);
            let mut files = parse_git_status_output(raw.as_ref());
            let total_changed_files = files.len();
            let truncated = total_changed_files > changed_file_limit;
            files.truncate(changed_file_limit);
            QaLoopChangedFilesSummary {
                available: true,
                total_changed_files,
                shown_changed_files: files.len(),
                truncated,
                error: None,
                files,
            }
        }
    }
}

fn parse_git_status_output(raw: &str) -> Vec<QaLoopChangedFile> {
    let mut files = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            continue;
        }
        let status = trimmed
            .get(0..2)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("??")
            .to_string();
        let mut path = trimmed
            .get(3..)
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        if let Some((_, renamed_to)) = path.split_once(" -> ") {
            path = renamed_to.trim().to_string();
        }
        if path.is_empty() {
            continue;
        }
        files.push(QaLoopChangedFile { status, path });
    }
    files.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.status.cmp(&right.status))
    });
    files
}

fn elapsed_ms(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

fn bounded_output(raw: &[u8], max_output_bytes: usize) -> (String, usize, bool) {
    let rendered = String::from_utf8_lossy(raw).into_owned();
    let (bounded, truncated) = truncate_utf8(&rendered, max_output_bytes);
    (bounded, raw.len(), truncated)
}

fn truncate_utf8(value: &str, limit: usize) -> (String, bool) {
    if value.len() <= limit {
        return (value.to_string(), false);
    }
    let mut end = limit;
    while end > 0 && !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    (value[..end].to_string(), true)
}

fn parse_positive_u64_flag(flag: &str, value: &str) -> Result<u64> {
    let parsed = value
        .parse::<u64>()
        .with_context(|| format!("{flag} expects an integer value"))?;
    if parsed == 0 {
        bail!("{flag} must be greater than 0");
    }
    Ok(parsed)
}

fn parse_non_negative_usize_flag(flag: &str, value: &str) -> Result<usize> {
    value
        .parse::<usize>()
        .with_context(|| format!("{flag} expects a non-negative integer value"))
}

fn parse_positive_usize_flag(flag: &str, value: &str) -> Result<usize> {
    let parsed = parse_non_negative_usize_flag(flag, value)?;
    if parsed == 0 {
        bail!("{flag} must be greater than 0");
    }
    Ok(parsed)
}

pub(crate) fn render_qa_loop_report(report: &QaLoopReport) -> String {
    let mut lines = vec![format!(
        "qa-loop summary: outcome={} stages={} completed={} passed={} failed={} attempts={} duration_ms={} config={}",
        report.outcome.as_str(),
        report.total_stages,
        report.completed_stages,
        report.passed_stages,
        report.failed_stages,
        report.total_attempts,
        report.duration_ms,
        report.config_source
    )];

    if let Some(stage) = report.root_cause_stage.as_deref() {
        lines.push(format!("qa-loop root-cause: stage={stage}"));
    }

    for stage in &report.stages {
        lines.push(format!(
            "qa-loop stage: name={} status={} attempts={} retries={} timeout_ms={} duration_ms={} command={}",
            stage.name,
            stage.status.as_str(),
            stage.attempts.len(),
            stage.retry_failures,
            stage.timeout_ms,
            stage.duration_ms,
            stage.command
        ));
        for attempt in &stage.attempts {
            lines.push(format!(
                "qa-loop attempt: stage={} attempt={} status={} exit_code={} duration_ms={} stdout_bytes={} stderr_bytes={} stdout_truncated={} stderr_truncated={} error={}",
                stage.name,
                attempt.attempt,
                attempt.status.as_str(),
                attempt
                    .exit_code
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                attempt.duration_ms,
                attempt.stdout_total_bytes,
                attempt.stderr_total_bytes,
                attempt.stdout_truncated,
                attempt.stderr_truncated,
                attempt.error.as_deref().unwrap_or("none"),
            ));
            if let Some(preview) = output_preview(attempt.stdout.as_str()) {
                lines.push(format!(
                    "qa-loop attempt stdout: stage={} attempt={} preview={}",
                    stage.name, attempt.attempt, preview
                ));
            }
            if let Some(preview) = output_preview(attempt.stderr.as_str()) {
                lines.push(format!(
                    "qa-loop attempt stderr: stage={} attempt={} preview={}",
                    stage.name, attempt.attempt, preview
                ));
            }
        }
    }

    lines.push(format!(
        "qa-loop changed-files: available={} total={} shown={} truncated={} error={}",
        report.changed_files.available,
        report.changed_files.total_changed_files,
        report.changed_files.shown_changed_files,
        report.changed_files.truncated,
        report.changed_files.error.as_deref().unwrap_or("none"),
    ));
    for file in &report.changed_files.files {
        lines.push(format!(
            "qa-loop changed-file: status={} path={}",
            file.status, file.path
        ));
    }

    lines.join("\n")
}

pub(crate) fn render_qa_loop_report_json(report: &QaLoopReport) -> String {
    serde_json::to_string(report).unwrap_or_else(|_| "{}".to_string())
}

fn output_preview(value: &str) -> Option<String> {
    let normalized = value.trim().replace('\r', "\\r").replace('\n', "\\n");
    if normalized.is_empty() {
        return None;
    }
    let mut chars = normalized.chars();
    let preview = chars
        .by_ref()
        .take(QA_LOOP_PREVIEW_CHARS)
        .collect::<String>();
    if chars.next().is_some() {
        Some(format!("{preview}..."))
    } else {
        Some(preview)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_qa_loop_config(path: &Path, value: &Value) {
        std::fs::write(path, format!("{value}\n")).expect("write qa-loop config");
    }

    #[test]
    fn unit_parse_qa_loop_command_args_supports_config_json_and_overrides() {
        let options = parse_qa_loop_command_args(
            "--json --config /tmp/qa-loop.json --stage-timeout-ms 500 --retry-failures 2 --max-output-bytes 1024 --changed-file-limit 8",
        )
        .expect("parse qa-loop command args");
        assert_eq!(options.output_format, QaLoopOutputFormat::Json);
        assert_eq!(
            options.config_path,
            Some(PathBuf::from("/tmp/qa-loop.json"))
        );
        assert_eq!(options.stage_timeout_ms, Some(500));
        assert_eq!(options.retry_failures, Some(2));
        assert_eq!(options.max_output_bytes, Some(1024));
        assert_eq!(options.changed_file_limit, Some(8));
    }

    #[test]
    fn regression_parse_qa_loop_command_args_rejects_unknown_flag() {
        let error = parse_qa_loop_command_args("--unknown").expect_err("unknown flag should fail");
        assert!(error.to_string().contains(QA_LOOP_USAGE));
    }

    #[test]
    fn functional_run_qa_loop_executes_configured_stage_and_renders_text() {
        let temp = tempdir().expect("tempdir");
        let config_path = temp.path().join("qa-loop.json");
        write_qa_loop_config(
            &config_path,
            &serde_json::json!({
                "schema_version": 1,
                "stages": [
                    {"name": "smoke", "command": "echo qa-loop-ok"}
                ],
                "changed_file_limit": 4
            }),
        );

        let options = QaLoopCommandOptions {
            config_path: Some(config_path),
            ..QaLoopCommandOptions::default()
        };
        let report = run_qa_loop(temp.path(), &options).expect("run qa-loop");
        assert_eq!(report.outcome, QaLoopOutcome::Pass);
        assert_eq!(report.stages.len(), 1);
        assert_eq!(report.stages[0].status, QaLoopStageStatus::Pass);
        assert_eq!(report.stages[0].attempts.len(), 1);

        let text = render_qa_loop_report(&report);
        assert!(text.contains("qa-loop summary: outcome=pass"));
        assert!(text.contains("qa-loop stage: name=smoke status=pass"));
        assert!(text.contains("qa-loop attempt stdout: stage=smoke attempt=1 preview=qa-loop-ok"));
    }

    #[test]
    fn integration_run_qa_loop_retries_until_eventual_success() {
        let temp = tempdir().expect("tempdir");
        let config_path = temp.path().join("qa-loop.json");
        write_qa_loop_config(
            &config_path,
            &serde_json::json!({
                "schema_version": 1,
                "stages": [
                    {
                        "name": "flaky",
                        "command": "if [ -f .qa-loop-pass ]; then echo pass; else touch .qa-loop-pass; echo retry-needed 1>&2; exit 1; fi"
                    }
                ]
            }),
        );

        let options = QaLoopCommandOptions {
            config_path: Some(config_path),
            retry_failures: Some(1),
            ..QaLoopCommandOptions::default()
        };
        let report = run_qa_loop(temp.path(), &options).expect("run qa-loop");
        assert_eq!(report.outcome, QaLoopOutcome::Pass);
        assert_eq!(report.stages.len(), 1);
        assert_eq!(report.stages[0].attempts.len(), 2);
        assert_eq!(report.stages[0].attempts[0].status, QaLoopStageStatus::Fail);
        assert_eq!(report.stages[0].attempts[1].status, QaLoopStageStatus::Pass);
    }

    #[test]
    fn integration_collect_changed_files_reports_git_status_when_available() {
        let temp = tempdir().expect("tempdir");
        let git_init = Command::new("git")
            .arg("init")
            .current_dir(temp.path())
            .output();
        if git_init.is_err() {
            return;
        }
        let output = git_init.expect("git init output");
        if !output.status.success() {
            return;
        }

        std::fs::write(temp.path().join("tracked.txt"), "hello").expect("write tracked file");

        let summary = collect_changed_files(temp.path(), 10);
        assert!(summary.available);
        assert!(summary.total_changed_files >= 1);
        assert!(!summary.files.is_empty());
    }

    #[test]
    fn regression_run_qa_loop_rejects_invalid_config_schema() {
        let temp = tempdir().expect("tempdir");
        let config_path = temp.path().join("qa-loop.json");
        write_qa_loop_config(
            &config_path,
            &serde_json::json!({
                "schema_version": 99,
                "stages": [
                    {"name": "smoke", "command": "echo ok"}
                ]
            }),
        );
        let options = QaLoopCommandOptions {
            config_path: Some(config_path),
            ..QaLoopCommandOptions::default()
        };
        let error = run_qa_loop(temp.path(), &options).expect_err("invalid schema should fail");
        assert!(error
            .to_string()
            .contains("unsupported qa-loop config schema version"));
    }

    #[test]
    fn unit_parse_git_status_output_maps_statuses_and_rename_targets() {
        let raw = " M src/main.rs\nR  old/name.rs -> new/name.rs\n?? untracked.txt\n";
        let parsed = parse_git_status_output(raw);
        assert_eq!(
            parsed,
            vec![
                QaLoopChangedFile {
                    status: "R".to_string(),
                    path: "new/name.rs".to_string(),
                },
                QaLoopChangedFile {
                    status: "M".to_string(),
                    path: "src/main.rs".to_string(),
                },
                QaLoopChangedFile {
                    status: "??".to_string(),
                    path: "untracked.txt".to_string(),
                },
            ]
        );
    }

    #[test]
    fn regression_execute_qa_loop_cli_command_returns_usage_for_invalid_arguments() {
        let output = execute_qa_loop_cli_command("--bad-flag");
        assert_eq!(output, QA_LOOP_USAGE);
    }
}
