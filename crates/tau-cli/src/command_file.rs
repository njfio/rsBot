use std::path::Path;

use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `CommandFileEntry` used across Tau components.
pub struct CommandFileEntry {
    pub line_number: usize,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `CommandFileReport` used across Tau components.
pub struct CommandFileReport {
    pub total: usize,
    pub executed: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub halted_early: bool,
}

pub fn parse_command_file(path: &Path) -> Result<Vec<CommandFileEntry>> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read command file {}", path.display()))?;
    let mut entries = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        entries.push(CommandFileEntry {
            line_number: index + 1,
            command: trimmed.to_string(),
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{parse_command_file, CommandFileEntry};

    fn unique_temp_path(file_name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should advance")
            .as_nanos();
        path.push(format!(
            "tau_cli_command_file_{file_name}_{}_{}_{}.txt",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed"),
            nanos
        ));
        path
    }

    #[test]
    fn unit_parse_command_file_skips_blank_and_comment_lines() {
        let path = unique_temp_path("unit");
        std::fs::write(
            &path,
            "# header\n\n/policy\n  # indented comment\n/session   \n",
        )
        .expect("write command file");

        let entries = parse_command_file(&path).expect("parse command file");
        std::fs::remove_file(&path).expect("remove temp file");

        assert_eq!(
            entries,
            vec![
                CommandFileEntry {
                    line_number: 3,
                    command: "/policy".to_string(),
                },
                CommandFileEntry {
                    line_number: 5,
                    command: "/session".to_string(),
                },
            ]
        );
    }

    #[test]
    fn functional_parse_command_file_preserves_input_order() {
        let path = unique_temp_path("functional");
        std::fs::write(&path, "/first\n/second\n/third\n").expect("write command file");

        let entries = parse_command_file(&path).expect("parse command file");
        std::fs::remove_file(&path).expect("remove temp file");

        let commands = entries
            .iter()
            .map(|entry| entry.command.as_str())
            .collect::<Vec<_>>();
        assert_eq!(commands, vec!["/first", "/second", "/third"]);
    }

    #[test]
    fn integration_parse_command_file_reads_nested_filesystem_path() {
        let mut directory = std::env::temp_dir();
        directory.push(format!(
            "tau_cli_command_file_integration_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time should advance")
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).expect("create temp directory");
        let path = directory.join("commands.txt");
        std::fs::write(&path, "/help\n").expect("write command file");

        let entries = parse_command_file(&path).expect("parse command file");

        std::fs::remove_file(&path).expect("remove temp file");
        std::fs::remove_dir_all(&directory).expect("remove temp directory");

        assert_eq!(
            entries,
            vec![CommandFileEntry {
                line_number: 1,
                command: "/help".to_string(),
            }]
        );
    }

    #[test]
    fn regression_parse_command_file_missing_path_returns_contextual_error() {
        let path = unique_temp_path("missing");
        let error = parse_command_file(&path).expect_err("missing file should fail");
        let message = error.to_string();
        assert!(message.contains("failed to read command file"));
        assert!(message.contains(path.to_string_lossy().as_ref()));
    }
}
