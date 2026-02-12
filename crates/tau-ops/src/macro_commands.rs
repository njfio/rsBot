use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use tau_cli::{canonical_command_name, parse_command};
use tau_core::write_text_atomic;

pub const MACRO_SCHEMA_VERSION: u32 = 1;
pub const MACRO_USAGE: &str = "usage: /macro <save|run|list|show|delete> ...";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroCommand {
    List,
    Save {
        name: String,
        commands_file: PathBuf,
    },
    Run {
        name: String,
        dry_run: bool,
    },
    Show {
        name: String,
    },
    Delete {
        name: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroExecutionAction {
    Continue,
    Exit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MacroFile {
    pub schema_version: u32,
    pub macros: BTreeMap<String, Vec<String>>,
}

pub fn default_macro_config_path() -> Result<PathBuf> {
    Ok(std::env::current_dir()
        .context("failed to resolve current working directory")?
        .join(".tau")
        .join("macros.json"))
}

pub fn validate_macro_name(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        bail!("macro name must not be empty");
    };
    if !first.is_ascii_alphabetic() {
        bail!("macro name '{}' must start with an ASCII letter", name);
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_')) {
        bail!(
            "macro name '{}' must contain only ASCII letters, digits, '-' or '_'",
            name
        );
    }
    Ok(())
}

pub fn parse_macro_command(command_args: &str) -> Result<MacroCommand> {
    const USAGE_LIST: &str = "usage: /macro list";
    const USAGE_SAVE: &str = "usage: /macro save <name> <commands_file>";
    const USAGE_RUN: &str = "usage: /macro run <name> [--dry-run]";
    const USAGE_SHOW: &str = "usage: /macro show <name>";
    const USAGE_DELETE: &str = "usage: /macro delete <name>";

    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("{MACRO_USAGE}");
    }

    match tokens[0] {
        "list" => {
            if tokens.len() != 1 {
                bail!("{USAGE_LIST}");
            }
            Ok(MacroCommand::List)
        }
        "save" => {
            if tokens.len() != 3 {
                bail!("{USAGE_SAVE}");
            }
            validate_macro_name(tokens[1])?;
            Ok(MacroCommand::Save {
                name: tokens[1].to_string(),
                commands_file: PathBuf::from(tokens[2]),
            })
        }
        "run" => {
            if !(2..=3).contains(&tokens.len()) {
                bail!("{USAGE_RUN}");
            }
            validate_macro_name(tokens[1])?;
            let dry_run = if tokens.len() == 3 {
                if tokens[2] != "--dry-run" {
                    bail!("{USAGE_RUN}");
                }
                true
            } else {
                false
            };
            Ok(MacroCommand::Run {
                name: tokens[1].to_string(),
                dry_run,
            })
        }
        "show" => {
            if tokens.len() != 2 {
                bail!("{USAGE_SHOW}");
            }
            validate_macro_name(tokens[1])?;
            Ok(MacroCommand::Show {
                name: tokens[1].to_string(),
            })
        }
        "delete" => {
            if tokens.len() != 2 {
                bail!("{USAGE_DELETE}");
            }
            validate_macro_name(tokens[1])?;
            Ok(MacroCommand::Delete {
                name: tokens[1].to_string(),
            })
        }
        other => bail!("unknown subcommand '{}'; {MACRO_USAGE}", other),
    }
}

pub fn load_macro_file(path: &Path) -> Result<BTreeMap<String, Vec<String>>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read macro file {}", path.display()))?;
    let parsed = serde_json::from_str::<MacroFile>(&raw)
        .with_context(|| format!("failed to parse macro file {}", path.display()))?;
    if parsed.schema_version != MACRO_SCHEMA_VERSION {
        bail!(
            "unsupported macro schema_version {} in {} (expected {})",
            parsed.schema_version,
            path.display(),
            MACRO_SCHEMA_VERSION
        );
    }
    Ok(parsed.macros)
}

pub fn save_macro_file(path: &Path, macros: &BTreeMap<String, Vec<String>>) -> Result<()> {
    let payload = MacroFile {
        schema_version: MACRO_SCHEMA_VERSION,
        macros: macros.clone(),
    };
    let mut encoded = serde_json::to_string_pretty(&payload).context("failed to encode macros")?;
    encoded.push('\n');
    let parent = path.parent().ok_or_else(|| {
        anyhow!(
            "macro config path {} does not have a parent directory",
            path.display()
        )
    })?;
    std::fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create macro config directory {}",
            parent.display()
        )
    })?;
    write_text_atomic(path, &encoded)
}

