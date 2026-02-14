//! Tests for extension manifest parsing, registration, and runtime hook behavior.

use super::{
    apply_extension_message_transforms, discover_extension_runtime_registrations,
    dispatch_extension_registered_command, dispatch_extension_runtime_hook,
    evaluate_extension_policy_override, execute_extension_process_hook,
    execute_extension_registered_tool, extension_shell_fallback_candidates,
    format_extension_process_stdin_payload, list_extension_manifests,
    parse_message_transform_response_prompt, parse_policy_override_response,
    render_extension_list_report, render_extension_manifest_report, required_permission_for_hook,
    validate_extension_manifest, ExtensionHook, ExtensionListReport, ExtensionManifest,
    ExtensionManifestSummary, ExtensionPermission, ExtensionRegisteredCommandAction,
    ExtensionRuntime, PolicyOverrideDecision,
};
use std::{fs, path::PathBuf};
use tempfile::tempdir;

#[test]
fn unit_validate_extension_manifest_accepts_minimal_schema() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("extension.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant"
}"#,
    )
    .expect("write manifest");

    let summary = validate_extension_manifest(&manifest_path).expect("valid manifest");
    assert_eq!(summary.id, "issue-assistant");
    assert_eq!(summary.version, "0.1.0");
    assert_eq!(summary.runtime, "process");
    assert_eq!(summary.entrypoint, "bin/assistant");
    assert_eq!(summary.hook_count, 0);
    assert_eq!(summary.permission_count, 0);
    assert_eq!(summary.timeout_ms, 5_000);
}

#[test]
fn regression_validate_extension_manifest_rejects_parent_dir_entrypoint() {
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
    .expect("write manifest");

    let error =
        validate_extension_manifest(&manifest_path).expect_err("parent traversal should fail");
    assert!(error
        .to_string()
        .contains("must not contain parent traversals"));
}

#[test]
fn regression_validate_extension_manifest_rejects_duplicate_hooks() {
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
  "hooks": ["run-start", "run-start"]
}"#,
    )
    .expect("write manifest");

    let error =
        validate_extension_manifest(&manifest_path).expect_err("duplicate hooks should fail");
    assert!(error.to_string().contains("contains duplicate entries"));
}

#[test]
fn unit_render_extension_manifest_report_is_deterministic() {
    let summary = ExtensionManifestSummary {
        manifest_path: PathBuf::from("extensions/issue-assistant/extension.json"),
        id: "issue-assistant".to_string(),
        version: "0.1.0".to_string(),
        runtime: "process".to_string(),
        entrypoint: "bin/assistant".to_string(),
        hook_count: 2,
        permission_count: 2,
        timeout_ms: 60_000,
    };
    let manifest = ExtensionManifest {
        schema_version: 1,
        id: "issue-assistant".to_string(),
        version: "0.1.0".to_string(),
        runtime: ExtensionRuntime::Process,
        entrypoint: "bin/assistant".to_string(),
        hooks: vec![ExtensionHook::RunStart, ExtensionHook::RunEnd],
        permissions: vec![ExtensionPermission::Network, ExtensionPermission::ReadFiles],
        tools: vec![],
        commands: vec![],
        timeout_ms: 60_000,
    };

    let report = render_extension_manifest_report(&summary, &manifest);
    assert!(report.contains("extension show:"));
    assert!(report.contains("- id: issue-assistant"));
    assert!(report.contains("- hooks (2):\n- run-end\n- run-start"));
    assert!(report.contains("- permissions (2):\n- network\n- read-files"));
}

#[test]
fn unit_render_extension_list_report_is_deterministic() {
    let report = ExtensionListReport {
        list_root: PathBuf::from("extensions"),
        entries: vec![super::ExtensionListEntry {
            manifest_path: PathBuf::from("extensions/issue-assistant/extension.json"),
            id: "issue-assistant".to_string(),
            version: "0.1.0".to_string(),
            runtime: "process".to_string(),
        }],
        invalid_entries: vec![super::ExtensionListInvalidEntry {
            manifest_path: PathBuf::from("extensions/bad/extension.json"),
            error: "unsupported extension manifest schema".to_string(),
        }],
    };

    let rendered = render_extension_list_report(&report);
    assert!(rendered.contains("extension list: root=extensions count=1 invalid=1"));
    assert!(rendered.contains(
        "extension: id=issue-assistant version=0.1.0 runtime=process manifest=extensions/issue-assistant/extension.json"
    ));
    assert!(rendered.contains("invalid: manifest=extensions/bad/extension.json error=unsupported extension manifest schema"));
}

