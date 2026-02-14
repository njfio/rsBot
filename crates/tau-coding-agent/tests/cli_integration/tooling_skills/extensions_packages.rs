//! CLI integration coverage for extension and package lifecycle flags.

use super::*;

#[test]
fn package_validate_flag_reports_manifest_summary_and_exits() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("package.json");
    fs::write(
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

    let mut cmd = binary_command();
    cmd.args([
        "--package-validate",
        manifest_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package validate:"))
        .stdout(predicate::str::contains("name=starter-bundle"))
        .stdout(predicate::str::contains("total_components=2"));
}

#[test]
fn extension_validate_flag_reports_manifest_summary_and_exits() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("extension.json");
    fs::write(
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

    let mut cmd = binary_command();
    cmd.args([
        "--extension-validate",
        manifest_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("extension validate:"))
        .stdout(predicate::str::contains("id=issue-assistant"))
        .stdout(predicate::str::contains("permissions=2"))
        .stdout(predicate::str::contains("timeout_ms=60000"));
}

#[test]
fn regression_extension_validate_flag_rejects_invalid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("extension.json");
    fs::write(
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

    let mut cmd = binary_command();
    cmd.args([
        "--extension-validate",
        manifest_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "unsupported extension manifest schema",
    ));
}

#[test]
fn extension_show_flag_reports_manifest_inventory_and_exits() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("extension.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant",
  "hooks": ["run-end", "run-start"],
  "permissions": ["network", "read-files"],
  "timeout_ms": 60000
}"#,
    )
    .expect("write extension manifest");

    let mut cmd = binary_command();
    cmd.args([
        "--extension-show",
        manifest_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("extension show:"))
        .stdout(predicate::str::contains("- hooks (2):"))
        .stdout(predicate::str::contains("- run-end"))
        .stdout(predicate::str::contains("- run-start"))
        .stdout(predicate::str::contains("- permissions (2):"))
        .stdout(predicate::str::contains("- network"))
        .stdout(predicate::str::contains("- read-files"));
}

#[test]
fn regression_extension_show_flag_rejects_invalid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("extension.json");
    fs::write(
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

    let mut cmd = binary_command();
    cmd.args([
        "--extension-show",
        manifest_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "unsupported extension manifest schema",
    ));
}

#[test]
fn extension_list_flag_reports_valid_and_invalid_entries() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let valid_dir = root.join("issue-assistant");
    fs::create_dir_all(&valid_dir).expect("create valid extension dir");
    fs::write(
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
    fs::create_dir_all(&invalid_dir).expect("create invalid extension dir");
    fs::write(
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

    let mut cmd = binary_command();
    cmd.args([
        "--extension-list",
        "--extension-list-root",
        root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("extension list:"))
        .stdout(predicate::str::contains("count=1"))
        .stdout(predicate::str::contains("invalid=1"))
        .stdout(predicate::str::contains(
            "extension: id=issue-assistant version=0.1.0 runtime=process",
        ))
        .stdout(predicate::str::contains("invalid: manifest="))
        .stdout(predicate::str::contains(
            "unsupported extension manifest schema",
        ));
}

#[test]
fn regression_extension_list_flag_rejects_non_directory_root() {
    let temp = tempdir().expect("tempdir");
    let root_file = temp.path().join("extensions.json");
    fs::write(&root_file, "{}").expect("write root file");

    let mut cmd = binary_command();
    cmd.args([
        "--extension-list",
        "--extension-list-root",
        root_file.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("is not a directory"));
}

#[test]
fn extension_exec_flag_runs_process_hook_and_reports_success() {
    let temp = tempdir().expect("tempdir");
    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&bin_dir).expect("create bin dir");
    let script_path = bin_dir.join("hook.sh");
    fs::write(
        &script_path,
        "#!/usr/bin/env bash\nread -r _input\nprintf '{\"ok\":true,\"result\":\"hook-processed\"}'\n",
    )
    .expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&script_path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("set executable permissions");
    }

    let manifest_path = temp.path().join("extension.json");
    fs::write(
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
    fs::write(&payload_path, r#"{"event":"created"}"#).expect("write payload");

    let mut cmd = binary_command();
    cmd.args([
        "--extension-exec-manifest",
        manifest_path.to_str().expect("utf8 path"),
        "--extension-exec-hook",
        "run-start",
        "--extension-exec-payload-file",
        payload_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("extension exec:"))
        .stdout(predicate::str::contains("hook=run-start"))
        .stdout(predicate::str::contains("extension exec response:"))
        .stdout(predicate::str::contains("\"ok\":true"));
}

#[test]
fn regression_extension_exec_flag_rejects_invalid_response() {
    let temp = tempdir().expect("tempdir");
    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&bin_dir).expect("create bin dir");
    let script_path = bin_dir.join("bad.sh");
    fs::write(
        &script_path,
        "#!/usr/bin/env bash\nread -r _input\nprintf 'not-json'\n",
    )
    .expect("write script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&script_path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("set executable permissions");
    }

    let manifest_path = temp.path().join("extension.json");
    fs::write(
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
    fs::write(&payload_path, r#"{"event":"created"}"#).expect("write payload");

    let mut cmd = binary_command();
    cmd.args([
        "--extension-exec-manifest",
        manifest_path.to_str().expect("utf8 path"),
        "--extension-exec-hook",
        "run-start",
        "--extension-exec-payload-file",
        payload_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("response must be valid JSON"));
}

#[test]
fn extension_runtime_hooks_wrap_prompt_with_run_start_and_run_end() {
    let temp = tempdir().expect("tempdir");
    let extension_root = temp.path().join("extensions");
    let extension_dir = extension_root.join("issue-assistant");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let requests_path = extension_dir.join("requests.ndjson");
    let script_path = extension_dir.join("hook.sh");
    fs::write(
        &script_path,
        format!(
            "#!/usr/bin/env bash\nset -euo pipefail\ninput=\"$(cat)\"\nprintf '%s\\n' \"$input\" >> \"{}\"\nprintf '{{\"ok\":true}}'\n",
            requests_path.display()
        ),
    )
    .expect("write hook script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&script_path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("set executable permissions");
    }

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["run-start", "run-end"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write extension manifest");

    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "runtime hooks ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 3, "total_tokens": 13}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "hello runtime hooks",
        "--no-session",
        "--extension-runtime-hooks",
        "--extension-runtime-root",
        extension_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("runtime hooks ok"));

    let raw = fs::read_to_string(&requests_path).expect("read requests log");
    let rows = raw
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("valid json row"))
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["hook"], "run-start");
    assert_eq!(rows[1]["hook"], "run-end");
    assert_eq!(rows[0]["payload"]["schema_version"], 1);
    assert_eq!(rows[1]["payload"]["schema_version"], 1);
    assert_eq!(rows[0]["payload"]["hook"], "run-start");
    assert_eq!(rows[1]["payload"]["hook"], "run-end");
    assert!(rows[0]["payload"]["emitted_at_ms"].as_u64().is_some());
    assert!(rows[1]["payload"]["emitted_at_ms"].as_u64().is_some());
    assert_eq!(rows[0]["payload"]["data"]["prompt"], "hello runtime hooks");
    assert_eq!(rows[1]["payload"]["data"]["status"], "completed");

    openai.assert_calls(1);
}

#[test]
fn regression_extension_runtime_hook_timeout_does_not_fail_prompt() {
    let temp = tempdir().expect("tempdir");
    let extension_root = temp.path().join("extensions");
    let extension_dir = extension_root.join("slow-extension");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("hook.sh");
    fs::write(
        &script_path,
        "#!/usr/bin/env bash\nsleep 1\nprintf '{\"ok\":true}'\n",
    )
    .expect("write hook script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&script_path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("set executable permissions");
    }

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "slow-extension",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["run-start", "run-end"],
  "permissions": ["run-commands"],
  "timeout_ms": 20
}"#,
    )
    .expect("write extension manifest");

    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST).path("/v1/chat/completions");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "prompt still completed"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 3, "total_tokens": 13}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "hello runtime hooks",
        "--no-session",
        "--extension-runtime-hooks",
        "--extension-runtime-root",
        extension_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("prompt still completed"))
        .stderr(predicate::str::contains("timed out"));

    openai.assert_calls(1);
}

#[test]
fn extension_message_transform_hook_rewrites_prompt_before_model_request() {
    let temp = tempdir().expect("tempdir");
    let extension_root = temp.path().join("extensions");
    let extension_dir = extension_root.join("transformer");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("transform.sh");
    fs::write(
        &script_path,
        "#!/usr/bin/env bash\ncat >/dev/null\nprintf '{\"prompt\":\"transformed prompt text\"}'\n",
    )
    .expect("write transform script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&script_path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("set executable permissions");
    }
    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "transformer",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "transform.sh",
  "hooks": ["message-transform"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write extension manifest");

    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .body_includes("transformed prompt text");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "transform ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 12, "completion_tokens": 3, "total_tokens": 15}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "original prompt text",
        "--no-session",
        "--extension-runtime-hooks",
        "--extension-runtime-root",
        extension_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("transform ok"));

    openai.assert_calls(1);
}

#[test]
fn regression_extension_message_transform_invalid_response_falls_back_to_original_prompt() {
    let temp = tempdir().expect("tempdir");
    let extension_root = temp.path().join("extensions");
    let extension_dir = extension_root.join("broken-transformer");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("transform.sh");
    fs::write(
        &script_path,
        "#!/usr/bin/env bash\ncat >/dev/null\nprintf '{\"prompt\":123}'\n",
    )
    .expect("write transform script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&script_path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("set executable permissions");
    }
    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "broken-transformer",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "transform.sh",
  "hooks": ["message-transform"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write extension manifest");

    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .body_includes("original prompt text");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "fallback ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 12, "completion_tokens": 3, "total_tokens": 15}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "original prompt text",
        "--no-session",
        "--extension-runtime-hooks",
        "--extension-runtime-root",
        extension_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("fallback ok"))
        .stderr(predicate::str::contains("must be a string"));

    openai.assert_calls(1);
}

#[test]
fn regression_package_validate_flag_rejects_invalid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 9,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut cmd = binary_command();
    cmd.args([
        "--package-validate",
        manifest_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "unsupported package manifest schema",
    ));
}

