#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `DemoIndexIssueCommand` values.
pub enum DemoIndexIssueCommand<RunCommand> {
    List,
    Report,
    Run(RunCommand),
}

pub fn parse_demo_index_issue_command<RunCommand, F>(
    remainder: &str,
    usage_message: &str,
    parse_run_command: F,
) -> std::result::Result<DemoIndexIssueCommand<RunCommand>, String>
where
    F: Fn(&str) -> std::result::Result<RunCommand, String>,
{
    let mut parts = remainder.splitn(2, char::is_whitespace);
    let subcommand = parts.next().unwrap_or_default().trim();
    let sub_remainder = parts.next().unwrap_or_default().trim();
    match subcommand {
        "list" if sub_remainder.is_empty() => Ok(DemoIndexIssueCommand::List),
        "report" if sub_remainder.is_empty() => Ok(DemoIndexIssueCommand::Report),
        "run" => parse_run_command(sub_remainder).map(DemoIndexIssueCommand::Run),
        _ => Err(usage_message.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_demo_index_issue_command, DemoIndexIssueCommand};

    const USAGE: &str = "Usage: /tau demo-index <list|run ...|report>";

    #[test]
    fn unit_parse_demo_index_issue_command_parses_list() {
        let parsed = parse_demo_index_issue_command::<String, _>("list", USAGE, |_raw| {
            Ok("ignored".to_string())
        })
        .expect("parse list command");
        assert!(matches!(parsed, DemoIndexIssueCommand::List));
    }

    #[test]
    fn functional_parse_demo_index_issue_command_parses_report() {
        let parsed = parse_demo_index_issue_command::<String, _>("report", USAGE, |_raw| {
            Ok("ignored".to_string())
        })
        .expect("parse report command");
        assert!(matches!(parsed, DemoIndexIssueCommand::Report));
    }

    #[test]
    fn integration_parse_demo_index_issue_command_delegates_run_parser() {
        let parsed = parse_demo_index_issue_command("run smoke --timeout 20", USAGE, |raw| {
            Ok(raw.to_string())
        })
        .expect("parse run command");
        assert!(matches!(
            parsed,
            DemoIndexIssueCommand::Run(args) if args == "smoke --timeout 20"
        ));
    }

    #[test]
    fn regression_parse_demo_index_issue_command_surfaces_usage_for_unknown_subcommand() {
        let error = parse_demo_index_issue_command::<String, _>("invalid", USAGE, |_raw| {
            Ok("ignored".to_string())
        })
        .expect_err("unknown subcommand should fail");
        assert_eq!(error, USAGE);
    }
}
