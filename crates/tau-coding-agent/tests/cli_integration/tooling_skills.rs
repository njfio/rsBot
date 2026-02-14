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

#[test]
fn prompt_file_flag_runs_one_shot_prompt() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .body_includes("prompt from file");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "file prompt ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let prompt_path = temp.path().join("prompt.txt");
    fs::write(&prompt_path, "prompt from file").expect("write prompt");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt-file",
        prompt_path.to_str().expect("utf8 path"),
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("file prompt ok"));

    openai.assert_calls(1);
}

#[test]
fn prompt_template_file_flag_renders_and_runs_one_shot_prompt() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .body_includes("Summarize src/main.rs with focus on retries.");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "template prompt ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 7, "completion_tokens": 2, "total_tokens": 9}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("prompt-template.txt");
    fs::write(
        &template_path,
        "Summarize {{module}} with focus on {{focus}}.",
    )
    .expect("write template");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt-template-file",
        template_path.to_str().expect("utf8 path"),
        "--prompt-template-var",
        "module=src/main.rs",
        "--prompt-template-var",
        "focus=retries",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("template prompt ok"));

    openai.assert_calls(1);
}

#[test]
fn prompt_file_dash_reads_prompt_from_stdin() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .body_includes("prompt from stdin");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "stdin prompt ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt-file",
        "-",
        "--no-session",
    ])
    .write_stdin("prompt from stdin");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("stdin prompt ok"));

    openai.assert_calls(1);
}

#[test]
fn regression_prompt_file_dash_rejects_empty_stdin() {
    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--prompt-file",
        "-",
        "--no-session",
    ])
    .write_stdin(" \n\t");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("stdin prompt"))
        .stderr(predicate::str::contains("is empty"));
}

#[test]
fn regression_empty_prompt_file_fails_fast() {
    let temp = tempdir().expect("tempdir");
    let prompt_path = temp.path().join("empty-prompt.txt");
    fs::write(&prompt_path, " \n\t").expect("write prompt");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--prompt-file",
        prompt_path.to_str().expect("utf8 path"),
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("prompt file"))
        .stderr(predicate::str::contains("is empty"));
}

#[test]
fn regression_prompt_template_file_missing_variable_fails_fast() {
    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("prompt-template.txt");
    fs::write(&template_path, "Summarize {{path}} and {{goal}}").expect("write template");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--prompt-template-file",
        template_path.to_str().expect("utf8 path"),
        "--prompt-template-var",
        "path=src/lib.rs",
        "--no-session",
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "missing a --prompt-template-var value",
    ));
}

#[test]
fn regression_prompt_template_var_requires_key_value_shape() {
    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("prompt-template.txt");
    fs::write(&template_path, "Summarize {{path}}").expect("write template");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--prompt-template-file",
        template_path.to_str().expect("utf8 path"),
        "--prompt-template-var",
        "path",
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("invalid --prompt-template-var"));
}

#[test]
fn system_prompt_file_flag_overrides_inline_system_prompt() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .body_includes("system prompt from file")
            .body_excludes(
                "You are a focused coding assistant. Prefer concrete steps and safe edits.",
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "system prompt file ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let system_prompt_path = temp.path().join("system-prompt.txt");
    fs::write(&system_prompt_path, "system prompt from file").expect("write system prompt");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt-file",
        system_prompt_path.to_str().expect("utf8 path"),
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("system prompt file ok"));

    openai.assert_calls(1);
}

#[test]
fn regression_empty_system_prompt_file_fails_fast() {
    let temp = tempdir().expect("tempdir");
    let system_prompt_path = temp.path().join("empty-system-prompt.txt");
    fs::write(&system_prompt_path, "  \n\t").expect("write system prompt");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt-file",
        system_prompt_path.to_str().expect("utf8 path"),
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("system prompt file"))
        .stderr(predicate::str::contains("is empty"));
}

#[test]
fn tool_audit_log_flag_creates_audit_log_file() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "audit log ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let audit_path = temp.path().join("tool-audit.jsonl");
    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--tool-audit-log",
        audit_path.to_str().expect("utf8 path"),
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("audit log ok"));

    assert!(audit_path.exists());
    openai.assert_calls(1);
}

