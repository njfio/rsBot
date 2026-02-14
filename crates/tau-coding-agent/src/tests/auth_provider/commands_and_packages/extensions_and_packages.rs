//! Tests for extension and package command lifecycles, error handling, and startup activation.

use super::*;

#[test]
fn functional_execute_extension_validate_command_succeeds_for_valid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("extension.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant",
  "hooks": ["run-start", "run-end"],
  "permissions": ["read-files", "network"],
  "timeout_ms": 60000
}"#,
    )
    .expect("write extension manifest");

    let mut cli = test_cli();
    cli.extension_validate = Some(manifest_path);
    execute_extension_validate_command(&cli).expect("extension validate should succeed");
}

#[test]
fn regression_execute_extension_validate_command_rejects_invalid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("extension.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "../escape.sh"
}"#,
    )
    .expect("write extension manifest");

    let mut cli = test_cli();
    cli.extension_validate = Some(manifest_path);
    let error =
        execute_extension_validate_command(&cli).expect_err("unsafe entrypoint should fail");
    assert!(error
        .to_string()
        .contains("must not contain parent traversals"));
}

#[test]
fn functional_execute_extension_show_command_succeeds_for_valid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("extension.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant",
  "hooks": ["run-end", "run-start"],
  "permissions": ["network", "read-files"]
}"#,
    )
    .expect("write extension manifest");

    let mut cli = test_cli();
    cli.extension_show = Some(manifest_path);
    execute_extension_show_command(&cli).expect("extension show should succeed");
}

#[test]
fn regression_execute_extension_show_command_rejects_invalid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("extension.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 9,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant"
}"#,
    )
    .expect("write extension manifest");

    let mut cli = test_cli();
    cli.extension_show = Some(manifest_path);
    let error = execute_extension_show_command(&cli).expect_err("invalid schema should fail");
    assert!(error
        .to_string()
        .contains("unsupported extension manifest schema"));
}

#[test]
fn functional_execute_extension_list_command_reports_mixed_inventory() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let valid_dir = root.join("issue-assistant");
    std::fs::create_dir_all(&valid_dir).expect("create valid extension dir");
    std::fs::write(
        valid_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant"
}"#,
    )
    .expect("write valid extension manifest");
    let invalid_dir = root.join("broken");
    std::fs::create_dir_all(&invalid_dir).expect("create invalid extension dir");
    std::fs::write(
        invalid_dir.join("extension.json"),
        r#"{
  "schema_version": 9,
  "id": "broken",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant"
}"#,
    )
    .expect("write invalid extension manifest");

    let mut cli = test_cli();
    cli.extension_list = true;
    cli.extension_list_root = root;
    execute_extension_list_command(&cli).expect("extension list should succeed");
}

#[test]
fn regression_execute_extension_list_command_rejects_non_directory_root() {
    let temp = tempdir().expect("tempdir");
    let root_file = temp.path().join("extensions.json");
    std::fs::write(&root_file, "{}").expect("write root file");

    let mut cli = test_cli();
    cli.extension_list = true;
    cli.extension_list_root = root_file;
    let error =
        execute_extension_list_command(&cli).expect_err("non-directory extension root should fail");
    assert!(error.to_string().contains("is not a directory"));
}