#[test]
fn package_show_flag_reports_manifest_inventory_and_exits() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("package.json");
    fs::write(
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

    let mut cmd = binary_command();
    cmd.args(["--package-show", manifest_path.to_str().expect("utf8 path")]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package show:"))
        .stdout(predicate::str::contains("templates (1):"))
        .stdout(predicate::str::contains("- review => templates/review.txt"))
        .stdout(predicate::str::contains("skills (1):"));
}

#[test]
fn regression_package_show_flag_rejects_invalid_manifest() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "invalid",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut cmd = binary_command();
    cmd.args(["--package-show", manifest_path.to_str().expect("utf8 path")]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("must follow x.y.z"));
}

#[test]
fn package_install_flag_installs_bundle_files_and_exits() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    fs::write(package_root.join("templates/review.txt"), "template body")
        .expect("write template source");

    let manifest_path = package_root.join("package.json");
    fs::write(
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

    let mut cmd = binary_command();
    cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package install:"))
        .stdout(predicate::str::contains("name=starter-bundle"))
        .stdout(predicate::str::contains("total_components=1"));
    assert!(install_root
        .join("starter-bundle/1.0.0/templates/review.txt")
        .exists());
}

#[test]
fn package_install_flag_installs_remote_bundle_files_and_exits() {
    let server = MockServer::start();
    let remote_body = "remote template body";
    let remote_mock = server.mock(|when, then| {
        when.method(GET).path("/templates/review.txt");
        then.status(200).body(remote_body);
    });

    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(&package_root).expect("create package root");
    let checksum = format!("{:x}", Sha256::digest(remote_body.as_bytes()));
    let manifest_path = package_root.join("package.json");
    fs::write(
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

    let mut cmd = binary_command();
    cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package install:"))
        .stdout(predicate::str::contains("name=starter-bundle"))
        .stdout(predicate::str::contains("total_components=1"));
    assert_eq!(
        fs::read_to_string(install_root.join("starter-bundle/1.0.0/templates/review.txt"))
            .expect("read installed template"),
        remote_body
    );
    remote_mock.assert();
}

#[test]
fn package_install_flag_accepts_valid_signed_manifest_when_required() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    fs::write(package_root.join("templates/review.txt"), "template body")
        .expect("write template source");

    let manifest_path = package_root.join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "signing_key": "publisher",
  "signature_file": "package.sig",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    let signature = signing_key.sign(&fs::read(&manifest_path).expect("read manifest bytes"));
    fs::write(
        package_root.join("package.sig"),
        BASE64.encode(signature.to_bytes()),
    )
    .expect("write signature");
    let trust_root = format!(
        "publisher={}",
        BASE64.encode(signing_key.verifying_key().as_bytes())
    );

    let install_root = temp.path().join("installed");
    let mut cmd = binary_command();
    cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
        "--require-signed-packages",
        "--skill-trust-root",
        trust_root.as_str(),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package install:"))
        .stdout(predicate::str::contains("name=starter-bundle"));
}