#[test]
fn telemetry_log_flag_creates_prompt_telemetry_record() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "telemetry log ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let telemetry_path = temp.path().join("prompt-telemetry.jsonl");
    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--telemetry-log",
        telemetry_path.to_str().expect("utf8 path"),
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("telemetry log ok"));

    assert!(telemetry_path.exists());
    let raw = fs::read_to_string(&telemetry_path).expect("read telemetry log");
    let lines = raw.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 1);
    let record: serde_json::Value = serde_json::from_str(lines[0]).expect("parse telemetry record");
    assert_eq!(record["record_type"], "prompt_telemetry_v1");
    assert_eq!(record["provider"], "openai");
    assert_eq!(record["model"], "gpt-4o-mini");
    assert_eq!(record["status"], "completed");
    assert_eq!(record["success"], true);
    assert_eq!(record["token_usage"]["total_tokens"], 6);
    assert_eq!(record["redaction_policy"]["prompt_content"], "omitted");
    openai.assert_calls(1);
}

#[test]
fn interactive_audit_summary_command_reports_aggregates() {
    let temp = tempdir().expect("tempdir");
    let audit_path = temp.path().join("audit.jsonl");
    let rows = [
        json!({
            "event": "tool_execution_end",
            "tool_name": "bash",
            "duration_ms": 25,
            "is_error": false
        }),
        json!({
            "record_type": "prompt_telemetry_v1",
            "provider": "openai",
            "status": "completed",
            "success": true,
            "duration_ms": 90,
            "token_usage": {
                "input_tokens": 3,
                "output_tokens": 1,
                "total_tokens": 4
            }
        }),
    ]
    .iter()
    .map(serde_json::Value::to_string)
    .collect::<Vec<_>>()
    .join("\n");
    fs::write(&audit_path, format!("{rows}\n")).expect("write audit file");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--no-session",
    ])
    .write_stdin(format!(
        "/audit-summary {}\n/quit\n",
        audit_path.to_str().expect("utf8 path")
    ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("audit summary: path="))
        .stdout(predicate::str::contains("tool_breakdown:"))
        .stdout(predicate::str::contains("provider_breakdown:"))
        .stdout(predicate::str::contains("bash count=1"))
        .stdout(predicate::str::contains("openai count=1"));
}

#[test]
fn regression_audit_summary_command_handles_missing_file_without_exiting() {
    let temp = tempdir().expect("tempdir");
    let missing_path = temp.path().join("missing.jsonl");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--no-session",
    ])
    .write_stdin(format!(
        "/audit-summary {}\n/quit\n",
        missing_path.to_str().expect("utf8 path")
    ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("audit summary error:"))
        .stdout(predicate::str::contains("failed to open audit file"));
}

#[test]
fn selected_skill_is_included_in_system_prompt() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .json_body_includes(
                json!({
                    "messages": [{
                        "role": "system",
                        "content": "base\n\n# Skill: focus\nAlways use checklist"
                    }]
                })
                .to_string(),
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 6, "completion_tokens": 1, "total_tokens": 7}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("create skills dir");
    fs::write(skills_dir.join("focus.md"), "Always use checklist").expect("write skill file");

    let mut cmd = binary_command();
    cmd.args([
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
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill",
        "focus",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("ok"));
    openai.assert_calls(1);
}

