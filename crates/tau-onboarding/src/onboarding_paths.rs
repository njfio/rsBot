use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use tau_cli::Cli;

pub fn resolve_tau_root(cli: &Cli) -> PathBuf {
    if let Some(session_parent) = cli
        .session
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        if session_parent
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "sessions")
        {
            if let Some(root) = session_parent
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
            {
                return root.to_path_buf();
            }
        }
    }

    cli.credential_store
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(".tau"))
}

pub fn collect_bootstrap_directories(cli: &Cli, tau_root: &Path) -> Vec<PathBuf> {
    let mut directories = BTreeSet::new();
    maybe_insert_directory(&mut directories, Some(tau_root));
    maybe_insert_directory(&mut directories, Some(&tau_root.join("reports")));
    maybe_insert_directory(&mut directories, cli.session.parent());
    maybe_insert_directory(&mut directories, cli.credential_store.parent());
    maybe_insert_directory(&mut directories, Some(&cli.skills_dir));
    maybe_insert_directory(&mut directories, cli.model_catalog_cache.parent());
    maybe_insert_directory(&mut directories, Some(&cli.channel_store_root));
    maybe_insert_directory(&mut directories, Some(&cli.events_dir));
    maybe_insert_directory(&mut directories, cli.events_state_path.parent());
    maybe_insert_directory(&mut directories, Some(&cli.github_state_dir));
    maybe_insert_directory(&mut directories, Some(&cli.slack_state_dir));
    maybe_insert_directory(&mut directories, Some(&cli.package_install_root));
    maybe_insert_directory(&mut directories, Some(&cli.package_update_root));
    maybe_insert_directory(&mut directories, Some(&cli.package_list_root));
    maybe_insert_directory(&mut directories, Some(&cli.package_remove_root));
    maybe_insert_directory(&mut directories, Some(&cli.package_rollback_root));
    maybe_insert_directory(&mut directories, Some(&cli.package_conflicts_root));
    maybe_insert_directory(&mut directories, Some(&cli.package_activate_root));
    maybe_insert_directory(&mut directories, Some(&cli.package_activate_destination));
    maybe_insert_directory(&mut directories, Some(&cli.extension_list_root));
    maybe_insert_directory(&mut directories, Some(&cli.extension_runtime_root));
    directories.into_iter().collect()
}

fn maybe_insert_directory(directories: &mut BTreeSet<PathBuf>, path: Option<&Path>) {
    let Some(path) = path else {
        return;
    };
    if path.as_os_str().is_empty() {
        return;
    }
    directories.insert(path.to_path_buf());
}

pub fn parse_yes_no_response(raw: &str, default_yes: bool) -> bool {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" => default_yes,
        "y" | "yes" => true,
        "n" | "no" => false,
        _ => default_yes,
    }
}

#[cfg(test)]
mod tests {
    use super::{collect_bootstrap_directories, parse_yes_no_response, resolve_tau_root};
    use clap::Parser;
    use std::collections::BTreeSet;
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
        cli.session = tau_root.join("sessions/default.jsonl");
        cli.credential_store = tau_root.join("credentials.json");
        cli.skills_dir = tau_root.join("skills");
        cli.model_catalog_cache = tau_root.join("models/catalog.json");
        cli.channel_store_root = tau_root.join("channel-store");
        cli.events_dir = tau_root.join("events");
        cli.events_state_path = tau_root.join("events/state.json");
        cli.dashboard_state_dir = tau_root.join("dashboard");
        cli.github_state_dir = tau_root.join("github-issues");
        cli.slack_state_dir = tau_root.join("slack");
        cli.package_install_root = tau_root.join("packages");
        cli.package_update_root = tau_root.join("packages");
        cli.package_list_root = tau_root.join("packages");
        cli.package_remove_root = tau_root.join("packages");
        cli.package_rollback_root = tau_root.join("packages");
        cli.package_conflicts_root = tau_root.join("packages");
        cli.package_activate_root = tau_root.join("packages");
        cli.package_activate_destination = tau_root.join("packages-active");
        cli.extension_list_root = tau_root.join("extensions");
        cli.extension_runtime_root = tau_root.join("extensions");
        cli.daemon_state_dir = tau_root.join("daemon");
    }

    #[test]
    fn unit_parse_yes_no_response_accepts_supported_values() {
        assert!(parse_yes_no_response("yes", false));
        assert!(parse_yes_no_response("Y", false));
        assert!(!parse_yes_no_response("n", true));
        assert!(!parse_yes_no_response("no", true));
        assert!(parse_yes_no_response("", true));
        assert!(!parse_yes_no_response("", false));
    }

    #[test]
    fn functional_resolve_tau_root_prefers_sessions_parent() {
        let mut cli = parse_cli_with_stack();
        let temp = tempdir().expect("tempdir");
        apply_workspace_paths(&mut cli, temp.path());
        let tau_root = resolve_tau_root(&cli);
        assert_eq!(tau_root, temp.path().join(".tau"));
    }

    #[test]
    fn regression_collect_bootstrap_directories_returns_deduplicated_expected_paths() {
        let mut cli = parse_cli_with_stack();
        let temp = tempdir().expect("tempdir");
        apply_workspace_paths(&mut cli, temp.path());
        cli.package_update_root = cli.package_install_root.clone();
        cli.package_list_root = cli.package_install_root.clone();
        cli.package_remove_root = cli.package_install_root.clone();

        let tau_root = resolve_tau_root(&cli);
        let directories = collect_bootstrap_directories(&cli, &tau_root);
        let unique_count = directories.iter().collect::<BTreeSet<_>>().len();
        assert_eq!(directories.len(), unique_count);
        assert!(directories.contains(&tau_root));
        assert!(directories.contains(&tau_root.join("reports")));
        assert!(directories.contains(&cli.skills_dir));
        assert!(directories.contains(&cli.channel_store_root));
        assert!(directories.contains(&cli.extension_runtime_root));
    }
}
