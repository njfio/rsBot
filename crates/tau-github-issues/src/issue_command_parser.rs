use crate::issue_command_envelope::parse_issue_command_envelope;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `ParsedIssueCommand` values.
pub enum ParsedIssueCommand<Core, Special> {
    Core(Core),
    Special(Special),
    Invalid { message: String },
    Unknown { command: String },
}

pub fn parse_issue_command<Core, Special, FCore, FSpecial>(
    body: &str,
    command_prefix: &str,
    usage: &str,
    parse_core: FCore,
    parse_special: FSpecial,
) -> Option<ParsedIssueCommand<Core, Special>>
where
    FCore: Fn(&str, &str) -> Option<std::result::Result<Core, String>>,
    FSpecial: Fn(&str, &str) -> Option<std::result::Result<Special, String>>,
{
    let envelope = parse_issue_command_envelope(body, command_prefix, usage)?;
    match envelope {
        Err(message) => Some(ParsedIssueCommand::Invalid { message }),
        Ok(envelope) => {
            if let Some(core) = parse_core(envelope.command, envelope.remainder) {
                return Some(match core {
                    Ok(command) => ParsedIssueCommand::Core(command),
                    Err(message) => ParsedIssueCommand::Invalid { message },
                });
            }

            if let Some(special) = parse_special(envelope.command, envelope.remainder) {
                return Some(match special {
                    Ok(command) => ParsedIssueCommand::Special(command),
                    Err(message) => ParsedIssueCommand::Invalid { message },
                });
            }

            Some(ParsedIssueCommand::Unknown {
                command: envelope.command.to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_issue_command, ParsedIssueCommand};

    #[test]
    fn unit_parse_issue_command_returns_none_for_non_matching_prefix() {
        let parsed = parse_issue_command::<String, String, _, _>(
            "/other run hi",
            "/tau",
            "usage",
            |_command, _remainder| None,
            |_command, _remainder| None,
        );
        assert!(parsed.is_none());
    }

    #[test]
    fn functional_parse_issue_command_prefers_core_over_special() {
        let parsed = parse_issue_command(
            "/tau run hello",
            "/tau",
            "usage",
            |command, remainder| {
                if command == "run" {
                    Some(Ok(format!("core:{remainder}")))
                } else {
                    None
                }
            },
            |_command, _remainder| Some(Ok("special".to_string())),
        )
        .expect("parsed");
        assert_eq!(parsed, ParsedIssueCommand::Core("core:hello".to_string()));
    }

    #[test]
    fn integration_parse_issue_command_routes_special_and_unknown_commands() {
        let parsed = parse_issue_command::<String, String, _, _>(
            "/tau auth status",
            "/tau",
            "usage",
            |_command, _remainder| None,
            |command, remainder| {
                if command == "auth" {
                    Some(Ok(format!("special:{remainder}")))
                } else {
                    None
                }
            },
        )
        .expect("parsed");
        assert_eq!(
            parsed,
            ParsedIssueCommand::Special("special:status".to_string())
        );

        let unknown = parse_issue_command::<String, String, _, _>(
            "/tau unknown",
            "/tau",
            "usage",
            |_command, _remainder| None,
            |_command, _remainder| None,
        )
        .expect("parsed");
        assert_eq!(
            unknown,
            ParsedIssueCommand::Unknown {
                command: "unknown".to_string(),
            }
        );
    }

    #[test]
    fn regression_parse_issue_command_returns_invalid_for_usage_and_parse_errors() {
        let usage = parse_issue_command::<String, String, _, _>(
            "/tau",
            "/tau",
            "usage",
            |_command, _remainder| None,
            |_command, _remainder| None,
        )
        .expect("parsed");
        assert_eq!(
            usage,
            ParsedIssueCommand::Invalid {
                message: "usage".to_string(),
            }
        );

        let core_invalid = parse_issue_command::<String, String, _, _>(
            "/tau run",
            "/tau",
            "usage",
            |command, _remainder| {
                if command == "run" {
                    Some(Err("core usage".to_string()))
                } else {
                    None
                }
            },
            |_command, _remainder| None,
        )
        .expect("parsed");
        assert_eq!(
            core_invalid,
            ParsedIssueCommand::Invalid {
                message: "core usage".to_string(),
            }
        );
    }
}
