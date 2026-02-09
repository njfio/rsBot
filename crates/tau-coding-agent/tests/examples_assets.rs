use std::{
    fs,
    path::{Path, PathBuf},
};

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::tempdir;

fn binary_command() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("tau-coding-agent"))
}

fn repo_root() -> PathBuf {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_root
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root should exist")
        .to_path_buf()
}

fn example_path(suffix: &str) -> PathBuf {
    repo_root().join("examples").join(suffix)
}

#[test]
fn unit_starter_manifest_has_expected_component_shape() {
    let manifest_path = example_path("starter/package.json");
    let raw = fs::read_to_string(&manifest_path).expect("read starter manifest");
    let parsed: Value = serde_json::from_str(&raw).expect("parse starter manifest");

    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["name"], "tau-starter-bundle");
    assert_eq!(parsed["version"], "1.0.0");
    assert_eq!(
        parsed["templates"]
            .as_array()
            .expect("templates array")
            .len(),
        1
    );
    assert_eq!(parsed["skills"].as_array().expect("skills array").len(), 1);
    assert_eq!(
        parsed["extensions"]
            .as_array()
            .expect("extensions array")
            .len(),
        3
    );
    assert_eq!(parsed["themes"].as_array().expect("themes array").len(), 1);
}

#[test]
fn functional_cli_validates_checked_in_examples() {
    let manifest_path = example_path("starter/package.json");
    let extension_path = example_path("extensions/issue-assistant/extension.json");
    let extension_payload_path = example_path("extensions/issue-assistant/payload.json");
    let events_dir = example_path("events");
    let events_state_path = example_path("events-state.json");

    let mut package_validate = binary_command();
    package_validate.args([
        "--package-validate",
        manifest_path.to_str().expect("utf8 manifest path"),
    ]);
    package_validate
        .assert()
        .success()
        .stdout(predicate::str::contains("package validate:"))
        .stdout(predicate::str::contains("name=tau-starter-bundle"));

    let mut extension_validate = binary_command();
    extension_validate.args([
        "--extension-validate",
        extension_path.to_str().expect("utf8 extension path"),
    ]);
    extension_validate
        .assert()
        .success()
        .stdout(predicate::str::contains("extension validate:"))
        .stdout(predicate::str::contains("id=issue-assistant"));

    #[cfg(unix)]
    {
        let mut extension_exec = binary_command();
        extension_exec.args([
            "--extension-exec-manifest",
            extension_path.to_str().expect("utf8 extension path"),
            "--extension-exec-hook",
            "run-start",
            "--extension-exec-payload-file",
            extension_payload_path.to_str().expect("utf8 payload path"),
        ]);
        extension_exec
            .assert()
            .success()
            .stdout(predicate::str::contains("extension exec:"))
            .stdout(predicate::str::contains("extension exec response:"))
            .stdout(predicate::str::contains("extension-demo-processed"));
    }

    let mut events_validate = binary_command();
    events_validate.args([
        "--events-dir",
        events_dir.to_str().expect("utf8 events dir"),
        "--events-state-path",
        events_state_path.to_str().expect("utf8 state path"),
        "--events-validate",
    ]);
    events_validate
        .assert()
        .success()
        .stdout(predicate::str::contains("events validate:"))
        .stdout(predicate::str::contains("invalid_files=0"))
        .stdout(predicate::str::contains("malformed_files=0"));
}

#[test]
fn integration_package_install_works_with_checked_in_starter_bundle() {
    let manifest_path = example_path("starter/package.json");
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");

    let mut install = binary_command();
    install.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 manifest path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 install root"),
    ]);
    install
        .assert()
        .success()
        .stdout(predicate::str::contains("package install:"))
        .stdout(predicate::str::contains("name=tau-starter-bundle"))
        .stdout(predicate::str::contains("total_components=6"));

    let installed_root = install_root.join("tau-starter-bundle/1.0.0");
    assert!(installed_root.join("templates/review.txt").is_file());
    assert!(installed_root.join("skills/checks/SKILL.md").is_file());
    assert!(installed_root
        .join("extensions/issue-assistant/extension.json")
        .is_file());
    assert!(installed_root
        .join("extensions/issue-assistant/runtime.sh")
        .is_file());
    assert!(installed_root
        .join("extensions/issue-assistant/payload.json")
        .is_file());
    assert!(installed_root.join("themes/tau-default.json").is_file());
}

#[test]
fn regression_readme_example_paths_exist_on_disk() {
    let readme_path = repo_root().join("README.md");
    let readme = fs::read_to_string(&readme_path).expect("read README");
    let expected_paths = [
        "./examples/starter/package.json",
        "./examples/extensions",
        "./examples/extensions/issue-assistant/extension.json",
        "./examples/extensions/issue-assistant/payload.json",
        "./examples/events",
        "./examples/events-state.json",
    ];

    for expected in expected_paths {
        assert!(
            readme.contains(expected),
            "README should reference {expected}"
        );
        let relative = expected.strip_prefix("./").unwrap_or(expected);
        assert!(
            repo_root().join(relative).exists(),
            "referenced path should exist: {expected}"
        );
    }
}
