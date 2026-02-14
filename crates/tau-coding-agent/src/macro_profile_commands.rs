use std::path::Path;

#[cfg(test)]
use anyhow::Result;
use tau_agent_core::Agent;
pub(crate) use tau_onboarding::profile_commands::execute_profile_command;
pub(crate) use tau_onboarding::profile_store::default_profile_store_path;
pub(crate) use tau_ops::default_macro_config_path;
use tau_ops::{execute_macro_command_with_runner, MacroExecutionAction};
use tau_session::SessionRuntime;

use crate::commands::{handle_command_with_session_import_mode, CommandAction, COMMAND_NAMES};
use crate::runtime_types::CommandExecutionContext;

#[cfg(test)]
pub(crate) use tau_onboarding::profile_commands::{
    parse_profile_command, render_profile_diffs, render_profile_list, render_profile_show,
    ProfileCommand, PROFILE_USAGE,
};
#[cfg(test)]
pub(crate) use tau_onboarding::profile_store::{
    load_profile_store, save_profile_store, validate_profile_name,
};
#[cfg(test)]
pub(crate) use tau_onboarding::profile_store::{ProfileStoreFile, PROFILE_SCHEMA_VERSION};

#[cfg(test)]
use tau_ops::validate_macro_command_entry as validate_macro_command_entry_with_command_names;
#[cfg(test)]
pub(crate) use tau_ops::{
    load_macro_file, parse_macro_command, render_macro_list, render_macro_show, save_macro_file,
    MacroCommand,
};
#[cfg(test)]
pub(crate) use tau_ops::{validate_macro_name, MacroFile, MACRO_SCHEMA_VERSION, MACRO_USAGE};

#[cfg(test)]
pub(crate) fn validate_macro_command_entry(command: &str) -> Result<()> {
    validate_macro_command_entry_with_command_names(command, COMMAND_NAMES)
}

pub(crate) fn execute_macro_command(
    command_args: &str,
    macro_path: &Path,
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    command_context: CommandExecutionContext<'_>,
) -> String {
    execute_macro_command_with_runner(command_args, macro_path, COMMAND_NAMES, |command| {
        match handle_command_with_session_import_mode(
            command,
            agent,
            session_runtime,
            command_context,
        ) {
            Ok(CommandAction::Continue) => Ok(MacroExecutionAction::Continue),
            Ok(CommandAction::Exit) => Ok(MacroExecutionAction::Exit),
            Err(error) => Err(error),
        }
    })
}
