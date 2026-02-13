use anyhow::{anyhow, bail, Result};

use super::TransportHealthInspectTarget;

pub(super) fn parse_transport_health_inspect_target(
    raw: &str,
) -> Result<TransportHealthInspectTarget> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!(
            "invalid --transport-health-inspect '{}', expected slack, github, github:owner/repo, multi-channel, multi-agent, browser-automation, memory, dashboard, gateway, deployment, custom-command, or voice",
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
    if trimmed.eq_ignore_ascii_case("browser-automation")
        || trimmed.eq_ignore_ascii_case("browserautomation")
        || trimmed.eq_ignore_ascii_case("browser")
    {
        return Ok(TransportHealthInspectTarget::BrowserAutomation);
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
            "invalid --transport-health-inspect '{}', expected slack, github, github:owner/repo, multi-channel, multi-agent, browser-automation, memory, dashboard, gateway, deployment, custom-command, or voice",
            raw
        );
    };
    if !transport.eq_ignore_ascii_case("github") {
        bail!(
            "invalid --transport-health-inspect '{}', expected slack, github, github:owner/repo, multi-channel, multi-agent, browser-automation, memory, dashboard, gateway, deployment, custom-command, or voice",
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

pub(super) fn parse_github_repo_slug(raw: &str) -> Result<(String, String)> {
    let trimmed = raw.trim();
    let (owner, repo) = trimmed.split_once('/').ok_or_else(|| {
        anyhow!(
            "invalid --github-status-inspect target '{}', expected owner/repo",
            raw
        )
    })?;
    let owner = owner.trim();
    let repo = repo.trim();
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        bail!(
            "invalid --github-status-inspect target '{}', expected owner/repo",
            raw
        );
    }
    Ok((owner.to_string(), repo.to_string()))
}
