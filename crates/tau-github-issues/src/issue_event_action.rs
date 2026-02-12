#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `EventAction` values.
pub enum EventAction<Command> {
    RunPrompt { prompt: String },
    Command(Command),
}

pub fn event_action_from_body<Command, F>(body: &str, parse_command: F) -> EventAction<Command>
where
    F: Fn(&str) -> Option<Command>,
{
    match parse_command(body) {
        Some(command) => EventAction::Command(command),
        None => EventAction::RunPrompt {
            prompt: body.trim().to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{event_action_from_body, EventAction};

    #[test]
    fn unit_event_action_from_body_wraps_prompt_when_parser_returns_none() {
        let action = event_action_from_body::<String, _>("  hello  ", |_body| None);
        assert_eq!(
            action,
            EventAction::RunPrompt {
                prompt: "hello".to_string(),
            }
        );
    }

    #[test]
    fn functional_event_action_from_body_returns_command_when_parser_matches() {
        let action = event_action_from_body("run", |_body| Some("command".to_string()));
        assert_eq!(action, EventAction::Command("command".to_string()));
    }

    #[test]
    fn integration_event_action_from_body_passes_raw_body_to_parser() {
        let action = event_action_from_body("  /tau status  ", |body| {
            if body == "  /tau status  " {
                Some("parsed".to_string())
            } else {
                None
            }
        });
        assert_eq!(action, EventAction::Command("parsed".to_string()));
    }

    #[test]
    fn regression_event_action_from_body_preserves_empty_prompt_when_trimmed() {
        let action = event_action_from_body::<String, _>("   ", |_body| None);
        assert_eq!(
            action,
            EventAction::RunPrompt {
                prompt: "".to_string(),
            }
        );
    }
}