#[test]
fn package_install_flag_accepts_remote_signed_manifest_when_required() {
    let server = MockServer::start();
    let remote_body = "remote signed template";
    let remote_mock = server.mock(|when, then| {
        when.method(GET).path("/templates/review.txt");
        then.status(200).body(remote_body);
    });

    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(&package_root).expect("create package root");
    let checksum = format!("{:x}", Sha256::digest(remote_body.as_bytes()));
    let manifest_path = package_root.join("package.json");
    fs::write(
        &manifest_path,
        format!(
            r#"{{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "signing_key": "publisher",
  "signature_file": "package.sig",
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

    let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    let signature = signing_key.sign(&fs::read(&manifest_path).expect("read manifest bytes"));
    fs::write(
        package_root.join("package.sig"),
        BASE64.encode(signature.to_bytes()),
    )
    .expect("write signature");
    let trust_root = format!(
        "publisher={}",
        BASE64.encode(signing_key.verifying_key().as_bytes())
    );

    let install_root = temp.path().join("installed");
    let mut cmd = binary_command();
    cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
        "--require-signed-packages",
        "--skill-trust-root",
        trust_root.as_str(),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package install:"))
        .stdout(predicate::str::contains("name=starter-bundle"));
    remote_mock.assert();
}

#[test]
fn regression_package_install_flag_rejects_missing_component_source() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(package_root.join("templates")).expect("create templates dir");

    let manifest_path = package_root.join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/missing.txt"}]
}"#,
    )
    .expect("write manifest");
    let install_root = temp.path().join("installed");

    let mut cmd = binary_command();
    cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