pub fn load_macro_commands(commands_file: &Path) -> Result<Vec<String>> {
    let raw = std::fs::read_to_string(commands_file)
        .with_context(|| format!("failed to read commands file {}", commands_file.display()))?;
    let commands = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with('#'))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if commands.is_empty() {
        bail!(
            "commands file {} does not contain runnable commands",
            commands_file.display()
        );
    }
    Ok(commands)
}

pub fn validate_macro_command_entry(command: &str, command_names: &[&str]) -> Result<()> {
    let parsed = parse_command(command)
        .ok_or_else(|| anyhow!("invalid macro command '{command}': command must start with '/'"))?;
    let name = canonical_command_name(parsed.name);
    if !command_names.contains(&name) {
        bail!("invalid macro command '{command}': unknown command '{name}'");
    }
    if matches!(name, "/quit" | "/exit") {
        bail!("invalid macro command '{command}': exit commands are not allowed");
    }
    if name == "/macro" {
        bail!("invalid macro command '{command}': nested /macro commands are not allowed");
    }
    Ok(())
}

pub fn validate_macro_commands(commands: &[String], command_names: &[&str]) -> Result<()> {
    for (index, command) in commands.iter().enumerate() {
        validate_macro_command_entry(command, command_names)
            .with_context(|| format!("macro command #{index} failed validation"))?;
    }
    Ok(())
}

pub fn render_macro_list(path: &Path, macros: &BTreeMap<String, Vec<String>>) -> String {
    let mut lines = vec![format!(
        "macro list: path={} count={}",
        path.display(),
        macros.len()
    )];
    if macros.is_empty() {
        lines.push("macros: none".to_string());
        return lines.join("\n");
    }
    for (name, commands) in macros {
        lines.push(format!("macro: name={} commands={}", name, commands.len()));
    }
    lines.join("\n")
}

pub fn render_macro_show(path: &Path, name: &str, commands: &[String]) -> String {
    let mut lines = vec![format!(
        "macro show: path={} name={} commands={}",
        path.display(),
        name,
        commands.len()
    )];
    for (index, command) in commands.iter().enumerate() {
        lines.push(format!("command: index={} value={command}", index));
    }
    lines.join("\n")
}

