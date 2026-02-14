use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use tau_browser_automation::browser_automation_contract::load_browser_automation_contract_fixture;
use tau_browser_automation::browser_automation_live::{
    run_browser_automation_live_fixture_with_persistence, BrowserAutomationLivePersistenceConfig,
    BrowserAutomationLivePolicy, BrowserAutomationLiveRunSummary, BrowserSessionManager,
    PlaywrightCliActionExecutor,
};

#[derive(Debug, Clone)]
struct HarnessCli {
    fixture: PathBuf,
    state_dir: PathBuf,
    playwright_cli: String,
    summary_json_out: PathBuf,
    artifact_retention_days: Option<u64>,
    action_timeout_ms: u64,
    max_actions_per_case: usize,
    allow_unsafe_actions: bool,
}

impl HarnessCli {
    fn parse() -> Result<Self> {
        let mut fixture: Option<PathBuf> = None;
        let mut state_dir: Option<PathBuf> = None;
        let mut playwright_cli: Option<String> = None;
        let mut summary_json_out: Option<PathBuf> = None;
        let mut artifact_retention_days: Option<u64> = Some(7);
        let mut action_timeout_ms: u64 = 5_000;
        let mut max_actions_per_case: usize = 8;
        let mut allow_unsafe_actions = false;

        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--help" | "-h" => {
                    print_usage();
                    std::process::exit(0);
                }
                "--fixture" => fixture = Some(PathBuf::from(require_arg_value(&mut args, &flag)?)),
                "--state-dir" => {
                    state_dir = Some(PathBuf::from(require_arg_value(&mut args, &flag)?))
                }
                "--playwright-cli" => {
                    playwright_cli = Some(require_arg_value(&mut args, &flag)?);
                }
                "--summary-json-out" => {
                    summary_json_out = Some(PathBuf::from(require_arg_value(&mut args, &flag)?));
                }
                "--artifact-retention-days" => {
                    let raw = require_arg_value(&mut args, &flag)?;
                    artifact_retention_days = if raw.trim().eq_ignore_ascii_case("none") {
                        None
                    } else {
                        Some(parse_positive_u64(&raw, &flag)?)
                    };
                }
                "--action-timeout-ms" => {
                    action_timeout_ms =
                        parse_positive_u64(&require_arg_value(&mut args, &flag)?, &flag)?;
                }
                "--max-actions-per-case" => {
                    max_actions_per_case =
                        parse_positive_usize(&require_arg_value(&mut args, &flag)?, &flag)?;
                }
                "--allow-unsafe-actions" => {
                    allow_unsafe_actions = true;
                }
                other => {
                    bail!("unknown argument '{other}'");
                }
            }
        }

        let fixture = fixture.context("--fixture is required")?;
        let state_dir = state_dir.context("--state-dir is required")?;
        let playwright_cli = playwright_cli.context("--playwright-cli is required")?;
        if playwright_cli.trim().is_empty() {
            bail!("--playwright-cli cannot be empty");
        }

        let summary_json_out =
            summary_json_out.unwrap_or_else(|| state_dir.join("browser-live-summary.json"));

        Ok(Self {
            fixture,
            state_dir,
            playwright_cli,
            summary_json_out,
            artifact_retention_days,
            action_timeout_ms,
            max_actions_per_case,
            allow_unsafe_actions,
        })
    }
}

fn require_arg_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .with_context(|| format!("missing value for {flag}"))
}

fn parse_positive_u64(raw: &str, flag: &str) -> Result<u64> {
    let parsed = raw
        .parse::<u64>()
        .with_context(|| format!("invalid numeric value for {flag}: '{raw}'"))?;
    if parsed == 0 {
        bail!("{flag} must be greater than 0");
    }
    Ok(parsed)
}