#[test]
fn regression_list_extension_manifests_reports_invalid_entries_without_failing() {
    let temp = tempdir().expect("tempdir");
    let good_dir = temp.path().join("good");
    fs::create_dir_all(&good_dir).expect("create good dir");
    fs::write(
        good_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant"
}"#,
    )
    .expect("write valid extension");

    let bad_dir = temp.path().join("bad");
    fs::create_dir_all(&bad_dir).expect("create bad dir");
    fs::write(
        bad_dir.join("extension.json"),
        r#"{
  "schema_version": 9,
  "id": "broken",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bin/assistant"
}"#,
    )
    .expect("write invalid extension");

    let report = list_extension_manifests(temp.path()).expect("list should succeed");
    assert_eq!(report.entries.len(), 1);
    assert_eq!(report.invalid_entries.len(), 1);
    assert_eq!(report.entries[0].id, "issue-assistant");
    assert!(report.invalid_entries[0]
        .error
        .contains("unsupported extension manifest schema"));
}

#[test]
fn regression_list_extension_manifests_rejects_non_directory_root() {
    let temp = tempdir().expect("tempdir");
    let root_file = temp.path().join("extensions.json");
    fs::write(&root_file, "{}").expect("write root file");

    let error = list_extension_manifests(&root_file).expect_err("non-directory root should fail");
    assert!(error.to_string().contains("is not a directory"));
}

fn make_executable(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("set executable permissions");
    }
}