#[test]
fn install_skill_flag_installs_skill_before_prompt() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .json_body_includes(
                json!({
                    "messages": [{
                        "role": "system",
                        "content": "base\n\n# Skill: installable\nInstalled skill body"
                    }]
                })
                .to_string(),
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok install"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 6, "completion_tokens": 1, "total_tokens": 7}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    let source_skill = temp.path().join("installable.md");
    fs::write(&source_skill, "Installed skill body").expect("write source skill");

    let mut cmd = binary_command();
    cmd.args([
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
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--install-skill",
        source_skill.to_str().expect("utf8 path"),
        "--skill",
        "installable",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills install: installed=1"))
        .stdout(predicate::str::contains("ok install"));
    assert!(skills_dir.join("installable.md").exists());
    openai.assert_calls(1);
}

#[test]
fn skills_lock_write_flag_generates_lockfile_for_local_install() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok lock"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 1, "total_tokens": 5}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    let source_skill = temp.path().join("installable.md");
    fs::write(&source_skill, "Installed skill body").expect("write source skill");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--install-skill",
        source_skill.to_str().expect("utf8 path"),
        "--skills-lock-write",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills lock write: path="))
        .stdout(predicate::str::contains("ok lock"));

    let lock_path = skills_dir.join("skills.lock.json");
    assert!(lock_path.exists());
    let raw = fs::read_to_string(&lock_path).expect("read lockfile");
    let lock: serde_json::Value = serde_json::from_str(&raw).expect("parse lockfile");
    assert_eq!(lock["schema_version"], 1);
    assert_eq!(lock["entries"][0]["file"], "installable.md");
    assert_eq!(lock["entries"][0]["source"]["kind"], "local");
    openai.assert_calls(1);
}

#[test]
fn skills_sync_flag_succeeds_for_matching_lockfile() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");
    let sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": sha,
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(skills_dir.join("skills.lock.json"), format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skills-sync",
        "--no-session",
    ])
    .write_stdin("/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills sync: in-sync"));
}

#[test]
fn regression_skills_sync_flag_fails_on_drift() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "actual body").expect("write skill");
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": "deadbeef",
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(skills_dir.join("skills.lock.json"), format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skills-sync",
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("skills sync drift detected"));
}

#[test]
fn interactive_skills_list_command_prints_inventory() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("zeta.md"), "zeta body").expect("write zeta");
    fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-list\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills list: path="))
        .stdout(predicate::str::contains("count=2"))
        .stdout(predicate::str::contains("skill: name=alpha file=alpha.md"))
        .stdout(predicate::str::contains("skill: name=zeta file=zeta.md"));
}

#[test]
fn regression_interactive_skills_list_command_with_args_prints_usage_and_continues() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-list extra\n/help skills-list\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("usage: /skills-list"))
        .stdout(predicate::str::contains("command: /skills-list"));
}

#[test]
fn interactive_skills_show_command_displays_skill_metadata_and_content() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("checklist.md"), "Always run tests").expect("write skill");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-show checklist\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills show: path="))
        .stdout(predicate::str::contains("name=checklist"))
        .stdout(predicate::str::contains("file=checklist.md"))
        .stdout(predicate::str::contains("Always run tests"));
}

#[test]
fn regression_interactive_skills_show_command_reports_unknown_skill_and_continues() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("known.md"), "known body").expect("write skill");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-show missing\n/help skills-show\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills show error: path="))
        .stdout(predicate::str::contains("unknown skill 'missing'"))
        .stdout(predicate::str::contains("usage: /skills-show <name>"));
}

#[test]
fn interactive_skills_search_command_ranks_name_hits_before_content_hits() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("checklist.md"), "Always run tests").expect("write checklist");
    fs::write(skills_dir.join("quality.md"), "Use checklist for review").expect("write quality");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-search checklist\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills search: path="))
        .stdout(predicate::str::contains("matched=2"))
        .stdout(predicate::str::contains(
            "skill: name=checklist file=checklist.md match=name",
        ))
        .stdout(predicate::str::contains(
            "skill: name=quality file=quality.md match=content",
        ));
}

#[test]
fn regression_interactive_skills_search_command_invalid_limit_reports_error_and_continues() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("checklist.md"), "Always run tests").expect("write skill");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-search checklist 0\n/help skills-search\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills search error: path="))
        .stdout(predicate::str::contains(
            "max_results must be greater than zero",
        ))
        .stdout(predicate::str::contains(
            "usage: /skills-search <query> [max_results]",
        ));
}

#[test]
fn interactive_skills_lock_diff_command_reports_in_sync_state() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");
    let sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": sha,
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(skills_dir.join("skills.lock.json"), format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-lock-diff\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills lock diff: in-sync"))
        .stdout(predicate::str::contains("expected_entries=1"))
        .stdout(predicate::str::contains("actual_entries=1"));
}