#[test]
fn functional_execute_extension_exec_command_runs_process_hook() {
    let temp = tempdir().expect("tempdir");
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).expect("create bin directory");
    let script_path = bin_dir.join("hook.sh");
    std::fs::write(
        &script_path,
        "#!/usr/bin/env bash\nread -r _input\nprintf '{\"ok\":true,\"result\":\"hook-processed\"}'\n",
    )
    .expect("write hook script");
    make_script_executable(&script_path);
    let manifest_path = temp.path().join("extension.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/hook.sh",
  "hooks": ["run-start"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");
    let payload_path = temp.path().join("payload.json");
    std::fs::write(&payload_path, r#"{"event":"created"}"#).expect("write payload");

    let mut cli = test_cli();
    cli.extension_exec_manifest = Some(manifest_path);
    cli.extension_exec_hook = Some("run-start".to_string());
    cli.extension_exec_payload_file = Some(payload_path);
    execute_extension_exec_command(&cli).expect("extension exec should succeed");
}

#[test]
fn regression_execute_extension_exec_command_rejects_undeclared_hook() {
    let temp = tempdir().expect("tempdir");
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).expect("create bin directory");
    let script_path = bin_dir.join("hook.sh");
    std::fs::write(
        &script_path,
        "#!/usr/bin/env bash\nread -r _input\nprintf '{\"ok\":true}'\n",
    )
    .expect("write hook script");
    make_script_executable(&script_path);
    let manifest_path = temp.path().join("extension.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/hook.sh",
  "hooks": ["run-end"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");
    let payload_path = temp.path().join("payload.json");
    std::fs::write(&payload_path, r#"{"event":"created"}"#).expect("write payload");

    let mut cli = test_cli();
    cli.extension_exec_manifest = Some(manifest_path);
    cli.extension_exec_hook = Some("run-start".to_string());
    cli.extension_exec_payload_file = Some(payload_path);
    let error = execute_extension_exec_command(&cli).expect_err("undeclared hook should fail");
    assert!(error.to_string().contains("does not declare hook"));
}

#[test]
fn regression_execute_extension_exec_command_enforces_timeout() {
    let temp = tempdir().expect("tempdir");
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).expect("create bin directory");
    let script_path = bin_dir.join("slow.sh");
    std::fs::write(
        &script_path,
        "#!/usr/bin/env bash\nsleep 1\nprintf '{\"ok\":true}'\n",
    )
    .expect("write hook script");
    make_script_executable(&script_path);
    let manifest_path = temp.path().join("extension.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/slow.sh",
  "hooks": ["run-start"],
  "permissions": ["run-commands"],
  "timeout_ms": 20
}"#,
    )
    .expect("write manifest");
    let payload_path = temp.path().join("payload.json");
    std::fs::write(&payload_path, r#"{"event":"created"}"#).expect("write payload");

    let mut cli = test_cli();
    cli.extension_exec_manifest = Some(manifest_path);
    cli.extension_exec_hook = Some("run-start".to_string());
    cli.extension_exec_payload_file = Some(payload_path);
    let error = execute_extension_exec_command(&cli).expect_err("timeout should fail");
    assert!(error.to_string().contains("timed out"));
}

#[test]
fn regression_execute_extension_exec_command_rejects_invalid_json_response() {
    let temp = tempdir().expect("tempdir");
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).expect("create bin directory");
    let script_path = bin_dir.join("bad.sh");
    std::fs::write(
        &script_path,
        "#!/usr/bin/env bash\nread -r _input\nprintf 'not-json'\n",
    )
    .expect("write hook script");
    make_script_executable(&script_path);
    let manifest_path = temp.path().join("extension.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/bad.sh",
  "hooks": ["run-start"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");
    let payload_path = temp.path().join("payload.json");
    std::fs::write(&payload_path, r#"{"event":"created"}"#).expect("write payload");

    let mut cli = test_cli();
    cli.extension_exec_manifest = Some(manifest_path);
    cli.extension_exec_hook = Some("run-start".to_string());
    cli.extension_exec_payload_file = Some(payload_path);
    let error =
        execute_extension_exec_command(&cli).expect_err("invalid output should be rejected");
    assert!(error.to_string().contains("response must be valid JSON"));
}

#[test]
fn functional_execute_package_validate_command_succeeds_for_valid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut cli = test_cli();
    cli.package_validate = Some(manifest_path);
    execute_package_validate_command(&cli).expect("package validate should succeed");
}

#[test]
fn regression_execute_package_validate_command_rejects_invalid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 9,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut cli = test_cli();
    cli.package_validate = Some(manifest_path);
    let error = execute_package_validate_command(&cli).expect_err("invalid schema should fail");
    assert!(error
        .to_string()
        .contains("unsupported package manifest schema"));
}

#[test]
fn functional_execute_package_show_command_succeeds_for_valid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}],
  "skills": [{"id":"checks","path":"skills/checks/SKILL.md"}]
}"#,
    )
    .expect("write manifest");

    let mut cli = test_cli();
    cli.package_show = Some(manifest_path);
    execute_package_show_command(&cli).expect("package show should succeed");
}

