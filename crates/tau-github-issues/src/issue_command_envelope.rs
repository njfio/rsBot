#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `IssueCommandEnvelope` used across Tau components.
pub struct IssueCommandEnvelope<'a> {
    pub command: &'a str,
    pub remainder: &'a str,
}

pub fn parse_issue_command_envelope<'a>(
    body: &'a str,
    command_prefix: &str,
    usage: &str,
) -> Option<std::result::Result<IssueCommandEnvelope<'a>, String>> {
    let trimmed = body.trim();
    let mut pieces = trimmed.split_whitespace();
    let prefix = pieces.next()?;
    if prefix != command_prefix {
        return None;
    }

    let args = trimmed[prefix.len()..].trim();
    if args.is_empty() {
        return Some(Err(usage.to_string()));
    }

    let mut parts = args.splitn(2, char::is_whitespace);
    let command = parts.next().unwrap_or_default();
    let remainder = parts.next().unwrap_or_default().trim();
    Some(Ok(IssueCommandEnvelope { command, remainder }))
}

#[cfg(test)]
mod tests {
    use super::{parse_issue_command_envelope, IssueCommandEnvelope};

    #[test]
    fn unit_parse_issue_command_envelope_returns_none_for_non_matching_prefix() {
        let parsed = parse_issue_command_envelope("/other run hi", "/tau", "usage");
        assert!(parsed.is_none());
    }

    #[test]
    fn functional_parse_issue_command_envelope_splits_command_and_remainder() {
        let parsed = parse_issue_command_envelope("/tau run hello world", "/tau", "usage")
            .expect("matched")
            .expect("parsed");
        assert_eq!(
            parsed,
            IssueCommandEnvelope {
                command: "run",
                remainder: "hello world",
            }
        );
    }

    #[test]
    fn integration_parse_issue_command_envelope_trims_whitespace_and_preserves_empty_remainder() {
        let parsed = parse_issue_command_envelope("  /tau status   ", "/tau", "usage")
            .expect("matched")
            .expect("parsed");
        assert_eq!(
            parsed,
            IssueCommandEnvelope {
                command: "status",
                remainder: "",
            }
        );
    }

    #[test]
    fn regression_parse_issue_command_envelope_returns_usage_for_empty_arguments() {
        let error = parse_issue_command_envelope("/tau", "/tau", "usage")
            .expect("matched")
            .expect_err("usage");
        assert_eq!(error, "usage");
    }
}
