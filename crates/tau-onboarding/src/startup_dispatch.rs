use std::path::{Path, PathBuf};

use tau_cli::Cli;
use tau_skills::default_skills_lock_path;

pub fn resolve_runtime_skills_dir(cli: &Cli, activation_applied: bool) -> PathBuf {
    if !activation_applied {
        return cli.skills_dir.clone();
    }
    let activated_skills_dir = cli.package_activate_destination.join("skills");
    if activated_skills_dir.is_dir() {
        return activated_skills_dir;
    }
    cli.skills_dir.clone()
}

pub fn resolve_runtime_skills_lock_path(
    cli: &Cli,
    bootstrap_lock_path: &Path,
    effective_skills_dir: &Path,
) -> PathBuf {
    if effective_skills_dir == cli.skills_dir {
        bootstrap_lock_path.to_path_buf()
    } else {
        default_skills_lock_path(effective_skills_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_runtime_skills_dir, resolve_runtime_skills_lock_path};
    use clap::Parser;
    use std::path::PathBuf;
    use tau_cli::Cli;
    use tau_skills::default_skills_lock_path;
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

    #[test]
    fn unit_resolve_runtime_skills_lock_path_prefers_bootstrap_lock_for_default_skills_dir() {
        let mut cli = parse_cli_with_stack();
        let workspace = tempdir().expect("tempdir");
        let skills_dir = workspace.path().join(".tau/skills");
        cli.skills_dir = skills_dir.clone();

        let bootstrap_lock_path = workspace.path().join(".tau/skills.lock.json");
        let resolved = resolve_runtime_skills_lock_path(&cli, &bootstrap_lock_path, &skills_dir);
        assert_eq!(resolved, bootstrap_lock_path);
    }

    #[test]
    fn functional_resolve_runtime_skills_dir_prefers_activated_directory_when_present() {
        let mut cli = parse_cli_with_stack();
        let workspace = tempdir().expect("tempdir");
        cli.skills_dir = workspace.path().join(".tau/skills");
        cli.package_activate_destination = workspace.path().join("packages-active");

        let activated_skills_dir = cli.package_activate_destination.join("skills");
        std::fs::create_dir_all(&activated_skills_dir).expect("create activated skills dir");

        let resolved = resolve_runtime_skills_dir(&cli, true);
        assert_eq!(resolved, activated_skills_dir);
    }

    #[test]
    fn regression_resolve_runtime_skills_dir_falls_back_when_activation_output_missing() {
        let mut cli = parse_cli_with_stack();
        let workspace = tempdir().expect("tempdir");
        let base_skills_dir = workspace.path().join(".tau/skills");
        cli.skills_dir = base_skills_dir.clone();
        cli.package_activate_destination = workspace.path().join("packages-active");

        let resolved = resolve_runtime_skills_dir(&cli, true);
        assert_eq!(resolved, base_skills_dir);
    }

    #[test]
    fn regression_resolve_runtime_skills_lock_path_uses_effective_directory_when_switched() {
        let mut cli = parse_cli_with_stack();
        let workspace = tempdir().expect("tempdir");
        cli.skills_dir = workspace.path().join(".tau/skills");
        let bootstrap_lock_path = workspace.path().join(".tau/skills.lock.json");

        let activated_skills_dir = workspace.path().join("packages-active/skills");
        let resolved =
            resolve_runtime_skills_lock_path(&cli, &bootstrap_lock_path, &activated_skills_dir);

        assert_eq!(resolved, default_skills_lock_path(&activated_skills_dir));
        assert_ne!(resolved, PathBuf::from(bootstrap_lock_path));
    }
}
