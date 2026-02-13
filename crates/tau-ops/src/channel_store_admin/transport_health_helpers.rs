use std::path::Path;

use anyhow::{bail, Context, Result};
use tau_cli::Cli;

use super::{TransportHealthInspectRow, TransportHealthInspectTarget, TransportHealthSnapshot};

pub(super) fn collect_transport_health_rows(
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

fn load_transport_health_snapshot(state_path: &Path) -> Result<TransportHealthSnapshot> {
    let raw = std::fs::read_to_string(state_path)
        .with_context(|| format!("failed to read state file {}", state_path.display()))?;
    let parsed = serde_json::from_str::<super::TransportHealthStateFile>(&raw)
        .with_context(|| format!("failed to parse state file {}", state_path.display()))?;
    Ok(parsed.health)
}

fn decode_repo_target_from_dir_name(dir_name: &str) -> String {
    if let Some((owner, repo)) = dir_name.split_once("__") {
        if !owner.is_empty() && !repo.is_empty() {
            return format!("{owner}/{repo}");
        }
    }
    dir_name.to_string()
}

pub(super) fn sanitize_for_path(raw: &str) -> String {
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