#[test]
fn integration_interactive_skills_lock_diff_command_supports_json_output() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");
    let lock_path = temp.path().join("custom.lock.json");
    let sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": sha,
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(&lock_path, format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin(format!(
        "/skills-lock-diff {} --json\n/quit\n",
        lock_path.display()
    ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"status\":\"in_sync\""))
        .stdout(predicate::str::contains("\"in_sync\":true"));
}

#[test]
fn regression_interactive_skills_lock_diff_command_invalid_args_reports_error_and_continues() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-lock-diff one two\n/help skills-lock-diff\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills lock diff error: path="))
        .stdout(predicate::str::contains(
            "usage: /skills-lock-diff [lockfile_path] [--json]",
        ));
}

#[test]
fn interactive_skills_prune_command_dry_run_lists_candidates_without_deleting() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("tracked.md"), "tracked body").expect("write tracked");
    fs::write(skills_dir.join("stale.md"), "stale body").expect("write stale");
    let tracked_sha = format!("{:x}", Sha256::digest("tracked body".as_bytes()));
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "tracked",
            "file": "tracked.md",
            "sha256": tracked_sha,
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(skills_dir.join("skills.lock.json"), format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-prune\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills prune: mode=dry-run"))
        .stdout(predicate::str::contains(
            "prune: file=stale.md action=would_delete",
        ));

    assert!(skills_dir.join("stale.md").exists());
}

#[test]
fn integration_interactive_skills_prune_command_apply_deletes_untracked_files() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("tracked.md"), "tracked body").expect("write tracked");
    fs::write(skills_dir.join("stale.md"), "stale body").expect("write stale");
    let tracked_sha = format!("{:x}", Sha256::digest("tracked body".as_bytes()));
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "tracked",
            "file": "tracked.md",
            "sha256": tracked_sha,
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(skills_dir.join("skills.lock.json"), format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-prune --apply\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills prune: mode=apply"))
        .stdout(predicate::str::contains(
            "prune: file=stale.md action=delete",
        ))
        .stdout(predicate::str::contains(
            "prune: file=stale.md status=deleted",
        ))
        .stdout(predicate::str::contains(
            "skills prune result: mode=apply deleted=1 failed=0",
        ));

    assert!(skills_dir.join("tracked.md").exists());
    assert!(!skills_dir.join("stale.md").exists());
}

#[test]
fn regression_interactive_skills_prune_command_missing_lockfile_reports_error_and_continues() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("stale.md"), "stale body").expect("write stale");
    let missing_lock = temp.path().join("missing.lock.json");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin(format!(
        "/skills-prune {} --apply\n/help skills-prune\n/quit\n",
        missing_lock.display()
    ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills prune error: path="))
        .stdout(predicate::str::contains("failed to read skills lockfile"))
        .stdout(predicate::str::contains(
            "usage: /skills-prune [lockfile_path] [--dry-run|--apply]",
        ));
}

#[test]
fn regression_interactive_skills_prune_command_rejects_unsafe_lockfile_entry() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("stale.md"), "stale body").expect("write stale");
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "escape",
            "file": "../escape.md",
            "sha256": "abc123",
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(skills_dir.join("skills.lock.json"), format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-prune\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills prune error: path="))
        .stdout(predicate::str::contains(
            "unsafe lockfile entry '../escape.md'",
        ));
}

#[test]
fn integration_interactive_skills_trust_list_command_reports_mixed_statuses() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    let trust_path = temp.path().join("trust-roots.json");
    let payload = json!({
        "roots": [
            {
                "id": "zeta",
                "public_key": "eg==",
                "revoked": false,
                "expires_unix": 1,
                "rotated_from": null
            },
            {
                "id": "alpha",
                "public_key": "YQ==",
                "revoked": false,
                "expires_unix": null,
                "rotated_from": null
            },
            {
                "id": "beta",
                "public_key": "Yg==",
                "revoked": true,
                "expires_unix": null,
                "rotated_from": "alpha"
            }
        ]
    });
    fs::write(&trust_path, format!("{payload}\n")).expect("write trust file");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill-trust-root-file",
        trust_path.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-trust-list\n/quit\n");

    let output = cmd.assert().success().get_output().stdout.clone();
    let stdout = String::from_utf8(output).expect("stdout should be utf8");
    assert!(stdout.contains("skills trust list: path="));
    assert!(stdout.contains("count=3"));
    let alpha_index = stdout
        .find("root: id=alpha revoked=false")
        .expect("alpha row");
    let beta_index = stdout.find("root: id=beta revoked=true").expect("beta row");
    let zeta_index = stdout
        .find("root: id=zeta revoked=false")
        .expect("zeta row");
    assert!(alpha_index < beta_index);
    assert!(beta_index < zeta_index);
    assert!(stdout.contains("rotated_from=alpha status=revoked"));
    assert!(stdout.contains("expires_unix=1 rotated_from=none status=expired"));
}

