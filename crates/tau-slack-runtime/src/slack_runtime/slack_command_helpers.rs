//! Slack `/tau` command parsing and command-response rendering helpers.

use super::{
    slack_render_helpers::{normalize_slack_message_text, slack_metadata_marker},
    SlackBridgeEvent, SlackCommand,
};
use crate::slack_helpers::truncate_for_slack;

pub(super) fn slack_command_usage() -> String {
    [
        "Supported `/tau` commands:",
        "- `/tau help`",
        "- `/tau status`",
        "- `/tau health`",
        "- `/tau stop`",
        "- `/tau artifacts [purge|run <run_id>|show <artifact_id>]`",
        "- `/tau canvas <create|update|show|export|import> ...`",
    ]
    .join("\n")
}

pub(super) fn rbac_action_for_slack_command(command: Option<&SlackCommand>) -> String {
    match command {
        Some(SlackCommand::Help) => "command:/tau-help".to_string(),
        Some(SlackCommand::Status) => "command:/tau-status".to_string(),
        Some(SlackCommand::Health) => "command:/tau-health".to_string(),
        Some(SlackCommand::Stop) => "command:/tau-stop".to_string(),
        Some(SlackCommand::Artifacts { .. }) => "command:/tau-artifacts".to_string(),
        Some(SlackCommand::ArtifactShow { .. }) => "command:/tau-artifacts-show".to_string(),
        Some(SlackCommand::Canvas { .. }) => "command:/tau-canvas".to_string(),
        Some(SlackCommand::Invalid { .. }) => "command:/tau-invalid".to_string(),
        None => "command:/tau-run".to_string(),
    }
}

pub(super) fn parse_slack_command(
    event: &SlackBridgeEvent,
    bot_user_id: &str,
) -> Option<SlackCommand> {
    let normalized = normalize_slack_message_text(event, bot_user_id);
    let trimmed = normalized.trim();
    let mut pieces = trimmed.split_whitespace();
    let command_prefix = pieces.next()?;
    if command_prefix != "/tau" {
        return None;
    }

    let args = trimmed[command_prefix.len()..].trim();
    if args.is_empty() {
        return Some(SlackCommand::Invalid {
            message: slack_command_usage(),
        });
    }
    let mut parts = args.splitn(2, char::is_whitespace);
    let command = parts.next().unwrap_or_default();
    let remainder = parts.next().unwrap_or_default().trim();
    let parsed = match command {
        "help" => {
            if remainder.is_empty() {
                SlackCommand::Help
            } else {
                SlackCommand::Invalid {
                    message: "Usage: /tau help".to_string(),
                }
            }
        }
        "status" => {
            if remainder.is_empty() {
                SlackCommand::Status
            } else {
                SlackCommand::Invalid {
                    message: "Usage: /tau status".to_string(),
                }
            }
        }
        "health" => {
            if remainder.is_empty() {
                SlackCommand::Health
            } else {
                SlackCommand::Invalid {
                    message: "Usage: /tau health".to_string(),
                }
            }
        }
        "stop" => {
            if remainder.is_empty() {
                SlackCommand::Stop
            } else {
                SlackCommand::Invalid {
                    message: "Usage: /tau stop".to_string(),
                }
            }
        }
        "artifacts" => {
            if remainder.is_empty() {
                SlackCommand::Artifacts {
                    purge: false,
                    run_id: None,
                }
            } else if remainder == "purge" {
                SlackCommand::Artifacts {
                    purge: true,
                    run_id: None,
                }
            } else {
                let mut artifact_args = remainder.split_whitespace();
                match (
                    artifact_args.next(),
                    artifact_args.next(),
                    artifact_args.next(),
                ) {
                    (Some("run"), Some(run_id), None) => SlackCommand::Artifacts {
                        purge: false,
                        run_id: Some(run_id.to_string()),
                    },
                    (Some("show"), Some(artifact_id), None) => SlackCommand::ArtifactShow {
                        artifact_id: artifact_id.to_string(),
                    },
                    _ => SlackCommand::Invalid {
                        message: "Usage: /tau artifacts [purge|run <run_id>|show <artifact_id>]"
                            .to_string(),
                    },
                }
            }
        }
        "canvas" => {
            if remainder.is_empty() {
                SlackCommand::Invalid {
                    message: "Usage: /tau canvas <create|update|show|export|import> ..."
                        .to_string(),
                }
            } else {
                SlackCommand::Canvas {
                    args: remainder.to_string(),
                }
            }
        }
        _ => SlackCommand::Invalid {
            message: format!(
                "Unknown command `{}`.\n\n{}",
                command,
                slack_command_usage()
            ),
        },
    };
    Some(parsed)
}

pub(super) fn render_slack_command_response(
    event: &SlackBridgeEvent,
    command_name: &str,
    status: &str,
    message: &str,
) -> String {
    let mut content = if message.trim().is_empty() {
        "Tau command response.".to_string()
    } else {
        message.trim().to_string()
    };
    let command = if command_name.trim().is_empty() {
        "unknown"
    } else {
        command_name.trim()
    };
    let status_label = if status.trim().is_empty() {
        "reported"
    } else {
        status.trim()
    };
    content.push_str("\n\n---\n");
    content.push_str(&format!(
        "{}\nTau command `{}` | status `{}`",
        slack_metadata_marker(&event.key),
        command,
        status_label
    ));
    truncate_for_slack(&content, 38_000)
}