#[test]
fn regression_execute_package_show_command_rejects_invalid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "invalid",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut cli = test_cli();
    cli.package_show = Some(manifest_path);
    let error = execute_package_show_command(&cli).expect_err("invalid version should fail");
    assert!(error.to_string().contains("must follow x.y.z"));
}

#[test]
fn functional_execute_package_install_command_succeeds_for_valid_manifest() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    std::fs::write(package_root.join("templates/review.txt"), "template")
        .expect("write template source");

    let manifest_path = package_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let install_root = temp.path().join("installed");
    let mut cli = test_cli();
    cli.package_install = Some(manifest_path);
    cli.package_install_root = install_root.clone();

    execute_package_install_command(&cli).expect("package install should succeed");
    assert!(install_root
        .join("starter-bundle/1.0.0/templates/review.txt")
        .exists());
}

#[test]
fn functional_execute_package_install_command_supports_remote_sources_with_checksum() {
    let server = MockServer::start();
    let remote_body = "remote template body";
    let remote_mock = server.mock(|when, then| {
        when.method(GET).path("/templates/review.txt");
        then.status(200).body(remote_body);
    });

    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    std::fs::create_dir_all(&package_root).expect("create package root");
    let checksum = format!("{:x}", Sha256::digest(remote_body.as_bytes()));
    let manifest_path = package_root.join("package.json");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{{
    "id":"review",
    "path":"templates/review.txt",
    "url":"{}/templates/review.txt",
    "sha256":"sha256:{checksum}"
  }}]
}}"#,
            server.base_url()
        ),
    )
    .expect("write manifest");

    let install_root = temp.path().join("installed");
    let mut cli = test_cli();
    cli.package_install = Some(manifest_path);
    cli.package_install_root = install_root.clone();
    execute_package_install_command(&cli).expect("package install should succeed");
    assert_eq!(
        std::fs::read_to_string(install_root.join("starter-bundle/1.0.0/templates/review.txt"))
            .expect("read installed template"),
        remote_body
    );
    remote_mock.assert();
}

#[test]
fn regression_execute_package_install_command_rejects_missing_component_source() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");

    let manifest_path = package_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/missing.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut cli = test_cli();
    cli.package_install = Some(manifest_path);
    cli.package_install_root = temp.path().join("installed");
    let error = execute_package_install_command(&cli).expect_err("missing source should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_execute_package_install_command_rejects_remote_checksum_mismatch() {
    let server = MockServer::start();
    let remote_mock = server.mock(|when, then| {
        when.method(GET).path("/templates/review.txt");
        then.status(200).body("remote template");
    });

    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    std::fs::create_dir_all(&package_root).expect("create package root");
    let manifest_path = package_root.join("package.json");
    std::fs::write(
        &manifest_path,
        format!(
            r#"{{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{{
    "id":"review",
    "path":"templates/review.txt",
    "url":"{}/templates/review.txt",
    "sha256":"sha256:{}"
  }}]
}}"#,
            server.base_url(),
            "0".repeat(64)
        ),
    )
    .expect("write manifest");

    let mut cli = test_cli();
    cli.package_install = Some(manifest_path);
    cli.package_install_root = temp.path().join("installed");
    let error =
        execute_package_install_command(&cli).expect_err("checksum mismatch should fail install");
    assert!(error.to_string().contains("checksum mismatch"));
    remote_mock.assert();
}

#[test]
fn regression_execute_package_install_command_rejects_unsigned_when_required() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    std::fs::write(package_root.join("templates/review.txt"), "template")
        .expect("write template source");
    let manifest_path = package_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut cli = test_cli();
    cli.package_install = Some(manifest_path);
    cli.package_install_root = temp.path().join("installed");
    cli.require_signed_packages = true;
    let error =
        execute_package_install_command(&cli).expect_err("unsigned package should fail policy");
    assert!(error
        .to_string()
        .contains("must include signing_key and signature_file"));
}

