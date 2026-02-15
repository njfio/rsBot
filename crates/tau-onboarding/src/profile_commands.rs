use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Result};

use crate::profile_store::{load_profile_store, save_profile_store, validate_profile_name};
use crate::startup_config::ProfileDefaults;

pub const PROFILE_USAGE: &str = "usage: /profile <save|load|list|show|delete> ...";

#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `ProfileCommand` values.
pub enum ProfileCommand {
    Save { name: String },
    Load { name: String },
    List,
    Show { name: String },
    Delete { name: String },
}

pub fn parse_profile_command(command_args: &str) -> Result<ProfileCommand> {
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

pub fn render_profile_diffs(current: &ProfileDefaults, loaded: &ProfileDefaults) -> Vec<String> {
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
    if current.policy.runtime_heartbeat_enabled != loaded.policy.runtime_heartbeat_enabled {
        diffs.push(format!(
            "diff: field=policy.runtime_heartbeat_enabled current={} loaded={}",
            current.policy.runtime_heartbeat_enabled, loaded.policy.runtime_heartbeat_enabled
        ));
    }
    if current.policy.runtime_heartbeat_interval_ms != loaded.policy.runtime_heartbeat_interval_ms {
        diffs.push(format!(
            "diff: field=policy.runtime_heartbeat_interval_ms current={} loaded={}",
            current.policy.runtime_heartbeat_interval_ms,
            loaded.policy.runtime_heartbeat_interval_ms
        ));
    }
    if current.policy.runtime_heartbeat_state_path != loaded.policy.runtime_heartbeat_state_path {
        diffs.push(format!(
            "diff: field=policy.runtime_heartbeat_state_path current={} loaded={}",
            current.policy.runtime_heartbeat_state_path, loaded.policy.runtime_heartbeat_state_path
        ));
    }
    if current.policy.runtime_self_repair_enabled != loaded.policy.runtime_self_repair_enabled {
        diffs.push(format!(
            "diff: field=policy.runtime_self_repair_enabled current={} loaded={}",
            current.policy.runtime_self_repair_enabled, loaded.policy.runtime_self_repair_enabled
        ));
    }
    if current.policy.runtime_self_repair_timeout_ms != loaded.policy.runtime_self_repair_timeout_ms
    {
        diffs.push(format!(
            "diff: field=policy.runtime_self_repair_timeout_ms current={} loaded={}",
            current.policy.runtime_self_repair_timeout_ms,
            loaded.policy.runtime_self_repair_timeout_ms
        ));
    }
    if current.policy.runtime_self_repair_max_retries
        != loaded.policy.runtime_self_repair_max_retries
    {
        diffs.push(format!(
            "diff: field=policy.runtime_self_repair_max_retries current={} loaded={}",
            current.policy.runtime_self_repair_max_retries,
            loaded.policy.runtime_self_repair_max_retries
        ));
    }
    if current.policy.runtime_self_repair_tool_builds_dir
        != loaded.policy.runtime_self_repair_tool_builds_dir
    {
        diffs.push(format!(
            "diff: field=policy.runtime_self_repair_tool_builds_dir current={} loaded={}",
            current.policy.runtime_self_repair_tool_builds_dir,
            loaded.policy.runtime_self_repair_tool_builds_dir
        ));
    }
    if current.policy.runtime_self_repair_orphan_max_age_seconds
        != loaded.policy.runtime_self_repair_orphan_max_age_seconds
    {
        diffs.push(format!(
            "diff: field=policy.runtime_self_repair_orphan_max_age_seconds current={} loaded={}",
            current.policy.runtime_self_repair_orphan_max_age_seconds,
            loaded.policy.runtime_self_repair_orphan_max_age_seconds
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

pub fn render_profile_list(
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

pub fn render_profile_show(profile_path: &Path, name: &str, profile: &ProfileDefaults) -> String {
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
        "value: policy.runtime_heartbeat_enabled={}",
        profile.policy.runtime_heartbeat_enabled
    ));
    lines.push(format!(
        "value: policy.runtime_heartbeat_interval_ms={}",
        profile.policy.runtime_heartbeat_interval_ms
    ));
    lines.push(format!(
        "value: policy.runtime_heartbeat_state_path={}",
        profile.policy.runtime_heartbeat_state_path
    ));
    lines.push(format!(
        "value: policy.runtime_self_repair_enabled={}",
        profile.policy.runtime_self_repair_enabled
    ));
    lines.push(format!(
        "value: policy.runtime_self_repair_timeout_ms={}",
        profile.policy.runtime_self_repair_timeout_ms
    ));
    lines.push(format!(
        "value: policy.runtime_self_repair_max_retries={}",
        profile.policy.runtime_self_repair_max_retries
    ));
    lines.push(format!(
        "value: policy.runtime_self_repair_tool_builds_dir={}",
        profile.policy.runtime_self_repair_tool_builds_dir
    ));
    lines.push(format!(
        "value: policy.runtime_self_repair_orphan_max_age_seconds={}",
        profile.policy.runtime_self_repair_orphan_max_age_seconds
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

pub fn execute_profile_command(
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use tau_provider::ProviderAuthMethod;
    use tempfile::tempdir;

    use super::{
        execute_profile_command, parse_profile_command, render_profile_diffs, render_profile_list,
        render_profile_show, ProfileCommand, PROFILE_USAGE,
    };
    use crate::profile_store::{
        default_profile_store_path, load_profile_store, save_profile_store, ProfileStoreFile,
        PROFILE_SCHEMA_VERSION,
    };
    use crate::startup_config::{
        ProfileAuthDefaults, ProfileDefaults, ProfileMcpDefaults, ProfilePolicyDefaults,
        ProfileSessionDefaults,
    };

    fn sample_profile_defaults() -> ProfileDefaults {
        ProfileDefaults {
            model: "openai/gpt-4o-mini".to_string(),
            fallback_models: vec![],
            session: ProfileSessionDefaults {
                enabled: true,
                path: Some(".tau/sessions/default.sqlite".to_string()),
                import_mode: "merge".to_string(),
            },
            policy: ProfilePolicyDefaults {
                tool_policy_preset: "balanced".to_string(),
                bash_profile: "balanced".to_string(),
                bash_dry_run: false,
                os_sandbox_mode: "off".to_string(),
                enforce_regular_files: true,
                bash_timeout_ms: 500,
                max_command_length: 4096,
                max_tool_output_bytes: 1024,
                max_file_read_bytes: 2048,
                max_file_write_bytes: 2048,
                allow_command_newlines: true,
                runtime_heartbeat_enabled: true,
                runtime_heartbeat_interval_ms: 5_000,
                runtime_heartbeat_state_path: ".tau/runtime-heartbeat/state.json".to_string(),
                runtime_self_repair_enabled: true,
                runtime_self_repair_timeout_ms: 300_000,
                runtime_self_repair_max_retries: 2,
                runtime_self_repair_tool_builds_dir: ".tau/tool-builds".to_string(),
                runtime_self_repair_orphan_max_age_seconds: 3_600,
            },
            mcp: ProfileMcpDefaults {
                context_providers: vec![],
            },
            auth: ProfileAuthDefaults::default(),
        }
    }

    #[test]
    fn functional_parse_profile_command_supports_lifecycle_subcommands_and_usage_errors() {
        assert_eq!(
            parse_profile_command("save baseline").expect("parse save"),
            ProfileCommand::Save {
                name: "baseline".to_string(),
            }
        );
        assert_eq!(
            parse_profile_command("load baseline").expect("parse load"),
            ProfileCommand::Load {
                name: "baseline".to_string(),
            }
        );
        assert_eq!(
            parse_profile_command("list").expect("parse list"),
            ProfileCommand::List
        );
        assert_eq!(
            parse_profile_command("show baseline").expect("parse show"),
            ProfileCommand::Show {
                name: "baseline".to_string(),
            }
        );
        assert_eq!(
            parse_profile_command("delete baseline").expect("parse delete"),
            ProfileCommand::Delete {
                name: "baseline".to_string(),
            }
        );

        let error = parse_profile_command("").expect_err("empty args should fail");
        assert!(error.to_string().contains(PROFILE_USAGE));

        let error = parse_profile_command("save").expect_err("missing name should fail");
        assert!(error.to_string().contains("usage: /profile save <name>"));

        let error = parse_profile_command("list extra")
            .expect_err("list with trailing arguments should fail");
        assert!(error.to_string().contains("usage: /profile list"));

        let error = parse_profile_command("show").expect_err("show missing name should fail");
        assert!(error.to_string().contains("usage: /profile show <name>"));

        let error =
            parse_profile_command("unknown baseline").expect_err("unknown subcommand should fail");
        assert!(error.to_string().contains("unknown subcommand 'unknown'"));
    }

    #[test]
    fn unit_save_and_load_profile_store_round_trip_schema_and_values() {
        let temp = tempdir().expect("tempdir");
        let profile_path = temp.path().join(".tau").join("profiles.json");
        let mut alternate = sample_profile_defaults();
        alternate.model = "google/gemini-2.5-pro".to_string();
        let profiles = BTreeMap::from([
            ("baseline".to_string(), sample_profile_defaults()),
            ("alt".to_string(), alternate.clone()),
        ]);

        save_profile_store(&profile_path, &profiles).expect("save profiles");
        let loaded = load_profile_store(&profile_path).expect("load profiles");
        assert_eq!(loaded, profiles);

        let raw = std::fs::read_to_string(&profile_path).expect("read profile file");
        let parsed = serde_json::from_str::<ProfileStoreFile>(&raw).expect("parse profile file");
        assert_eq!(parsed.schema_version, PROFILE_SCHEMA_VERSION);
        assert_eq!(parsed.profiles, profiles);
    }

    #[test]
    fn regression_load_profile_store_backfills_auth_defaults_for_legacy_profiles() {
        let temp = tempdir().expect("tempdir");
        let profile_path = temp.path().join(".tau").join("profiles.json");
        std::fs::create_dir_all(
            profile_path
                .parent()
                .expect("profile path should have parent"),
        )
        .expect("mkdir profile dir");
        std::fs::write(
            &profile_path,
            serde_json::json!({
                "schema_version": PROFILE_SCHEMA_VERSION,
                "profiles": {
                    "legacy": {
                        "model": "openai/gpt-4o-mini",
                        "fallback_models": [],
                        "session": {
                            "enabled": true,
                            "path": ".tau/sessions/default.sqlite",
                            "import_mode": "merge"
                        },
                        "policy": {
                            "tool_policy_preset": "balanced",
                            "bash_profile": "balanced",
                            "bash_dry_run": false,
                            "os_sandbox_mode": "off",
                            "enforce_regular_files": true,
                            "bash_timeout_ms": 500,
                            "max_command_length": 4096,
                            "max_tool_output_bytes": 1024,
                            "max_file_read_bytes": 2048,
                            "max_file_write_bytes": 2048,
                            "allow_command_newlines": true
                        }
                    }
                }
            })
            .to_string(),
        )
        .expect("write legacy profile store");

        let loaded = load_profile_store(&profile_path).expect("load legacy profiles");
        let legacy = loaded.get("legacy").expect("legacy profile");
        assert_eq!(legacy.auth.openai, ProviderAuthMethod::ApiKey);
        assert_eq!(legacy.auth.anthropic, ProviderAuthMethod::ApiKey);
        assert_eq!(legacy.auth.google, ProviderAuthMethod::ApiKey);
    }

    #[test]
    fn functional_render_profile_diffs_reports_changed_fields() {
        let current = sample_profile_defaults();
        let mut loaded = current.clone();
        loaded.model = "google/gemini-2.5-pro".to_string();
        loaded.policy.max_command_length = 2048;
        loaded.session.import_mode = "replace".to_string();

        let diffs = render_profile_diffs(&current, &loaded);
        assert_eq!(diffs.len(), 3);
        assert!(diffs.iter().any(|line| line
            .contains("field=model current=openai/gpt-4o-mini loaded=google/gemini-2.5-pro")));
        assert!(diffs
            .iter()
            .any(|line| line.contains("field=session.import_mode current=merge loaded=replace")));
        assert!(diffs
            .iter()
            .any(|line| line.contains("field=policy.max_command_length current=4096 loaded=2048")));
    }

    #[test]
    fn functional_render_profile_diffs_reports_changed_auth_modes() {
        let current = sample_profile_defaults();
        let mut loaded = current.clone();
        loaded.auth.openai = ProviderAuthMethod::OauthToken;
        loaded.auth.google = ProviderAuthMethod::Adc;

        let diffs = render_profile_diffs(&current, &loaded);
        assert!(diffs
            .iter()
            .any(|line| line.contains("field=auth.openai current=api_key loaded=oauth_token")));
        assert!(diffs
            .iter()
            .any(|line| line.contains("field=auth.google current=api_key loaded=adc")));
    }

    #[test]
    fn unit_render_profile_list_and_show_produce_deterministic_output() {
        let profile_path = PathBuf::from("/tmp/profiles.json");
        let mut alternate = sample_profile_defaults();
        alternate.model = "google/gemini-2.5-pro".to_string();
        let profiles = BTreeMap::from([
            ("zeta".to_string(), sample_profile_defaults()),
            ("alpha".to_string(), alternate.clone()),
        ]);

        let list_output = render_profile_list(&profile_path, &profiles);
        assert!(list_output.contains("profile list: path=/tmp/profiles.json profiles=2"));
        let alpha_index = list_output.find("profile: name=alpha").expect("alpha row");
        let zeta_index = list_output.find("profile: name=zeta").expect("zeta row");
        assert!(alpha_index < zeta_index);

        let show_output = render_profile_show(&profile_path, "alpha", &alternate);
        assert!(
            show_output.contains("profile show: path=/tmp/profiles.json name=alpha status=found")
        );
        assert!(show_output.contains("value: model=google/gemini-2.5-pro"));
        assert!(show_output.contains("value: fallback_models=none"));
        assert!(show_output.contains("value: session.path=.tau/sessions/default.sqlite"));
        assert!(show_output.contains("value: policy.max_command_length=4096"));
        assert!(show_output.contains("value: auth.openai=api_key"));
    }

    #[test]
    fn integration_execute_profile_command_full_lifecycle_roundtrip() {
        let temp = tempdir().expect("tempdir");
        let profile_path = temp.path().join(".tau").join("profiles.json");
        let current = sample_profile_defaults();

        let save_output = execute_profile_command("save baseline", &profile_path, &current);
        assert!(save_output.contains("profile save: path="));
        assert!(save_output.contains("name=baseline"));
        assert!(save_output.contains("status=saved"));

        let load_output = execute_profile_command("load baseline", &profile_path, &current);
        assert!(load_output.contains("profile load: path="));
        assert!(load_output.contains("name=baseline"));
        assert!(load_output.contains("status=in_sync"));
        assert!(load_output.contains("diffs=0"));

        let list_output = execute_profile_command("list", &profile_path, &current);
        assert!(list_output.contains("profile list: path="));
        assert!(list_output.contains("profiles=1"));
        assert!(list_output.contains("profile: name=baseline"));

        let show_output = execute_profile_command("show baseline", &profile_path, &current);
        assert!(show_output.contains("profile show: path="));
        assert!(show_output.contains("name=baseline status=found"));
        assert!(show_output.contains("value: model=openai/gpt-4o-mini"));

        let mut changed = current.clone();
        changed.model = "anthropic/claude-sonnet-4-20250514".to_string();
        let diff_output = execute_profile_command("load baseline", &profile_path, &changed);
        assert!(diff_output.contains("status=diff"));
        assert!(diff_output.contains("diff: field=model"));

        let delete_output = execute_profile_command("delete baseline", &profile_path, &current);
        assert!(delete_output.contains("profile delete: path="));
        assert!(delete_output.contains("name=baseline"));
        assert!(delete_output.contains("status=deleted"));
        assert!(delete_output.contains("remaining=0"));

        let list_after_delete = execute_profile_command("list", &profile_path, &current);
        assert!(list_after_delete.contains("profiles=0"));
        assert!(list_after_delete.contains("names=none"));
    }

    #[test]
    fn regression_execute_profile_command_reports_unknown_profile_and_schema_errors() {
        let temp = tempdir().expect("tempdir");
        let profile_path = temp.path().join(".tau").join("profiles.json");
        let current = sample_profile_defaults();

        let missing_output = execute_profile_command("load missing", &profile_path, &current);
        assert!(missing_output.contains("profile error: path="));
        assert!(missing_output.contains("unknown profile 'missing'"));

        let missing_show = execute_profile_command("show missing", &profile_path, &current);
        assert!(missing_show.contains("profile error: path="));
        assert!(missing_show.contains("unknown profile 'missing'"));

        let missing_delete = execute_profile_command("delete missing", &profile_path, &current);
        assert!(missing_delete.contains("profile error: path="));
        assert!(missing_delete.contains("unknown profile 'missing'"));

        std::fs::create_dir_all(
            profile_path
                .parent()
                .expect("profile path should include parent dir"),
        )
        .expect("create profile dir");
        let invalid = serde_json::json!({
            "schema_version": 999,
            "profiles": {
                "baseline": current
            }
        });
        std::fs::write(&profile_path, format!("{invalid}\n")).expect("write invalid schema");

        let schema_output = execute_profile_command("load baseline", &profile_path, &current);
        assert!(schema_output.contains("profile error: path="));
        assert!(schema_output.contains("unsupported profile schema_version 999"));
    }

    #[test]
    fn regression_default_profile_store_path_uses_project_local_profiles_file() {
        let path = default_profile_store_path().expect("resolve profile store path");
        assert!(path.ends_with(Path::new(".tau").join("profiles.json")));
    }
}
