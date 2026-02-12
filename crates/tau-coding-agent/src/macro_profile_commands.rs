use super::*;

pub(crate) use tau_onboarding::profile_commands::execute_profile_command;
#[cfg(test)]
pub(crate) use tau_onboarding::profile_commands::{
    parse_profile_command, render_profile_diffs, render_profile_list, render_profile_show,
    ProfileCommand, PROFILE_USAGE,
};
pub(crate) use tau_onboarding::profile_store::default_profile_store_path;
#[cfg(test)]
pub(crate) use tau_onboarding::profile_store::{
    load_profile_store, save_profile_store, validate_profile_name,
};
#[cfg(test)]
pub(crate) use tau_onboarding::profile_store::{ProfileStoreFile, PROFILE_SCHEMA_VERSION};

#[cfg(test)]
use tau_ops::validate_macro_command_entry as validate_macro_command_entry_with_command_names;
use tau_ops::validate_macro_commands as validate_macro_commands_with_command_names;
pub(crate) use tau_ops::{
    default_macro_config_path, load_macro_commands, load_macro_file, parse_macro_command,
    render_macro_list, render_macro_show, save_macro_file, MacroCommand,
};
#[cfg(test)]
pub(crate) use tau_ops::{validate_macro_name, MacroFile, MACRO_SCHEMA_VERSION, MACRO_USAGE};

#[cfg(test)]
pub(crate) fn validate_macro_command_entry(command: &str) -> Result<()> {
    validate_macro_command_entry_with_command_names(command, COMMAND_NAMES)
}

fn validate_macro_commands(commands: &[String]) -> Result<()> {
    validate_macro_commands_with_command_names(commands, COMMAND_NAMES)
}

pub(crate) fn execute_macro_command(
    command_args: &str,
    macro_path: &Path,
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    command_context: CommandExecutionContext<'_>,
) -> String {
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
            if let Err(error) = validate_macro_commands(&commands) {
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
            if let Err(error) = validate_macro_commands(commands) {
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
                match handle_command_with_session_import_mode(
                    command,
                    agent,
                    session_runtime,
                    command_context.tool_policy_json,
                    command_context.session_import_mode,
                    command_context.profile_defaults,
                    command_context.skills_command_config,
                    command_context.auth_command_config,
                    command_context.model_catalog,
                    command_context.extension_commands,
                ) {
                    Ok(CommandAction::Continue) => {}
                    Ok(CommandAction::Exit) => {
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