#[test]
fn functional_execute_package_update_command_updates_existing_package() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    let template_path = package_root.join("templates/review.txt");
    std::fs::write(&template_path, "template-v1").expect("write template source");
    let manifest_path = package_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let install_root = temp.path().join("installed");
    let mut install_cli = test_cli();
    install_cli.package_install = Some(manifest_path.clone());
    install_cli.package_install_root = install_root.clone();
    execute_package_install_command(&install_cli).expect("package install should succeed");

    std::fs::write(&template_path, "template-v2").expect("update template source");
    let mut update_cli = test_cli();
    update_cli.package_update = Some(manifest_path);
    update_cli.package_update_root = install_root.clone();
    execute_package_update_command(&update_cli).expect("package update should succeed");
    assert_eq!(
        std::fs::read_to_string(install_root.join("starter-bundle/1.0.0/templates/review.txt"))
            .expect("read updated template"),
        "template-v2"
    );
}

#[test]
fn regression_execute_package_update_command_rejects_missing_target() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    std::fs::write(package_root.join("templates/review.txt"), "template")
        .expect("write template source");
    let manifest_path = package_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut cli = test_cli();
    cli.package_update = Some(manifest_path);
    cli.package_update_root = temp.path().join("installed");
    let error = execute_package_update_command(&cli).expect_err("missing target should fail");
    assert!(error.to_string().contains("is not installed"));
}

#[test]
fn functional_execute_package_list_command_reports_installed_packages() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    std::fs::write(package_root.join("templates/review.txt"), "template")
        .expect("write template source");
    let manifest_path = package_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let install_root = temp.path().join("installed");
    let mut install_cli = test_cli();
    install_cli.package_install = Some(manifest_path);
    install_cli.package_install_root = install_root.clone();
    execute_package_install_command(&install_cli).expect("package install should succeed");

    let mut list_cli = test_cli();
    list_cli.package_list = true;
    list_cli.package_list_root = install_root;
    execute_package_list_command(&list_cli).expect("package list should succeed");
}

#[test]
fn regression_execute_package_list_command_rejects_non_directory_root() {
    let temp = tempdir().expect("tempdir");
    let root_file = temp.path().join("not-a-directory.txt");
    std::fs::write(&root_file, "file root").expect("write root file");

    let mut cli = test_cli();
    cli.package_list = true;
    cli.package_list_root = root_file;
    let error = execute_package_list_command(&cli).expect_err("non-directory root should fail");
    assert!(error.to_string().contains("is not a directory"));
}

#[test]
fn functional_execute_package_remove_command_removes_installed_package() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    std::fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    std::fs::write(package_root.join("templates/review.txt"), "template")
        .expect("write template source");
    let manifest_path = package_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let install_root = temp.path().join("installed");
    let mut install_cli = test_cli();
    install_cli.package_install = Some(manifest_path);
    install_cli.package_install_root = install_root.clone();
    execute_package_install_command(&install_cli).expect("package install should succeed");

    let mut remove_cli = test_cli();
    remove_cli.package_remove = Some("starter-bundle@1.0.0".to_string());
    remove_cli.package_remove_root = install_root.clone();
    execute_package_remove_command(&remove_cli).expect("package remove should succeed");
    assert!(!install_root.join("starter-bundle/1.0.0").exists());
}

#[test]
fn regression_execute_package_remove_command_rejects_invalid_coordinate() {
    let mut cli = test_cli();
    cli.package_remove = Some("starter-bundle".to_string());
    cli.package_remove_root = PathBuf::from(".tau/packages");
    let error =
        execute_package_remove_command(&cli).expect_err("invalid coordinate format should fail");
    assert!(error.to_string().contains("must follow <name>@<version>"));
}

#[test]
fn functional_execute_package_rollback_command_removes_non_target_versions() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let install_version = |version: &str, body: &str| {
        let source_root = temp.path().join(format!("bundle-{version}"));
        std::fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
        std::fs::write(source_root.join("templates/review.txt"), body)
            .expect("write template source");
        let manifest_path = source_root.join("package.json");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "{version}",
  "templates": [{{"id":"review","path":"templates/review.txt"}}]
}}"#
            ),
        )
        .expect("write manifest");

        let mut install_cli = test_cli();
        install_cli.package_install = Some(manifest_path);
        install_cli.package_install_root = install_root.clone();
        execute_package_install_command(&install_cli).expect("package install should succeed");
    };

    install_version("1.0.0", "v1");
    install_version("2.0.0", "v2");

    let mut rollback_cli = test_cli();
    rollback_cli.package_rollback = Some("starter-bundle@1.0.0".to_string());
    rollback_cli.package_rollback_root = install_root.clone();
    execute_package_rollback_command(&rollback_cli).expect("package rollback should succeed");
    assert!(install_root.join("starter-bundle/1.0.0").exists());
    assert!(!install_root.join("starter-bundle/2.0.0").exists());
}

