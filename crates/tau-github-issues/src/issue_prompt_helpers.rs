/// Collect non-empty assistant reply text from message history.
pub fn collect_assistant_reply(messages: &[tau_ai::Message]) -> String {
    let content = messages
        .iter()
        .filter(|message| message.role == tau_ai::MessageRole::Assistant)
        .map(|message| message.text_content())
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if content.trim().is_empty() {
        "I couldn't generate a textual response for this event.".to_string()
    } else {
        content
    }
}

/// Build summarize prompt for one issue thread with optional focus.
pub fn build_summarize_prompt(repo_slug: &str, issue_number: u64, focus: Option<&str>) -> String {
    match focus {
        Some(focus) => format!(
            "Summarize the current GitHub issue thread for {} issue #{} with focus on: {}.\nInclude decisions, open questions, blockers, and immediate next steps.",
            repo_slug,
            issue_number,
            focus
        ),
        None => format!(
            "Summarize the current GitHub issue thread for {} issue #{}.\nInclude decisions, open questions, blockers, and immediate next steps.",
            repo_slug,
            issue_number
        ),
    }
}

#[cfg(test)]
mod tests {
    use tau_ai::Message;

    use super::{build_summarize_prompt, collect_assistant_reply};

    #[test]
    fn unit_collect_assistant_reply_returns_default_when_no_assistant_text_exists() {
        let messages = vec![Message::user("hello"), Message::assistant_text("   ")];
        assert_eq!(
            collect_assistant_reply(&messages),
            "I couldn't generate a textual response for this event."
        );
    }

    #[test]
    fn functional_collect_assistant_reply_concatenates_non_empty_assistant_messages() {
        let messages = vec![
            Message::assistant_text("first response"),
            Message::user("ignored"),
            Message::assistant_text("second response"),
        ];
        assert_eq!(
            collect_assistant_reply(&messages),
            "first response\n\nsecond response"
        );
    }

    #[test]
    fn integration_build_summarize_prompt_with_focus_includes_focus_and_context() {
        let prompt = build_summarize_prompt("owner/repo", 42, Some("test failures"));
        assert!(prompt.contains("owner/repo issue #42"));
        assert!(prompt.contains("focus on: test failures"));
    }

    #[test]
    fn regression_build_summarize_prompt_without_focus_stable_shape() {
        let prompt = build_summarize_prompt("owner/repo", 7, None);
        assert_eq!(
            prompt,
            "Summarize the current GitHub issue thread for owner/repo issue #7.\nInclude decisions, open questions, blockers, and immediate next steps."
        );
    }
}
