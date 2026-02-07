use super::*;

pub(crate) const DOCTOR_USAGE: &str = "usage: /doctor [--json]";
pub(crate) const POLICY_USAGE: &str = "usage: /policy";
pub(crate) const AUDIT_SUMMARY_USAGE: &str = "usage: /audit-summary <path>";

#[derive(Debug, Default)]
pub(crate) struct ToolAuditAggregate {
    pub(crate) count: u64,
    pub(crate) error_count: u64,
    pub(crate) durations_ms: Vec<u64>,
}

#[derive(Debug, Default)]
pub(crate) struct ProviderAuditAggregate {
    pub(crate) count: u64,
    pub(crate) error_count: u64,
    pub(crate) durations_ms: Vec<u64>,
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) total_tokens: u64,
}

#[derive(Debug, Default)]
pub(crate) struct AuditSummary {
    pub(crate) record_count: u64,
    pub(crate) tool_event_count: u64,
    pub(crate) prompt_record_count: u64,
    pub(crate) tools: BTreeMap<String, ToolAuditAggregate>,
    pub(crate) providers: BTreeMap<String, ProviderAuditAggregate>,
}

pub(crate) fn summarize_audit_file(path: &Path) -> Result<AuditSummary> {
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

pub(crate) fn percentile_duration_ms(values: &[u64], percentile_numerator: u64) -> u64 {
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

pub(crate) fn render_audit_summary(path: &Path, summary: &AuditSummary) -> String {
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

pub(crate) fn execute_audit_summary_command(command_args: &str) -> String {
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
pub(crate) enum DoctorStatus {
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
pub(crate) struct DoctorCheckResult {
    pub(crate) key: String,
    pub(crate) status: DoctorStatus,
    pub(crate) code: String,
    pub(crate) path: Option<String>,
    pub(crate) action: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DoctorCommandOutputFormat {
    Text,
    Json,
}

pub(crate) fn parse_doctor_command_args(command_args: &str) -> Result<DoctorCommandOutputFormat> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Ok(DoctorCommandOutputFormat::Text);
    }
    if tokens.len() == 1 && tokens[0] == "--json" {
        return Ok(DoctorCommandOutputFormat::Json);
    }
    bail!("{DOCTOR_USAGE}");
}

pub(crate) fn provider_key_env_var(provider: Provider) -> &'static str {
    match provider {
        Provider::OpenAi => "OPENAI_API_KEY",
        Provider::Anthropic => "ANTHROPIC_API_KEY",
        Provider::Google => "GEMINI_API_KEY",
    }
}

pub(crate) fn provider_key_present(cli: &Cli, provider: Provider) -> bool {
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

pub(crate) fn build_doctor_command_config(
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
            DoctorProviderKeyStatus {
                provider_kind: provider,
                provider: provider.as_str().to_string(),
                key_env_var: provider_key_env_var(provider).to_string(),
                present: provider_key_present(cli, provider),
                auth_mode,
                mode_supported: capability.supported,
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
        session_enabled: !cli.no_session,
        session_path: cli.session.clone(),
        skills_dir: cli.skills_dir.clone(),
        skills_lock_path: skills_lock_path.to_path_buf(),
        trust_root_path: cli.skill_trust_root_file.clone(),
    }
}

pub(crate) fn run_doctor_checks(config: &DoctorCommandConfig) -> Vec<DoctorCheckResult> {
    let mut checks = Vec::new();
    checks.push(DoctorCheckResult {
        key: "model".to_string(),
        status: DoctorStatus::Pass,
        code: config.model.clone(),
        path: None,
        action: None,
    });

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

    checks
}

pub(crate) fn render_doctor_report(checks: &[DoctorCheckResult]) -> String {
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

pub(crate) fn render_doctor_report_json(checks: &[DoctorCheckResult]) -> String {
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

pub(crate) fn execute_doctor_command(
    config: &DoctorCommandConfig,
    format: DoctorCommandOutputFormat,
) -> String {
    let checks = run_doctor_checks(config);
    match format {
        DoctorCommandOutputFormat::Text => render_doctor_report(&checks),
        DoctorCommandOutputFormat::Json => render_doctor_report_json(&checks),
    }
}

pub(crate) fn execute_doctor_cli_command(
    config: &DoctorCommandConfig,
    command_args: &str,
) -> String {
    let format = match parse_doctor_command_args(command_args) {
        Ok(format) => format,
        Err(_) => return DOCTOR_USAGE.to_string(),
    };
    execute_doctor_command(config, format)
}

pub(crate) fn execute_policy_command(
    command_args: &str,
    tool_policy_json: &serde_json::Value,
) -> Result<String> {
    if !command_args.trim().is_empty() {
        bail!("{POLICY_USAGE}");
    }
    Ok(tool_policy_json.to_string())
}