#[test]
fn regression_execute_package_rollback_command_rejects_missing_target() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.package_rollback = Some("starter-bundle@1.0.0".to_string());
    cli.package_rollback_root = temp.path().join("installed");
    let error =
        execute_package_rollback_command(&cli).expect_err("missing target version should fail");
    assert!(error.to_string().contains("is not installed"));
}

#[test]
fn regression_execute_package_rollback_command_rejects_invalid_coordinate() {
    let mut cli = test_cli();
    cli.package_rollback = Some("../starter@1.0.0".to_string());
    cli.package_rollback_root = PathBuf::from(".tau/packages");
    let error = execute_package_rollback_command(&cli).expect_err("invalid coordinate should fail");
    assert!(error
        .to_string()
        .contains("must not contain path separators"));
}

#[test]
fn functional_execute_package_conflicts_command_reports_detected_collisions() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let install_package = |name: &str, body: &str| {
        let source_root = temp.path().join(format!("bundle-{name}"));
        std::fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
        std::fs::write(source_root.join("templates/review.txt"), body)
            .expect("write template source");
        let manifest_path = source_root.join("package.json");
        std::fs::write(
            &manifest_path,
            format!(
                r#"{{
  "schema_version": 1,
  "name": "{name}",
  "version": "1.0.0",
  "templates": [{{"id":"review","path":"templates/review.txt"}}]
}}"#
            ),
        )
        .expect("write manifest");
        let mut install_cli = test_cli();
        install_cli.package_install = Some(manifest_path);
        install_cli.package_install_root = install_root.clone();
        execute_package_install_command(&install_cli).expect("package install should succeed");
    };

    install_package("alpha", "alpha body");
    install_package("zeta", "zeta body");

    let mut cli = test_cli();
    cli.package_conflicts = true;
    cli.package_conflicts_root = install_root;
    execute_package_conflicts_command(&cli).expect("package conflicts should succeed");
}

#[test]
fn regression_execute_package_conflicts_command_rejects_non_directory_root() {
    let temp = tempdir().expect("tempdir");
    let root_file = temp.path().join("not-a-directory.txt");
    std::fs::write(&root_file, "file root").expect("write root file");

    let mut cli = test_cli();
    cli.package_conflicts = true;
    cli.package_conflicts_root = root_file;
    let error =
        execute_package_conflicts_command(&cli).expect_err("non-directory root should fail");
    assert!(error.to_string().contains("is not a directory"));
}

#[test]
fn functional_execute_package_activate_command_materializes_components() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let source_root = temp.path().join("bundle");
    std::fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
    std::fs::create_dir_all(source_root.join("skills/checks")).expect("create skills dir");
    std::fs::write(source_root.join("templates/review.txt"), "template body")
        .expect("write template source");
    std::fs::write(source_root.join("skills/checks/SKILL.md"), "# checks")
        .expect("write skill source");
    let manifest_path = source_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}],
  "skills": [{"id":"checks","path":"skills/checks/SKILL.md"}]
}"#,
    )
    .expect("write manifest");

    let mut install_cli = test_cli();
    install_cli.package_install = Some(manifest_path);
    install_cli.package_install_root = install_root.clone();
    execute_package_install_command(&install_cli).expect("package install should succeed");

    let destination_root = temp.path().join("activated");
    let mut cli = test_cli();
    cli.package_activate = true;
    cli.package_activate_root = install_root;
    cli.package_activate_destination = destination_root.clone();
    execute_package_activate_command(&cli).expect("package activate should succeed");
    assert_eq!(
        std::fs::read_to_string(destination_root.join("templates/review.txt"))
            .expect("read activated template"),
        "template body"
    );
    assert_eq!(
        std::fs::read_to_string(destination_root.join("skills/checks/SKILL.md"))
            .expect("read activated skill"),
        "# checks"
    );
}

