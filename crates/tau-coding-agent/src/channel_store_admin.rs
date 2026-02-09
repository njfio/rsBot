use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
enum TransportHealthInspectTarget {
    Slack,
    GithubAll,
    GithubRepo { owner: String, repo: String },
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
            "invalid --transport-health-inspect '{}', expected slack, github, or github:owner/repo",
            raw
        );
    }
    if trimmed.eq_ignore_ascii_case("slack") {
        return Ok(TransportHealthInspectTarget::Slack);
    }
    if trimmed.eq_ignore_ascii_case("github") {
        return Ok(TransportHealthInspectTarget::GithubAll);
    }

    let Some((transport, repo_slug)) = trimmed.split_once(':') else {
        bail!(
            "invalid --transport-health-inspect '{}', expected slack, github, or github:owner/repo",
            raw
        );
    };
    if !transport.eq_ignore_ascii_case("github") {
        bail!(
            "invalid --transport-health-inspect '{}', expected slack, github, or github:owner/repo",
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
        collect_transport_health_rows, parse_transport_health_inspect_target,
        render_transport_health_row, render_transport_health_rows, TransportHealthInspectRow,
        TransportHealthInspectTarget,
    };
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
        let github_repo_dir = github_root.join("owner__repo");
        std::fs::create_dir_all(&github_repo_dir).expect("create github repo dir");
        std::fs::create_dir_all(&slack_root).expect("create slack dir");

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

        let mut cli = Cli::parse_from(["tau-rs"]);
        cli.github_state_dir = github_root;
        cli.slack_state_dir = slack_root;

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

        let rendered =
            render_transport_health_rows(&[github_rows[0].clone(), slack_rows[0].clone()]);
        assert!(rendered.contains("transport=github"));
        assert!(rendered.contains("transport=slack"));
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
}
