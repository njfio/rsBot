#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedCommand<'a> {
    pub name: &'a str,
    pub args: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    pub name: &'static str,
    pub usage: &'static str,
    pub description: &'static str,
    pub details: &'static str,
    pub example: &'static str,
}

pub fn parse_command(input: &str) -> Option<ParsedCommand<'_>> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or_default();
    let args = parts.next().map(str::trim).unwrap_or_default();
    Some(ParsedCommand { name, args })
}

pub fn canonical_command_name(name: &str) -> &str {
    if name == "/exit" {
        "/quit"
    } else {
        name
    }
}

pub fn normalize_help_topic(topic: &str) -> String {
    let trimmed = topic.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

pub fn render_help_overview(command_specs: &[CommandSpec]) -> String {
    let mut lines = vec!["commands:".to_string()];
    for spec in command_specs {
        lines.push(format!("  {:<22} {}", spec.usage, spec.description));
    }
    lines.push("tip: run /help <command> for details".to_string());
    lines.join("\n")
}

pub fn render_command_help(topic: &str, command_specs: &[CommandSpec]) -> Option<String> {
    let normalized = normalize_help_topic(topic);
    let command_name = canonical_command_name(&normalized);
    let spec = command_specs
        .iter()
        .find(|entry| entry.name == command_name)?;
    Some(format!(
        "command: {}\nusage: {}\n{}\n{}\nexample: {}",
        spec.name, spec.usage, spec.description, spec.details, spec.example
    ))
}

pub fn unknown_help_topic_message(topic: &str, command_names: &[&str]) -> String {
    match suggest_command(topic, command_names) {
        Some(suggestion) => format!(
            "unknown help topic: {topic}\ndid you mean {suggestion}?\nrun /help for command list"
        ),
        None => format!("unknown help topic: {topic}\nrun /help for command list"),
    }
}

pub fn unknown_command_message(command: &str, command_names: &[&str]) -> String {
    match suggest_command(command, command_names) {
        Some(suggestion) => {
            format!("unknown command: {command}\ndid you mean {suggestion}?\nrun /help for command list")
        }
        None => format!("unknown command: {command}\nrun /help for command list"),
    }
}

fn suggest_command<'a>(command: &str, command_names: &'a [&str]) -> Option<&'a str> {
    let command = canonical_command_name(command);
    if command.is_empty() {
        return None;
    }

    if let Some(prefix_match) = command_names
        .iter()
        .copied()
        .find(|candidate| candidate.starts_with(command))
    {
        return Some(prefix_match);
    }

    let mut best: Option<(&str, usize)> = None;
    for candidate in command_names.iter().copied() {
        let distance = levenshtein_distance(command, candidate);
        match best {
            Some((_, best_distance)) if distance >= best_distance => {}
            _ => best = Some((candidate, distance)),
        }
    }

    let (candidate, distance) = best?;
    let threshold = match command.len() {
        0..=4 => 1,
        5..=8 => 2,
        _ => 3,
    };
    if distance <= threshold {
        Some(candidate)
    } else {
        None
    }
}

fn levenshtein_distance(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    if a.is_empty() {
        return b.chars().count();
    }
    if b.is_empty() {
        return a.chars().count();
    }

    let b_chars = b.chars().collect::<Vec<_>>();
    let mut previous = (0..=b_chars.len()).collect::<Vec<_>>();
    let mut current = vec![0; b_chars.len() + 1];

    for (i, left) in a.chars().enumerate() {
        current[0] = i + 1;
        for (j, right) in b_chars.iter().enumerate() {
            let substitution_cost = if left == *right { 0 } else { 1 };
            let deletion = previous[j + 1] + 1;
            let insertion = current[j] + 1;
            let substitution = previous[j] + substitution_cost;
            current[j + 1] = deletion.min(insertion).min(substitution);
        }
        previous.clone_from_slice(&current);
    }

    previous[b_chars.len()]
}

#[cfg(test)]
mod tests {
    use super::{
        parse_command, render_command_help, render_help_overview, unknown_command_message,
        CommandSpec,
    };

    const TEST_SPECS: &[CommandSpec] = &[
        CommandSpec {
            name: "/policy",
            usage: "/policy",
            description: "Show policy",
            details: "Print effective policy JSON.",
            example: "/policy",
        },
        CommandSpec {
            name: "/quit",
            usage: "/quit",
            description: "Exit",
            details: "Alias: /exit",
            example: "/quit",
        },
    ];
    const TEST_NAMES: &[&str] = &["/policy", "/quit", "/exit"];

    #[test]
    fn unit_parse_command_returns_name_and_args_for_slash_input() {
        let parsed = parse_command("  /policy   --json ").expect("command should parse");
        assert_eq!(parsed.name, "/policy");
        assert_eq!(parsed.args, "--json");
    }

    #[test]
    fn functional_render_help_overview_lists_usage_and_descriptions() {
        let output = render_help_overview(TEST_SPECS);
        assert!(output.contains("commands:"));
        assert!(output.contains("/policy"));
        assert!(output.contains("Show policy"));
    }

    #[test]
    fn integration_render_command_help_normalizes_topic_and_supports_exit_alias() {
        let output = render_command_help("exit", TEST_SPECS).expect("help should render");
        assert!(output.contains("command: /quit"));
        assert!(output.contains("Alias: /exit"));
    }

    #[test]
    fn regression_unknown_command_message_only_suggests_for_close_match() {
        let close = unknown_command_message("/polciy", TEST_NAMES);
        assert!(close.contains("did you mean /policy?"));

        let far = unknown_command_message("/zzzzzzzz", TEST_NAMES);
        assert!(!far.contains("did you mean"));
    }
}
