//! Profile bootstrap primitives used by onboarding.
//!
//! This module enforces profile-name validity and persistence invariants when
//! creating initial profile entries. On success, profile store mutations are
//! idempotent; on failure, callers receive contextual errors without partial
//! profile-state assumptions.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::profile_store::{load_profile_store, save_profile_store, validate_profile_name};
use crate::startup_config::ProfileDefaults;

pub const ONBOARDING_DEFAULT_PROFILE: &str = "default";

/// Resolve onboarding profile name, applying default when input is blank.
pub fn resolve_onboarding_profile_name(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    let profile_name = if trimmed.is_empty() {
        ONBOARDING_DEFAULT_PROFILE.to_string()
    } else {
        trimmed.to_string()
    };
    validate_profile_name(&profile_name)?;
    Ok(profile_name)
}

pub fn ensure_directory(
    path: &Path,
    directories_created: &mut Vec<String>,
    directories_existing: &mut Vec<String>,
) -> Result<()> {
    if path.exists() {
        if !path.is_dir() {
            bail!(
                "onboarding path '{}' exists but is not a directory",
                path.display()
            );
        }
        directories_existing.push(path.display().to_string());
    } else {
        std::fs::create_dir_all(path)
            .with_context(|| format!("failed to create directory {}", path.display()))?;
        directories_created.push(path.display().to_string());
    }
    Ok(())
}

/// Ensure onboarding profile store contains requested profile defaults.
pub fn ensure_profile_store_entry(
    profile_store_path: &Path,
    profile_name: &str,
    profile_defaults: &ProfileDefaults,
    repair_existing: bool,
) -> Result<&'static str> {
    let mut profiles = load_profile_store(profile_store_path)?;
    if let Some(existing) = profiles.get(profile_name) {
        if !repair_existing {
            return Ok("unchanged");
        }
        if existing == profile_defaults {
            return Ok("unchanged");
        }
        profiles.insert(profile_name.to_string(), profile_defaults.clone());
        save_profile_store(profile_store_path, &profiles)?;
        return Ok("repaired");
    }

    let file_existed = profile_store_path.exists();
    profiles.insert(profile_name.to_string(), profile_defaults.clone());
    save_profile_store(profile_store_path, &profiles)?;
    if file_existed {
        Ok("updated")
    } else {
        Ok("created")
    }
}

#[cfg(test)]
mod tests {
    use super::{ensure_directory, ensure_profile_store_entry, resolve_onboarding_profile_name};
    use crate::startup_config::build_profile_defaults;
    use clap::Parser;
    use std::path::Path;
    use tau_cli::Cli;
    use tempfile::tempdir;

    fn parse_cli_with_stack() -> Cli {
        std::thread::Builder::new()
            .name("tau-cli-parse".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(|| Cli::parse_from(["tau-rs"]))
            .expect("spawn cli parse thread")
            .join()
            .expect("join cli parse thread")
    }

    fn apply_workspace_paths(cli: &mut Cli, workspace: &Path) {
        let tau_root = workspace.join(".tau");
        cli.session = tau_root.join("sessions/default.sqlite");
        cli.credential_store = tau_root.join("credentials.json");
        cli.skills_dir = tau_root.join("skills");
        cli.model_catalog_cache = tau_root.join("models/catalog.json");
    }

    #[test]
    fn unit_resolve_onboarding_profile_name_defaults_and_validates() {
        assert_eq!(
            resolve_onboarding_profile_name("   ").expect("default profile"),
            "default"
        );
        assert_eq!(
            resolve_onboarding_profile_name("team-alpha").expect("trimmed profile"),
            "team-alpha"
        );
        let error = resolve_onboarding_profile_name("1bad").expect_err("invalid profile");
        assert!(error
            .to_string()
            .contains("must start with an ASCII letter"));
    }

    #[test]
    fn functional_ensure_directory_creates_and_tracks_existing_paths() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join(".tau/reports");
        let mut created = Vec::new();
        let mut existing = Vec::new();

        ensure_directory(&path, &mut created, &mut existing).expect("create directory");
        ensure_directory(&path, &mut created, &mut existing).expect("track existing directory");
        assert_eq!(created.len(), 1);
        assert_eq!(existing.len(), 1);
    }

    #[test]
    fn regression_ensure_directory_rejects_existing_non_directory_path() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("file-as-dir");
        std::fs::write(&path, "not a directory").expect("write file");
        let mut created = Vec::new();
        let mut existing = Vec::new();

        let error =
            ensure_directory(&path, &mut created, &mut existing).expect_err("should fail closed");
        assert!(error.to_string().contains("exists but is not a directory"));
    }

    #[test]
    fn integration_ensure_profile_store_entry_creates_then_preserves_existing_profile() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        let profile_store_path = temp.path().join(".tau/profiles.json");
        let defaults = build_profile_defaults(&cli);

        let first = ensure_profile_store_entry(&profile_store_path, "team", &defaults, false)
            .expect("create profile entry");
        assert_eq!(first, "created");

        let second = ensure_profile_store_entry(&profile_store_path, "team", &defaults, false)
            .expect("preserve existing profile entry");
        assert_eq!(second, "unchanged");
    }

    #[test]
    fn regression_ensure_profile_store_entry_rewrites_existing_profile_when_repair_requested() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        let profile_store_path = temp.path().join(".tau/profiles.json");
        let first_defaults = build_profile_defaults(&cli);
        ensure_profile_store_entry(&profile_store_path, "team", &first_defaults, false)
            .expect("create profile entry");

        cli.model = "google/gemini-2.5-pro".to_string();
        let repaired_defaults = build_profile_defaults(&cli);
        let action =
            ensure_profile_store_entry(&profile_store_path, "team", &repaired_defaults, true)
                .expect("repair profile");
        assert_eq!(action, "repaired");
    }
}
