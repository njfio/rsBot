use super::*;

#[test]
fn unit_normalize_daemon_subcommand_args_maps_action_and_alias_flags() {
    let normalized = normalize_daemon_subcommand_args(vec![
        "tau-rs".to_string(),
        "daemon".to_string(),
        "status".to_string(),
        "--json".to_string(),
        "--state-dir".to_string(),
        ".tau/ops-daemon".to_string(),
    ]);
    assert_eq!(
        normalized,
        vec![
            "tau-rs",
            "--daemon-status",
            "--daemon-status-json",
            "--daemon-state-dir",
            ".tau/ops-daemon",
        ]
    );
}
