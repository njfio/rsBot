#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueCoreCommand {
    Run { prompt: String },
    Stop,
    Status,
    Health,
    Compact,
    Help,
    Canvas { args: String },
    Summarize { focus: Option<String> },
}

pub fn parse_issue_core_command(
    command: &str,
    remainder: &str,
) -> Option<std::result::Result<IssueCoreCommand, String>> {
    let parsed = match command {
        "run" => {
            if remainder.is_empty() {
                Err("Usage: /tau run <prompt>".to_string())
            } else {
                Ok(IssueCoreCommand::Run {
                    prompt: remainder.to_string(),
                })
            }
        }
        "stop" => {
            if remainder.is_empty() {
                Ok(IssueCoreCommand::Stop)
            } else {
                Err("Usage: /tau stop".to_string())
            }
        }
        "status" => {
            if remainder.is_empty() {
                Ok(IssueCoreCommand::Status)
            } else {
                Err("Usage: /tau status".to_string())
            }
        }
        "health" => {
            if remainder.is_empty() {
                Ok(IssueCoreCommand::Health)
            } else {
                Err("Usage: /tau health".to_string())
            }
        }
        "compact" => {
            if remainder.is_empty() {
                Ok(IssueCoreCommand::Compact)
            } else {
                Err("Usage: /tau compact".to_string())
            }
        }
        "help" => {
            if remainder.is_empty() {
                Ok(IssueCoreCommand::Help)
            } else {
                Err("Usage: /tau help".to_string())
            }
        }
        "canvas" => {
            if remainder.is_empty() {
                Err("Usage: /tau canvas <create|update|show|export|import> ...".to_string())
            } else {
                Ok(IssueCoreCommand::Canvas {
                    args: remainder.to_string(),
                })
            }
        }
        "summarize" => Ok(IssueCoreCommand::Summarize {
            focus: (!remainder.is_empty()).then(|| remainder.to_string()),
        }),
        _ => return None,
    };
    Some(parsed)
}

#[cfg(test)]
mod tests {
    use super::{parse_issue_core_command, IssueCoreCommand};

    #[test]
    fn unit_parse_issue_core_command_returns_none_for_unknown_commands() {
        assert!(parse_issue_core_command("auth", "").is_none());
    }

    #[test]
    fn functional_parse_issue_core_command_parses_run_and_summarize() {
        let run = parse_issue_core_command("run", "hello world")
            .expect("known")
            .expect("valid");
        assert_eq!(
            run,
            IssueCoreCommand::Run {
                prompt: "hello world".to_string(),
            }
        );

        let summarize = parse_issue_core_command("summarize", "focus area")
            .expect("known")
            .expect("valid");
        assert_eq!(
            summarize,
            IssueCoreCommand::Summarize {
                focus: Some("focus area".to_string()),
            }
        );
    }

    #[test]
    fn integration_parse_issue_core_command_parses_control_commands() {
        let stop = parse_issue_core_command("stop", "")
            .expect("known")
            .expect("valid");
        assert_eq!(stop, IssueCoreCommand::Stop);

        let status = parse_issue_core_command("status", "")
            .expect("known")
            .expect("valid");
        assert_eq!(status, IssueCoreCommand::Status);

        let help = parse_issue_core_command("help", "")
            .expect("known")
            .expect("valid");
        assert_eq!(help, IssueCoreCommand::Help);
    }

    #[test]
    fn regression_parse_issue_core_command_returns_usage_for_invalid_shapes() {
        let run_error = parse_issue_core_command("run", "")
            .expect("known")
            .expect_err("usage");
        assert_eq!(run_error, "Usage: /tau run <prompt>");

        let stop_error = parse_issue_core_command("stop", "extra")
            .expect("known")
            .expect_err("usage");
        assert_eq!(stop_error, "Usage: /tau stop");

        let canvas_error = parse_issue_core_command("canvas", "")
            .expect("known")
            .expect_err("usage");
        assert_eq!(
            canvas_error,
            "Usage: /tau canvas <create|update|show|export|import> ..."
        );
    }
}
