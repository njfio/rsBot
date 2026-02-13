//! Runtime diagnostics, doctor checks, and audit reporting for Tau.
//!
//! Implements readiness checks, structured report generation, and operator
//! inspection commands consumed by startup and transport workflows.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::Value;
use tau_ai::{ModelRef, Provider};
use tau_cli::Cli;
use tau_provider::is_executable_available;
use tau_provider::{
    configured_provider_auth_method, load_credential_store, provider_auth_capability,
    provider_auth_mode_flag, resolve_api_key, resolve_credential_store_encryption_mode,
    CredentialStoreData, CredentialStoreEncryptionMode, ProviderAuthMethod,
};
use tau_release_channel::{
    compare_versions, default_release_channel_path, load_release_channel_store, release_lookup_url,
    resolve_latest_channel_release_cached, ReleaseChannel,
};

pub const DOCTOR_USAGE: &str = "usage: /doctor [--json] [--online]";
pub const POLICY_USAGE: &str = "usage: /policy";
pub const AUDIT_SUMMARY_USAGE: &str = "usage: /audit-summary <path>";
pub const MULTI_CHANNEL_READINESS_TELEGRAM_TOKEN_ENV: &str = "TAU_TELEGRAM_BOT_TOKEN";
pub const MULTI_CHANNEL_READINESS_DISCORD_TOKEN_ENV: &str = "TAU_DISCORD_BOT_TOKEN";
pub const MULTI_CHANNEL_READINESS_WHATSAPP_ACCESS_TOKEN_ENV: &str = "TAU_WHATSAPP_ACCESS_TOKEN";
pub const MULTI_CHANNEL_READINESS_WHATSAPP_PHONE_NUMBER_ID_ENV: &str =
    "TAU_WHATSAPP_PHONE_NUMBER_ID";
const MULTI_CHANNEL_READINESS_TELEGRAM_TOKEN_INTEGRATION_ID: &str = "telegram-bot-token";
const MULTI_CHANNEL_READINESS_DISCORD_TOKEN_INTEGRATION_ID: &str = "discord-bot-token";
const MULTI_CHANNEL_READINESS_WHATSAPP_ACCESS_TOKEN_INTEGRATION_ID: &str = "whatsapp-access-token";
const MULTI_CHANNEL_READINESS_WHATSAPP_PHONE_NUMBER_ID_INTEGRATION_ID: &str =
    "whatsapp-phone-number-id";

#[derive(Debug, Default)]
/// Public struct `ToolAuditAggregate` used across Tau components.
pub struct ToolAuditAggregate {
    pub count: u64,
    pub error_count: u64,
    pub durations_ms: Vec<u64>,
}

#[derive(Debug, Default)]
/// Public struct `ProviderAuditAggregate` used across Tau components.
pub struct ProviderAuditAggregate {
    pub count: u64,
    pub error_count: u64,
    pub durations_ms: Vec<u64>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Default)]
/// Public struct `AuditSummary` used across Tau components.
pub struct AuditSummary {
    pub record_count: u64,
    pub tool_event_count: u64,
    pub prompt_record_count: u64,
    pub tools: BTreeMap<String, ToolAuditAggregate>,
    pub providers: BTreeMap<String, ProviderAuditAggregate>,
}