#[test]
fn regression_package_install_flag_rejects_remote_checksum_mismatch() {
    let server = MockServer::start();
    let remote_mock = server.mock(|when, then| {
        when.method(GET).path("/templates/review.txt");
        then.status(200).body("remote template");
    });

    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(&package_root).expect("create package root");
    let manifest_path = package_root.join("package.json");
    fs::write(
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
    let install_root = temp.path().join("installed");

    let mut cmd = binary_command();
    cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("checksum mismatch"));
    remote_mock.assert();
}

#[test]
fn regression_package_install_flag_rejects_unsigned_when_signatures_required() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    fs::write(package_root.join("templates/review.txt"), "template body")
        .expect("write template source");
    let manifest_path = package_root.join("package.json");
    fs::write(
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

    let mut cmd = binary_command();
    cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
        "--require-signed-packages",
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "must include signing_key and signature_file",
    ));
}

#[test]
fn package_update_flag_updates_existing_bundle_and_exits() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    let template_path = package_root.join("templates/review.txt");
    fs::write(&template_path, "template body v1").expect("write template source");

    let manifest_path = package_root.join("package.json");
    fs::write(
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

    let mut install_cmd = binary_command();
    install_cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
    ]);
    install_cmd.assert().success();

    fs::write(&template_path, "template body v2").expect("update template source");
    let mut update_cmd = binary_command();
    update_cmd.args([
        "--package-update",
        manifest_path.to_str().expect("utf8 path"),
        "--package-update-root",
        install_root.to_str().expect("utf8 path"),
    ]);

    update_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("package update:"))
        .stdout(predicate::str::contains("updated=1"))
        .stdout(predicate::str::contains("name=starter-bundle"));
    assert_eq!(
        fs::read_to_string(install_root.join("starter-bundle/1.0.0/templates/review.txt"))
            .expect("read updated template"),
        "template body v2"
    );
}

