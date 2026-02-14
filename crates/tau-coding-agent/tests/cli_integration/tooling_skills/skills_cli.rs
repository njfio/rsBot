//! CLI integration coverage for skills selection, lock/sync, and interactive skills commands.

use super::*;

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