#[test]
fn regression_execute_package_activate_command_rejects_unsupported_conflict_policy() {
    let mut cli = test_cli();
    cli.package_activate = true;
    cli.package_activate_conflict_policy = "unsupported".to_string();
    let error = execute_package_activate_command(&cli)
        .expect_err("unsupported conflict policy should fail");
    assert!(error
        .to_string()
        .contains("unsupported package activation conflict policy"));
}

#[test]
fn regression_execute_package_activate_on_startup_is_noop_when_disabled() {
    let temp = tempdir().expect("tempdir");
    let destination_root = temp.path().join("activated");
    let mut cli = test_cli();
    cli.package_activate_root = temp.path().join("installed");
    cli.package_activate_destination = destination_root.clone();
    let report = execute_package_activate_on_startup(&cli)
        .expect("startup activation should allow disabled mode");
    assert!(report.is_none());
    assert!(!destination_root.exists());
}

#[test]
fn functional_execute_package_activate_on_startup_creates_runtime_skill_alias() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let source_root = temp.path().join("bundle");
    std::fs::create_dir_all(source_root.join("skills/checks")).expect("create skills dir");
    std::fs::write(source_root.join("skills/checks/SKILL.md"), "# checks")
        .expect("write skill source");
    let manifest_path = source_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "skills": [{"id":"checks","path":"skills/checks/SKILL.md"}]
}"#,
    )
    .expect("write manifest");

    let mut install_cli = test_cli();
    install_cli.package_install = Some(manifest_path);
    install_cli.package_install_root = install_root.clone();
    execute_package_install_command(&install_cli).expect("package install should succeed");

    let destination_root = temp.path().join("activated");
    let mut cli = test_cli();
    cli.package_activate_on_startup = true;
    cli.package_activate_root = install_root;
    cli.package_activate_destination = destination_root.clone();
    let report = execute_package_activate_on_startup(&cli)
        .expect("startup activation should succeed")
        .expect("startup activation should return report");
    assert_eq!(report.activated_components, 1);
    assert_eq!(
        std::fs::read_to_string(destination_root.join("skills/checks/SKILL.md"))
            .expect("read activated nested skill"),
        "# checks"
    );
    assert_eq!(
        std::fs::read_to_string(destination_root.join("skills/checks.md"))
            .expect("read activated skill alias"),
        "# checks"
    );
}

#[test]
fn integration_compose_startup_system_prompt_uses_activated_skill_aliases() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let source_root = temp.path().join("bundle");
    std::fs::create_dir_all(source_root.join("skills/checks")).expect("create skills dir");
    std::fs::write(
        source_root.join("skills/checks/SKILL.md"),
        "Always run tests",
    )
    .expect("write skill source");
    let manifest_path = source_root.join("package.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "skills": [{"id":"checks","path":"skills/checks/SKILL.md"}]
}"#,
    )
    .expect("write manifest");

    let mut install_cli = test_cli();
    install_cli.package_install = Some(manifest_path);
    install_cli.package_install_root = install_root.clone();
    execute_package_install_command(&install_cli).expect("package install should succeed");

    let destination_root = temp.path().join("activated");
    let mut activation_cli = test_cli();
    activation_cli.package_activate_on_startup = true;
    activation_cli.package_activate_root = install_root;
    activation_cli.package_activate_destination = destination_root.clone();
    execute_package_activate_on_startup(&activation_cli)
        .expect("startup activation should succeed");

    let mut cli = test_cli();
    cli.system_prompt = "base prompt".to_string();
    cli.skills = vec!["checks".to_string()];
    let composed = compose_startup_system_prompt(&cli, &destination_root.join("skills"))
        .expect("compose startup prompt");
    assert!(composed.contains("base prompt"));
    assert!(composed.contains("Always run tests"));
}

#[test]
fn regression_execute_package_activate_on_startup_rejects_unsupported_conflict_policy() {
    let mut cli = test_cli();
    cli.package_activate_on_startup = true;
    cli.package_activate_conflict_policy = "unsupported".to_string();
    let error = execute_package_activate_on_startup(&cli)
        .expect_err("unsupported conflict policy should fail");
    assert!(error
        .to_string()
        .contains("unsupported package activation conflict policy"));
}