#[test]
fn package_update_flag_accepts_signed_manifest_when_required() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    let template_path = package_root.join("templates/review.txt");
    fs::write(&template_path, "template body v1").expect("write template source");

    let manifest_path = package_root.join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "signing_key": "publisher",
  "signature_file": "package.sig",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    let write_signature = || {
        let signature = signing_key.sign(&fs::read(&manifest_path).expect("read manifest bytes"));
        fs::write(
            package_root.join("package.sig"),
            BASE64.encode(signature.to_bytes()),
        )
        .expect("write signature");
    };
    write_signature();
    let trust_root = format!(
        "publisher={}",
        BASE64.encode(signing_key.verifying_key().as_bytes())
    );

    let install_root = temp.path().join("installed");
    let mut install_cmd = binary_command();
    install_cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
        "--require-signed-packages",
        "--skill-trust-root",
        trust_root.as_str(),
    ]);
    install_cmd.assert().success();

    fs::write(&template_path, "template body v2").expect("update template source");
    write_signature();
    let mut update_cmd = binary_command();
    update_cmd.args([
        "--package-update",
        manifest_path.to_str().expect("utf8 path"),
        "--package-update-root",
        install_root.to_str().expect("utf8 path"),
        "--require-signed-packages",
        "--skill-trust-root",
        trust_root.as_str(),
    ]);

    update_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("package update:"))
        .stdout(predicate::str::contains("name=starter-bundle"));
}

#[test]
fn regression_package_update_flag_rejects_missing_target() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    fs::write(package_root.join("templates/review.txt"), "template body")
        .expect("write template source");
    let manifest_path = package_root.join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut cmd = binary_command();
    cmd.args([
        "--package-update",
        manifest_path.to_str().expect("utf8 path"),
        "--package-update-root",
        temp.path().join("installed").to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("is not installed"));
}

