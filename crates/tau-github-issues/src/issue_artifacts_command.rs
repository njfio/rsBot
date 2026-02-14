#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `ArtifactsIssueCommand` values.
pub enum ArtifactsIssueCommand {
    List,
    Purge,
    Run { run_id: String },
    Show { artifact_id: String },
}

pub fn parse_issue_artifacts_command(
    remainder: &str,
    usage: &str,
) -> std::result::Result<ArtifactsIssueCommand, String> {
    let trimmed = remainder.trim();
    if trimmed.is_empty() {
        return Ok(ArtifactsIssueCommand::List);
    }
    if trimmed == "purge" {
        return Ok(ArtifactsIssueCommand::Purge);
    }

    let mut parts = trimmed.split_whitespace();
    match (parts.next(), parts.next(), parts.next()) {
        (Some("run"), Some(run_id), None) => Ok(ArtifactsIssueCommand::Run {
            run_id: run_id.to_string(),
        }),
        (Some("show"), Some(artifact_id), None) => Ok(ArtifactsIssueCommand::Show {
            artifact_id: artifact_id.to_string(),
        }),
        _ => Err(usage.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_issue_artifacts_command, ArtifactsIssueCommand};

    const USAGE: &str = "Usage: /tau artifacts [purge|run <run_id>|show <artifact_id>]";

    #[test]
    fn unit_parse_issue_artifacts_command_supports_list_by_default() {
        let parsed = parse_issue_artifacts_command("", USAGE).expect("parse");
        assert_eq!(parsed, ArtifactsIssueCommand::List);
    }

    #[test]
    fn functional_parse_issue_artifacts_command_supports_purge_run_and_show() {
        let parsed = parse_issue_artifacts_command("purge", USAGE).expect("parse");
        assert_eq!(parsed, ArtifactsIssueCommand::Purge);

        let parsed = parse_issue_artifacts_command("run run-123", USAGE).expect("parse");
        assert_eq!(
            parsed,
            ArtifactsIssueCommand::Run {
                run_id: "run-123".to_string(),
            }
        );

        let parsed = parse_issue_artifacts_command("show artifact-456", USAGE).expect("parse");
        assert_eq!(
            parsed,
            ArtifactsIssueCommand::Show {
                artifact_id: "artifact-456".to_string(),
            }
        );
    }

    #[test]
    fn integration_parse_issue_artifacts_command_trims_surrounding_whitespace() {
        let parsed = parse_issue_artifacts_command("  run abc  ", USAGE).expect("parse");
        assert_eq!(
            parsed,
            ArtifactsIssueCommand::Run {
                run_id: "abc".to_string(),
            }
        );
    }

    #[test]
    fn regression_parse_issue_artifacts_command_returns_usage_for_invalid_inputs() {
        let error = parse_issue_artifacts_command("run", USAGE).expect_err("usage");
        assert_eq!(error, USAGE);

        let error = parse_issue_artifacts_command("show", USAGE).expect_err("usage");
        assert_eq!(error, USAGE);

        let error = parse_issue_artifacts_command("purge now", USAGE).expect_err("usage");
        assert_eq!(error, USAGE);
    }
}
