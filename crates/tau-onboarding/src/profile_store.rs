use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tau_core::write_text_atomic;

use crate::startup_config::ProfileDefaults;

pub const PROFILE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `ProfileStoreFile` used across Tau components.
pub struct ProfileStoreFile {
    pub schema_version: u32,
    pub profiles: BTreeMap<String, ProfileDefaults>,
}

pub fn default_profile_store_path() -> Result<PathBuf> {
    Ok(std::env::current_dir()
        .context("failed to resolve current working directory")?
        .join(".tau")
        .join("profiles.json"))
}

pub fn validate_profile_name(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        bail!("profile name must not be empty");
    };
    if !first.is_ascii_alphabetic() {
        bail!("profile name '{}' must start with an ASCII letter", name);
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_')) {
        bail!(
            "profile name '{}' must contain only ASCII letters, digits, '-' or '_'",
            name
        );
    }
    Ok(())
}

pub fn load_profile_store(path: &Path) -> Result<BTreeMap<String, ProfileDefaults>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read profile store {}", path.display()))?;
    let parsed = serde_json::from_str::<ProfileStoreFile>(&raw)
        .with_context(|| format!("failed to parse profile store {}", path.display()))?;
    if parsed.schema_version != PROFILE_SCHEMA_VERSION {
        bail!(
            "unsupported profile schema_version {} in {} (expected {})",
            parsed.schema_version,
            path.display(),
            PROFILE_SCHEMA_VERSION
        );
    }
    Ok(parsed.profiles)
}

pub fn save_profile_store(path: &Path, profiles: &BTreeMap<String, ProfileDefaults>) -> Result<()> {
    let payload = ProfileStoreFile {
        schema_version: PROFILE_SCHEMA_VERSION,
        profiles: profiles.clone(),
    };
    let mut encoded =
        serde_json::to_string_pretty(&payload).context("failed to encode profile store")?;
    encoded.push('\n');
    let parent = path.parent().ok_or_else(|| {
        anyhow!(
            "profile store path {} does not have a parent directory",
            path.display()
        )
    })?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create profile directory {}", parent.display()))?;
    write_text_atomic(path, &encoded)
}

#[cfg(test)]
mod tests {
    use super::{
        load_profile_store, save_profile_store, validate_profile_name, ProfileStoreFile,
        PROFILE_SCHEMA_VERSION,
    };
    use crate::startup_config::{
        ProfileAuthDefaults, ProfileDefaults, ProfileMcpDefaults, ProfilePolicyDefaults,
        ProfileSessionDefaults,
    };
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    fn sample_profile_defaults() -> ProfileDefaults {
        ProfileDefaults {
            model: "openai/gpt-4o-mini".to_string(),
            fallback_models: vec!["openai/gpt-4.1-mini".to_string()],
            session: ProfileSessionDefaults {
                enabled: true,
                path: Some(".tau/sessions/default.sqlite".to_string()),
                import_mode: "merge".to_string(),
            },
            policy: ProfilePolicyDefaults {
                tool_policy_preset: "strict".to_string(),
                bash_profile: "strict".to_string(),
                bash_dry_run: false,
                os_sandbox_mode: "workspace-write".to_string(),
                enforce_regular_files: true,
                bash_timeout_ms: 120_000,
                max_command_length: 8_192,
                max_tool_output_bytes: 262_144,
                max_file_read_bytes: 262_144,
                max_file_write_bytes: 262_144,
                allow_command_newlines: true,
                runtime_heartbeat_enabled: true,
                runtime_heartbeat_interval_ms: 5_000,
                runtime_heartbeat_state_path: ".tau/runtime-heartbeat/state.json".to_string(),
            },
            mcp: ProfileMcpDefaults::default(),
            auth: ProfileAuthDefaults::default(),
        }
    }

    #[test]
    fn unit_validate_profile_name_accepts_and_rejects_expected_inputs() {
        assert!(validate_profile_name("default").is_ok());
        assert!(validate_profile_name("team_alpha-1").is_ok());
        assert!(validate_profile_name("").is_err());
        assert!(validate_profile_name("1default").is_err());
        assert!(validate_profile_name("default!").is_err());
    }

    #[test]
    fn functional_profile_store_round_trip_persists_defaults() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join(".tau/profiles.json");
        let mut profiles = BTreeMap::new();
        profiles.insert("default".to_string(), sample_profile_defaults());

        save_profile_store(&path, &profiles).expect("save profile store");
        let loaded = load_profile_store(&path).expect("load profile store");
        assert_eq!(loaded, profiles);
    }

    #[test]
    fn regression_profile_store_rejects_schema_mismatch() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join(".tau/profiles.json");
        std::fs::create_dir_all(path.parent().expect("profiles parent")).expect("create parent");
        let payload = ProfileStoreFile {
            schema_version: PROFILE_SCHEMA_VERSION + 1,
            profiles: BTreeMap::new(),
        };
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&payload).expect("encode mismatch payload"),
        )
        .expect("write mismatch payload");

        let error = load_profile_store(&path).expect_err("schema mismatch should fail");
        assert!(error
            .to_string()
            .contains("unsupported profile schema_version"));
    }
}
