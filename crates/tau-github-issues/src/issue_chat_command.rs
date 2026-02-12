#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueChatCommand {
    Start,
    Resume,
    Reset,
    Export,
    Status,
    Summary,
    Replay,
    Show {
        limit: usize,
    },
    Search {
        query: String,
        role: Option<String>,
        limit: usize,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct IssueChatParseConfig<'a> {
    pub show_default_limit: usize,
    pub show_max_limit: usize,
    pub search_max_limit: usize,
    pub usage: &'a str,
    pub show_usage: &'a str,
    pub search_usage: &'a str,
}

pub fn parse_issue_chat_command<F>(
    remainder: &str,
    config: IssueChatParseConfig<'_>,
    parse_search_args: F,
) -> std::result::Result<IssueChatCommand, String>
where
    F: Fn(&str) -> std::result::Result<(String, Option<String>, usize), String>,
{
    let trimmed = remainder.trim();
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let chat_command = parts.next();
    let chat_remainder = parts.next().unwrap_or_default().trim();

    match chat_command {
        Some("start") if chat_remainder.is_empty() => Ok(IssueChatCommand::Start),
        Some("resume") if chat_remainder.is_empty() => Ok(IssueChatCommand::Resume),
        Some("reset") if chat_remainder.is_empty() => Ok(IssueChatCommand::Reset),
        Some("export") if chat_remainder.is_empty() => Ok(IssueChatCommand::Export),
        Some("status") if chat_remainder.is_empty() => Ok(IssueChatCommand::Status),
        Some("summary") if chat_remainder.is_empty() => Ok(IssueChatCommand::Summary),
        Some("replay") if chat_remainder.is_empty() => Ok(IssueChatCommand::Replay),
        Some("show") => {
            if chat_remainder.is_empty() {
                Ok(IssueChatCommand::Show {
                    limit: config.show_default_limit,
                })
            } else {
                let mut show_parts = chat_remainder.split_whitespace();
                match (show_parts.next(), show_parts.next()) {
                    (Some(raw), None) => match raw.parse::<usize>() {
                        Ok(limit) if limit > 0 => Ok(IssueChatCommand::Show {
                            limit: limit.min(config.show_max_limit),
                        }),
                        _ => Err(config.show_usage.to_string()),
                    },
                    _ => Err(config.show_usage.to_string()),
                }
            }
        }
        Some("search") => {
            if chat_remainder.is_empty() {
                Err(config.search_usage.to_string())
            } else {
                match parse_search_args(chat_remainder) {
                    Ok((query, role, limit)) if limit <= config.search_max_limit => {
                        Ok(IssueChatCommand::Search { query, role, limit })
                    }
                    _ => Err(config.search_usage.to_string()),
                }
            }
        }
        None => Err(config.usage.to_string()),
        _ => Err(config.usage.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_issue_chat_command, IssueChatCommand, IssueChatParseConfig};

    const CHAT_USAGE: &str =
        "Usage: /tau chat <start|resume|reset|export|status|summary|replay|show [limit]|search <query>>";
    const SHOW_USAGE: &str = "Usage: /tau chat show [limit]";
    const SEARCH_USAGE: &str = "Usage: /tau chat search <query> [--role <role>] [--limit <n>]";
    const TEST_CONFIG: IssueChatParseConfig<'_> = IssueChatParseConfig {
        show_default_limit: 10,
        show_max_limit: 50,
        search_max_limit: 50,
        usage: CHAT_USAGE,
        show_usage: SHOW_USAGE,
        search_usage: SEARCH_USAGE,
    };

    fn parse_search(raw: &str) -> std::result::Result<(String, Option<String>, usize), String> {
        let mut parts = raw.split_whitespace();
        match (parts.next(), parts.next(), parts.next()) {
            (Some(query), None, None) => Ok((query.to_string(), None, 10)),
            (Some(query), Some("--role"), Some(role)) => {
                Ok((query.to_string(), Some(role.to_string()), 10))
            }
            _ => Err("invalid".to_string()),
        }
    }

    #[test]
    fn unit_parse_issue_chat_command_returns_usage_when_subcommand_missing() {
        let error = parse_issue_chat_command("", TEST_CONFIG, parse_search).expect_err("usage");
        assert_eq!(error, CHAT_USAGE);
    }

    #[test]
    fn functional_parse_issue_chat_command_supports_primary_subcommands() {
        let start = parse_issue_chat_command("start", TEST_CONFIG, parse_search).expect("parse");
        assert_eq!(start, IssueChatCommand::Start);

        let summary =
            parse_issue_chat_command("summary", TEST_CONFIG, parse_search).expect("parse");
        assert_eq!(summary, IssueChatCommand::Summary);

        let replay = parse_issue_chat_command("replay", TEST_CONFIG, parse_search).expect("parse");
        assert_eq!(replay, IssueChatCommand::Replay);
    }

    #[test]
    fn integration_parse_issue_chat_command_handles_show_and_search_limits() {
        let show = parse_issue_chat_command("show 500", TEST_CONFIG, parse_search).expect("parse");
        assert_eq!(show, IssueChatCommand::Show { limit: 50 });

        let search =
            parse_issue_chat_command("search alpha --role assistant", TEST_CONFIG, parse_search)
                .expect("parse");
        assert_eq!(
            search,
            IssueChatCommand::Search {
                query: "alpha".to_string(),
                role: Some("assistant".to_string()),
                limit: 10,
            }
        );
    }

    #[test]
    fn regression_parse_issue_chat_command_returns_specific_usage_for_invalid_paths() {
        let show_error =
            parse_issue_chat_command("show foo", TEST_CONFIG, parse_search).expect_err("show");
        assert_eq!(show_error, SHOW_USAGE);

        let search_error =
            parse_issue_chat_command("search", TEST_CONFIG, parse_search).expect_err("search");
        assert_eq!(search_error, SEARCH_USAGE);

        let command_error =
            parse_issue_chat_command("unknown", TEST_CONFIG, parse_search).expect_err("chat");
        assert_eq!(command_error, CHAT_USAGE);
    }
}
