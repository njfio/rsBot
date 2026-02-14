#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `TauIssueAuthCommandKind` values.
pub enum TauIssueAuthCommandKind {
    Status,
    Matrix,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TauIssueAuthCommand` used across Tau components.
pub struct TauIssueAuthCommand {
    pub kind: TauIssueAuthCommandKind,
    pub args: String,
}

pub fn parse_issue_auth_command<F>(
    remainder: &str,
    usage_message: &str,
    resolve_kind: F,
) -> std::result::Result<TauIssueAuthCommand, String>
where
    F: Fn(&str) -> std::result::Result<Option<TauIssueAuthCommandKind>, String>,
{
    let trimmed = remainder.trim();
    if trimmed.is_empty() {
        return Err(usage_message.to_string());
    }
    match resolve_kind(trimmed) {
        Ok(Some(kind)) => Ok(TauIssueAuthCommand {
            kind,
            args: trimmed.to_string(),
        }),
        Ok(None) => Err(usage_message.to_string()),
        Err(error) => Err(format!("auth error: {error}\n\n{usage_message}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_issue_auth_command, TauIssueAuthCommand, TauIssueAuthCommandKind};

    const USAGE: &str = "Usage: /tau auth <status|matrix> [flags]";

    #[test]
    fn unit_parse_issue_auth_command_rejects_empty_remainder() {
        let error = parse_issue_auth_command("", USAGE, |_args| Ok(None))
            .expect_err("expected usage error");
        assert_eq!(error, USAGE);
    }

    #[test]
    fn functional_parse_issue_auth_command_accepts_status_resolution() {
        let parsed = parse_issue_auth_command("status --json", USAGE, |_args| {
            Ok(Some(TauIssueAuthCommandKind::Status))
        })
        .expect("parse auth status command");
        assert_eq!(
            parsed,
            TauIssueAuthCommand {
                kind: TauIssueAuthCommandKind::Status,
                args: "status --json".to_string(),
            }
        );
    }

    #[test]
    fn integration_parse_issue_auth_command_returns_usage_for_unsupported_command() {
        let error =
            parse_issue_auth_command("login", USAGE, |_args| Ok(None)).expect_err("usage error");
        assert_eq!(error, USAGE);
    }

    #[test]
    fn regression_parse_issue_auth_command_surfaces_parse_errors_with_usage_context() {
        let error = parse_issue_auth_command("status --bad-flag", USAGE, |_args| {
            Err("unexpected argument '--bad-flag'".to_string())
        })
        .expect_err("parse error");
        assert_eq!(
            error,
            "auth error: unexpected argument '--bad-flag'\n\nUsage: /tau auth <status|matrix> [flags]"
        );
    }
}
