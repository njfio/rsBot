use anyhow::{bail, Result};
use std::path::Path;
use tau_cli::{parse_command_file, CliCommandFileErrorMode, CommandFileReport};

/// Public enum `CommandFileAction` used across Tau components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandFileAction {
    Continue,
    Exit,
}

pub fn command_file_error_mode_label(mode: CliCommandFileErrorMode) -> &'static str {
    match mode {
        CliCommandFileErrorMode::FailFast => "fail-fast",
        CliCommandFileErrorMode::ContinueOnError => "continue-on-error",
    }
}

pub fn execute_command_file_with_handler<F>(
    path: &Path,
    mode: CliCommandFileErrorMode,
    mut handle_command: F,
) -> Result<CommandFileReport>
where
    F: FnMut(&str) -> Result<CommandFileAction>,
{
    let entries = parse_command_file(path)?;
    let mut report = CommandFileReport {
        total: entries.len(),
        executed: 0,
        succeeded: 0,
        failed: 0,
        halted_early: false,
    };

    for entry in entries {
        report.executed += 1;

        if !entry.command.starts_with('/') {
            report.failed += 1;
            println!(
                "command file error: path={} line={} command={} error=command must start with '/'",
                path.display(),
                entry.line_number,
                entry.command
            );
            if mode == CliCommandFileErrorMode::FailFast {
                report.halted_early = true;
                break;
            }
            continue;
        }

        match handle_command(&entry.command) {
            Ok(CommandFileAction::Continue) => {
                report.succeeded += 1;
            }
            Ok(CommandFileAction::Exit) => {
                report.succeeded += 1;
                report.halted_early = true;
                println!(
                    "command file notice: path={} line={} command={} action=exit",
                    path.display(),
                    entry.line_number,
                    entry.command
                );
                break;
            }
            Err(error) => {
                report.failed += 1;
                println!(
                    "command file error: path={} line={} command={} error={error}",
                    path.display(),
                    entry.line_number,
                    entry.command
                );
                if mode == CliCommandFileErrorMode::FailFast {
                    report.halted_early = true;
                    break;
                }
            }
        }
    }

    println!(
        "command file summary: path={} mode={} total={} executed={} succeeded={} failed={} halted_early={}",
        path.display(),
        command_file_error_mode_label(mode),
        report.total,
        report.executed,
        report.succeeded,
        report.failed,
        report.halted_early
    );

    if mode == CliCommandFileErrorMode::FailFast && report.failed > 0 {
        bail!(
            "command file execution failed: path={} failed={} mode={}",
            path.display(),
            report.failed,
            command_file_error_mode_label(mode)
        );
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::{
        command_file_error_mode_label, execute_command_file_with_handler, CommandFileAction,
    };
    use anyhow::{anyhow, Result};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::{fs, path::PathBuf};
    use tau_cli::CliCommandFileErrorMode;

    static TEMP_COMMAND_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempCommandFile {
        path: PathBuf,
        root: PathBuf,
    }

    impl TempCommandFile {
        fn write(contents: &str) -> Self {
            let unique = TEMP_COMMAND_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir()
                .join("tau-startup-command-file-runtime")
                .join(format!("case-{}-{unique}", std::process::id()));
            fs::create_dir_all(&root).expect("create temp command file root");
            let path = root.join("commands.txt");
            fs::write(&path, contents).expect("write command file");
            Self { path, root }
        }
    }

    impl Drop for TempCommandFile {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn unit_command_file_error_mode_label_matches_cli_values() {
        assert_eq!(
            command_file_error_mode_label(CliCommandFileErrorMode::FailFast),
            "fail-fast"
        );
        assert_eq!(
            command_file_error_mode_label(CliCommandFileErrorMode::ContinueOnError),
            "continue-on-error"
        );
    }

    #[test]
    fn functional_execute_command_file_runs_script_and_returns_summary() {
        let command_file = TempCommandFile::write("/session\n/help session\n");
        let observed_commands = Arc::new(Mutex::new(Vec::<String>::new()));
        let observed_commands_clone = Arc::clone(&observed_commands);

        let report = execute_command_file_with_handler(
            &command_file.path,
            CliCommandFileErrorMode::FailFast,
            |command| {
                observed_commands_clone
                    .lock()
                    .expect("lock observed commands")
                    .push(command.to_string());
                Ok(CommandFileAction::Continue)
            },
        )
        .expect("command file should execute");

        assert_eq!(report.total, 2);
        assert_eq!(report.executed, 2);
        assert_eq!(report.succeeded, 2);
        assert_eq!(report.failed, 0);
        assert!(!report.halted_early);
        assert_eq!(
            observed_commands
                .lock()
                .expect("lock observed commands")
                .as_slice(),
            ["/session", "/help session"]
        );
    }

    #[test]
    fn integration_execute_command_file_continue_on_error_runs_remaining_commands() {
        let command_file = TempCommandFile::write("/session\nnot-command\n/help session\n");
        let observed_commands = Arc::new(Mutex::new(Vec::<String>::new()));
        let observed_commands_clone = Arc::clone(&observed_commands);

        let report = execute_command_file_with_handler(
            &command_file.path,
            CliCommandFileErrorMode::ContinueOnError,
            |command| {
                observed_commands_clone
                    .lock()
                    .expect("lock observed commands")
                    .push(command.to_string());
                Ok(CommandFileAction::Continue)
            },
        )
        .expect("continue-on-error mode should not fail");

        assert_eq!(report.total, 3);
        assert_eq!(report.executed, 3);
        assert_eq!(report.succeeded, 2);
        assert_eq!(report.failed, 1);
        assert!(!report.halted_early);
        assert_eq!(
            observed_commands
                .lock()
                .expect("lock observed commands")
                .as_slice(),
            ["/session", "/help session"]
        );
    }

    #[test]
    fn regression_execute_command_file_fail_fast_stops_on_malformed_line() {
        let command_file = TempCommandFile::write("/session\nnot-command\n/help session\n");
        let observed_commands = Arc::new(Mutex::new(Vec::<String>::new()));
        let observed_commands_clone = Arc::clone(&observed_commands);

        let error = execute_command_file_with_handler(
            &command_file.path,
            CliCommandFileErrorMode::FailFast,
            |command| {
                observed_commands_clone
                    .lock()
                    .expect("lock observed commands")
                    .push(command.to_string());
                Ok(CommandFileAction::Continue)
            },
        )
        .expect_err("fail-fast mode should return error for malformed line");

        assert!(error.to_string().contains("command file execution failed"));
        assert!(error.to_string().contains("mode=fail-fast"));
        assert_eq!(
            observed_commands
                .lock()
                .expect("lock observed commands")
                .as_slice(),
            ["/session"]
        );
    }

    #[test]
    fn regression_execute_command_file_exit_action_halts_early() {
        let command_file = TempCommandFile::write("/session\n/quit\n/help session\n");
        let observed_commands = Arc::new(Mutex::new(Vec::<String>::new()));
        let observed_commands_clone = Arc::clone(&observed_commands);

        let report = execute_command_file_with_handler(
            &command_file.path,
            CliCommandFileErrorMode::FailFast,
            |command| {
                observed_commands_clone
                    .lock()
                    .expect("lock observed commands")
                    .push(command.to_string());
                if command == "/quit" {
                    Ok(CommandFileAction::Exit)
                } else {
                    Ok(CommandFileAction::Continue)
                }
            },
        )
        .expect("exit action should still return successful report");

        assert_eq!(report.total, 3);
        assert_eq!(report.executed, 2);
        assert_eq!(report.succeeded, 2);
        assert_eq!(report.failed, 0);
        assert!(report.halted_early);
        assert_eq!(
            observed_commands
                .lock()
                .expect("lock observed commands")
                .as_slice(),
            ["/session", "/quit"]
        );
    }

    #[test]
    fn regression_execute_command_file_continue_on_error_surfaces_handler_errors() {
        let command_file = TempCommandFile::write("/session\n/help session\n");
        let observed_commands = Arc::new(Mutex::new(Vec::<String>::new()));
        let observed_commands_clone = Arc::clone(&observed_commands);

        let report = execute_command_file_with_handler(
            &command_file.path,
            CliCommandFileErrorMode::ContinueOnError,
            |command| -> Result<CommandFileAction> {
                observed_commands_clone
                    .lock()
                    .expect("lock observed commands")
                    .push(command.to_string());
                if command == "/help session" {
                    return Err(anyhow!("simulated handler failure"));
                }
                Ok(CommandFileAction::Continue)
            },
        )
        .expect("continue-on-error should allow handler failure");

        assert_eq!(report.total, 2);
        assert_eq!(report.executed, 2);
        assert_eq!(report.succeeded, 1);
        assert_eq!(report.failed, 1);
        assert!(!report.halted_early);
        assert_eq!(
            observed_commands
                .lock()
                .expect("lock observed commands")
                .as_slice(),
            ["/session", "/help session"]
        );
    }
}