pub fn execute_macro_command_with_runner<F>(
    command_args: &str,
    macro_path: &Path,
    command_names: &[&str],
    mut run_command: F,
) -> String
where
    F: FnMut(&str) -> Result<MacroExecutionAction>,
{
    let command = match parse_macro_command(command_args) {
        Ok(command) => command,
        Err(error) => {
            return format!("macro error: path={} error={error}", macro_path.display());
        }
    };

    let mut macros = match load_macro_file(macro_path) {
        Ok(macros) => macros,
        Err(error) => {
            return format!("macro error: path={} error={error}", macro_path.display());
        }
    };

    match command {
        MacroCommand::List => render_macro_list(macro_path, &macros),
        MacroCommand::Save {
            name,
            commands_file,
        } => {
            let commands = match load_macro_commands(&commands_file) {
                Ok(commands) => commands,
                Err(error) => {
                    return format!(
                        "macro error: path={} name={} error={error}",
                        macro_path.display(),
                        name
                    );
                }
            };
            if let Err(error) = validate_macro_commands(&commands, command_names) {
                return format!(
                    "macro error: path={} name={} error={error}",
                    macro_path.display(),
                    name
                );
            }
            macros.insert(name.clone(), commands.clone());
            match save_macro_file(macro_path, &macros) {
                Ok(()) => format!(
                    "macro save: path={} name={} source={} commands={}",
                    macro_path.display(),
                    name,
                    commands_file.display(),
                    commands.len()
                ),
                Err(error) => format!(
                    "macro error: path={} name={} error={error}",
                    macro_path.display(),
                    name
                ),
            }
        }
        MacroCommand::Run { name, dry_run } => {
            let Some(commands) = macros.get(&name) else {
                return format!(
                    "macro error: path={} name={} error=unknown macro '{}'",
                    macro_path.display(),
                    name,
                    name
                );
            };
            if let Err(error) = validate_macro_commands(commands, command_names) {
                return format!(
                    "macro error: path={} name={} error={error}",
                    macro_path.display(),
                    name
                );
            }
            if dry_run {
                let mut lines = vec![format!(
                    "macro run: path={} name={} mode=dry-run commands={}",
                    macro_path.display(),
                    name,
                    commands.len()
                )];
                for command in commands {
                    lines.push(format!("plan: command={command}"));
                }
                return lines.join("\n");
            }

            for command in commands {
                match run_command(command) {
                    Ok(MacroExecutionAction::Continue) => {}
                    Ok(MacroExecutionAction::Exit) => {
                        return format!(
                            "macro error: path={} name={} error=exit command is not allowed in macros",
                            macro_path.display(),
                            name
                        );
                    }
                    Err(error) => {
                        return format!(
                            "macro error: path={} name={} command={} error={error}",
                            macro_path.display(),
                            name,
                            command
                        );
                    }
                }
            }

            format!(
                "macro run: path={} name={} mode=apply commands={} executed={}",
                macro_path.display(),
                name,
                commands.len(),
                commands.len()
            )
        }
        MacroCommand::Show { name } => {
            let Some(commands) = macros.get(&name) else {
                return format!(
                    "macro error: path={} name={} error=unknown macro '{}'",
                    macro_path.display(),
                    name,
                    name
                );
            };
            render_macro_show(macro_path, &name, commands)
        }
        MacroCommand::Delete { name } => {
            if !macros.contains_key(&name) {
                return format!(
                    "macro error: path={} name={} error=unknown macro '{}'",
                    macro_path.display(),
                    name,
                    name
                );
            }

            macros.remove(&name);
            match save_macro_file(macro_path, &macros) {
                Ok(()) => format!(
                    "macro delete: path={} name={} status=deleted remaining={}",
                    macro_path.display(),
                    name,
                    macros.len()
                ),
                Err(error) => format!(
                    "macro error: path={} name={} error={error}",
                    macro_path.display(),
                    name
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::{
        execute_macro_command_with_runner, load_macro_commands, load_macro_file,
        parse_macro_command, render_macro_list, render_macro_show, save_macro_file,
        validate_macro_command_entry, validate_macro_commands, validate_macro_name, MacroCommand,
        MacroExecutionAction, MacroFile, MACRO_SCHEMA_VERSION, MACRO_USAGE,
    };

    const TEST_COMMAND_NAMES: &[&str] = &["/help", "/session", "/skills-list", "/macro"];

    #[test]
    fn unit_validate_macro_name_accepts_and_rejects_expected_inputs() {
        validate_macro_name("quick_check-1").expect("valid macro name");

        let error = validate_macro_name("").expect_err("empty macro name should fail");
        assert!(error.to_string().contains("must not be empty"));

        let error =
            validate_macro_name("1quick").expect_err("macro name starting with digit should fail");
        assert!(error
            .to_string()
            .contains("must start with an ASCII letter"));

        let error = validate_macro_name("quick.check")
            .expect_err("macro name with punctuation should fail");
        assert!(error
            .to_string()
            .contains("must contain only ASCII letters, digits, '-' or '_'"));
    }

    #[test]
    fn functional_parse_macro_command_supports_lifecycle_and_usage_rules() {
        assert_eq!(
            parse_macro_command("list").expect("parse list"),
            MacroCommand::List
        );
        assert_eq!(
            parse_macro_command("save quick /tmp/quick.commands").expect("parse save"),
            MacroCommand::Save {
                name: "quick".to_string(),
                commands_file: PathBuf::from("/tmp/quick.commands"),
            }
        );
        assert_eq!(
            parse_macro_command("run quick").expect("parse run"),
            MacroCommand::Run {
                name: "quick".to_string(),
                dry_run: false,
            }
        );
        assert_eq!(
            parse_macro_command("run quick --dry-run").expect("parse dry run"),
            MacroCommand::Run {
                name: "quick".to_string(),
                dry_run: true,
            }
        );
        assert_eq!(
            parse_macro_command("show quick").expect("parse show"),
            MacroCommand::Show {
                name: "quick".to_string(),
            }
        );
        assert_eq!(
            parse_macro_command("delete quick").expect("parse delete"),
            MacroCommand::Delete {
                name: "quick".to_string(),
            }
        );

        let error = parse_macro_command("").expect_err("missing args should fail");
        assert!(error.to_string().contains(MACRO_USAGE));

        let error =
            parse_macro_command("run quick --apply").expect_err("unknown run flag should fail");
        assert!(error
            .to_string()
            .contains("usage: /macro run <name> [--dry-run]"));
    }

    #[test]
    fn unit_save_and_load_macro_file_round_trip_schema_and_values() {
        let temp = tempdir().expect("tempdir");
        let macro_path = temp.path().join(".tau").join("macros.json");
        let macros = BTreeMap::from([
            (
                "quick".to_string(),
                vec!["/help".to_string(), "/session".to_string()],
            ),
            ("lint".to_string(), vec!["/skills-list".to_string()]),
        ]);

        save_macro_file(&macro_path, &macros).expect("save macros");
        let loaded = load_macro_file(&macro_path).expect("load macros");
        assert_eq!(loaded, macros);

        let raw = std::fs::read_to_string(&macro_path).expect("read macro file");
        let parsed = serde_json::from_str::<MacroFile>(&raw).expect("parse macro file");
        assert_eq!(parsed.schema_version, MACRO_SCHEMA_VERSION);
        assert_eq!(parsed.macros, macros);
    }

    #[test]
    fn regression_load_macro_file_rejects_schema_mismatch() {
        let temp = tempdir().expect("tempdir");
        let macro_path = temp.path().join(".tau").join("macros.json");
        std::fs::create_dir_all(macro_path.parent().expect("macro parent")).expect("create parent");
        let payload = MacroFile {
            schema_version: MACRO_SCHEMA_VERSION + 1,
            macros: BTreeMap::new(),
        };
        std::fs::write(
            &macro_path,
            serde_json::to_string_pretty(&payload).expect("encode mismatch payload"),
        )
        .expect("write mismatch payload");

        let error = load_macro_file(&macro_path).expect_err("schema mismatch should fail");
        assert!(error
            .to_string()
            .contains("unsupported macro schema_version"));
    }

    #[test]
    fn functional_load_macro_commands_and_render_helpers_are_deterministic() {
        let temp = tempdir().expect("tempdir");
        let commands_file = temp.path().join("quick.commands");
        std::fs::write(
            &commands_file,
            "# comment\n\n  /help  \n/session\n   # another comment\n",
        )
        .expect("write command file");

        let commands = load_macro_commands(&commands_file).expect("load commands");
        assert_eq!(commands, vec!["/help".to_string(), "/session".to_string()]);

        let mut macros = BTreeMap::new();
        macros.insert("zeta".to_string(), vec!["/session".to_string()]);
        macros.insert("alpha".to_string(), vec!["/help".to_string()]);

        let list_output = render_macro_list(&commands_file, &macros);
        assert!(list_output.contains("macro list: path="));
        let alpha_index = list_output.find("macro: name=alpha").expect("alpha row");
        let zeta_index = list_output.find("macro: name=zeta").expect("zeta row");
        assert!(alpha_index < zeta_index);

        let show_output = render_macro_show(&commands_file, "alpha", &macros["alpha"]);
        assert!(show_output.contains("macro show: path="));
        assert!(show_output.contains("name=alpha"));
        assert!(show_output.contains("command: index=0 value=/help"));
    }

    #[test]
    fn integration_execute_macro_command_with_runner_handles_full_lifecycle() {
        let temp = tempdir().expect("tempdir");
        let macro_path = temp.path().join(".tau").join("macros.json");
        let commands_file = temp.path().join("quick.commands");
        std::fs::write(&commands_file, "/help\n/session\n").expect("write command file");

        let save_output = execute_macro_command_with_runner(
            &format!("save quick {}", commands_file.display()),
            &macro_path,
            TEST_COMMAND_NAMES,
            |_command| Ok(MacroExecutionAction::Continue),
        );
        assert!(save_output.contains("macro save: path="));
        assert!(save_output.contains("name=quick"));
        assert!(save_output.contains("commands=2"));

        let list_output = execute_macro_command_with_runner(
            "list",
            &macro_path,
            TEST_COMMAND_NAMES,
            |_command| Ok(MacroExecutionAction::Continue),
        );
        assert!(list_output.contains("macro list: path="));
        assert!(list_output.contains("count=1"));
        assert!(list_output.contains("macro: name=quick commands=2"));

        let show_output = execute_macro_command_with_runner(
            "show quick",
            &macro_path,
            TEST_COMMAND_NAMES,
            |_command| Ok(MacroExecutionAction::Continue),
        );
        assert!(show_output.contains("macro show: path="));
        assert!(show_output.contains("name=quick commands=2"));
        assert!(show_output.contains("command: index=0 value=/help"));

        let dry_run_output = execute_macro_command_with_runner(
            "run quick --dry-run",
            &macro_path,
            TEST_COMMAND_NAMES,
            |_command| Ok(MacroExecutionAction::Continue),
        );
        assert!(dry_run_output.contains("mode=dry-run"));
        assert!(dry_run_output.contains("plan: command=/help"));

        let mut executed = Vec::new();
        let run_output = execute_macro_command_with_runner(
            "run quick",
            &macro_path,
            TEST_COMMAND_NAMES,
            |command| {
                executed.push(command.to_string());
                Ok(MacroExecutionAction::Continue)
            },
        );
        assert_eq!(executed, vec!["/help".to_string(), "/session".to_string()]);
        assert!(run_output.contains("mode=apply"));
        assert!(run_output.contains("executed=2"));

        let delete_output = execute_macro_command_with_runner(
            "delete quick",
            &macro_path,
            TEST_COMMAND_NAMES,
            |_command| Ok(MacroExecutionAction::Continue),
        );
        assert!(delete_output.contains("status=deleted"));
        assert!(delete_output.contains("remaining=0"));
    }

    #[test]
    fn regression_execute_macro_command_with_runner_reports_runner_and_lookup_errors() {
        let temp = tempdir().expect("tempdir");
        let macro_path = temp.path().join(".tau").join("macros.json");
        let commands_file = temp.path().join("quick.commands");
        std::fs::write(&commands_file, "/help\n").expect("write command file");

        execute_macro_command_with_runner(
            &format!("save quick {}", commands_file.display()),
            &macro_path,
            TEST_COMMAND_NAMES,
            |_command| Ok(MacroExecutionAction::Continue),
        );

        let missing_output = execute_macro_command_with_runner(
            "run missing",
            &macro_path,
            TEST_COMMAND_NAMES,
            |_command| Ok(MacroExecutionAction::Continue),
        );
        assert!(missing_output.contains("unknown macro 'missing'"));

        let exit_output = execute_macro_command_with_runner(
            "run quick",
            &macro_path,
            TEST_COMMAND_NAMES,
            |_command| Ok(MacroExecutionAction::Exit),
        );
        assert!(exit_output.contains("exit command is not allowed in macros"));

        let error_output = execute_macro_command_with_runner(
            "run quick",
            &macro_path,
            TEST_COMMAND_NAMES,
            |_command| Err(anyhow!("runner failed")),
        );
        assert!(error_output.contains("command=/help"));
        assert!(error_output.contains("runner failed"));
    }

    #[test]
    fn unit_validate_macro_command_entry_and_collection_reject_invalid_commands() {
        validate_macro_command_entry("/help", TEST_COMMAND_NAMES).expect("known command");

        let nested_error = validate_macro_command_entry("/macro list", TEST_COMMAND_NAMES)
            .expect_err("nested macro should fail");
        assert!(nested_error
            .to_string()
            .contains("nested /macro commands are not allowed"));

        let unknown_error = validate_macro_command_entry("/unknown", TEST_COMMAND_NAMES)
            .expect_err("unknown command should fail");
        assert!(unknown_error.to_string().contains("unknown command"));

        let exit_error = validate_macro_command_entry("/exit", &["/quit"])
            .expect_err("exit command should fail");
        assert!(exit_error
            .to_string()
            .contains("exit commands are not allowed"));

        let invalid_batch = vec!["/help".to_string(), "not-command".to_string()];
        let error = validate_macro_commands(&invalid_batch, TEST_COMMAND_NAMES)
            .expect_err("batch validation should fail");
        assert!(error
            .to_string()
            .contains("macro command #1 failed validation"));
    }
}