#[test]
fn package_conflicts_flag_reports_conflicts_and_invalid_entries() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");

    let install_package = |name: &str, body: &str| {
        let source_root = temp.path().join(format!("bundle-{name}"));
        fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
        fs::write(source_root.join("templates/review.txt"), body).expect("write template source");
        let manifest_path = source_root.join("package.json");
        fs::write(
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
        let mut install_cmd = binary_command();
        install_cmd.args([
            "--package-install",
            manifest_path.to_str().expect("utf8 path"),
            "--package-install-root",
            install_root.to_str().expect("utf8 path"),
        ]);
        install_cmd.assert().success();
    };
    install_package("alpha", "alpha body");
    install_package("zeta", "zeta body");

    let invalid_dir = install_root.join("broken/9.9.9");
    fs::create_dir_all(&invalid_dir).expect("create invalid dir");
    fs::write(
        invalid_dir.join("package.json"),
        r#"{
  "schema_version": 99,
  "name": "broken",
  "version": "9.9.9",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write invalid manifest");

    let mut cmd = binary_command();
    cmd.args([
        "--package-conflicts",
        "--package-conflicts-root",
        install_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package conflicts:"))
        .stdout(predicate::str::contains("conflicts=1"))
        .stdout(predicate::str::contains("invalid=1"))
        .stdout(predicate::str::contains("conflict: kind=templates"))
        .stdout(predicate::str::contains("package invalid:"));
}

#[test]
fn regression_package_conflicts_flag_reports_none_when_no_conflicts_exist() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");

    let install_package = |name: &str, path: &str| {
        let source_root = temp.path().join(format!("bundle-{name}"));
        let component_dir = source_root.join(
            std::path::Path::new(path)
                .parent()
                .expect("component parent"),
        );
        fs::create_dir_all(&component_dir).expect("create component dir");
        fs::write(source_root.join(path), format!("{name} body")).expect("write template source");
        let manifest_path = source_root.join("package.json");
        fs::write(
            &manifest_path,
            format!(
                r#"{{
  "schema_version": 1,
  "name": "{name}",
  "version": "1.0.0",
  "templates": [{{"id":"review","path":"{path}"}}]
}}"#
            ),
        )
        .expect("write manifest");
        let mut install_cmd = binary_command();
        install_cmd.args([
            "--package-install",
            manifest_path.to_str().expect("utf8 path"),
            "--package-install-root",
            install_root.to_str().expect("utf8 path"),
        ]);
        install_cmd.assert().success();
    };
    install_package("alpha", "templates/review-a.txt");
    install_package("zeta", "templates/review-z.txt");

    let mut cmd = binary_command();
    cmd.args([
        "--package-conflicts",
        "--package-conflicts-root",
        install_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package conflicts:"))
        .stdout(predicate::str::contains("conflicts=0"))
        .stdout(predicate::str::contains("conflicts: none"));
}

#[test]
fn package_activate_flag_materializes_components_and_exits() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let destination_root = temp.path().join("activated");
    let source_root = temp.path().join("bundle");
    fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
    fs::create_dir_all(source_root.join("skills/checks")).expect("create skills dir");
    fs::write(source_root.join("templates/review.txt"), "template body")
        .expect("write template source");
    fs::write(source_root.join("skills/checks/SKILL.md"), "# checks").expect("write skill source");
    let manifest_path = source_root.join("package.json");
    fs::write(
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

    let mut install_cmd = binary_command();
    install_cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
    ]);
    install_cmd.assert().success();

    let mut activate_cmd = binary_command();
    activate_cmd.args([
        "--package-activate",
        "--package-activate-root",
        install_root.to_str().expect("utf8 path"),
        "--package-activate-destination",
        destination_root.to_str().expect("utf8 path"),
    ]);

    activate_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("package activate:"))
        .stdout(predicate::str::contains("policy=error"))
        .stdout(predicate::str::contains("activated_components=2"));
    assert_eq!(
        fs::read_to_string(destination_root.join("templates/review.txt"))
            .expect("read activated template"),
        "template body"
    );
    assert_eq!(
        fs::read_to_string(destination_root.join("skills/checks/SKILL.md"))
            .expect("read activated skill"),
        "# checks"
    );
    assert_eq!(
        fs::read_to_string(destination_root.join("skills/checks.md"))
            .expect("read activated skill alias"),
        "# checks"
    );
}

#[test]
fn integration_package_activate_on_startup_loads_activated_skill_for_prompt() {
    let temp = tempdir().expect("tempdir");
    let source_root = temp.path().join("bundle");
    fs::create_dir_all(source_root.join("skills/checks")).expect("create skills dir");
    fs::write(
        source_root.join("skills/checks/SKILL.md"),
        "Activated checks body",
    )
    .expect("write skill source");
    let manifest_path = source_root.join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "starter-bundle",
  "version": "1.0.0",
  "skills": [{"id":"checks","path":"skills/checks/SKILL.md"}]
}"#,
    )
    .expect("write manifest");

    let mut install_cmd = binary_command();
    install_cmd.current_dir(temp.path()).args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        ".tau/packages",
    ]);
    install_cmd.assert().success();

    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .json_body_includes(
                json!({
                    "messages": [{
                        "role": "system",
                        "content": "base\n\n# Skill: checks\nActivated checks body"
                    }]
                })
                .to_string(),
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok startup activation"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 2, "total_tokens": 10}
        }));
    });

    let mut cmd = binary_command();
    cmd.current_dir(temp.path()).args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt",
        "base",
        "--package-activate-on-startup",
        "--package-activate-root",
        ".tau/packages",
        "--package-activate-destination",
        ".tau/packages-active",
        "--skill",
        "checks",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package activate:"))
        .stdout(predicate::str::contains("ok startup activation"));
    openai.assert_calls(1);
    assert_eq!(
        fs::read_to_string(temp.path().join(".tau/packages-active/skills/checks.md"))
            .expect("read activated alias"),
        "Activated checks body"
    );
}