pub fn summarize_audit_file(path: &Path) -> Result<AuditSummary> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("failed to open audit file {}", path.display()))?;
    let reader = std::io::BufReader::new(file);

    let mut summary = AuditSummary::default();
    for (line_no, raw_line) in std::io::BufRead::lines(reader).enumerate() {
        let line = raw_line.with_context(|| {
            format!(
                "failed to read line {} from {}",
                line_no + 1,
                path.display()
            )
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        summary.record_count = summary.record_count.saturating_add(1);
        let value: Value = serde_json::from_str(trimmed).with_context(|| {
            format!(
                "failed to parse JSON at line {} in {}",
                line_no + 1,
                path.display()
            )
        })?;

        if value.get("event").and_then(Value::as_str) == Some("tool_execution_end") {
            summary.tool_event_count = summary.tool_event_count.saturating_add(1);
            let tool_name = value
                .get("tool_name")
                .and_then(Value::as_str)
                .unwrap_or("unknown_tool")
                .to_string();
            let duration_ms = value.get("duration_ms").and_then(Value::as_u64);
            let is_error = value
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let aggregate = summary.tools.entry(tool_name).or_default();
            aggregate.count = aggregate.count.saturating_add(1);
            if is_error {
                aggregate.error_count = aggregate.error_count.saturating_add(1);
            }
            if let Some(duration_ms) = duration_ms {
                aggregate.durations_ms.push(duration_ms);
            }
            continue;
        }

        if value.get("record_type").and_then(Value::as_str) == Some("prompt_telemetry_v1") {
            summary.prompt_record_count = summary.prompt_record_count.saturating_add(1);
            let provider = value
                .get("provider")
                .and_then(Value::as_str)
                .unwrap_or("unknown_provider")
                .to_string();
            let duration_ms = value
                .get("duration_ms")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let status = value.get("status").and_then(Value::as_str);
            let success = value
                .get("success")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| status == Some("completed"));

            let usage = value
                .get("token_usage")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let input_tokens = usage
                .get("input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let output_tokens = usage
                .get("output_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let total_tokens = usage
                .get("total_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0);

            let aggregate = summary.providers.entry(provider).or_default();
            aggregate.count = aggregate.count.saturating_add(1);
            if !success {
                aggregate.error_count = aggregate.error_count.saturating_add(1);
            }
            if duration_ms > 0 {
                aggregate.durations_ms.push(duration_ms);
            }
            aggregate.input_tokens = aggregate.input_tokens.saturating_add(input_tokens);
            aggregate.output_tokens = aggregate.output_tokens.saturating_add(output_tokens);
            aggregate.total_tokens = aggregate.total_tokens.saturating_add(total_tokens);
        }
    }

    Ok(summary)
}

pub fn percentile_duration_ms(values: &[u64], percentile_numerator: u64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let len = sorted.len() as u64;
    let rank = len.saturating_mul(percentile_numerator).saturating_add(99) / 100;
    let index = rank.saturating_sub(1).min(len.saturating_sub(1)) as usize;
    sorted[index]
}

pub fn render_audit_summary(path: &Path, summary: &AuditSummary) -> String {
    let mut lines = vec![format!(
        "audit summary: path={} records={} tool_events={} prompt_records={}",
        path.display(),
        summary.record_count,
        summary.tool_event_count,
        summary.prompt_record_count
    )];

    lines.push("tool_breakdown:".to_string());
    if summary.tools.is_empty() {
        lines.push("  none".to_string());
    } else {
        for (tool_name, aggregate) in &summary.tools {
            let error_rate = if aggregate.count == 0 {
                0.0
            } else {
                (aggregate.error_count as f64 / aggregate.count as f64) * 100.0
            };
            lines.push(format!(
                "  {} count={} error_rate={:.2}% p50_ms={} p95_ms={}",
                tool_name,
                aggregate.count,
                error_rate,
                percentile_duration_ms(&aggregate.durations_ms, 50),
                percentile_duration_ms(&aggregate.durations_ms, 95),
            ));
        }
    }

    lines.push("provider_breakdown:".to_string());
    if summary.providers.is_empty() {
        lines.push("  none".to_string());
    } else {
        for (provider, aggregate) in &summary.providers {
            let error_rate = if aggregate.count == 0 {
                0.0
            } else {
                (aggregate.error_count as f64 / aggregate.count as f64) * 100.0
            };
            lines.push(format!(
                "  {} count={} error_rate={:.2}% p50_ms={} p95_ms={} input_tokens={} output_tokens={} total_tokens={}",
                provider,
                aggregate.count,
                error_rate,
                percentile_duration_ms(&aggregate.durations_ms, 50),
                percentile_duration_ms(&aggregate.durations_ms, 95),
                aggregate.input_tokens,
                aggregate.output_tokens,
                aggregate.total_tokens,
            ));
        }
    }

    lines.join("\n")
}

pub fn execute_audit_summary_command(command_args: &str) -> String {
    if command_args.trim().is_empty() {
        return AUDIT_SUMMARY_USAGE.to_string();
    }

    let path = PathBuf::from(command_args);
    match summarize_audit_file(&path) {
        Ok(summary) => render_audit_summary(&path, &summary),
        Err(error) => format!("audit summary error: {error}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `DoctorStatus` values.
pub enum DoctorStatus {
    Pass,
    Warn,
    Fail,
}

impl DoctorStatus {
    fn as_str(self) -> &'static str {
        match self {
            DoctorStatus::Pass => "pass",
            DoctorStatus::Warn => "warn",
            DoctorStatus::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `DoctorCheckResult` used across Tau components.
pub struct DoctorCheckResult {
    pub key: String,
    pub status: DoctorStatus,
    pub code: String,
    pub path: Option<String>,
    pub action: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `DoctorProviderKeyStatus` used across Tau components.
pub struct DoctorProviderKeyStatus {
    pub provider_kind: Provider,
    pub provider: String,
    pub key_env_var: String,
    pub present: bool,
    pub auth_mode: ProviderAuthMethod,
    pub mode_supported: bool,
    pub login_backend_enabled: bool,
    pub login_backend_executable: Option<String>,
    pub login_backend_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `DoctorMultiChannelReadinessConfig` used across Tau components.
pub struct DoctorMultiChannelReadinessConfig {
    pub ingress_dir: PathBuf,
    pub credential_store_path: PathBuf,
    pub credential_store_encryption: CredentialStoreEncryptionMode,
    pub credential_store_key: Option<String>,
    pub telegram_bot_token: Option<String>,
    pub discord_bot_token: Option<String>,
    pub whatsapp_access_token: Option<String>,
    pub whatsapp_phone_number_id: Option<String>,
}

impl Default for DoctorMultiChannelReadinessConfig {
    fn default() -> Self {
        Self {
            ingress_dir: PathBuf::from(".tau/multi-channel/live-ingress"),
            credential_store_path: PathBuf::from(".tau/credentials.json"),
            credential_store_encryption: CredentialStoreEncryptionMode::None,
            credential_store_key: None,
            telegram_bot_token: None,
            discord_bot_token: None,
            whatsapp_access_token: None,
            whatsapp_phone_number_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `DoctorCommandConfig` used across Tau components.
pub struct DoctorCommandConfig {
    pub model: String,
    pub provider_keys: Vec<DoctorProviderKeyStatus>,
    pub release_channel_path: PathBuf,
    pub release_lookup_cache_path: PathBuf,
    pub release_lookup_cache_ttl_ms: u64,
    pub browser_automation_playwright_cli: String,
    pub session_enabled: bool,
    pub session_path: PathBuf,
    pub skills_dir: PathBuf,
    pub skills_lock_path: PathBuf,
    pub trust_root_path: Option<PathBuf>,
    pub multi_channel_live_readiness: DoctorMultiChannelReadinessConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `DoctorCommandOutputFormat` values.
pub enum DoctorCommandOutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Public struct `DoctorCommandArgs` used across Tau components.
pub struct DoctorCommandArgs {
    pub output_format: DoctorCommandOutputFormat,
    pub online: bool,
}

impl Default for DoctorCommandArgs {
    fn default() -> Self {
        Self {
            output_format: DoctorCommandOutputFormat::Text,
            online: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Public struct `DoctorCheckOptions` used across Tau components.
pub struct DoctorCheckOptions {
    pub online: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `MultiChannelReadinessOutputFormat` values.
pub enum MultiChannelReadinessOutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `MultiChannelLiveReadinessReport` used across Tau components.
pub struct MultiChannelLiveReadinessReport {
    pub checks: Vec<DoctorCheckResult>,
    pub pass: usize,
    pub warn: usize,
    pub fail: usize,
    pub gate: String,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `BrowserAutomationReadinessOutputFormat` values.
pub enum BrowserAutomationReadinessOutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `BrowserAutomationReadinessReport` used across Tau components.
pub struct BrowserAutomationReadinessReport {
    pub checks: Vec<DoctorCheckResult>,
    pub pass: usize,
    pub warn: usize,
    pub fail: usize,
    pub gate: String,
    pub reason_codes: Vec<String>,
}

pub fn parse_doctor_command_args(command_args: &str) -> Result<DoctorCommandArgs> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let mut args = DoctorCommandArgs::default();
    for token in tokens {
        match token {
            "--json" => {
                if args.output_format == DoctorCommandOutputFormat::Json {
                    bail!("{DOCTOR_USAGE}");
                }
                args.output_format = DoctorCommandOutputFormat::Json;
            }
            "--online" => {
                if args.online {
                    bail!("{DOCTOR_USAGE}");
                }
                args.online = true;
            }
            _ => bail!("{DOCTOR_USAGE}"),
        }
    }
    Ok(args)
}

pub fn provider_key_env_var(provider: Provider) -> &'static str {
    match provider {
        Provider::OpenAi => "OPENAI_API_KEY",
        Provider::Anthropic => "ANTHROPIC_API_KEY",
        Provider::Google => "GEMINI_API_KEY",
    }
}

pub fn provider_key_present(cli: &Cli, provider: Provider) -> bool {
    match provider {
        Provider::OpenAi => {
            resolve_api_key(vec![cli.openai_api_key.clone(), cli.api_key.clone()]).is_some()
        }
        Provider::Anthropic => {
            resolve_api_key(vec![cli.anthropic_api_key.clone(), cli.api_key.clone()]).is_some()
        }
        Provider::Google => {
            resolve_api_key(vec![cli.google_api_key.clone(), cli.api_key.clone()]).is_some()
        }
    }
}

fn resolve_non_empty_env_var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn build_multi_channel_live_readiness_config(cli: &Cli) -> DoctorMultiChannelReadinessConfig {
    DoctorMultiChannelReadinessConfig {
        ingress_dir: cli.multi_channel_live_ingress_dir.clone(),
        credential_store_path: cli.credential_store.clone(),
        credential_store_encryption: resolve_credential_store_encryption_mode(cli),
        credential_store_key: cli.credential_store_key.clone(),
        telegram_bot_token: resolve_non_empty_env_var(MULTI_CHANNEL_READINESS_TELEGRAM_TOKEN_ENV),
        discord_bot_token: resolve_non_empty_env_var(MULTI_CHANNEL_READINESS_DISCORD_TOKEN_ENV),
        whatsapp_access_token: resolve_non_empty_env_var(
            MULTI_CHANNEL_READINESS_WHATSAPP_ACCESS_TOKEN_ENV,
        ),
        whatsapp_phone_number_id: resolve_non_empty_env_var(
            MULTI_CHANNEL_READINESS_WHATSAPP_PHONE_NUMBER_ID_ENV,
        ),
    }
}

pub fn build_doctor_command_config(
    cli: &Cli,
    primary_model: &ModelRef,
    fallback_models: &[ModelRef],
    skills_lock_path: &Path,
) -> DoctorCommandConfig {
    let mut providers = Vec::new();
    providers.push(primary_model.provider);
    for model in fallback_models {
        if !providers.contains(&model.provider) {
            providers.push(model.provider);
        }
    }
    providers.sort_by_key(|provider| provider.as_str().to_string());
    let provider_keys = providers
        .into_iter()
        .map(|provider| {
            let auth_mode = configured_provider_auth_method(cli, provider);
            let capability = provider_auth_capability(provider, auth_mode);
            let (login_backend_enabled, login_backend_executable, login_backend_available) =
                if provider == Provider::OpenAi
                    && matches!(
                        auth_mode,
                        ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken
                    )
                {
                    (
                        cli.openai_codex_backend,
                        Some(cli.openai_codex_cli.clone()),
                        cli.openai_codex_backend && is_executable_available(&cli.openai_codex_cli),
                    )
                } else if provider == Provider::Anthropic
                    && matches!(
                        auth_mode,
                        ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken
                    )
                {
                    (
                        cli.anthropic_claude_backend,
                        Some(cli.anthropic_claude_cli.clone()),
                        cli.anthropic_claude_backend
                            && is_executable_available(&cli.anthropic_claude_cli),
                    )
                } else if provider == Provider::Google
                    && matches!(
                        auth_mode,
                        ProviderAuthMethod::OauthToken | ProviderAuthMethod::Adc
                    )
                {
                    (
                        cli.google_gemini_backend,
                        Some(cli.google_gemini_cli.clone()),
                        cli.google_gemini_backend
                            && is_executable_available(&cli.google_gemini_cli),
                    )
                } else {
                    (false, None, false)
                };
            DoctorProviderKeyStatus {
                provider_kind: provider,
                provider: provider.as_str().to_string(),
                key_env_var: provider_key_env_var(provider).to_string(),
                present: provider_key_present(cli, provider),
                auth_mode,
                mode_supported: capability.supported,
                login_backend_enabled,
                login_backend_executable,
                login_backend_available,
            }
        })
        .collect::<Vec<_>>();

    DoctorCommandConfig {
        model: format!(
            "{}/{}",
            primary_model.provider.as_str(),
            primary_model.model
        ),
        provider_keys,
        release_channel_path: default_release_channel_path()
            .unwrap_or_else(|_| PathBuf::from(".tau/release-channel.json")),
        release_lookup_cache_path: cli.doctor_release_cache_file.clone(),
        release_lookup_cache_ttl_ms: cli.doctor_release_cache_ttl_ms,
        browser_automation_playwright_cli: cli.browser_automation_playwright_cli.clone(),
        session_enabled: !cli.no_session,
        session_path: cli.session.clone(),
        skills_dir: cli.skills_dir.clone(),
        skills_lock_path: skills_lock_path.to_path_buf(),
        trust_root_path: cli.skill_trust_root_file.clone(),
        multi_channel_live_readiness: build_multi_channel_live_readiness_config(cli),
    }
}

fn integration_secret_available(store: Option<&CredentialStoreData>, integration_id: &str) -> bool {
    store
        .and_then(|payload| payload.integrations.get(integration_id))
        .filter(|entry| !entry.revoked)
        .and_then(|entry| entry.secret.as_ref())
        .map(|secret| !secret.trim().is_empty())
        .unwrap_or(false)
}

fn build_multi_channel_readiness_summary(
    checks: Vec<DoctorCheckResult>,
) -> MultiChannelLiveReadinessReport {
    let pass = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Pass)
        .count();
    let warn = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Warn)
        .count();
    let fail = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Fail)
        .count();
    let reason_codes = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Fail)
        .map(|item| format!("{}:{}", item.key, item.code))
        .collect::<Vec<_>>();

    MultiChannelLiveReadinessReport {
        checks,
        pass,
        warn,
        fail,
        gate: if fail == 0 {
            "pass".to_string()
        } else {
            "fail".to_string()
        },
        reason_codes,
    }
}

fn build_browser_automation_readiness_summary(
    checks: Vec<DoctorCheckResult>,
) -> BrowserAutomationReadinessReport {
    let pass = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Pass)
        .count();
    let warn = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Warn)
        .count();
    let fail = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Fail)
        .count();
    let reason_codes = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Fail)
        .map(|item| format!("{}:{}", item.key, item.code))
        .collect::<Vec<_>>();

    BrowserAutomationReadinessReport {
        checks,
        pass,
        warn,
        fail,
        gate: if fail == 0 {
            "pass".to_string()
        } else {
            "fail".to_string()
        },
        reason_codes,
    }
}

fn append_multi_channel_channel_readiness_check(
    checks: &mut Vec<DoctorCheckResult>,
    channel: &str,
    ingress_dir: &Path,
    missing_prerequisites: &[String],
) {
    let ingress_file = ingress_dir.join(format!("{channel}.ndjson"));
    let ingress_exists = ingress_file.exists();
    let ingress_is_file = ingress_file.is_file();

    let (status, code, action) = if !missing_prerequisites.is_empty() {
        (
            DoctorStatus::Fail,
            "missing_prerequisites".to_string(),
            Some(missing_prerequisites.join(" ")),
        )
    } else if ingress_exists && !ingress_is_file {
        (
            DoctorStatus::Fail,
            "inbox_not_file".to_string(),
            Some(format!(
                "replace {} with a writable NDJSON file",
                ingress_file.display()
            )),
        )
    } else if !ingress_exists {
        (
            DoctorStatus::Warn,
            "inbox_missing".to_string(),
            Some(format!(
                "create {} or start the adapter that writes it",
                ingress_file.display()
            )),
        )
    } else {
        (DoctorStatus::Pass, "ready".to_string(), None)
    };

    checks.push(DoctorCheckResult {
        key: format!("multi_channel_live.channel.{}", channel),
        status,
        code,
        path: Some(ingress_file.display().to_string()),
        action,
    });
}

pub fn evaluate_multi_channel_live_readiness(
    config: &DoctorMultiChannelReadinessConfig,
) -> MultiChannelLiveReadinessReport {
    let mut checks = Vec::new();
    let credential_store = match load_credential_store(
        &config.credential_store_path,
        config.credential_store_encryption,
        config.credential_store_key.as_deref(),
    ) {
        Ok(store) => {
            let code = if config.credential_store_path.exists() {
                "readable".to_string()
            } else {
                "missing".to_string()
            };
            let status = if config.credential_store_path.exists() {
                DoctorStatus::Pass
            } else {
                DoctorStatus::Warn
            };
            checks.push(DoctorCheckResult {
                key: "multi_channel_live.credential_store".to_string(),
                status,
                code,
                path: Some(config.credential_store_path.display().to_string()),
                action: if config.credential_store_path.exists() {
                    None
                } else {
                    Some(
                        "set channel secrets via env vars or /integration-auth set <integration-id> <secret>"
                            .to_string(),
                    )
                },
            });
            Some(store)
        }
        Err(error) => {
            checks.push(DoctorCheckResult {
                key: "multi_channel_live.credential_store".to_string(),
                status: DoctorStatus::Fail,
                code: format!("load_error:{error}"),
                path: Some(config.credential_store_path.display().to_string()),
                action: Some(
                    "repair credential store contents or provide channel secrets via env vars"
                        .to_string(),
                ),
            });
            None
        }
    };

    if config.ingress_dir.exists() {
        checks.push(DoctorCheckResult {
            key: "multi_channel_live.ingress_dir".to_string(),
            status: if config.ingress_dir.is_dir() {
                DoctorStatus::Pass
            } else {
                DoctorStatus::Fail
            },
            code: if config.ingress_dir.is_dir() {
                "ready".to_string()
            } else {
                "not_dir".to_string()
            },
            path: Some(config.ingress_dir.display().to_string()),
            action: if config.ingress_dir.is_dir() {
                None
            } else {
                Some("set --multi-channel-live-ingress-dir to a directory path".to_string())
            },
        });
    } else {
        checks.push(DoctorCheckResult {
            key: "multi_channel_live.ingress_dir".to_string(),
            status: DoctorStatus::Fail,
            code: "missing".to_string(),
            path: Some(config.ingress_dir.display().to_string()),
            action: Some(
                "create --multi-channel-live-ingress-dir before starting live runner".to_string(),
            ),
        });
    }
    append_multi_channel_policy_readiness_checks(&mut checks, &config.ingress_dir);

    let telegram_ready = config
        .telegram_bot_token
        .as_ref()
        .map(|token| !token.trim().is_empty())
        .unwrap_or(false)
        || integration_secret_available(
            credential_store.as_ref(),
            MULTI_CHANNEL_READINESS_TELEGRAM_TOKEN_INTEGRATION_ID,
        );
    let telegram_missing = if telegram_ready {
        Vec::new()
    } else {
        vec![format!(
            "set {} or /integration-auth set {} <secret>",
            MULTI_CHANNEL_READINESS_TELEGRAM_TOKEN_ENV,
            MULTI_CHANNEL_READINESS_TELEGRAM_TOKEN_INTEGRATION_ID
        )]
    };
    append_multi_channel_channel_readiness_check(
        &mut checks,
        "telegram",
        &config.ingress_dir,
        &telegram_missing,
    );

    let discord_ready = config
        .discord_bot_token
        .as_ref()
        .map(|token| !token.trim().is_empty())
        .unwrap_or(false)
        || integration_secret_available(
            credential_store.as_ref(),
            MULTI_CHANNEL_READINESS_DISCORD_TOKEN_INTEGRATION_ID,
        );
    let discord_missing = if discord_ready {
        Vec::new()
    } else {
        vec![format!(
            "set {} or /integration-auth set {} <secret>",
            MULTI_CHANNEL_READINESS_DISCORD_TOKEN_ENV,
            MULTI_CHANNEL_READINESS_DISCORD_TOKEN_INTEGRATION_ID
        )]
    };
    append_multi_channel_channel_readiness_check(
        &mut checks,
        "discord",
        &config.ingress_dir,
        &discord_missing,
    );

    let whatsapp_access_token_ready = config
        .whatsapp_access_token
        .as_ref()
        .map(|token| !token.trim().is_empty())
        .unwrap_or(false)
        || integration_secret_available(
            credential_store.as_ref(),
            MULTI_CHANNEL_READINESS_WHATSAPP_ACCESS_TOKEN_INTEGRATION_ID,
        );
    let whatsapp_phone_number_id_ready = config
        .whatsapp_phone_number_id
        .as_ref()
        .map(|token| !token.trim().is_empty())
        .unwrap_or(false)
        || integration_secret_available(
            credential_store.as_ref(),
            MULTI_CHANNEL_READINESS_WHATSAPP_PHONE_NUMBER_ID_INTEGRATION_ID,
        );
    let mut whatsapp_missing = Vec::new();
    if !whatsapp_access_token_ready {
        whatsapp_missing.push(format!(
            "set {} or /integration-auth set {} <secret>",
            MULTI_CHANNEL_READINESS_WHATSAPP_ACCESS_TOKEN_ENV,
            MULTI_CHANNEL_READINESS_WHATSAPP_ACCESS_TOKEN_INTEGRATION_ID
        ));
    }
    if !whatsapp_phone_number_id_ready {
        whatsapp_missing.push(format!(
            "set {} or /integration-auth set {} <value>",
            MULTI_CHANNEL_READINESS_WHATSAPP_PHONE_NUMBER_ID_ENV,
            MULTI_CHANNEL_READINESS_WHATSAPP_PHONE_NUMBER_ID_INTEGRATION_ID
        ));
    }
    append_multi_channel_channel_readiness_check(
        &mut checks,
        "whatsapp",
        &config.ingress_dir,
        &whatsapp_missing,
    );

    build_multi_channel_readiness_summary(checks)
}

fn append_multi_channel_policy_readiness_checks(
    checks: &mut Vec<DoctorCheckResult>,
    ingress_dir: &Path,
) {
    let state_dir_guess = ingress_dir.parent().unwrap_or(ingress_dir);
    let policy_path = tau_multi_channel::channel_policy_path_for_state_dir(state_dir_guess);
    if !policy_path.exists() {
        checks.push(DoctorCheckResult {
            key: "multi_channel_live.channel_policy".to_string(),
            status: DoctorStatus::Warn,
            code: "missing".to_string(),
            path: Some(policy_path.display().to_string()),
            action: Some(
                "create channel-policy.json to override default dm/group/allowFrom behavior"
                    .to_string(),
            ),
        });
        checks.push(DoctorCheckResult {
            key: "multi_channel_live.channel_policy.risk".to_string(),
            status: DoctorStatus::Warn,
            code: "unknown_without_policy_file".to_string(),
            path: Some(policy_path.display().to_string()),
            action: Some(
                "add explicit dmPolicy/allowFrom/groupPolicy/requireMention rules before production rollout"
                    .to_string(),
            ),
        });
        return;
    }

    let policy = match tau_multi_channel::load_multi_channel_policy_file(&policy_path) {
        Ok(policy) => {
            checks.push(DoctorCheckResult {
                key: "multi_channel_live.channel_policy".to_string(),
                status: DoctorStatus::Pass,
                code: "ready".to_string(),
                path: Some(policy_path.display().to_string()),
                action: None,
            });
            policy
        }
        Err(error) => {
            checks.push(DoctorCheckResult {
                key: "multi_channel_live.channel_policy".to_string(),
                status: DoctorStatus::Fail,
                code: format!("parse_error:{error}"),
                path: Some(policy_path.display().to_string()),
                action: Some("repair channel-policy.json schema and values".to_string()),
            });
            checks.push(DoctorCheckResult {
                key: "multi_channel_live.channel_policy.risk".to_string(),
                status: DoctorStatus::Fail,
                code: "blocked_by_policy_parse_error".to_string(),
                path: Some(policy_path.display().to_string()),
                action: Some("fix policy parse errors before rollout".to_string()),
            });
            return;
        }
    };

    let open_dm_channels = tau_multi_channel::collect_open_dm_risk_channels(&policy);
    if open_dm_channels.is_empty() {
        checks.push(DoctorCheckResult {
            key: "multi_channel_live.channel_policy.risk".to_string(),
            status: DoctorStatus::Pass,
            code: "no_open_dm_risk".to_string(),
            path: Some(policy_path.display().to_string()),
            action: None,
        });
        return;
    }

    let status = if policy.strict_mode {
        DoctorStatus::Fail
    } else {
        DoctorStatus::Warn
    };
    let code = if policy.strict_mode {
        "unsafe_open_dm_fail"
    } else {
        "unsafe_open_dm_warn"
    };
    let channel_list = open_dm_channels.join(",");
    checks.push(DoctorCheckResult {
        key: "multi_channel_live.channel_policy.risk".to_string(),
        status,
        code: code.to_string(),
        path: Some(policy_path.display().to_string()),
        action: Some(format!(
            "set allowFrom=allowlist_or_pairing or dmPolicy=deny for channels: {}",
            channel_list
        )),
    });
}

pub fn render_multi_channel_live_readiness_report(
    report: &MultiChannelLiveReadinessReport,
) -> String {
    let reason_codes = if report.reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.reason_codes.join(",")
    };

    let mut lines = vec![format!(
        "multi-channel live readiness summary: checks={} pass={} warn={} fail={} gate={} reason_codes={}",
        report.checks.len(),
        report.pass,
        report.warn,
        report.fail,
        report.gate,
        reason_codes
    )];
    for check in &report.checks {
        lines.push(format!(
            "multi-channel live readiness check: key={} status={} code={} path={} action={}",
            check.key,
            check.status.as_str(),
            check.code,
            check.path.as_deref().unwrap_or("none"),
            check.action.as_deref().unwrap_or("none"),
        ));
    }
    lines.join("\n")
}

pub fn render_multi_channel_live_readiness_report_json(
    report: &MultiChannelLiveReadinessReport,
) -> String {
    serde_json::json!({
        "summary": {
            "checks": report.checks.len(),
            "pass": report.pass,
            "warn": report.warn,
            "fail": report.fail,
            "gate": report.gate,
            "reason_codes": report.reason_codes,
        },
        "checks": report
            .checks
            .iter()
            .map(|check| {
                serde_json::json!({
                    "key": check.key,
                    "status": check.status.as_str(),
                    "code": check.code,
                    "path": check.path,
                    "action": check.action,
                })
            })
            .collect::<Vec<_>>(),
    })
    .to_string()
}

pub fn execute_multi_channel_live_readiness_preflight_command(cli: &Cli) -> Result<()> {
    let readiness_config = build_multi_channel_live_readiness_config(cli);
    let report = evaluate_multi_channel_live_readiness(&readiness_config);
    let output_format = if cli.multi_channel_live_readiness_json {
        MultiChannelReadinessOutputFormat::Json
    } else {
        MultiChannelReadinessOutputFormat::Text
    };
    let output = match output_format {
        MultiChannelReadinessOutputFormat::Text => {
            render_multi_channel_live_readiness_report(&report)
        }
        MultiChannelReadinessOutputFormat::Json => {
            render_multi_channel_live_readiness_report_json(&report)
        }
    };
    println!("{output}");
    if report.fail > 0 {
        let reason_codes = if report.reason_codes.is_empty() {
            "unknown".to_string()
        } else {
            report.reason_codes.join(",")
        };
        bail!(
            "multi-channel live readiness gate: status=fail fail={} reason_codes={}",
            report.fail,
            reason_codes
        );
    }
    Ok(())
}

pub fn evaluate_browser_automation_readiness(
    playwright_cli: &str,
) -> BrowserAutomationReadinessReport {
    let mut checks = Vec::new();

    let npx_available = is_executable_available("npx");
    checks.push(DoctorCheckResult {
        key: "browser_automation.npx".to_string(),
        status: if npx_available {
            DoctorStatus::Pass
        } else {
            DoctorStatus::Fail
        },
        code: if npx_available {
            "ready".to_string()
        } else {
            "missing".to_string()
        },
        path: None,
        action: if npx_available {
            None
        } else {
            Some("install Node.js/npm so `npx` is available".to_string())
        },
    });

    let cli_path = playwright_cli.trim();
    if cli_path.is_empty() {
        checks.push(DoctorCheckResult {
            key: "browser_automation.playwright_cli".to_string(),
            status: DoctorStatus::Fail,
            code: "invalid_config".to_string(),
            path: None,
            action: Some(
                "set --browser-automation-playwright-cli to a non-empty executable".to_string(),
            ),
        });
    } else if is_executable_available(cli_path) {
        checks.push(DoctorCheckResult {
            key: "browser_automation.playwright_cli".to_string(),
            status: DoctorStatus::Pass,
            code: "ready".to_string(),
            path: Some(cli_path.to_string()),
            action: None,
        });
    } else {
        checks.push(DoctorCheckResult {
            key: "browser_automation.playwright_cli".to_string(),
            status: DoctorStatus::Warn,
            code: "missing".to_string(),
            path: Some(cli_path.to_string()),
            action: Some(
                "install Playwright CLI (`npm install -g @playwright/mcp`) or point --browser-automation-playwright-cli to a wrapper script".to_string(),
            ),
        });
    }

    build_browser_automation_readiness_summary(checks)
}

pub fn render_browser_automation_readiness_report(
    report: &BrowserAutomationReadinessReport,
) -> String {
    let reason_codes = if report.reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.reason_codes.join(",")
    };

    let mut lines = vec![format!(
        "browser automation readiness summary: checks={} pass={} warn={} fail={} gate={} reason_codes={}",
        report.checks.len(),
        report.pass,
        report.warn,
        report.fail,
        report.gate,
        reason_codes
    )];
    for check in &report.checks {
        lines.push(format!(
            "browser automation readiness check: key={} status={} code={} path={} action={}",
            check.key,
            check.status.as_str(),
            check.code,
            check.path.as_deref().unwrap_or("none"),
            check.action.as_deref().unwrap_or("none"),
        ));
    }
    lines.join("\n")
}

pub fn render_browser_automation_readiness_report_json(
    report: &BrowserAutomationReadinessReport,
) -> String {
    serde_json::json!({
        "summary": {
            "checks": report.checks.len(),
            "pass": report.pass,
            "warn": report.warn,
            "fail": report.fail,
            "gate": report.gate,
            "reason_codes": report.reason_codes,
        },
        "checks": report
            .checks
            .iter()
            .map(|check| {
                serde_json::json!({
                    "key": check.key,
                    "status": check.status.as_str(),
                    "code": check.code,
                    "path": check.path,
                    "action": check.action,
                })
            })
            .collect::<Vec<_>>(),
    })
    .to_string()
}

pub fn execute_browser_automation_preflight_command(cli: &Cli) -> Result<()> {
    let report = evaluate_browser_automation_readiness(&cli.browser_automation_playwright_cli);
    let output_format = if cli.browser_automation_preflight_json {
        BrowserAutomationReadinessOutputFormat::Json
    } else {
        BrowserAutomationReadinessOutputFormat::Text
    };
    let output = match output_format {
        BrowserAutomationReadinessOutputFormat::Text => {
            render_browser_automation_readiness_report(&report)
        }
        BrowserAutomationReadinessOutputFormat::Json => {
            render_browser_automation_readiness_report_json(&report)
        }
    };
    println!("{output}");
    if report.fail > 0 {
        let reason_codes = if report.reason_codes.is_empty() {
            "unknown".to_string()
        } else {
            report.reason_codes.join(",")
        };
        bail!(
            "browser automation preflight gate: status=fail fail={} reason_codes={}",
            report.fail,
            reason_codes
        );
    }
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn run_doctor_checks(config: &DoctorCommandConfig) -> Vec<DoctorCheckResult> {
    run_doctor_checks_with_options(config, DoctorCheckOptions::default())
}

pub fn run_doctor_checks_with_options(
    config: &DoctorCommandConfig,
    options: DoctorCheckOptions,
) -> Vec<DoctorCheckResult> {
    run_doctor_checks_with_release_lookup(config, options, |channel| {
        resolve_latest_channel_release_cached(
            channel,
            release_lookup_url(),
            &config.release_lookup_cache_path,
            config.release_lookup_cache_ttl_ms,
        )
        .map(|resolution| resolution.latest)
    })
}

fn run_doctor_checks_with_release_lookup<F>(
    config: &DoctorCommandConfig,
    options: DoctorCheckOptions,
    release_lookup: F,
) -> Vec<DoctorCheckResult>
where
    F: Fn(ReleaseChannel) -> Result<Option<String>>,
{
    let mut checks = Vec::new();
    checks.push(DoctorCheckResult {
        key: "model".to_string(),
        status: DoctorStatus::Pass,
        code: config.model.clone(),
        path: None,
        action: None,
    });

    let mut configured_release_channel = None;
    match load_release_channel_store(&config.release_channel_path) {
        Ok(Some(channel)) => {
            configured_release_channel = Some(channel);
            checks.push(DoctorCheckResult {
                key: "release_channel".to_string(),
                status: DoctorStatus::Pass,
                code: channel.as_str().to_string(),
                path: Some(config.release_channel_path.display().to_string()),
                action: None,
            });
        }
        Ok(None) => {
            configured_release_channel = Some(ReleaseChannel::Stable);
            checks.push(DoctorCheckResult {
                key: "release_channel".to_string(),
                status: DoctorStatus::Pass,
                code: "default_stable".to_string(),
                path: Some(config.release_channel_path.display().to_string()),
                action: Some("run /release-channel set stable|beta|dev to persist".to_string()),
            });
        }
        Err(error) => {
            checks.push(DoctorCheckResult {
                key: "release_channel".to_string(),
                status: DoctorStatus::Fail,
                code: format!("invalid_store:{error}"),
                path: Some(config.release_channel_path.display().to_string()),
                action: Some("run /release-channel set stable|beta|dev to repair".to_string()),
            });
        }
    }

    let release_update_check = if !options.online {
        DoctorCheckResult {
            key: "release_update".to_string(),
            status: DoctorStatus::Warn,
            code: "skipped_offline".to_string(),
            path: Some(config.release_channel_path.display().to_string()),
            action: Some("run /doctor --online to include remote release checks".to_string()),
        }
    } else if let Some(channel) = configured_release_channel {
        match release_lookup(channel) {
            Ok(Some(latest)) => match compare_versions(env!("CARGO_PKG_VERSION"), &latest) {
                Some(std::cmp::Ordering::Less) => DoctorCheckResult {
                    key: "release_update".to_string(),
                    status: DoctorStatus::Warn,
                    code: "update_available".to_string(),
                    path: Some(config.release_channel_path.display().to_string()),
                    action: Some(format!(
                        "new release detected for channel={} current={} latest={}",
                        channel,
                        env!("CARGO_PKG_VERSION"),
                        latest
                    )),
                },
                Some(std::cmp::Ordering::Equal | std::cmp::Ordering::Greater) => {
                    DoctorCheckResult {
                        key: "release_update".to_string(),
                        status: DoctorStatus::Pass,
                        code: "up_to_date".to_string(),
                        path: Some(config.release_channel_path.display().to_string()),
                        action: None,
                    }
                }
                None => DoctorCheckResult {
                    key: "release_update".to_string(),
                    status: DoctorStatus::Warn,
                    code: "version_parse_unknown".to_string(),
                    path: Some(config.release_channel_path.display().to_string()),
                    action: Some(format!(
                        "unable to compare versions current={} latest={}",
                        env!("CARGO_PKG_VERSION"),
                        latest
                    )),
                },
            },
            Ok(None) => DoctorCheckResult {
                key: "release_update".to_string(),
                status: DoctorStatus::Warn,
                code: "no_release_records".to_string(),
                path: Some(config.release_channel_path.display().to_string()),
                action: Some("no releases returned by upstream; retry later".to_string()),
            },
            Err(error) => DoctorCheckResult {
                key: "release_update".to_string(),
                status: DoctorStatus::Warn,
                code: format!("lookup_error:{error}"),
                path: Some(config.release_channel_path.display().to_string()),
                action: Some("check network access and rerun /doctor --online".to_string()),
            },
        }
    } else {
        DoctorCheckResult {
            key: "release_update".to_string(),
            status: DoctorStatus::Warn,
            code: "lookup_skipped_invalid_store".to_string(),
            path: Some(config.release_channel_path.display().to_string()),
            action: Some(
                "run /release-channel set stable|beta|dev before online checks".to_string(),
            ),
        }
    };
    checks.push(release_update_check);

    for provider_check in &config.provider_keys {
        let mode_status = if provider_check.mode_supported {
            DoctorStatus::Pass
        } else {
            DoctorStatus::Fail
        };
        checks.push(DoctorCheckResult {
            key: format!("provider_auth_mode.{}", provider_check.provider),
            status: mode_status,
            code: provider_check.auth_mode.as_str().to_string(),
            path: None,
            action: if provider_check.mode_supported {
                None
            } else {
                Some(format!(
                    "set {} api-key",
                    provider_auth_mode_flag(provider_check.provider_kind)
                ))
            },
        });

        let (status, code, action) = if provider_check.auth_mode == ProviderAuthMethod::ApiKey {
            if provider_check.present {
                (DoctorStatus::Pass, "present".to_string(), None)
            } else {
                (
                    DoctorStatus::Fail,
                    "missing".to_string(),
                    Some(format!("set {}", provider_check.key_env_var)),
                )
            }
        } else {
            (
                DoctorStatus::Warn,
                "not_required_for_mode".to_string(),
                None,
            )
        };
        checks.push(DoctorCheckResult {
            key: format!("provider_key.{}", provider_check.provider),
            status,
            code,
            path: None,
            action,
        });

        if (provider_check.provider_kind == Provider::OpenAi
            && matches!(
                provider_check.auth_mode,
                ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken
            ))
            || (provider_check.provider_kind == Provider::Anthropic
                && matches!(
                    provider_check.auth_mode,
                    ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken
                ))
            || (provider_check.provider_kind == Provider::Google
                && matches!(
                    provider_check.auth_mode,
                    ProviderAuthMethod::OauthToken | ProviderAuthMethod::Adc
                ))
        {
            let (backend_flag, executable_flag, default_executable) =
                if provider_check.provider_kind == Provider::OpenAi {
                    ("--openai-codex-backend=true", "--openai-codex-cli", "codex")
                } else if provider_check.provider_kind == Provider::Anthropic {
                    (
                        "--anthropic-claude-backend=true",
                        "--anthropic-claude-cli",
                        "claude",
                    )
                } else {
                    (
                        "--google-gemini-backend=true",
                        "--google-gemini-cli",
                        "gemini",
                    )
                };
            let (status, code, action) = if !provider_check.login_backend_enabled {
                (
                    DoctorStatus::Fail,
                    "backend_disabled".to_string(),
                    Some(format!("set {backend_flag}")),
                )
            } else if provider_check.login_backend_available {
                (DoctorStatus::Pass, "ready".to_string(), None)
            } else {
                let executable = provider_check
                    .login_backend_executable
                    .as_deref()
                    .unwrap_or(default_executable);
                (
                    DoctorStatus::Fail,
                    "missing_executable".to_string(),
                    Some(format!(
                        "install '{}' or set {} to an available executable",
                        executable, executable_flag
                    )),
                )
            };
            checks.push(DoctorCheckResult {
                key: format!("provider_backend.{}", provider_check.provider),
                status,
                code,
                path: None,
                action,
            });
        }
    }

    if !config.session_enabled {
        checks.push(DoctorCheckResult {
            key: "session_path".to_string(),
            status: DoctorStatus::Warn,
            code: "session_disabled".to_string(),
            path: Some(config.session_path.display().to_string()),
            action: Some("omit --no-session to enable persistence".to_string()),
        });
    } else if config.session_path.exists() {
        match std::fs::metadata(&config.session_path) {
            Ok(metadata) if metadata.is_file() => checks.push(DoctorCheckResult {
                key: "session_path".to_string(),
                status: DoctorStatus::Pass,
                code: "readable".to_string(),
                path: Some(config.session_path.display().to_string()),
                action: None,
            }),
            Ok(_) => checks.push(DoctorCheckResult {
                key: "session_path".to_string(),
                status: DoctorStatus::Fail,
                code: "not_file".to_string(),
                path: Some(config.session_path.display().to_string()),
                action: Some("choose a regular file path for --session".to_string()),
            }),
            Err(error) => checks.push(DoctorCheckResult {
                key: "session_path".to_string(),
                status: DoctorStatus::Fail,
                code: format!("metadata_error:{error}"),
                path: Some(config.session_path.display().to_string()),
                action: Some("fix session path permissions".to_string()),
            }),
        }
    } else {
        let parent_exists = config
            .session_path
            .parent()
            .map(|parent| parent.exists())
            .unwrap_or(false);
        checks.push(DoctorCheckResult {
            key: "session_path".to_string(),
            status: if parent_exists {
                DoctorStatus::Warn
            } else {
                DoctorStatus::Fail
            },
            code: if parent_exists {
                "missing_will_create".to_string()
            } else {
                "missing_parent".to_string()
            },
            path: Some(config.session_path.display().to_string()),
            action: if parent_exists {
                Some("run a prompt or command to create the session file".to_string())
            } else {
                Some("create the parent directory for --session".to_string())
            },
        });
    }

    if config.skills_dir.exists() {
        match std::fs::metadata(&config.skills_dir) {
            Ok(metadata) if metadata.is_dir() => checks.push(DoctorCheckResult {
                key: "skills_dir".to_string(),
                status: DoctorStatus::Pass,
                code: "readable_dir".to_string(),
                path: Some(config.skills_dir.display().to_string()),
                action: None,
            }),
            Ok(_) => checks.push(DoctorCheckResult {
                key: "skills_dir".to_string(),
                status: DoctorStatus::Fail,
                code: "not_dir".to_string(),
                path: Some(config.skills_dir.display().to_string()),
                action: Some("set --skills-dir to an existing directory".to_string()),
            }),
            Err(error) => checks.push(DoctorCheckResult {
                key: "skills_dir".to_string(),
                status: DoctorStatus::Fail,
                code: format!("metadata_error:{error}"),
                path: Some(config.skills_dir.display().to_string()),
                action: Some("fix skills directory permissions".to_string()),
            }),
        }
    } else {
        checks.push(DoctorCheckResult {
            key: "skills_dir".to_string(),
            status: DoctorStatus::Warn,
            code: "missing".to_string(),
            path: Some(config.skills_dir.display().to_string()),
            action: Some("create --skills-dir or install at least one skill".to_string()),
        });
    }

    if config.skills_lock_path.exists() {
        match std::fs::read_to_string(&config.skills_lock_path) {
            Ok(_) => checks.push(DoctorCheckResult {
                key: "skills_lock".to_string(),
                status: DoctorStatus::Pass,
                code: "readable".to_string(),
                path: Some(config.skills_lock_path.display().to_string()),
                action: None,
            }),
            Err(error) => checks.push(DoctorCheckResult {
                key: "skills_lock".to_string(),
                status: DoctorStatus::Fail,
                code: format!("read_error:{error}"),
                path: Some(config.skills_lock_path.display().to_string()),
                action: Some("fix lockfile permissions or regenerate lockfile".to_string()),
            }),
        }
    } else {
        checks.push(DoctorCheckResult {
            key: "skills_lock".to_string(),
            status: DoctorStatus::Warn,
            code: "missing".to_string(),
            path: Some(config.skills_lock_path.display().to_string()),
            action: Some("run /skills-lock-write to generate lockfile".to_string()),
        });
    }

    match config.trust_root_path.as_ref() {
        Some(path) if path.exists() => match std::fs::read_to_string(path) {
            Ok(_) => checks.push(DoctorCheckResult {
                key: "trust_root".to_string(),
                status: DoctorStatus::Pass,
                code: "readable".to_string(),
                path: Some(path.display().to_string()),
                action: None,
            }),
            Err(error) => checks.push(DoctorCheckResult {
                key: "trust_root".to_string(),
                status: DoctorStatus::Fail,
                code: format!("read_error:{error}"),
                path: Some(path.display().to_string()),
                action: Some("fix trust-root file permissions".to_string()),
            }),
        },
        Some(path) => checks.push(DoctorCheckResult {
            key: "trust_root".to_string(),
            status: DoctorStatus::Warn,
            code: "missing".to_string(),
            path: Some(path.display().to_string()),
            action: Some("create trust-root file or adjust --skill-trust-root-file".to_string()),
        }),
        None => checks.push(DoctorCheckResult {
            key: "trust_root".to_string(),
            status: DoctorStatus::Warn,
            code: "not_configured".to_string(),
            path: None,
            action: Some("configure --skill-trust-root-file when using signed skills".to_string()),
        }),
    }

    checks.extend(
        evaluate_browser_automation_readiness(&config.browser_automation_playwright_cli).checks,
    );
    checks
        .extend(evaluate_multi_channel_live_readiness(&config.multi_channel_live_readiness).checks);

    checks
}

pub fn run_doctor_checks_with_lookup<F>(
    config: &DoctorCommandConfig,
    options: DoctorCheckOptions,
    release_lookup: F,
) -> Vec<DoctorCheckResult>
where
    F: Fn(ReleaseChannel) -> Result<Option<String>>,
{
    run_doctor_checks_with_release_lookup(config, options, release_lookup)
}

pub fn render_doctor_report(checks: &[DoctorCheckResult]) -> String {
    let pass = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Pass)
        .count();
    let warn = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Warn)
        .count();
    let fail = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Fail)
        .count();

    let mut lines = vec![format!(
        "doctor summary: checks={} pass={} warn={} fail={}",
        checks.len(),
        pass,
        warn,
        fail
    )];

    for check in checks {
        lines.push(format!(
            "doctor check: key={} status={} code={} path={} action={}",
            check.key,
            check.status.as_str(),
            check.code,
            check.path.as_deref().unwrap_or("none"),
            check.action.as_deref().unwrap_or("none")
        ));
    }

    lines.join("\n")
}

pub fn render_doctor_report_json(checks: &[DoctorCheckResult]) -> String {
    let pass = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Pass)
        .count();
    let warn = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Warn)
        .count();
    let fail = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Fail)
        .count();

    serde_json::json!({
        "summary": {
            "checks": checks.len(),
            "pass": pass,
            "warn": warn,
            "fail": fail,
        },
        "checks": checks
            .iter()
            .map(|check| {
                serde_json::json!({
                    "key": check.key,
                    "status": check.status.as_str(),
                    "code": check.code,
                    "path": check.path,
                    "action": check.action,
                })
            })
            .collect::<Vec<_>>()
    })
    .to_string()
}

pub fn execute_doctor_command(
    config: &DoctorCommandConfig,
    format: DoctorCommandOutputFormat,
) -> String {
    execute_doctor_command_with_options(config, format, DoctorCheckOptions::default())
}

pub fn execute_doctor_command_with_options(
    config: &DoctorCommandConfig,
    format: DoctorCommandOutputFormat,
    options: DoctorCheckOptions,
) -> String {
    let checks = run_doctor_checks_with_options(config, options);
    match format {
        DoctorCommandOutputFormat::Text => render_doctor_report(&checks),
        DoctorCommandOutputFormat::Json => render_doctor_report_json(&checks),
    }
}

pub fn execute_doctor_cli_command(config: &DoctorCommandConfig, command_args: &str) -> String {
    let args = match parse_doctor_command_args(command_args) {
        Ok(args) => args,
        Err(_) => return DOCTOR_USAGE.to_string(),
    };
    if args.online {
        execute_doctor_command_with_options(
            config,
            args.output_format,
            DoctorCheckOptions { online: true },
        )
    } else {
        execute_doctor_command(config, args.output_format)
    }
}

pub fn execute_policy_command(
    command_args: &str,
    tool_policy_json: &serde_json::Value,
) -> Result<String> {
    if !command_args.trim().is_empty() {
        bail!("{POLICY_USAGE}");
    }
    Ok(tool_policy_json.to_string())
}
