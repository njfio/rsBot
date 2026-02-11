use super::*;
pub(crate) use tau_onboarding::profile_store::{
    default_profile_store_path, load_profile_store, save_profile_store, validate_profile_name,
};
#[cfg(test)]
pub(crate) use tau_onboarding::profile_store::{ProfileStoreFile, PROFILE_SCHEMA_VERSION};

pub(crate) const MACRO_SCHEMA_VERSION: u32 = 1;
pub(crate) const MACRO_USAGE: &str = "usage: /macro <save|run|list|show|delete> ...";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MacroCommand {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct MacroFile {
    pub(crate) schema_version: u32,
    pub(crate) macros: BTreeMap<String, Vec<String>>,
}

pub(crate) fn default_macro_config_path() -> Result<PathBuf> {
    Ok(std::env::current_dir()
        .context("failed to resolve current working directory")?
        .join(".tau")
        .join("macros.json"))
}

pub(crate) fn validate_macro_name(name: &str) -> Result<()> {
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

pub(crate) fn parse_macro_command(command_args: &str) -> Result<MacroCommand> {
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

pub(crate) fn load_macro_file(path: &Path) -> Result<BTreeMap<String, Vec<String>>> {
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

pub(crate) fn save_macro_file(path: &Path, macros: &BTreeMap<String, Vec<String>>) -> Result<()> {
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

fn load_macro_commands(commands_file: &Path) -> Result<Vec<String>> {
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

pub(crate) fn validate_macro_command_entry(command: &str) -> Result<()> {
    let parsed = parse_command(command)
        .ok_or_else(|| anyhow!("invalid macro command '{command}': command must start with '/'"))?;
    let name = canonical_command_name(parsed.name);
    if !COMMAND_NAMES.contains(&name) {
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

fn validate_macro_commands(commands: &[String]) -> Result<()> {
    for (index, command) in commands.iter().enumerate() {
        validate_macro_command_entry(command)
            .with_context(|| format!("macro command #{index} failed validation"))?;
    }
    Ok(())
}

pub(crate) fn render_macro_list(path: &Path, macros: &BTreeMap<String, Vec<String>>) -> String {
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

pub(crate) fn render_macro_show(path: &Path, name: &str, commands: &[String]) -> String {
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

pub(crate) const PROFILE_USAGE: &str = "usage: /profile <save|load|list|show|delete> ...";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProfileCommand {
    Save { name: String },
    Load { name: String },
    List,
    Show { name: String },
    Delete { name: String },
}

pub(crate) fn parse_profile_command(command_args: &str) -> Result<ProfileCommand> {
    const USAGE_SAVE: &str = "usage: /profile save <name>";
    const USAGE_LOAD: &str = "usage: /profile load <name>";
    const USAGE_LIST: &str = "usage: /profile list";
    const USAGE_SHOW: &str = "usage: /profile show <name>";
    const USAGE_DELETE: &str = "usage: /profile delete <name>";

    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("{PROFILE_USAGE}");
    }

    match tokens[0] {
        "save" => {
            if tokens.len() != 2 {
                bail!("{USAGE_SAVE}");
            }
            validate_profile_name(tokens[1])?;
            Ok(ProfileCommand::Save {
                name: tokens[1].to_string(),
            })
        }
        "load" => {
            if tokens.len() != 2 {
                bail!("{USAGE_LOAD}");
            }
            validate_profile_name(tokens[1])?;
            Ok(ProfileCommand::Load {
                name: tokens[1].to_string(),
            })
        }
        "list" => {
            if tokens.len() != 1 {
                bail!("{USAGE_LIST}");
            }
            Ok(ProfileCommand::List)
        }
        "show" => {
            if tokens.len() != 2 {
                bail!("{USAGE_SHOW}");
            }
            validate_profile_name(tokens[1])?;
            Ok(ProfileCommand::Show {
                name: tokens[1].to_string(),
            })
        }
        "delete" => {
            if tokens.len() != 2 {
                bail!("{USAGE_DELETE}");
            }
            validate_profile_name(tokens[1])?;
            Ok(ProfileCommand::Delete {
                name: tokens[1].to_string(),
            })
        }
        other => bail!("unknown subcommand '{}'; {PROFILE_USAGE}", other),
    }
}

pub(crate) fn render_profile_diffs(
    current: &ProfileDefaults,
    loaded: &ProfileDefaults,
) -> Vec<String> {
    fn to_list(values: &[String]) -> String {
        if values.is_empty() {
            "none".to_string()
        } else {
            values.join(",")
        }
    }

    let mut diffs = Vec::new();
    if current.model != loaded.model {
        diffs.push(format!(
            "diff: field=model current={} loaded={}",
            current.model, loaded.model
        ));
    }
    if current.fallback_models != loaded.fallback_models {
        diffs.push(format!(
            "diff: field=fallback_models current={} loaded={}",
            to_list(&current.fallback_models),
            to_list(&loaded.fallback_models)
        ));
    }
    if current.session.enabled != loaded.session.enabled {
        diffs.push(format!(
            "diff: field=session.enabled current={} loaded={}",
            current.session.enabled, loaded.session.enabled
        ));
    }
    if current.session.path != loaded.session.path {
        diffs.push(format!(
            "diff: field=session.path current={} loaded={}",
            current.session.path.as_deref().unwrap_or("none"),
            loaded.session.path.as_deref().unwrap_or("none")
        ));
    }
    if current.session.import_mode != loaded.session.import_mode {
        diffs.push(format!(
            "diff: field=session.import_mode current={} loaded={}",
            current.session.import_mode, loaded.session.import_mode
        ));
    }
    if current.policy.tool_policy_preset != loaded.policy.tool_policy_preset {
        diffs.push(format!(
            "diff: field=policy.tool_policy_preset current={} loaded={}",
            current.policy.tool_policy_preset, loaded.policy.tool_policy_preset
        ));
    }
    if current.policy.bash_profile != loaded.policy.bash_profile {
        diffs.push(format!(
            "diff: field=policy.bash_profile current={} loaded={}",
            current.policy.bash_profile, loaded.policy.bash_profile
        ));
    }
    if current.policy.bash_dry_run != loaded.policy.bash_dry_run {
        diffs.push(format!(
            "diff: field=policy.bash_dry_run current={} loaded={}",
            current.policy.bash_dry_run, loaded.policy.bash_dry_run
        ));
    }
    if current.policy.os_sandbox_mode != loaded.policy.os_sandbox_mode {
        diffs.push(format!(
            "diff: field=policy.os_sandbox_mode current={} loaded={}",
            current.policy.os_sandbox_mode, loaded.policy.os_sandbox_mode
        ));
    }
    if current.policy.enforce_regular_files != loaded.policy.enforce_regular_files {
        diffs.push(format!(
            "diff: field=policy.enforce_regular_files current={} loaded={}",
            current.policy.enforce_regular_files, loaded.policy.enforce_regular_files
        ));
    }
    if current.policy.bash_timeout_ms != loaded.policy.bash_timeout_ms {
        diffs.push(format!(
            "diff: field=policy.bash_timeout_ms current={} loaded={}",
            current.policy.bash_timeout_ms, loaded.policy.bash_timeout_ms
        ));
    }
    if current.policy.max_command_length != loaded.policy.max_command_length {
        diffs.push(format!(
            "diff: field=policy.max_command_length current={} loaded={}",
            current.policy.max_command_length, loaded.policy.max_command_length
        ));
    }
    if current.policy.max_tool_output_bytes != loaded.policy.max_tool_output_bytes {
        diffs.push(format!(
            "diff: field=policy.max_tool_output_bytes current={} loaded={}",
            current.policy.max_tool_output_bytes, loaded.policy.max_tool_output_bytes
        ));
    }
    if current.policy.max_file_read_bytes != loaded.policy.max_file_read_bytes {
        diffs.push(format!(
            "diff: field=policy.max_file_read_bytes current={} loaded={}",
            current.policy.max_file_read_bytes, loaded.policy.max_file_read_bytes
        ));
    }
    if current.policy.max_file_write_bytes != loaded.policy.max_file_write_bytes {
        diffs.push(format!(
            "diff: field=policy.max_file_write_bytes current={} loaded={}",
            current.policy.max_file_write_bytes, loaded.policy.max_file_write_bytes
        ));
    }
    if current.policy.allow_command_newlines != loaded.policy.allow_command_newlines {
        diffs.push(format!(
            "diff: field=policy.allow_command_newlines current={} loaded={}",
            current.policy.allow_command_newlines, loaded.policy.allow_command_newlines
        ));
    }
    if current.mcp.context_providers != loaded.mcp.context_providers {
        diffs.push(format!(
            "diff: field=mcp.context_providers current={} loaded={}",
            to_list(&current.mcp.context_providers),
            to_list(&loaded.mcp.context_providers)
        ));
    }
    if current.auth.openai != loaded.auth.openai {
        diffs.push(format!(
            "diff: field=auth.openai current={} loaded={}",
            current.auth.openai.as_str(),
            loaded.auth.openai.as_str()
        ));
    }
    if current.auth.anthropic != loaded.auth.anthropic {
        diffs.push(format!(
            "diff: field=auth.anthropic current={} loaded={}",
            current.auth.anthropic.as_str(),
            loaded.auth.anthropic.as_str()
        ));
    }
    if current.auth.google != loaded.auth.google {
        diffs.push(format!(
            "diff: field=auth.google current={} loaded={}",
            current.auth.google.as_str(),
            loaded.auth.google.as_str()
        ));
    }

    diffs
}

pub(crate) fn render_profile_list(
    profile_path: &Path,
    profiles: &BTreeMap<String, ProfileDefaults>,
) -> String {
    if profiles.is_empty() {
        return format!(
            "profile list: path={} profiles=0 names=none",
            profile_path.display()
        );
    }

    let mut lines = vec![format!(
        "profile list: path={} profiles={}",
        profile_path.display(),
        profiles.len()
    )];
    for name in profiles.keys() {
        lines.push(format!("profile: name={name}"));
    }
    lines.join("\n")
}

pub(crate) fn render_profile_show(
    profile_path: &Path,
    name: &str,
    profile: &ProfileDefaults,
) -> String {
    let fallback_models = if profile.fallback_models.is_empty() {
        "none".to_string()
    } else {
        profile.fallback_models.join(",")
    };
    let mut lines = vec![format!(
        "profile show: path={} name={} status=found",
        profile_path.display(),
        name
    )];
    lines.push(format!("value: model={}", profile.model));
    lines.push(format!("value: fallback_models={fallback_models}"));
    lines.push(format!(
        "value: session.enabled={}",
        profile.session.enabled
    ));
    lines.push(format!(
        "value: session.path={}",
        profile.session.path.as_deref().unwrap_or("none")
    ));
    lines.push(format!(
        "value: session.import_mode={}",
        profile.session.import_mode
    ));
    lines.push(format!(
        "value: policy.tool_policy_preset={}",
        profile.policy.tool_policy_preset
    ));
    lines.push(format!(
        "value: policy.bash_profile={}",
        profile.policy.bash_profile
    ));
    lines.push(format!(
        "value: policy.bash_dry_run={}",
        profile.policy.bash_dry_run
    ));
    lines.push(format!(
        "value: policy.os_sandbox_mode={}",
        profile.policy.os_sandbox_mode
    ));
    lines.push(format!(
        "value: policy.enforce_regular_files={}",
        profile.policy.enforce_regular_files
    ));
    lines.push(format!(
        "value: policy.bash_timeout_ms={}",
        profile.policy.bash_timeout_ms
    ));
    lines.push(format!(
        "value: policy.max_command_length={}",
        profile.policy.max_command_length
    ));
    lines.push(format!(
        "value: policy.max_tool_output_bytes={}",
        profile.policy.max_tool_output_bytes
    ));
    lines.push(format!(
        "value: policy.max_file_read_bytes={}",
        profile.policy.max_file_read_bytes
    ));
    lines.push(format!(
        "value: policy.max_file_write_bytes={}",
        profile.policy.max_file_write_bytes
    ));
    lines.push(format!(
        "value: policy.allow_command_newlines={}",
        profile.policy.allow_command_newlines
    ));
    lines.push(format!(
        "value: mcp.context_providers={}",
        if profile.mcp.context_providers.is_empty() {
            "none".to_string()
        } else {
            profile.mcp.context_providers.join(",")
        }
    ));
    lines.push(format!(
        "value: auth.openai={}",
        profile.auth.openai.as_str()
    ));
    lines.push(format!(
        "value: auth.anthropic={}",
        profile.auth.anthropic.as_str()
    ));
    lines.push(format!(
        "value: auth.google={}",
        profile.auth.google.as_str()
    ));
    lines.join("\n")
}

pub(crate) fn execute_profile_command(
    command_args: &str,
    profile_path: &Path,
    current_defaults: &ProfileDefaults,
) -> String {
    let command = match parse_profile_command(command_args) {
        Ok(command) => command,
        Err(error) => {
            return format!(
                "profile error: path={} error={error}",
                profile_path.display()
            );
        }
    };
    let mut profiles = match load_profile_store(profile_path) {
        Ok(profiles) => profiles,
        Err(error) => {
            return format!(
                "profile error: path={} error={error}",
                profile_path.display()
            );
        }
    };

    match command {
        ProfileCommand::Save { name } => {
            profiles.insert(name.clone(), current_defaults.clone());
            match save_profile_store(profile_path, &profiles) {
                Ok(()) => format!(
                    "profile save: path={} name={} status=saved",
                    profile_path.display(),
                    name
                ),
                Err(error) => format!(
                    "profile error: path={} name={} error={error}",
                    profile_path.display(),
                    name
                ),
            }
        }
        ProfileCommand::List => render_profile_list(profile_path, &profiles),
        ProfileCommand::Show { name } => {
            let Some(loaded) = profiles.get(&name) else {
                return format!(
                    "profile error: path={} name={} error=unknown profile '{}'",
                    profile_path.display(),
                    name,
                    name
                );
            };
            render_profile_show(profile_path, &name, loaded)
        }
        ProfileCommand::Load { name } => {
            let Some(loaded) = profiles.get(&name) else {
                return format!(
                    "profile error: path={} name={} error=unknown profile '{}'",
                    profile_path.display(),
                    name,
                    name
                );
            };
            let diffs = render_profile_diffs(current_defaults, loaded);
            if diffs.is_empty() {
                return format!(
                    "profile load: path={} name={} status=in_sync diffs=0",
                    profile_path.display(),
                    name
                );
            }
            let mut lines = vec![format!(
                "profile load: path={} name={} status=diff diffs={}",
                profile_path.display(),
                name,
                diffs.len()
            )];
            lines.extend(diffs);
            lines.join("\n")
        }
        ProfileCommand::Delete { name } => {
            if profiles.remove(&name).is_none() {
                return format!(
                    "profile error: path={} name={} error=unknown profile '{}'",
                    profile_path.display(),
                    name,
                    name
                );
            }
            match save_profile_store(profile_path, &profiles) {
                Ok(()) => format!(
                    "profile delete: path={} name={} status=deleted remaining={}",
                    profile_path.display(),
                    name,
                    profiles.len()
                ),
                Err(error) => format!(
                    "profile error: path={} name={} error={error}",
                    profile_path.display(),
                    name
                ),
            }
        }
    }
}