#[test]
fn package_activate_flag_keep_last_policy_resolves_conflicts() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let destination_root = temp.path().join("activated");
    let install_package = |name: &str, body: &str| {
        let source_root = temp.path().join(format!("bundle-{name}"));
        fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
        fs::write(source_root.join("templates/review.txt"), body).expect("write template source");
        let manifest_path = source_root.join("package.json");
        fs::write(
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
        let mut install_cmd = binary_command();
        install_cmd.args([
            "--package-install",
            manifest_path.to_str().expect("utf8 path"),
            "--package-install-root",
            install_root.to_str().expect("utf8 path"),
        ]);
        install_cmd.assert().success();
    };
    install_package("alpha", "alpha body");
    install_package("zeta", "zeta body");

    let mut activate_cmd = binary_command();
    activate_cmd.args([
        "--package-activate",
        "--package-activate-root",
        install_root.to_str().expect("utf8 path"),
        "--package-activate-destination",
        destination_root.to_str().expect("utf8 path"),
        "--package-activate-conflict-policy",
        "keep-last",
    ]);

    activate_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("package activate:"))
        .stdout(predicate::str::contains("policy=keep-last"))
        .stdout(predicate::str::contains("conflicts_detected=1"));
    assert_eq!(
        fs::read_to_string(destination_root.join("templates/review.txt"))
            .expect("read activated template"),
        "zeta body"
    );
}

#[test]
fn regression_package_activate_flag_error_policy_rejects_conflicts() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let install_package = |name: &str| {
        let source_root = temp.path().join(format!("bundle-{name}"));
        fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
        fs::write(
            source_root.join("templates/review.txt"),
            format!("{name} body"),
        )
        .expect("write template source");
        let manifest_path = source_root.join("package.json");
        fs::write(
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
        let mut install_cmd = binary_command();
        install_cmd.args([
            "--package-install",
            manifest_path.to_str().expect("utf8 path"),
            "--package-install-root",
            install_root.to_str().expect("utf8 path"),
        ]);
        install_cmd.assert().success();
    };
    install_package("alpha");
    install_package("zeta");

    let mut cmd = binary_command();
    cmd.args([
        "--package-activate",
        "--package-activate-root",
        install_root.to_str().expect("utf8 path"),
        "--package-activate-destination",
        temp.path().join("activated").to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("package activation conflict"));
}

#[test]
fn regression_package_activate_flag_rejects_invalid_installed_manifest_entries() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let source_root = temp.path().join("bundle");
    fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
    fs::write(source_root.join("templates/review.txt"), "valid body").expect("write template");
    let manifest_path = source_root.join("package.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "name": "valid-bundle",
  "version": "1.0.0",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write manifest");

    let mut install_cmd = binary_command();
    install_cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
    ]);
    install_cmd.assert().success();

    let invalid_dir = install_root.join("broken/9.9.9");
    fs::create_dir_all(&invalid_dir).expect("create invalid dir");
    fs::write(
        invalid_dir.join("package.json"),
        r#"{
  "schema_version": 99,
  "name": "broken",
  "version": "9.9.9",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write invalid manifest");

    let mut cmd = binary_command();
    cmd.args([
        "--package-activate",
        "--package-activate-root",
        install_root.to_str().expect("utf8 path"),
        "--package-activate-destination",
        temp.path().join("activated").to_str().expect("utf8 path"),
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "invalid installed package entries",
    ));
}

#[test]
fn package_list_flag_reports_installed_packages_and_exits() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    fs::write(package_root.join("templates/review.txt"), "template body")
        .expect("write template source");

    let manifest_path = package_root.join("package.json");
    fs::write(
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

    let mut install_cmd = binary_command();
    install_cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
    ]);
    install_cmd.assert().success();

    let mut list_cmd = binary_command();
    list_cmd.args([
        "--package-list",
        "--package-list-root",
        install_root.to_str().expect("utf8 path"),
    ]);
    list_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("package list:"))
        .stdout(predicate::str::contains("packages=1"))
        .stdout(predicate::str::contains("invalid=0"))
        .stdout(predicate::str::contains(
            "package: name=starter-bundle version=1.0.0",
        ));
}