#[test]
fn functional_execute_extension_process_hook_runs_process_runtime() {
    let temp = tempdir().expect("tempdir");
    let script_path = temp.path().join("hook.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf '{\"ok\":true,\"result\":\"hook-processed\"}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    let manifest_path = temp.path().join("extension.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["run-start"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");

    let payload = serde_json::json!({"event":"created"});
    let summary = execute_extension_process_hook(&manifest_path, "run-start", &payload)
        .expect("extension execution should succeed");
    assert_eq!(summary.id, "issue-assistant");
    assert_eq!(summary.hook, "run-start");
    assert!(summary.response.contains("\"ok\":true"));
}

#[test]
fn regression_execute_extension_process_hook_rejects_undeclared_hook() {
    let temp = tempdir().expect("tempdir");
    let script_path = temp.path().join("hook.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf '{\"ok\":true}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    let manifest_path = temp.path().join("extension.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["run-end"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");

    let payload = serde_json::json!({"event":"created"});
    let error = execute_extension_process_hook(&manifest_path, "run-start", &payload)
        .expect_err("undeclared hook should fail");
    assert!(error.to_string().contains("does not declare hook"));
}

#[test]
fn regression_execute_extension_process_hook_enforces_timeout() {
    let temp = tempdir().expect("tempdir");
    let script_path = temp.path().join("slow.sh");
    fs::write(&script_path, "#!/bin/sh\nwhile :; do :; done\n").expect("write script");
    make_executable(&script_path);

    let manifest_path = temp.path().join("extension.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "slow.sh",
  "hooks": ["run-start"],
  "permissions": ["run-commands"],
  "timeout_ms": 20
}"#,
    )
    .expect("write manifest");

    let payload = serde_json::json!({"event":"created"});
    let error = execute_extension_process_hook(&manifest_path, "run-start", &payload)
        .expect_err("timeout should fail");
    assert!(error.to_string().contains("timed out"));
}

#[test]
fn regression_execute_extension_process_hook_rejects_invalid_json_output() {
    let temp = tempdir().expect("tempdir");
    let script_path = temp.path().join("bad-output.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf 'not-json'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    let manifest_path = temp.path().join("extension.json");
    fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "issue-assistant",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "bad-output.sh",
  "hooks": ["run-start"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");

    let payload = serde_json::json!({"event":"created"});
    let error = execute_extension_process_hook(&manifest_path, "run-start", &payload)
        .expect_err("invalid output should fail");
    assert!(error.to_string().contains("response must be valid JSON"));
}

#[test]
fn unit_dispatch_extension_runtime_hook_orders_execution_deterministically() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let alpha_dir = root.join("alpha");
    let beta_dir = root.join("beta");
    fs::create_dir_all(&alpha_dir).expect("create alpha dir");
    fs::create_dir_all(&beta_dir).expect("create beta dir");

    let alpha_script = alpha_dir.join("hook.sh");
    fs::write(
        &alpha_script,
        "#!/bin/sh\nread -r _input\nprintf '{\"ok\":true}'\n",
    )
    .expect("write alpha script");
    make_executable(&alpha_script);

    let beta_script = beta_dir.join("hook.sh");
    fs::write(
        &beta_script,
        "#!/bin/sh\nread -r _input\nprintf '{\"ok\":true}'\n",
    )
    .expect("write beta script");
    make_executable(&beta_script);

    fs::write(
        alpha_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "aaa-extension",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["run-start"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write alpha manifest");
    fs::write(
        beta_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "zzz-extension",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["run-start"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write beta manifest");

    let report = dispatch_extension_runtime_hook(&root, "run-start", &serde_json::json!({}));
    assert_eq!(report.discovered, 2);
    assert_eq!(report.executed, 2);
    assert_eq!(
        report.executed_ids,
        vec![
            "aaa-extension@1.0.0".to_string(),
            "zzz-extension@1.0.0".to_string()
        ]
    );
}

#[test]
fn functional_dispatch_extension_runtime_hook_runs_process_extensions() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let extension_dir = root.join("issue-assistant");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("hook.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf '{\"ok\":true}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

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
    .expect("write manifest");

    let report = dispatch_extension_runtime_hook(
        &root,
        "run-start",
        &serde_json::json!({"event":"started"}),
    );
    assert_eq!(report.executed, 1);
    assert_eq!(report.failed, 0);
    assert!(report.diagnostics.is_empty());
}

#[test]
fn regression_dispatch_extension_runtime_hook_isolates_failures() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let good_dir = root.join("good");
    let bad_dir = root.join("bad");
    fs::create_dir_all(&good_dir).expect("create good dir");
    fs::create_dir_all(&bad_dir).expect("create bad dir");

    let good_script = good_dir.join("hook.sh");
    fs::write(
        &good_script,
        "#!/bin/sh\nread -r _input\nprintf '{\"ok\":true}'\n",
    )
    .expect("write good script");
    make_executable(&good_script);

    let bad_script = bad_dir.join("slow.sh");
    fs::write(&bad_script, "#!/bin/sh\nsleep 1\nprintf '{\"ok\":true}'\n")
        .expect("write bad script");
    make_executable(&bad_script);

    fs::write(
        good_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "good-extension",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["run-start"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write good manifest");
    fs::write(
        bad_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "bad-extension",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "slow.sh",
  "hooks": ["run-start"],
  "permissions": ["run-commands"],
  "timeout_ms": 20
}"#,
    )
    .expect("write bad manifest");

    let report = dispatch_extension_runtime_hook(&root, "run-start", &serde_json::json!({}));
    assert_eq!(report.discovered, 2);
    assert_eq!(report.executed, 1);
    assert_eq!(report.failed, 1);
    assert!(report
        .diagnostics
        .iter()
        .any(|line| line.contains("timed out")));
}

#[test]
fn regression_dispatch_extension_runtime_hook_skips_invalid_manifests() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let valid_dir = root.join("valid");
    let invalid_dir = root.join("invalid");
    fs::create_dir_all(&valid_dir).expect("create valid dir");
    fs::create_dir_all(&invalid_dir).expect("create invalid dir");

    let script_path = valid_dir.join("hook.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf '{\"ok\":true}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    fs::write(
        valid_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "valid-extension",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["run-start"],
  "permissions": ["run-commands"]
}"#,
    )
    .expect("write valid manifest");
    fs::write(
        invalid_dir.join("extension.json"),
        r#"{
  "schema_version": 9,
  "id": "invalid-extension",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "hook.sh"
}"#,
    )
    .expect("write invalid manifest");

    let report = dispatch_extension_runtime_hook(&root, "run-start", &serde_json::json!({}));
    assert_eq!(report.executed, 1);
    assert_eq!(report.skipped_invalid, 1);
    assert!(report
        .diagnostics
        .iter()
        .any(|line| line.contains("skipped invalid manifest")));
}

#[test]
fn functional_dispatch_extension_runtime_hook_skips_missing_permission() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let extension_dir = root.join("missing-permission");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("hook.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf '{\"ok\":true}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "missing-permission",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["run-start"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");

    let report = dispatch_extension_runtime_hook(&root, "run-start", &serde_json::json!({}));
    assert_eq!(report.executed, 0);
    assert_eq!(report.skipped_permission_denied, 1);
    assert!(report
        .diagnostics
        .iter()
        .any(|line| line.contains("missing required permission=run-commands")));
}

#[test]
fn unit_parse_message_transform_response_prompt_accepts_valid_prompt() {
    let prompt =
        parse_message_transform_response_prompt(r#"{"prompt":"refined prompt"}"#).expect("ok");
    assert_eq!(prompt.as_deref(), Some("refined prompt"));
}

#[test]
fn regression_parse_message_transform_response_prompt_rejects_non_string_prompt() {
    let error = parse_message_transform_response_prompt(r#"{"prompt":42}"#)
        .expect_err("non-string prompt should fail");
    assert!(error.to_string().contains("must be a string"));
}

#[test]
fn unit_format_extension_process_stdin_payload_appends_newline() {
    let payload = format_extension_process_stdin_payload(r#"{"hook":"run-start"}"#);
    assert_eq!(payload, "{\"hook\":\"run-start\"}\n");
}

#[test]
fn unit_extension_shell_fallback_candidates_include_sh() {
    let candidates = extension_shell_fallback_candidates();
    assert!(candidates.contains(&"sh"));
    #[cfg(unix)]
    assert_eq!(candidates.first().copied(), Some("/bin/sh"));
}

#[test]
fn functional_apply_extension_message_transforms_rewrites_prompt() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let extension_dir = root.join("transformer");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("transform.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nIFS= read -r _input\nprintf '{\"prompt\":\"rewritten prompt\"}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

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
    .expect("write manifest");

    let result = apply_extension_message_transforms(&root, "original prompt");
    assert_eq!(
        result.prompt, "rewritten prompt",
        "transform diagnostics: {:?}",
        result.diagnostics
    );
    assert_eq!(result.executed, 1);
    assert_eq!(result.applied, 1);
    assert_eq!(result.applied_ids, vec!["transformer@0.1.0".to_string()]);
}

#[test]
fn integration_apply_extension_message_transforms_supports_strict_line_readers() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let extension_dir = root.join("strict-transformer");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("transform.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nset -eu\nIFS= read -r _input\nprintf '{\"prompt\":\"strict rewritten\"}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "strict-transformer",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "transform.sh",
  "hooks": ["message-transform"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");

    let result = apply_extension_message_transforms(&root, "original prompt");
    assert_eq!(result.prompt, "strict rewritten");
    assert_eq!(result.executed, 1);
    assert_eq!(result.applied, 1);
}

#[test]
fn integration_apply_extension_message_transforms_applies_in_deterministic_order() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let a_dir = root.join("a");
    let b_dir = root.join("b");
    fs::create_dir_all(&a_dir).expect("create a dir");
    fs::create_dir_all(&b_dir).expect("create b dir");

    let a_script = a_dir.join("transform.sh");
    fs::write(
        &a_script,
        "#!/bin/sh\nIFS= read -r _input\nprintf '{\"prompt\":\"alpha\"}'\n",
    )
    .expect("write a script");
    make_executable(&a_script);
    let b_script = b_dir.join("transform.sh");
    fs::write(
        &b_script,
        "#!/bin/sh\nIFS= read -r _input\nprintf '{\"prompt\":\"beta\"}'\n",
    )
    .expect("write b script");
    make_executable(&b_script);

    fs::write(
        a_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "a-extension",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "transform.sh",
  "hooks": ["message-transform"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write a manifest");
    fs::write(
        b_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "b-extension",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "transform.sh",
  "hooks": ["message-transform"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write b manifest");

    let result = apply_extension_message_transforms(&root, "seed");
    assert_eq!(result.prompt, "beta");
    assert_eq!(result.applied, 2);
    assert_eq!(
        result.applied_ids,
        vec![
            "a-extension@1.0.0".to_string(),
            "b-extension@1.0.0".to_string()
        ]
    );
}

#[test]
fn regression_apply_extension_message_transforms_falls_back_on_invalid_output() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let extension_dir = root.join("broken-transformer");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("transform.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nIFS= read -r _input\nprintf '{\"prompt\":123}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

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
    .expect("write manifest");

    let result = apply_extension_message_transforms(&root, "original prompt");
    assert_eq!(result.prompt, "original prompt");
    assert_eq!(result.executed, 1);
    assert_eq!(result.applied, 0);
    assert!(result
        .diagnostics
        .iter()
        .any(|line| line.contains("must be a string")));
}

#[test]
fn regression_apply_extension_message_transforms_remains_stable_over_repeated_runs() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let extension_dir = root.join("stable-transformer");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("transform.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nset -eu\nIFS= read -r _input\nprintf '{\"prompt\":\"stable rewritten\"}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "stable-transformer",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "transform.sh",
  "hooks": ["message-transform"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");

    for _ in 0..24 {
        let result = apply_extension_message_transforms(&root, "original prompt");
        assert_eq!(result.prompt, "stable rewritten");
        assert_eq!(result.executed, 1);
        assert_eq!(result.applied, 1);
        assert!(result.diagnostics.is_empty());
    }
}

#[test]
fn regression_apply_extension_message_transforms_skips_missing_permission() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let extension_dir = root.join("missing-permission");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("transform.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nIFS= read -r _input\nprintf '{\"prompt\":\"rewritten\"}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "missing-permission",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "transform.sh",
  "hooks": ["message-transform"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");

    let result = apply_extension_message_transforms(&root, "original prompt");
    assert_eq!(result.prompt, "original prompt");
    assert_eq!(result.executed, 0);
    assert_eq!(result.applied, 0);
    assert_eq!(result.skipped_permission_denied, 1);
    assert!(result
        .diagnostics
        .iter()
        .any(|line| line.contains("missing required permission=run-commands")));
}

#[test]
fn unit_parse_policy_override_response_accepts_allow_decision() {
    let response =
        parse_policy_override_response(r#"{"decision":"allow"}"#).expect("response parses");
    assert_eq!(response.decision, PolicyOverrideDecision::Allow);
    assert_eq!(response.reason, None);
}

#[test]
fn unit_required_permission_for_policy_override_hook_is_run_commands() {
    assert_eq!(
        required_permission_for_hook(&ExtensionHook::PolicyOverride),
        Some(ExtensionPermission::RunCommands)
    );
    assert_eq!(
        required_permission_for_hook(&ExtensionHook::RunStart),
        Some(ExtensionPermission::RunCommands)
    );
}

#[test]
fn unit_parse_policy_override_response_accepts_deny_decision_with_reason() {
    let response = parse_policy_override_response(r#"{"decision":"deny","reason":"blocked"}"#)
        .expect("response parses");
    assert_eq!(response.decision, PolicyOverrideDecision::Deny);
    assert_eq!(response.reason.as_deref(), Some("blocked"));
}

#[test]
fn regression_parse_policy_override_response_rejects_invalid_decision() {
    let error = parse_policy_override_response(r#"{"decision":"defer"}"#)
        .expect_err("invalid decision should fail");
    assert!(error.to_string().contains("must be 'allow' or 'deny'"));
}

#[test]
fn functional_evaluate_extension_policy_override_denies_command() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let extension_dir = root.join("policy-enforcer");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("policy.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf '{\"decision\":\"deny\",\"reason\":\"blocked by extension\"}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "policy-enforcer",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "policy.sh",
  "hooks": ["policy-override"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");

    let result = evaluate_extension_policy_override(
        &root,
        &serde_json::json!({"command":"printf 'ok'","tool":"bash"}),
    );
    assert!(!result.allowed);
    assert_eq!(result.denied, 1);
    assert_eq!(result.evaluated, 1);
    assert_eq!(result.denied_by.as_deref(), Some("policy-enforcer@1.0.0"));
    assert_eq!(result.reason.as_deref(), Some("blocked by extension"));
}

#[test]
fn integration_evaluate_extension_policy_override_allows_command() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let extension_dir = root.join("policy-enforcer");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("policy.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf '{\"decision\":\"allow\"}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "policy-enforcer",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "policy.sh",
  "hooks": ["policy-override"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");

    let result = evaluate_extension_policy_override(
        &root,
        &serde_json::json!({"command":"printf 'ok'","tool":"bash"}),
    );
    assert!(result.allowed);
    assert_eq!(result.denied, 0);
    assert_eq!(result.evaluated, 1);
    assert_eq!(result.reason, None);
}

#[test]
fn regression_evaluate_extension_policy_override_fails_closed_on_invalid_response() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let extension_dir = root.join("broken-policy");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("policy.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf '{\"decision\":123}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "broken-policy",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "policy.sh",
  "hooks": ["policy-override"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");

    let result = evaluate_extension_policy_override(
        &root,
        &serde_json::json!({"command":"printf 'ok'","tool":"bash"}),
    );
    assert!(!result.allowed);
    assert_eq!(result.denied, 1);
    assert_eq!(result.denied_by.as_deref(), Some("broken-policy@1.0.0"));
    assert!(result
        .reason
        .as_deref()
        .unwrap_or_default()
        .contains("invalid response"));
    assert!(result
        .diagnostics
        .iter()
        .any(|line| line.contains("invalid response")));
}

#[test]
fn regression_evaluate_extension_policy_override_fails_closed_on_missing_permission() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let extension_dir = root.join("missing-permission");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("policy.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf '{\"decision\":\"allow\"}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "missing-permission",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "policy.sh",
  "hooks": ["policy-override"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write manifest");

    let result = evaluate_extension_policy_override(
        &root,
        &serde_json::json!({"command":"printf 'ok'","tool":"bash"}),
    );
    assert!(!result.allowed);
    assert_eq!(result.denied, 1);
    assert_eq!(result.permission_denied, 1);
    assert_eq!(
        result.denied_by.as_deref(),
        Some("missing-permission@1.0.0")
    );
    assert!(result
        .reason
        .as_deref()
        .unwrap_or_default()
        .contains("requires 'run-commands' permission"));
    assert!(result
        .diagnostics
        .iter()
        .any(|line| line.contains("missing required permission=run-commands")));
}

#[test]
fn unit_validate_extension_manifest_rejects_duplicate_registered_tool_names() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("extension.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "tool-registry",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "tool.sh",
  "permissions": ["run-commands"],
  "tools": [
{
  "name": "triage",
  "description": "first",
  "parameters": {"type":"object","properties":{}}
},
{
  "name": "triage",
  "description": "second",
  "parameters": {"type":"object","properties":{}}
}
  ]
}"#,
    )
    .expect("write manifest");

    let error =
        validate_extension_manifest(&manifest_path).expect_err("duplicate tools should fail");
    assert!(error.to_string().contains("duplicate name 'triage'"));
}

#[test]
fn unit_validate_extension_manifest_rejects_invalid_registered_command_name() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("extension.json");
    std::fs::write(
        &manifest_path,
        r#"{
  "schema_version": 1,
  "id": "command-registry",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "command.sh",
  "permissions": ["run-commands"],
  "commands": [
{
  "name": "/Bad Name",
  "description": "invalid"
}
  ]
}"#,
    )
    .expect("write manifest");

    let error =
        validate_extension_manifest(&manifest_path).expect_err("invalid command names should fail");
    assert!(error.to_string().contains("must not contain whitespace"));
}

#[test]
fn functional_discover_extension_runtime_registrations_collects_tools_and_commands() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let extension_dir = root.join("registry");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("runtime.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf '{\"output\":\"ok\",\"content\":{\"status\":\"ok\"}}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "registry",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "runtime.sh",
  "permissions": ["run-commands"],
  "tools": [
{
  "name": "issue_triage",
  "description": "Triage issue labels",
  "parameters": {
    "type": "object",
    "properties": {
      "title": { "type": "string" }
    },
    "required": ["title"],
    "additionalProperties": false
  }
}
  ],
  "commands": [
{
  "name": "triage-now",
  "description": "Run triage command",
  "usage": "/triage-now <id>"
}
  ]
}"#,
    )
    .expect("write manifest");

    let summary = discover_extension_runtime_registrations(&root, &["/help"]);
    assert_eq!(summary.discovered, 1);
    assert_eq!(summary.registered_tools.len(), 1);
    assert_eq!(summary.registered_tools[0].name, "issue_triage");
    assert_eq!(summary.registered_commands.len(), 1);
    assert_eq!(summary.registered_commands[0].name, "/triage-now");
    assert!(summary.diagnostics.is_empty());
}

#[test]
fn regression_discover_extension_runtime_registrations_blocks_builtin_name_conflicts() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let extension_dir = root.join("conflict");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("runtime.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf '{\"output\":\"ok\",\"content\":{\"status\":\"ok\"}}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "conflict",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "runtime.sh",
  "permissions": ["run-commands"],
  "tools": [
{
  "name": "read",
  "description": "conflict",
  "parameters": {"type":"object","properties":{}}
}
  ],
  "commands": [
{
  "name": "/help",
  "description": "conflict"
}
  ]
}"#,
    )
    .expect("write manifest");

    let summary = discover_extension_runtime_registrations(&root, &["/help"]);
    assert!(summary.registered_tools.is_empty());
    assert!(summary.registered_commands.is_empty());
    assert_eq!(summary.skipped_name_conflict, 2);
    assert!(summary
        .diagnostics
        .iter()
        .any(|line| line.contains("name conflicts with built-in tool")));
    assert!(summary
        .diagnostics
        .iter()
        .any(|line| line.contains("name conflicts with built-in command")));
}

#[test]
fn functional_dispatch_extension_registered_command_returns_output() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let extension_dir = root.join("commands");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("runtime.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf '{\"output\":\"command complete\",\"action\":\"continue\"}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "commands",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "runtime.sh",
  "permissions": ["run-commands"],
  "commands": [
{
  "name": "/triage-now",
  "description": "Run triage command"
}
  ]
}"#,
    )
    .expect("write manifest");

    let summary = discover_extension_runtime_registrations(&root, &[]);
    let result =
        dispatch_extension_registered_command(&summary.registered_commands, "/triage-now", "123")
            .expect("dispatch should succeed")
            .expect("command should match");
    assert_eq!(result.output.as_deref(), Some("command complete"));
    assert_eq!(result.action, ExtensionRegisteredCommandAction::Continue);
}

#[test]
fn integration_execute_extension_registered_tool_returns_content() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let extension_dir = root.join("tools");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("runtime.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf '{\"content\":{\"status\":\"ok\",\"message\":\"done\"},\"is_error\":false}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "tools",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "runtime.sh",
  "permissions": ["run-commands"],
  "tools": [
{
  "name": "issue_triage",
  "description": "Triage issue labels",
  "parameters": {"type":"object","properties":{}}
}
  ]
}"#,
    )
    .expect("write manifest");

    let summary = discover_extension_runtime_registrations(&root, &[]);
    let tool = summary
        .registered_tools
        .first()
        .expect("registered tool should exist");
    let result = execute_extension_registered_tool(tool, &serde_json::json!({"title":"bug"}))
        .expect("tool execution should succeed");
    assert_eq!(result.content["status"], "ok");
    assert_eq!(result.content["message"], "done");
    assert!(!result.is_error);
}

#[test]
fn regression_execute_extension_registered_tool_rejects_missing_content_field() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("extensions");
    let extension_dir = root.join("bad-tool");
    fs::create_dir_all(&extension_dir).expect("create extension dir");

    let script_path = extension_dir.join("runtime.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nread -r _input\nprintf '{\"is_error\":false}'\n",
    )
    .expect("write script");
    make_executable(&script_path);

    fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "bad-tool",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "runtime.sh",
  "permissions": ["run-commands"],
  "tools": [
{
  "name": "issue_triage",
  "description": "Triage issue labels",
  "parameters": {"type":"object","properties":{}}
}
  ]
}"#,
    )
    .expect("write manifest");

    let summary = discover_extension_runtime_registrations(&root, &[]);
    let tool = summary
        .registered_tools
        .first()
        .expect("registered tool should exist");
    let error = execute_extension_registered_tool(tool, &serde_json::json!({}))
        .expect_err("missing content should fail");
    assert!(error.to_string().contains("must include field 'content'"));
}
