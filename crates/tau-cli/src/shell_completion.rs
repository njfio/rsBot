use std::io::{self, Write};

use clap::CommandFactory;
use clap_complete::{
    generate,
    shells::{Bash, Fish, Zsh},
};

use crate::{Cli, CliShellCompletion};

/// Canonical binary name used in generated completion scripts.
pub const SHELL_COMPLETION_COMMAND_NAME: &str = "tau-coding-agent";

/// Render a shell completion script for the requested shell.
pub fn render_shell_completion(
    shell: CliShellCompletion,
    mut writer: impl Write,
) -> io::Result<()> {
    let mut command = Cli::command();
    match shell {
        CliShellCompletion::Bash => generate(
            Bash,
            &mut command,
            SHELL_COMPLETION_COMMAND_NAME,
            &mut writer,
        ),
        CliShellCompletion::Zsh => generate(
            Zsh,
            &mut command,
            SHELL_COMPLETION_COMMAND_NAME,
            &mut writer,
        ),
        CliShellCompletion::Fish => generate(
            Fish,
            &mut command,
            SHELL_COMPLETION_COMMAND_NAME,
            &mut writer,
        ),
    }
    Ok(())
}