#[test]
fn regression_package_list_flag_reports_invalid_manifest_entries() {
    let temp = tempdir().expect("tempdir");
    let list_root = temp.path().join("installed");
    let invalid_dir = list_root.join("broken/9.9.9");
    fs::create_dir_all(&invalid_dir).expect("create invalid dir");
    fs::write(
        invalid_dir.join("package.json"),
        r#"{
  "schema_version": 99,
  "name": "broken",
  "version": "9.9.9",
  "templates": [{"id":"review","path":"templates/review.txt"}]
}"#,
    )
    .expect("write invalid manifest");

    let mut cmd = binary_command();
    cmd.args([
        "--package-list",
        "--package-list-root",
        list_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("package list:"))
        .stdout(predicate::str::contains("packages=0"))
        .stdout(predicate::str::contains("invalid=1"))
        .stdout(predicate::str::contains("package invalid:"));
}

#[test]
fn package_remove_flag_removes_installed_bundle_and_exits() {
    let temp = tempdir().expect("tempdir");
    let package_root = temp.path().join("bundle");
    fs::create_dir_all(package_root.join("templates")).expect("create templates dir");
    fs::write(package_root.join("templates/review.txt"), "template body")
        .expect("write template source");
    let manifest_path = package_root.join("package.json");
    fs::write(
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

    let mut install_cmd = binary_command();
    install_cmd.args([
        "--package-install",
        manifest_path.to_str().expect("utf8 path"),
        "--package-install-root",
        install_root.to_str().expect("utf8 path"),
    ]);
    install_cmd.assert().success();

    let mut remove_cmd = binary_command();
    remove_cmd.args([
        "--package-remove",
        "starter-bundle@1.0.0",
        "--package-remove-root",
        install_root.to_str().expect("utf8 path"),
    ]);
    remove_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("package remove:"))
        .stdout(predicate::str::contains("status=removed"));
    assert!(!install_root.join("starter-bundle/1.0.0").exists());
}

#[test]
fn regression_package_remove_flag_rejects_invalid_coordinate() {
    let temp = tempdir().expect("tempdir");
    let mut cmd = binary_command();
    cmd.args([
        "--package-remove",
        "starter-bundle",
        "--package-remove-root",
        temp.path().to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("must follow <name>@<version>"));
}

#[test]
fn package_rollback_flag_keeps_target_and_removes_other_versions() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let install_version = |version: &str, body: &str| {
        let source_root = temp.path().join(format!("bundle-{version}"));
        fs::create_dir_all(source_root.join("templates")).expect("create templates dir");
        fs::write(source_root.join("templates/review.txt"), body).expect("write template source");
        let manifest_path = source_root.join("package.json");
        fs::write(
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

        let mut install_cmd = binary_command();
        install_cmd.args([
            "--package-install",
            manifest_path.to_str().expect("utf8 path"),
            "--package-install-root",
            install_root.to_str().expect("utf8 path"),
        ]);
        install_cmd.assert().success();
    };

    install_version("1.0.0", "v1");
    install_version("2.0.0", "v2");

    let mut rollback_cmd = binary_command();
    rollback_cmd.args([
        "--package-rollback",
        "starter-bundle@1.0.0",
        "--package-rollback-root",
        install_root.to_str().expect("utf8 path"),
    ]);
    rollback_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("package rollback:"))
        .stdout(predicate::str::contains("status=rolled_back"))
        .stdout(predicate::str::contains("removed_versions=1"));
    assert!(install_root.join("starter-bundle/1.0.0").exists());
    assert!(!install_root.join("starter-bundle/2.0.0").exists());
}

#[test]
fn regression_package_rollback_flag_rejects_missing_target() {
    let temp = tempdir().expect("tempdir");
    let install_root = temp.path().join("installed");
    let mut cmd = binary_command();
    cmd.args([
        "--package-rollback",
        "starter-bundle@1.0.0",
        "--package-rollback-root",
        install_root.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("is not installed"));
}