#[test]
fn regression_interactive_skills_trust_list_command_malformed_json_reports_error_and_continues() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    let trust_path = temp.path().join("trust-roots.json");
    fs::write(&trust_path, "{invalid-json").expect("write malformed trust file");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin(format!(
        "/skills-trust-list {}\n/help skills-trust-list\n/quit\n",
        trust_path.display()
    ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills trust list error: path="))
        .stdout(predicate::str::contains(
            "failed to parse trusted root file",
        ))
        .stdout(predicate::str::contains(
            "usage: /skills-trust-list [trust_root_file]",
        ));
}

#[test]
fn integration_interactive_skills_trust_mutation_commands_roundtrip() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    let trust_path = temp.path().join("trust-roots.json");
    let payload = json!({
        "roots": [
            {
                "id": "old",
                "public_key": "YQ==",
                "revoked": false,
                "expires_unix": null,
                "rotated_from": null
            }
        ]
    });
    fs::write(&trust_path, format!("{payload}\n")).expect("write trust file");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill-trust-root-file",
        trust_path.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin(
        "/skills-trust-add extra=Yg==\n/skills-trust-revoke extra\n/skills-trust-rotate old:new=Yw==\n/skills-trust-list\n/quit\n",
    );

    let output = cmd.assert().success().get_output().stdout.clone();
    let stdout = String::from_utf8(output).expect("stdout should be utf8");
    assert!(stdout.contains("skills trust add: path="));
    assert!(stdout.contains("id=extra"));
    assert!(stdout.contains("skills trust revoke: path="));
    assert!(stdout.contains("id=extra"));
    assert!(stdout.contains("skills trust rotate: path="));
    assert!(stdout.contains("old_id=old new_id=new"));
    assert!(stdout.contains("root: id=old"));
    assert!(stdout.contains("root: id=new"));
    assert!(stdout.contains("rotated_from=old status=active"));
    assert!(stdout.contains("root: id=extra"));
    assert!(stdout.contains("status=revoked"));

    let trust_raw = fs::read_to_string(&trust_path).expect("read trust file");
    assert!(trust_raw.contains("\"id\": \"old\""));
    assert!(trust_raw.contains("\"revoked\": true"));
    assert!(trust_raw.contains("\"id\": \"new\""));
    assert!(trust_raw.contains("\"rotated_from\": \"old\""));
}

#[test]
fn regression_interactive_skills_trust_add_without_configured_path_reports_error() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-trust-add root=YQ==\n/help skills-trust-add\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "skills trust add error: path=none",
        ))
        .stdout(predicate::str::contains(
            "usage: /skills-trust-add <id=base64_key> [trust_root_file]",
        ));
}

#[test]
fn regression_interactive_skills_trust_revoke_unknown_id_reports_error() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    let trust_path = temp.path().join("trust-roots.json");
    fs::write(&trust_path, "[]\n").expect("write trust file");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill-trust-root-file",
        trust_path.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-trust-revoke missing\n/quit\n");

    cmd.assert().success().stdout(predicate::str::contains(
        "cannot revoke unknown trust key id 'missing'",
    ));
}