fn parse_positive_usize(raw: &str, flag: &str) -> Result<usize> {
    let parsed = raw
        .parse::<usize>()
        .with_context(|| format!("invalid numeric value for {flag}: '{raw}'"))?;
    if parsed == 0 {
        bail!("{flag} must be greater than 0");
    }
    Ok(parsed)
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    Ok(())
}

fn write_summary_json(path: &Path, summary: &BrowserAutomationLiveRunSummary) -> Result<()> {
    ensure_parent_dir(path)?;
    let rendered = serde_json::to_string_pretty(summary).context("serialize live summary json")?;
    std::fs::write(path, rendered).with_context(|| format!("failed to write {}", path.display()))
}

fn print_usage() {
    println!(
        "Usage: browser_automation_live_harness \
--fixture <path> \
--state-dir <path> \
--playwright-cli <path> \
[--summary-json-out <path>] \
[--artifact-retention-days <n|none>] \
[--action-timeout-ms <n>] \
[--max-actions-per-case <n>] \
[--allow-unsafe-actions]"
    );
}

fn run() -> Result<()> {
    let cli = HarnessCli::parse()?;
    if !cli.fixture.exists() {
        bail!("fixture '{}' does not exist", cli.fixture.display());
    }
    if !cli.fixture.is_file() {
        bail!("fixture '{}' must point to a file", cli.fixture.display());
    }
    std::fs::create_dir_all(&cli.state_dir)
        .with_context(|| format!("failed to create {}", cli.state_dir.display()))?;

    let fixture = load_browser_automation_contract_fixture(&cli.fixture)
        .with_context(|| format!("failed to load fixture '{}'", cli.fixture.display()))?;
    let persistence = BrowserAutomationLivePersistenceConfig {
        state_dir: cli.state_dir.clone(),
        artifact_retention_days: cli.artifact_retention_days,
    };
    let policy = BrowserAutomationLivePolicy {
        action_timeout_ms: cli.action_timeout_ms,
        max_actions_per_case: cli.max_actions_per_case,
        allow_unsafe_actions: cli.allow_unsafe_actions,
    };

    let executor = PlaywrightCliActionExecutor::new(cli.playwright_cli.clone())
        .context("failed to initialize playwright action executor")?;
    let mut manager = BrowserSessionManager::new(executor);
    let summary = run_browser_automation_live_fixture_with_persistence(
        &fixture,
        &mut manager,
        &policy,
        Some(&persistence),
    )?;
    write_summary_json(&cli.summary_json_out, &summary)?;

    println!(
        "browser automation live harness summary: discovered={} success={} malformed={} retryable_failures={} timeout_failures={} denied_unsafe_actions={} denied_action_limit={} artifact_records={} health_state={}",
        summary.discovered_cases,
        summary.success_cases,
        summary.malformed_cases,
        summary.retryable_failures,
        summary.timeout_failures,
        summary.denied_unsafe_actions,
        summary.denied_action_limit,
        summary.artifact_records,
        summary.health_state,
    );
    for (index, entry) in summary.timeline.iter().enumerate() {
        println!(
            "timeline[{index}] case_id={} operation={} action={} replay_step={} status_code={} error_code={} artifact_types={}",
            entry.case_id,
            entry.operation,
            entry.action,
            entry.replay_step,
            entry.status_code,
            if entry.error_code.trim().is_empty() {
                "none"
            } else {
                entry.error_code.as_str()
            },
            if entry.artifact_types.is_empty() {
                "none".to_string()
            } else {
                entry.artifact_types.join(",")
            },
        );
    }
    println!("summary_json={}", cli.summary_json_out.display());
    println!(
        "channel_artifact_index={}",
        cli.state_dir
            .join("channel-store/channels/browser-automation/live/artifacts/index.jsonl")
            .display()
    );
    println!(
        "channel_log={}",
        cli.state_dir
            .join("channel-store/channels/browser-automation/live/log.jsonl")
            .display()
    );

    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("browser automation live harness failed: {error:#}");
        std::process::exit(1);
    }
}