#[test]
fn integration_interactive_skills_verify_command_reports_combined_compliance() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");
    fs::write(skills_dir.join("extra.md"), "untracked body").expect("write extra");

    let lock_path = skills_dir.join("skills.lock.json");
    let trust_path = temp.path().join("trust-roots.json");
    let skill_sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let signature = "c2ln";
    let signature_sha = format!("{:x}", Sha256::digest(signature.as_bytes()));
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": skill_sha,
            "source": {
                "kind": "remote",
                "url": "https://example.com/focus.md",
                "expected_sha256": skill_sha,
                "signing_key_id": "unknown",
                "signature": signature,
                "signer_public_key": "YQ==",
                "signature_sha256": signature_sha
            }
        }]
    });
    fs::write(&lock_path, format!("{lockfile}\n")).expect("write lock");
    let trust = json!({
        "roots": [{
            "id": "root",
            "public_key": "YQ==",
            "revoked": false,
            "expires_unix": null,
            "rotated_from": null
        }]
    });
    fs::write(&trust_path, format!("{trust}\n")).expect("write trust");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill-trust-root-file",
        trust_path.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-verify\n/skills-verify --json\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills verify: status=fail"))
        .stdout(predicate::str::contains(
            "sync: expected_entries=1 actual_entries=2",
        ))
        .stdout(predicate::str::contains("signature=untrusted key=unknown"))
        .stdout(predicate::str::contains("\"status\":\"fail\""));
}

#[test]
fn regression_interactive_skills_verify_command_invalid_args_report_usage() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-verify one two three\n/help skills-verify\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills verify error: path="))
        .stdout(predicate::str::contains(
            "usage: /skills-verify [lockfile_path] [trust_root_file] [--json]",
        ));
}

#[test]
fn interactive_skills_lock_write_command_writes_default_lockfile() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-lock-write\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills lock write: path="))
        .stdout(predicate::str::contains("entries=1"));

    let lock_path = skills_dir.join("skills.lock.json");
    let raw = fs::read_to_string(lock_path).expect("read lock");
    assert!(raw.contains("\"file\": \"focus.md\""));
}

#[test]
fn integration_interactive_skills_lock_write_command_accepts_optional_lockfile_path() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");
    let custom_lock_path = temp.path().join("custom.lock.json");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin(format!(
        "/skills-lock-write {}\n/quit\n",
        custom_lock_path.display()
    ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills lock write: path="))
        .stdout(predicate::str::contains(
            custom_lock_path.display().to_string(),
        ));

    let raw = fs::read_to_string(custom_lock_path).expect("read custom lock");
    assert!(raw.contains("\"file\": \"focus.md\""));
}

#[test]
fn regression_interactive_skills_lock_write_command_reports_error_and_continues_loop() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");
    let blocking_path = temp.path().join("blocking.lock");
    fs::create_dir_all(&blocking_path).expect("create blocking path");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin(format!(
        "/skills-lock-write {}\n/help skills-lock-write\n/quit\n",
        blocking_path.display()
    ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills lock write error: path="))
        .stdout(predicate::str::contains(
            "usage: /skills-lock-write [lockfile_path]",
        ));
}

#[test]
fn interactive_skills_sync_command_uses_default_lockfile_path() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");
    let sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": sha,
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(skills_dir.join("skills.lock.json"), format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-sync\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills sync: in-sync"))
        .stdout(predicate::str::contains("expected_entries=1"))
        .stdout(predicate::str::contains("actual_entries=1"));
}

#[test]
fn integration_interactive_skills_sync_command_accepts_optional_lockfile_path() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");
    let lock_path = temp.path().join("custom.lock.json");
    let sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": sha,
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(&lock_path, format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin(format!("/skills-sync {}\n/quit\n", lock_path.display()));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills sync: in-sync"))
        .stdout(predicate::str::contains(lock_path.display().to_string()));
}

#[test]
fn regression_interactive_skills_sync_command_reports_drift_and_continues_loop() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&skills_dir).expect("mkdir");
    fs::write(skills_dir.join("focus.md"), "actual body").expect("write skill");
    let lockfile = json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": "deadbeef",
            "source": {
                "kind": "unknown"
            }
        }]
    });
    fs::write(skills_dir.join("skills.lock.json"), format!("{lockfile}\n")).expect("write lock");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/skills-sync\n/help skills-sync\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("skills sync: drift"))
        .stdout(predicate::str::contains("changed=focus.md"))
        .stdout(predicate::str::contains(
            "usage: /skills-sync [lockfile_path]",
        ));
}

#[path = "tooling_skills/skills_registry_install.rs"]
mod skills_registry_install;
