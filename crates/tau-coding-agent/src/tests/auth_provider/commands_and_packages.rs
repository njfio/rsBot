use super::*;

#[test]
fn unit_default_macro_config_path_uses_project_local_file_location() {
    let path = default_macro_config_path().expect("resolve macro path");
    assert!(path.ends_with(Path::new(".tau").join("macros.json")));
}

#[test]
fn unit_validate_macro_name_accepts_and_rejects_expected_inputs() {
    validate_macro_name("quick_check").expect("valid macro name");

    let error = validate_macro_name("").expect_err("empty macro name should fail");
    assert!(error.to_string().contains("must not be empty"));

    let error =
        validate_macro_name("9check").expect_err("macro name starting with digit should fail");
    assert!(error
        .to_string()
        .contains("must start with an ASCII letter"));

    let error = validate_macro_name("check.list")
        .expect_err("macro name with unsupported punctuation should fail");
    assert!(error
        .to_string()
        .contains("must contain only ASCII letters, digits, '-' or '_'"));
}

#[test]
fn functional_parse_macro_command_supports_lifecycle_and_usage_rules() {
    assert_eq!(
        parse_macro_command("list").expect("parse list"),
        MacroCommand::List
    );
    assert_eq!(
        parse_macro_command("save quick /tmp/quick.commands").expect("parse save"),
        MacroCommand::Save {
            name: "quick".to_string(),
            commands_file: PathBuf::from("/tmp/quick.commands"),
        }
    );
    assert_eq!(
        parse_macro_command("run quick").expect("parse run"),
        MacroCommand::Run {
            name: "quick".to_string(),
            dry_run: false,
        }
    );
    assert_eq!(
        parse_macro_command("run quick --dry-run").expect("parse dry run"),
        MacroCommand::Run {
            name: "quick".to_string(),
            dry_run: true,
        }
    );
    assert_eq!(
        parse_macro_command("show quick").expect("parse show"),
        MacroCommand::Show {
            name: "quick".to_string(),
        }
    );
    assert_eq!(
        parse_macro_command("delete quick").expect("parse delete"),
        MacroCommand::Delete {
            name: "quick".to_string(),
        }
    );

    let error = parse_macro_command("").expect_err("missing args should fail");
    assert!(error.to_string().contains(MACRO_USAGE));

    let error = parse_macro_command("run quick --apply").expect_err("unknown run flag should fail");
    assert!(error
        .to_string()
        .contains("usage: /macro run <name> [--dry-run]"));

    let error =
        parse_macro_command("list extra").expect_err("list with extra arguments should fail");
    assert!(error.to_string().contains("usage: /macro list"));

    let error = parse_macro_command("show").expect_err("show without name should fail");
    assert!(error.to_string().contains("usage: /macro show <name>"));
}

#[test]
fn unit_validate_macro_command_entry_rejects_nested_unknown_and_exit_commands() {
    validate_macro_command_entry("/session").expect("known command should validate");

    let error =
        validate_macro_command_entry("session").expect_err("command without slash should fail");
    assert!(error.to_string().contains("must start with '/'"));

    let error =
        validate_macro_command_entry("/does-not-exist").expect_err("unknown command should fail");
    assert!(error
        .to_string()
        .contains("unknown command '/does-not-exist'"));

    let error =
        validate_macro_command_entry("/macro list").expect_err("nested macro command should fail");
    assert!(error
        .to_string()
        .contains("nested /macro commands are not allowed"));

    let error = validate_macro_command_entry("/quit").expect_err("exit commands should fail");
    assert!(error.to_string().contains("exit commands are not allowed"));
}

#[test]
fn unit_save_and_load_macro_file_round_trip_schema_and_values() {
    let temp = tempdir().expect("tempdir");
    let macro_path = temp.path().join(".tau").join("macros.json");
    let macros = BTreeMap::from([
        (
            "quick".to_string(),
            vec!["/session".to_string(), "/session-stats".to_string()],
        ),
        ("review".to_string(), vec!["/help session".to_string()]),
    ]);

    save_macro_file(&macro_path, &macros).expect("save macro file");

    let loaded = load_macro_file(&macro_path).expect("load macro file");
    assert_eq!(loaded, macros);

    let raw = std::fs::read_to_string(&macro_path).expect("read macro file");
    let parsed = serde_json::from_str::<MacroFile>(&raw).expect("parse macro file");
    assert_eq!(parsed.schema_version, MACRO_SCHEMA_VERSION);
    assert_eq!(parsed.macros, macros);
}

#[test]
fn functional_render_macro_helpers_support_empty_and_deterministic_order() {
    let path = Path::new("/tmp/macros.json");
    let empty = render_macro_list(path, &BTreeMap::new());
    assert!(empty.contains("count=0"));
    assert!(empty.contains("macros: none"));

    let macros = BTreeMap::from([
        ("beta".to_string(), vec!["/session".to_string()]),
        (
            "alpha".to_string(),
            vec!["/session".to_string(), "/session-stats".to_string()],
        ),
    ]);
    let output = render_macro_list(path, &macros);
    let alpha_index = output.find("macro: name=alpha").expect("alpha row");
    let beta_index = output.find("macro: name=beta").expect("beta row");
    assert!(alpha_index < beta_index);

    let show = render_macro_show(path, "alpha", macros.get("alpha").expect("alpha commands"));
    assert!(show.contains("macro show: path=/tmp/macros.json name=alpha commands=2"));
    assert!(show.contains("command: index=0 value=/session"));
    assert!(show.contains("command: index=1 value=/session-stats"));
}

#[test]
fn integration_execute_macro_command_save_show_run_delete_lifecycle() {
    let temp = tempdir().expect("tempdir");
    let macro_path = temp.path().join(".tau").join("macros.json");
    let commands_file = temp.path().join("rewind.commands");
    std::fs::write(&commands_file, "/branch 1\n/session\n").expect("write commands file");

    let session_path = temp.path().join("session.jsonl");
    let mut store = SessionStore::load(&session_path).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[Message::assistant_text("leaf")])
        .expect("append leaf")
        .expect("head id");
    let mut session_runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = session_runtime
        .as_ref()
        .expect("runtime")
        .store
        .lineage_messages(Some(head))
        .expect("lineage");
    agent.replace_messages(lineage);

    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let model_catalog = ModelCatalog::built_in();
    let skills_dir = temp.path().join("skills");
    let lock_path = default_skills_lock_path(&skills_dir);
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
    let command_context = CommandExecutionContext {
        tool_policy_json: &tool_policy_json,
        session_import_mode: SessionImportMode::Merge,
        profile_defaults: &profile_defaults,
        skills_command_config: &skills_command_config,
        auth_command_config: &auth_command_config,
        model_catalog: &model_catalog,
        extension_commands: &[],
    };

    let save_output = execute_macro_command(
        &format!("save rewind {}", commands_file.display()),
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(save_output.contains("macro save: path="));
    assert!(save_output.contains("name=rewind"));
    assert!(save_output.contains("commands=2"));

    let dry_run_output = execute_macro_command(
        "run rewind --dry-run",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(dry_run_output.contains("mode=dry-run"));
    assert!(dry_run_output.contains("plan: command=/branch 1"));
    assert_eq!(
        session_runtime
            .as_ref()
            .and_then(|runtime| runtime.active_head),
        Some(head)
    );

    let show_output = execute_macro_command(
        "show rewind",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(show_output.contains("macro show: path="));
    assert!(show_output.contains("name=rewind commands=2"));
    assert!(show_output.contains("command: index=0 value=/branch 1"));
    assert!(show_output.contains("command: index=1 value=/session"));

    let run_output = execute_macro_command(
        "run rewind",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(run_output.contains("macro run: path="));
    assert!(run_output.contains("mode=apply"));
    assert!(run_output.contains("executed=2"));
    assert_eq!(
        session_runtime
            .as_ref()
            .and_then(|runtime| runtime.active_head),
        Some(root)
    );

    let list_output = execute_macro_command(
        "list",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(list_output.contains("macro list: path="));
    assert!(list_output.contains("count=1"));
    assert!(list_output.contains("macro: name=rewind commands=2"));

    let delete_output = execute_macro_command(
        "delete rewind",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(delete_output.contains("macro delete: path="));
    assert!(delete_output.contains("name=rewind"));
    assert!(delete_output.contains("status=deleted"));
    assert!(delete_output.contains("remaining=0"));

    let final_list = execute_macro_command(
        "list",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(final_list.contains("count=0"));
    assert!(final_list.contains("macros: none"));
}

#[test]
fn regression_execute_macro_command_reports_missing_commands_file() {
    let temp = tempdir().expect("tempdir");
    let macro_path = temp.path().join(".tau").join("macros.json");
    let missing_commands_file = temp.path().join("missing.commands");
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let model_catalog = ModelCatalog::built_in();
    let skills_dir = temp.path().join("skills");
    let lock_path = default_skills_lock_path(&skills_dir);
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
    let command_context = CommandExecutionContext {
        tool_policy_json: &tool_policy_json,
        session_import_mode: SessionImportMode::Merge,
        profile_defaults: &profile_defaults,
        skills_command_config: &skills_command_config,
        auth_command_config: &auth_command_config,
        model_catalog: &model_catalog,
        extension_commands: &[],
    };
    let mut session_runtime = None;
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());

    let output = execute_macro_command(
        &format!("save quick {}", missing_commands_file.display()),
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(output.contains("macro error: path="));
    assert!(output.contains("failed to read commands file"));
}

#[test]
fn regression_execute_macro_command_reports_corrupt_macro_file() {
    let temp = tempdir().expect("tempdir");
    let macro_path = temp.path().join(".tau").join("macros.json");
    std::fs::create_dir_all(
        macro_path
            .parent()
            .expect("macro path should include a parent"),
    )
    .expect("create macro config dir");
    std::fs::write(&macro_path, "{invalid-json").expect("write malformed macro file");

    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let model_catalog = ModelCatalog::built_in();
    let skills_dir = temp.path().join("skills");
    let lock_path = default_skills_lock_path(&skills_dir);
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
    let command_context = CommandExecutionContext {
        tool_policy_json: &tool_policy_json,
        session_import_mode: SessionImportMode::Merge,
        profile_defaults: &profile_defaults,
        skills_command_config: &skills_command_config,
        auth_command_config: &auth_command_config,
        model_catalog: &model_catalog,
        extension_commands: &[],
    };
    let mut session_runtime = None;
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());

    let output = execute_macro_command(
        "list",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(output.contains("macro error: path="));
    assert!(output.contains("failed to parse macro file"));
}

#[test]
fn regression_execute_macro_command_rejects_unknown_macro_and_invalid_entries() {
    let temp = tempdir().expect("tempdir");
    let macro_path = temp.path().join(".tau").join("macros.json");
    let macros = BTreeMap::from([("broken".to_string(), vec!["/macro list".to_string()])]);
    save_macro_file(&macro_path, &macros).expect("save macro file");

    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let model_catalog = ModelCatalog::built_in();
    let skills_dir = temp.path().join("skills");
    let lock_path = default_skills_lock_path(&skills_dir);
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
    let command_context = CommandExecutionContext {
        tool_policy_json: &tool_policy_json,
        session_import_mode: SessionImportMode::Merge,
        profile_defaults: &profile_defaults,
        skills_command_config: &skills_command_config,
        auth_command_config: &auth_command_config,
        model_catalog: &model_catalog,
        extension_commands: &[],
    };
    let mut session_runtime = None;
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());

    let missing_output = execute_macro_command(
        "run missing",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(missing_output.contains("unknown macro 'missing'"));

    let missing_show = execute_macro_command(
        "show missing",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(missing_show.contains("unknown macro 'missing'"));

    let missing_delete = execute_macro_command(
        "delete missing",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(missing_delete.contains("unknown macro 'missing'"));

    let invalid_output = execute_macro_command(
        "run broken",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(invalid_output.contains("macro command #0 failed validation"));

    let delete_broken = execute_macro_command(
        "delete broken",
        &macro_path,
        &mut agent,
        &mut session_runtime,
        command_context,
    );
    assert!(delete_broken.contains("status=deleted"));
    assert!(delete_broken.contains("remaining=0"));
}

#[test]
fn unit_validate_profile_name_accepts_and_rejects_expected_inputs() {
    validate_profile_name("baseline_1").expect("valid profile name");

    let error = validate_profile_name("").expect_err("empty profile name should fail");
    assert!(error.to_string().contains("must not be empty"));

    let error = validate_profile_name("1baseline")
        .expect_err("profile name starting with digit should fail");
    assert!(error
        .to_string()
        .contains("must start with an ASCII letter"));

    let error = validate_profile_name("baseline.json")
        .expect_err("profile name with punctuation should fail");
    assert!(error
        .to_string()
        .contains("must contain only ASCII letters, digits, '-' or '_'"));
}

#[test]
fn functional_parse_profile_command_supports_lifecycle_subcommands_and_usage_errors() {
    assert_eq!(
        parse_profile_command("save baseline").expect("parse save"),
        ProfileCommand::Save {
            name: "baseline".to_string(),
        }
    );
    assert_eq!(
        parse_profile_command("load baseline").expect("parse load"),
        ProfileCommand::Load {
            name: "baseline".to_string(),
        }
    );
    assert_eq!(
        parse_profile_command("list").expect("parse list"),
        ProfileCommand::List
    );
    assert_eq!(
        parse_profile_command("show baseline").expect("parse show"),
        ProfileCommand::Show {
            name: "baseline".to_string(),
        }
    );
    assert_eq!(
        parse_profile_command("delete baseline").expect("parse delete"),
        ProfileCommand::Delete {
            name: "baseline".to_string(),
        }
    );

    let error = parse_profile_command("").expect_err("empty args should fail");
    assert!(error.to_string().contains(PROFILE_USAGE));

    let error = parse_profile_command("save").expect_err("missing name should fail");
    assert!(error.to_string().contains("usage: /profile save <name>"));

    let error =
        parse_profile_command("list extra").expect_err("list with trailing arguments should fail");
    assert!(error.to_string().contains("usage: /profile list"));

    let error = parse_profile_command("show").expect_err("show missing name should fail");
    assert!(error.to_string().contains("usage: /profile show <name>"));

    let error =
        parse_profile_command("unknown baseline").expect_err("unknown subcommand should fail");
    assert!(error.to_string().contains("unknown subcommand 'unknown'"));
}

#[test]
fn unit_save_and_load_profile_store_round_trip_schema_and_values() {
    let temp = tempdir().expect("tempdir");
    let profile_path = temp.path().join(".tau").join("profiles.json");
    let mut alternate = test_profile_defaults();
    alternate.model = "google/gemini-2.5-pro".to_string();
    let profiles = BTreeMap::from([
        ("baseline".to_string(), test_profile_defaults()),
        ("alt".to_string(), alternate.clone()),
    ]);

    save_profile_store(&profile_path, &profiles).expect("save profiles");
    let loaded = load_profile_store(&profile_path).expect("load profiles");
    assert_eq!(loaded, profiles);

    let raw = std::fs::read_to_string(&profile_path).expect("read profile file");
    let parsed = serde_json::from_str::<ProfileStoreFile>(&raw).expect("parse profile file");
    assert_eq!(parsed.schema_version, PROFILE_SCHEMA_VERSION);
    assert_eq!(parsed.profiles, profiles);
}

#[test]
fn regression_load_profile_store_backfills_auth_defaults_for_legacy_profiles() {
    let temp = tempdir().expect("tempdir");
    let profile_path = temp.path().join(".tau").join("profiles.json");
    std::fs::create_dir_all(
        profile_path
            .parent()
            .expect("profile path should have parent"),
    )
    .expect("mkdir profile dir");
    std::fs::write(
        &profile_path,
        serde_json::json!({
            "schema_version": PROFILE_SCHEMA_VERSION,
            "profiles": {
                "legacy": {
                    "model": "openai/gpt-4o-mini",
                    "fallback_models": [],
                    "session": {
                        "enabled": true,
                        "path": ".tau/sessions/default.jsonl",
                        "import_mode": "merge"
                    },
                    "policy": {
                        "tool_policy_preset": "balanced",
                        "bash_profile": "balanced",
                        "bash_dry_run": false,
                        "os_sandbox_mode": "off",
                        "enforce_regular_files": true,
                        "bash_timeout_ms": 500,
                        "max_command_length": 4096,
                        "max_tool_output_bytes": 1024,
                        "max_file_read_bytes": 2048,
                        "max_file_write_bytes": 2048,
                        "allow_command_newlines": true
                    }
                }
            }
        })
        .to_string(),
    )
    .expect("write legacy profile store");

    let loaded = load_profile_store(&profile_path).expect("load legacy profiles");
    let legacy = loaded.get("legacy").expect("legacy profile");
    assert_eq!(legacy.auth.openai, ProviderAuthMethod::ApiKey);
    assert_eq!(legacy.auth.anthropic, ProviderAuthMethod::ApiKey);
    assert_eq!(legacy.auth.google, ProviderAuthMethod::ApiKey);
}

#[test]
fn functional_render_profile_diffs_reports_changed_fields() {
    let current = test_profile_defaults();
    let mut loaded = current.clone();
    loaded.model = "google/gemini-2.5-pro".to_string();
    loaded.policy.max_command_length = 2048;
    loaded.session.import_mode = "replace".to_string();

    let diffs = render_profile_diffs(&current, &loaded);
    assert_eq!(diffs.len(), 3);
    assert!(diffs
        .iter()
        .any(|line| line
            .contains("field=model current=openai/gpt-4o-mini loaded=google/gemini-2.5-pro")));
    assert!(diffs
        .iter()
        .any(|line| line.contains("field=session.import_mode current=merge loaded=replace")));
    assert!(diffs
        .iter()
        .any(|line| line.contains("field=policy.max_command_length current=4096 loaded=2048")));
}

#[test]
fn functional_render_profile_diffs_reports_changed_auth_modes() {
    let current = test_profile_defaults();
    let mut loaded = current.clone();
    loaded.auth.openai = ProviderAuthMethod::OauthToken;
    loaded.auth.google = ProviderAuthMethod::Adc;

    let diffs = render_profile_diffs(&current, &loaded);
    assert!(diffs
        .iter()
        .any(|line| line.contains("field=auth.openai current=api_key loaded=oauth_token")));
    assert!(diffs
        .iter()
        .any(|line| line.contains("field=auth.google current=api_key loaded=adc")));
}

#[test]
fn unit_render_profile_list_and_show_produce_deterministic_output() {
    let profile_path = PathBuf::from("/tmp/profiles.json");
    let mut alternate = test_profile_defaults();
    alternate.model = "google/gemini-2.5-pro".to_string();
    let profiles = BTreeMap::from([
        ("zeta".to_string(), test_profile_defaults()),
        ("alpha".to_string(), alternate.clone()),
    ]);

    let list_output = render_profile_list(&profile_path, &profiles);
    assert!(list_output.contains("profile list: path=/tmp/profiles.json profiles=2"));
    let alpha_index = list_output.find("profile: name=alpha").expect("alpha row");
    let zeta_index = list_output.find("profile: name=zeta").expect("zeta row");
    assert!(alpha_index < zeta_index);

    let show_output = render_profile_show(&profile_path, "alpha", &alternate);
    assert!(show_output.contains("profile show: path=/tmp/profiles.json name=alpha status=found"));
    assert!(show_output.contains("value: model=google/gemini-2.5-pro"));
    assert!(show_output.contains("value: fallback_models=none"));
    assert!(show_output.contains("value: session.path=.tau/sessions/default.jsonl"));
    assert!(show_output.contains("value: policy.max_command_length=4096"));
    assert!(show_output.contains("value: auth.openai=api_key"));
}

#[test]
fn integration_execute_profile_command_full_lifecycle_roundtrip() {
    let temp = tempdir().expect("tempdir");
    let profile_path = temp.path().join(".tau").join("profiles.json");
    let current = test_profile_defaults();

    let save_output = execute_profile_command("save baseline", &profile_path, &current);
    assert!(save_output.contains("profile save: path="));
    assert!(save_output.contains("name=baseline"));
    assert!(save_output.contains("status=saved"));

    let load_output = execute_profile_command("load baseline", &profile_path, &current);
    assert!(load_output.contains("profile load: path="));
    assert!(load_output.contains("name=baseline"));
    assert!(load_output.contains("status=in_sync"));
    assert!(load_output.contains("diffs=0"));

    let list_output = execute_profile_command("list", &profile_path, &current);
    assert!(list_output.contains("profile list: path="));
    assert!(list_output.contains("profiles=1"));
    assert!(list_output.contains("profile: name=baseline"));

    let show_output = execute_profile_command("show baseline", &profile_path, &current);
    assert!(show_output.contains("profile show: path="));
    assert!(show_output.contains("name=baseline status=found"));
    assert!(show_output.contains("value: model=openai/gpt-4o-mini"));

    let mut changed = current.clone();
    changed.model = "anthropic/claude-sonnet-4-20250514".to_string();
    let diff_output = execute_profile_command("load baseline", &profile_path, &changed);
    assert!(diff_output.contains("status=diff"));
    assert!(diff_output.contains("diff: field=model"));

    let delete_output = execute_profile_command("delete baseline", &profile_path, &current);
    assert!(delete_output.contains("profile delete: path="));
    assert!(delete_output.contains("name=baseline"));
    assert!(delete_output.contains("status=deleted"));
    assert!(delete_output.contains("remaining=0"));

    let list_after_delete = execute_profile_command("list", &profile_path, &current);
    assert!(list_after_delete.contains("profiles=0"));
    assert!(list_after_delete.contains("names=none"));
}

#[test]
fn regression_execute_profile_command_reports_unknown_profile_and_schema_errors() {
    let temp = tempdir().expect("tempdir");
    let profile_path = temp.path().join(".tau").join("profiles.json");
    let current = test_profile_defaults();

    let missing_output = execute_profile_command("load missing", &profile_path, &current);
    assert!(missing_output.contains("profile error: path="));
    assert!(missing_output.contains("unknown profile 'missing'"));

    let missing_show = execute_profile_command("show missing", &profile_path, &current);
    assert!(missing_show.contains("profile error: path="));
    assert!(missing_show.contains("unknown profile 'missing'"));

    let missing_delete = execute_profile_command("delete missing", &profile_path, &current);
    assert!(missing_delete.contains("profile error: path="));
    assert!(missing_delete.contains("unknown profile 'missing'"));

    std::fs::create_dir_all(
        profile_path
            .parent()
            .expect("profile path should include parent dir"),
    )
    .expect("create profile dir");
    let invalid = serde_json::json!({
        "schema_version": 999,
        "profiles": {
            "baseline": current
        }
    });
    std::fs::write(&profile_path, format!("{invalid}\n")).expect("write invalid schema");

    let schema_output = execute_profile_command("load baseline", &profile_path, &current);
    assert!(schema_output.contains("profile error: path="));
    assert!(schema_output.contains("unsupported profile schema_version 999"));
}

#[test]
fn regression_default_profile_store_path_uses_project_local_profiles_file() {
    let path = default_profile_store_path().expect("resolve profile store path");
    assert!(path.ends_with(Path::new(".tau").join("profiles.json")));
}

#[test]
fn unit_command_file_error_mode_label_matches_cli_values() {
    assert_eq!(
        command_file_error_mode_label(CliCommandFileErrorMode::FailFast),
        "fail-fast"
    );
    assert_eq!(
        command_file_error_mode_label(CliCommandFileErrorMode::ContinueOnError),
        "continue-on-error"
    );
}

#[test]
fn unit_parse_command_file_skips_comments_blanks_and_keeps_line_numbers() {
    let temp = tempdir().expect("tempdir");
    let command_file = temp.path().join("commands.txt");
    std::fs::write(
        &command_file,
        "# comment\n\n  /session  \nnot-command\n   # another comment\n/help session\n",
    )
    .expect("write command file");

    let entries = parse_command_file(&command_file).expect("parse command file");
    assert_eq!(entries.len(), 3);
    assert_eq!(
        entries[0],
        CommandFileEntry {
            line_number: 3,
            command: "/session".to_string(),
        }
    );
    assert_eq!(
        entries[1],
        CommandFileEntry {
            line_number: 4,
            command: "not-command".to_string(),
        }
    );
    assert_eq!(
        entries[2],
        CommandFileEntry {
            line_number: 6,
            command: "/help session".to_string(),
        }
    );
}

#[test]
fn functional_execute_command_file_runs_script_and_returns_summary() {
    let temp = tempdir().expect("tempdir");
    let command_file = temp.path().join("commands.txt");
    std::fs::write(&command_file, "/session\n/help session\n").expect("write command file");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut session_runtime = None;
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let model_catalog = ModelCatalog::built_in();
    let skills_dir = temp.path().join("skills");
    let lock_path = default_skills_lock_path(&skills_dir);
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
    let command_context = test_command_context(
        &tool_policy_json,
        &profile_defaults,
        &skills_command_config,
        &auth_command_config,
        &model_catalog,
    );

    let report = execute_command_file(
        &command_file,
        CliCommandFileErrorMode::FailFast,
        &mut agent,
        &mut session_runtime,
        command_context,
    )
    .expect("execute command file");

    assert_eq!(
        report,
        CommandFileReport {
            total: 2,
            executed: 2,
            succeeded: 2,
            failed: 0,
            halted_early: false,
        }
    );
}

#[test]
fn integration_execute_command_file_continue_on_error_runs_remaining_commands() {
    let temp = tempdir().expect("tempdir");
    let command_file = temp.path().join("commands.txt");
    std::fs::write(&command_file, "/session\nnot-command\n/help session\n")
        .expect("write command file");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut session_runtime = None;
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let model_catalog = ModelCatalog::built_in();
    let skills_dir = temp.path().join("skills");
    let lock_path = default_skills_lock_path(&skills_dir);
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
    let command_context = test_command_context(
        &tool_policy_json,
        &profile_defaults,
        &skills_command_config,
        &auth_command_config,
        &model_catalog,
    );

    let report = execute_command_file(
        &command_file,
        CliCommandFileErrorMode::ContinueOnError,
        &mut agent,
        &mut session_runtime,
        command_context,
    )
    .expect("execute command file");

    assert_eq!(
        report,
        CommandFileReport {
            total: 3,
            executed: 3,
            succeeded: 2,
            failed: 1,
            halted_early: false,
        }
    );
}

#[test]
fn regression_execute_command_file_fail_fast_stops_on_malformed_line() {
    let temp = tempdir().expect("tempdir");
    let command_file = temp.path().join("commands.txt");
    std::fs::write(&command_file, "/session\nnot-command\n/help session\n")
        .expect("write command file");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut session_runtime = None;
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let model_catalog = ModelCatalog::built_in();
    let skills_dir = temp.path().join("skills");
    let lock_path = default_skills_lock_path(&skills_dir);
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
    let command_context = test_command_context(
        &tool_policy_json,
        &profile_defaults,
        &skills_command_config,
        &auth_command_config,
        &model_catalog,
    );

    let error = execute_command_file(
        &command_file,
        CliCommandFileErrorMode::FailFast,
        &mut agent,
        &mut session_runtime,
        command_context,
    )
    .expect_err("fail-fast should stop on malformed command line");
    assert!(error.to_string().contains("command file execution failed"));
}

#[test]
fn regression_parse_command_file_reports_missing_file() {
    let temp = tempdir().expect("tempdir");
    let missing = temp.path().join("missing-commands.txt");
    let error = parse_command_file(&missing).expect_err("missing command file should fail");
    assert!(error.to_string().contains("failed to read command file"));
}

#[test]
fn unit_validate_branch_alias_name_accepts_and_rejects_expected_inputs() {
    validate_branch_alias_name("hotfix_1").expect("valid alias");

    let error = validate_branch_alias_name("").expect_err("empty alias should fail");
    assert!(error.to_string().contains("must not be empty"));

    let error =
        validate_branch_alias_name("1hotfix").expect_err("alias starting with a digit should fail");
    assert!(error
        .to_string()
        .contains("must start with an ASCII letter"));

    let error = validate_branch_alias_name("hotfix.bad")
        .expect_err("alias with unsupported punctuation should fail");
    assert!(error
        .to_string()
        .contains("must contain only ASCII letters, digits, '-' or '_'"));
}

#[test]
fn functional_parse_branch_alias_command_supports_core_subcommands() {
    assert_eq!(
        parse_branch_alias_command("list").expect("parse list"),
        BranchAliasCommand::List
    );
    assert_eq!(
        parse_branch_alias_command("set hotfix 42").expect("parse set"),
        BranchAliasCommand::Set {
            name: "hotfix".to_string(),
            id: 42,
        }
    );
    assert_eq!(
        parse_branch_alias_command("use hotfix").expect("parse use"),
        BranchAliasCommand::Use {
            name: "hotfix".to_string(),
        }
    );

    let error = parse_branch_alias_command("").expect_err("missing args should fail");
    assert!(error.to_string().contains(BRANCH_ALIAS_USAGE));

    let error = parse_branch_alias_command("set hotfix nope").expect_err("invalid id should fail");
    assert!(error.to_string().contains("invalid branch id 'nope'"));

    let error =
        parse_branch_alias_command("delete hotfix").expect_err("unknown subcommand should fail");
    assert!(error.to_string().contains("unknown subcommand 'delete'"));
}

#[test]
fn unit_save_and_load_branch_aliases_round_trip_schema_and_values() {
    let temp = tempdir().expect("tempdir");
    let alias_path = temp.path().join("session.aliases.json");
    let aliases = BTreeMap::from([
        ("hotfix".to_string(), 7_u64),
        ("rollback".to_string(), 12_u64),
    ]);

    save_branch_aliases(&alias_path, &aliases).expect("save aliases");

    let loaded = load_branch_aliases(&alias_path).expect("load aliases");
    assert_eq!(loaded, aliases);

    let raw = std::fs::read_to_string(&alias_path).expect("read alias file");
    let parsed = serde_json::from_str::<BranchAliasFile>(&raw).expect("parse alias file");
    assert_eq!(parsed.schema_version, BRANCH_ALIAS_SCHEMA_VERSION);
    assert_eq!(parsed.aliases, aliases);
}

#[test]
fn integration_execute_branch_alias_command_supports_set_use_and_list_flow() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");
    let mut store = SessionStore::load(&session_path).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let stable = store
        .append_messages(Some(root), &[Message::assistant_text("stable branch")])
        .expect("append stable")
        .expect("stable id");
    let hot = store
        .append_messages(Some(root), &[Message::assistant_text("hot branch")])
        .expect("append hot")
        .expect("hot id");
    let mut runtime = SessionRuntime {
        store,
        active_head: Some(hot),
    };
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = runtime
        .store
        .lineage_messages(runtime.active_head)
        .expect("lineage");
    agent.replace_messages(lineage);

    let set_outcome = execute_branch_alias_command(&format!("set hotfix {stable}"), &mut runtime);
    assert!(set_outcome.message.contains("branch alias set: path="));
    assert!(set_outcome.message.contains("name=hotfix"));
    assert_eq!(runtime.active_head, Some(hot));

    let list_outcome = execute_branch_alias_command("list", &mut runtime);
    assert!(list_outcome.message.contains("branch alias list: path="));
    assert!(list_outcome.message.contains("count=1"));
    assert!(list_outcome
        .message
        .contains(&format!("alias: name=hotfix id={} status=ok", stable)));

    let use_outcome = execute_branch_alias_command("use hotfix", &mut runtime);
    if use_outcome.reload_active_head {
        agent.replace_messages(session_lineage_messages(&runtime).expect("lineage"));
    }
    assert!(use_outcome.message.contains("branch alias use: path="));
    assert!(use_outcome.message.contains(&format!("id={stable}")));
    assert_eq!(runtime.active_head, Some(stable));

    let alias_path = branch_alias_path_for_session(&session_path);
    let aliases = load_branch_aliases(&alias_path).expect("load aliases");
    assert_eq!(aliases.get("hotfix"), Some(&stable));
}

#[test]
fn regression_execute_branch_alias_command_reports_stale_alias_ids() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");
    let mut store = SessionStore::load(&session_path).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let mut runtime = SessionRuntime {
        store,
        active_head: Some(root),
    };
    let alias_path = branch_alias_path_for_session(&session_path);
    let aliases = BTreeMap::from([("legacy".to_string(), 999_u64)]);
    save_branch_aliases(&alias_path, &aliases).expect("save stale alias");

    let list_outcome = execute_branch_alias_command("list", &mut runtime);
    assert!(list_outcome.message.contains("count=1"));
    assert!(list_outcome
        .message
        .contains("alias: name=legacy id=999 status=stale"));

    let use_outcome = execute_branch_alias_command("use legacy", &mut runtime);
    assert!(use_outcome.message.contains("branch alias error: path="));
    assert!(use_outcome
        .message
        .contains("alias points to unknown session id 999"));
}

#[test]
fn regression_execute_branch_alias_command_reports_corrupt_alias_file() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");
    let mut store = SessionStore::load(&session_path).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let mut runtime = SessionRuntime {
        store,
        active_head: Some(root),
    };
    let alias_path = branch_alias_path_for_session(&session_path);
    std::fs::write(&alias_path, "{invalid-json").expect("write malformed alias file");

    let outcome = execute_branch_alias_command("list", &mut runtime);
    assert!(outcome.message.contains("branch alias error: path="));
    assert!(outcome.message.contains("failed to parse alias file"));
}

#[test]
fn functional_parse_session_bookmark_command_supports_lifecycle_subcommands() {
    assert_eq!(
        parse_session_bookmark_command("list").expect("parse list"),
        SessionBookmarkCommand::List
    );
    assert_eq!(
        parse_session_bookmark_command("set checkpoint 42").expect("parse set"),
        SessionBookmarkCommand::Set {
            name: "checkpoint".to_string(),
            id: 42,
        }
    );
    assert_eq!(
        parse_session_bookmark_command("use checkpoint").expect("parse use"),
        SessionBookmarkCommand::Use {
            name: "checkpoint".to_string(),
        }
    );
    assert_eq!(
        parse_session_bookmark_command("delete checkpoint").expect("parse delete"),
        SessionBookmarkCommand::Delete {
            name: "checkpoint".to_string(),
        }
    );

    let error = parse_session_bookmark_command("").expect_err("empty args should fail");
    assert!(error.to_string().contains(SESSION_BOOKMARK_USAGE));

    let error =
        parse_session_bookmark_command("set checkpoint nope").expect_err("invalid id should fail");
    assert!(error.to_string().contains("invalid bookmark id 'nope'"));

    let error =
        parse_session_bookmark_command("unknown checkpoint").expect_err("unknown subcommand");
    assert!(error.to_string().contains("unknown subcommand 'unknown'"));
}

#[test]
fn unit_save_and_load_session_bookmarks_round_trip_schema_and_values() {
    let temp = tempdir().expect("tempdir");
    let bookmark_path = temp.path().join("session.bookmarks.json");
    let bookmarks = BTreeMap::from([
        ("checkpoint".to_string(), 7_u64),
        ("investigation".to_string(), 42_u64),
    ]);

    save_session_bookmarks(&bookmark_path, &bookmarks).expect("save bookmarks");
    let loaded = load_session_bookmarks(&bookmark_path).expect("load bookmarks");
    assert_eq!(loaded, bookmarks);

    let raw = std::fs::read_to_string(&bookmark_path).expect("read bookmark file");
    let parsed = serde_json::from_str::<SessionBookmarkFile>(&raw).expect("parse bookmark file");
    assert_eq!(parsed.schema_version, SESSION_BOOKMARK_SCHEMA_VERSION);
    assert_eq!(parsed.bookmarks, bookmarks);
}

#[test]
fn integration_execute_session_bookmark_command_supports_set_use_list_delete_flow() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");
    let mut store = SessionStore::load(&session_path).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let stable = store
        .append_messages(Some(root), &[Message::user("stable branch")])
        .expect("append stable branch")
        .expect("stable id");

    let mut runtime = SessionRuntime {
        store,
        active_head: Some(root),
    };
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let initial_lineage = runtime
        .store
        .lineage_messages(runtime.active_head)
        .expect("initial lineage");
    agent.replace_messages(initial_lineage);

    let set_outcome =
        execute_session_bookmark_command(&format!("set checkpoint {stable}"), &mut runtime);
    assert!(set_outcome.message.contains("session bookmark set: path="));
    assert!(set_outcome.message.contains("name=checkpoint"));
    assert!(set_outcome.message.contains(&format!("id={stable}")));

    let list_outcome = execute_session_bookmark_command("list", &mut runtime);
    assert!(list_outcome
        .message
        .contains("session bookmark list: path="));
    assert!(list_outcome.message.contains("count=1"));
    assert!(list_outcome
        .message
        .contains(&format!("bookmark: name=checkpoint id={stable} status=ok")));

    let use_outcome = execute_session_bookmark_command("use checkpoint", &mut runtime);
    if use_outcome.reload_active_head {
        agent.replace_messages(session_lineage_messages(&runtime).expect("lineage"));
    }
    assert!(use_outcome.message.contains("session bookmark use: path="));
    assert!(use_outcome.message.contains(&format!("id={stable}")));
    assert_eq!(runtime.active_head, Some(stable));

    let delete_outcome = execute_session_bookmark_command("delete checkpoint", &mut runtime);
    assert!(delete_outcome
        .message
        .contains("session bookmark delete: path="));
    assert!(delete_outcome.message.contains("status=deleted"));
    assert!(delete_outcome.message.contains("remaining=0"));

    let final_list = execute_session_bookmark_command("list", &mut runtime);
    assert!(final_list.message.contains("count=0"));
    assert!(final_list.message.contains("bookmarks: none"));
}

#[test]
fn regression_execute_session_bookmark_command_reports_stale_ids() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");
    let mut store = SessionStore::load(&session_path).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let mut runtime = SessionRuntime {
        store,
        active_head: Some(root),
    };
    let bookmark_path = session_bookmark_path_for_session(&session_path);
    let bookmarks = BTreeMap::from([("legacy".to_string(), 999_u64)]);
    save_session_bookmarks(&bookmark_path, &bookmarks).expect("save stale bookmark");

    let list_outcome = execute_session_bookmark_command("list", &mut runtime);
    assert!(list_outcome.message.contains("count=1"));
    assert!(list_outcome
        .message
        .contains("bookmark: name=legacy id=999 status=stale"));

    let use_outcome = execute_session_bookmark_command("use legacy", &mut runtime);
    assert!(use_outcome
        .message
        .contains("session bookmark error: path="));
    assert!(use_outcome
        .message
        .contains("bookmark points to unknown session id 999"));
}

#[test]
fn regression_execute_session_bookmark_command_reports_corrupt_bookmark_file() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");
    let mut store = SessionStore::load(&session_path).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let mut runtime = SessionRuntime {
        store,
        active_head: Some(root),
    };
    let bookmark_path = session_bookmark_path_for_session(&session_path);
    std::fs::write(&bookmark_path, "{invalid-json").expect("write malformed bookmark file");

    let outcome = execute_session_bookmark_command("list", &mut runtime);
    assert!(outcome.message.contains("session bookmark error: path="));
    assert!(outcome
        .message
        .contains("failed to parse session bookmark file"));
}

#[test]
fn functional_render_help_overview_lists_known_commands() {
    let help = render_help_overview();
    assert!(help.contains("/help [command]"));
    assert!(help.contains("/session"));
    assert!(help.contains("/session-search <query> [--role <role>] [--limit <n>]"));
    assert!(help.contains("/session-stats"));
    assert!(help.contains("/session-diff [<left-id> <right-id>]"));
    assert!(help.contains("/doctor"));
    assert!(help.contains(
        "/release-channel [show|set <stable|beta|dev>|check|plan [--target <version>] [--dry-run]|apply [--target <version>] [--dry-run]|cache <show|clear|refresh|prune>]"
    ));
    assert!(help.contains("/session-graph-export <path>"));
    assert!(help.contains("/session-export <path>"));
    assert!(help.contains("/session-import <path>"));
    assert!(help.contains(
        "/session-merge <source-id> [target-id] [--strategy <append|squash|fast-forward>]"
    ));
    assert!(help.contains("/audit-summary <path>"));
    assert!(help.contains(MODELS_LIST_USAGE));
    assert!(help.contains(MODEL_SHOW_USAGE));
    assert!(help.contains("/skills-search <query> [max_results]"));
    assert!(help.contains("/skills-show <name>"));
    assert!(help.contains("/skills-list"));
    assert!(help.contains("/skills-lock-diff [lockfile_path] [--json]"));
    assert!(help.contains("/skills-prune [lockfile_path] [--dry-run|--apply]"));
    assert!(help.contains("/skills-trust-list [trust_root_file]"));
    assert!(help.contains("/skills-trust-add <id=base64_key> [trust_root_file]"));
    assert!(help.contains("/skills-trust-revoke <id> [trust_root_file]"));
    assert!(help.contains("/skills-trust-rotate <old_id:new_id=base64_key> [trust_root_file]"));
    assert!(help.contains("/skills-verify [lockfile_path] [trust_root_file] [--json]"));
    assert!(help.contains("/skills-lock-write [lockfile_path]"));
    assert!(help.contains("/skills-sync [lockfile_path]"));
    assert!(help.contains("/macro <save|run|list|show|delete> ..."));
    assert!(help.contains("/auth <login|reauth|status|logout|matrix> ..."));
    assert!(help.contains("/canvas <create|update|show|export|import>"));
    assert!(help.contains("/rbac <check|whoami> ..."));
    assert!(help.contains("/approvals <list|approve|reject> [--json] [--status <pending|approved|rejected|expired|consumed>] [request_id] [reason]"));
    assert!(help.contains("/integration-auth <set|status|rotate|revoke> ..."));
    assert!(help.contains("/profile <save|load|list|show|delete> ..."));
    assert!(help.contains("/branch <id>"));
    assert!(help.contains("/branch-alias <set|list|use> ..."));
    assert!(help.contains("/session-bookmark <set|list|use|delete> ..."));
    assert!(help.contains("/quit"));
}

#[test]
fn functional_render_command_help_supports_branch_topic_without_slash() {
    let help = render_command_help("branch").expect("render help");
    assert!(help.contains("command: /branch"));
    assert!(help.contains("usage: /branch <id>"));
    assert!(help.contains("example: /branch 12"));
}

#[test]
fn functional_render_command_help_supports_branch_alias_topic_without_slash() {
    let help = render_command_help("branch-alias").expect("render help");
    assert!(help.contains("command: /branch-alias"));
    assert!(help.contains("usage: /branch-alias <set|list|use> ..."));
    assert!(help.contains("example: /branch-alias set hotfix 42"));
}

#[test]
fn functional_render_command_help_supports_session_bookmark_topic_without_slash() {
    let help = render_command_help("session-bookmark").expect("render help");
    assert!(help.contains("command: /session-bookmark"));
    assert!(help.contains("usage: /session-bookmark <set|list|use|delete> ..."));
    assert!(help.contains("example: /session-bookmark set investigation 42"));
}

#[test]
fn functional_render_command_help_supports_macro_topic_without_slash() {
    let help = render_command_help("macro").expect("render help");
    assert!(help.contains("command: /macro"));
    assert!(help.contains("usage: /macro <save|run|list|show|delete> ..."));
    assert!(help.contains("example: /macro save quick-check /tmp/quick-check.commands"));
}

#[test]
fn functional_render_command_help_supports_integration_auth_topic_without_slash() {
    let help = render_command_help("integration-auth").expect("render help");
    assert!(help.contains("command: /integration-auth"));
    assert!(help.contains("usage: /integration-auth <set|status|rotate|revoke> ..."));
    assert!(help.contains("example: /integration-auth status github-token --json"));
}

#[test]
fn functional_render_command_help_supports_profile_topic_without_slash() {
    let help = render_command_help("profile").expect("render help");
    assert!(help.contains("command: /profile"));
    assert!(help.contains("usage: /profile <save|load|list|show|delete> ..."));
    assert!(help.contains("example: /profile save baseline"));
}

#[test]
fn functional_render_command_help_supports_canvas_topic_without_slash() {
    let help = render_command_help("canvas").expect("render help");
    assert!(help.contains("command: /canvas"));
    assert!(help.contains("usage: /canvas <create|update|show|export|import>"));
    assert!(help.contains("example: /canvas update architecture node-upsert"));
}

#[test]
fn functional_render_command_help_supports_rbac_topic_without_slash() {
    let help = render_command_help("rbac").expect("render help");
    assert!(help.contains("command: /rbac"));
    assert!(help.contains("usage: /rbac <check|whoami> ..."));
    assert!(help.contains("example: /rbac check command:/policy --json"));
}

#[test]
fn functional_render_command_help_supports_approvals_topic_without_slash() {
    let help = render_command_help("approvals").expect("render help");
    assert!(help.contains("command: /approvals"));
    assert!(help.contains("usage: /approvals <list|approve|reject>"));
    assert!(help.contains("example: /approvals list --status pending"));
}

#[test]
fn functional_render_command_help_supports_session_search_topic_without_slash() {
    let help = render_command_help("session-search").expect("render help");
    assert!(help.contains("command: /session-search"));
    assert!(help.contains("usage: /session-search <query> [--role <role>] [--limit <n>]"));
}

#[test]
fn functional_render_command_help_supports_session_stats_topic_without_slash() {
    let help = render_command_help("session-stats").expect("render help");
    assert!(help.contains("command: /session-stats"));
    assert!(help.contains("usage: /session-stats [--json]"));
}

#[test]
fn functional_render_command_help_supports_session_diff_topic_without_slash() {
    let help = render_command_help("session-diff").expect("render help");
    assert!(help.contains("command: /session-diff"));
    assert!(help.contains("usage: /session-diff [<left-id> <right-id>]"));
}

#[test]
fn functional_render_command_help_supports_doctor_topic_without_slash() {
    let help = render_command_help("doctor").expect("render help");
    assert!(help.contains("command: /doctor"));
    assert!(help.contains("usage: /doctor [--json] [--online]"));
    assert!(help.contains("example: /doctor"));
}

#[test]
fn functional_render_command_help_supports_release_channel_topic_without_slash() {
    let help = render_command_help("release-channel").expect("render help");
    assert!(help.contains("command: /release-channel"));
    assert!(help.contains(
        "usage: /release-channel [show|set <stable|beta|dev>|check|plan [--target <version>] [--dry-run]|apply [--target <version>] [--dry-run]|cache <show|clear|refresh|prune>]"
    ));
    assert!(help.contains("example: /release-channel set beta"));
}

#[test]
fn functional_render_command_help_supports_session_graph_export_topic_without_slash() {
    let help = render_command_help("session-graph-export").expect("render help");
    assert!(help.contains("command: /session-graph-export"));
    assert!(help.contains("usage: /session-graph-export <path>"));
}

#[test]
fn functional_render_command_help_supports_session_merge_topic_without_slash() {
    let help = render_command_help("session-merge").expect("render help");
    assert!(help.contains("command: /session-merge"));
    assert!(help.contains(
        "usage: /session-merge <source-id> [target-id] [--strategy <append|squash|fast-forward>]"
    ));
    assert!(help.contains("example: /session-merge 42 24 --strategy squash"));
}

#[test]
fn functional_render_command_help_supports_models_list_topic_without_slash() {
    let help = render_command_help("models-list").expect("render help");
    assert!(help.contains("command: /models-list"));
    assert!(help.contains(&format!("usage: {MODELS_LIST_USAGE}")));
}

#[test]
fn functional_render_command_help_supports_model_show_topic_without_slash() {
    let help = render_command_help("model-show").expect("render help");
    assert!(help.contains("command: /model-show"));
    assert!(help.contains(&format!("usage: {MODEL_SHOW_USAGE}")));
}

#[test]
fn functional_render_command_help_supports_skills_sync_topic_without_slash() {
    let help = render_command_help("skills-sync").expect("render help");
    assert!(help.contains("command: /skills-sync"));
    assert!(help.contains("usage: /skills-sync [lockfile_path]"));
}

#[test]
fn functional_render_command_help_supports_skills_lock_write_topic_without_slash() {
    let help = render_command_help("skills-lock-write").expect("render help");
    assert!(help.contains("command: /skills-lock-write"));
    assert!(help.contains("usage: /skills-lock-write [lockfile_path]"));
}

#[test]
fn functional_render_command_help_supports_skills_list_topic_without_slash() {
    let help = render_command_help("skills-list").expect("render help");
    assert!(help.contains("command: /skills-list"));
    assert!(help.contains("usage: /skills-list"));
}

#[test]
fn functional_render_command_help_supports_skills_show_topic_without_slash() {
    let help = render_command_help("skills-show").expect("render help");
    assert!(help.contains("command: /skills-show"));
    assert!(help.contains("usage: /skills-show <name>"));
}

#[test]
fn functional_render_command_help_supports_skills_search_topic_without_slash() {
    let help = render_command_help("skills-search").expect("render help");
    assert!(help.contains("command: /skills-search"));
    assert!(help.contains("usage: /skills-search <query> [max_results]"));
}

#[test]
fn functional_render_command_help_supports_skills_lock_diff_topic_without_slash() {
    let help = render_command_help("skills-lock-diff").expect("render help");
    assert!(help.contains("command: /skills-lock-diff"));
    assert!(help.contains("usage: /skills-lock-diff [lockfile_path] [--json]"));
}

#[test]
fn functional_render_command_help_supports_skills_prune_topic_without_slash() {
    let help = render_command_help("skills-prune").expect("render help");
    assert!(help.contains("command: /skills-prune"));
    assert!(help.contains("usage: /skills-prune [lockfile_path] [--dry-run|--apply]"));
}

#[test]
fn functional_render_command_help_supports_skills_trust_list_topic_without_slash() {
    let help = render_command_help("skills-trust-list").expect("render help");
    assert!(help.contains("command: /skills-trust-list"));
    assert!(help.contains("usage: /skills-trust-list [trust_root_file]"));
}

#[test]
fn functional_render_command_help_supports_skills_trust_add_topic_without_slash() {
    let help = render_command_help("skills-trust-add").expect("render help");
    assert!(help.contains("command: /skills-trust-add"));
    assert!(help.contains("usage: /skills-trust-add <id=base64_key> [trust_root_file]"));
}

#[test]
fn functional_render_command_help_supports_skills_trust_revoke_topic_without_slash() {
    let help = render_command_help("skills-trust-revoke").expect("render help");
    assert!(help.contains("command: /skills-trust-revoke"));
    assert!(help.contains("usage: /skills-trust-revoke <id> [trust_root_file]"));
}

#[test]
fn functional_render_command_help_supports_skills_trust_rotate_topic_without_slash() {
    let help = render_command_help("skills-trust-rotate").expect("render help");
    assert!(help.contains("command: /skills-trust-rotate"));
    assert!(
        help.contains("usage: /skills-trust-rotate <old_id:new_id=base64_key> [trust_root_file]")
    );
}

#[test]
fn functional_render_command_help_supports_skills_verify_topic_without_slash() {
    let help = render_command_help("skills-verify").expect("render help");
    assert!(help.contains("command: /skills-verify"));
    assert!(help.contains("usage: /skills-verify [lockfile_path] [trust_root_file] [--json]"));
}

#[test]
fn regression_unknown_command_message_suggests_closest_match() {
    let message = unknown_command_message("/polciy");
    assert!(message.contains("did you mean /policy?"));
}

#[test]
fn regression_unknown_command_message_without_close_match_has_no_suggestion() {
    let message = unknown_command_message("/zzzzzzzz");
    assert!(!message.contains("did you mean"));
}

#[test]
fn unit_format_id_list_renders_none_and_csv() {
    assert_eq!(format_id_list(&[]), "none");
    assert_eq!(format_id_list(&[1, 2, 42]), "1,2,42");
}

#[test]
fn unit_format_remap_ids_renders_none_and_pairs() {
    assert_eq!(format_remap_ids(&[]), "none");
    assert_eq!(format_remap_ids(&[(1, 3), (2, 4)]), "1->3,2->4");
}

#[test]
fn unit_resolve_skills_lock_path_uses_default_and_explicit_values() {
    let default_lock_path = PathBuf::from(".tau/skills/skills.lock.json");
    assert_eq!(
        resolve_skills_lock_path("", &default_lock_path),
        default_lock_path
    );
    assert_eq!(
        resolve_skills_lock_path("custom/lock.json", &default_lock_path),
        PathBuf::from("custom/lock.json")
    );
}

#[test]
fn unit_render_skills_sync_drift_details_uses_none_placeholders() {
    let report = crate::skills::SkillsSyncReport {
        expected_entries: 2,
        actual_entries: 2,
        ..crate::skills::SkillsSyncReport::default()
    };
    assert_eq!(
        render_skills_sync_drift_details(&report),
        "expected_entries=2 actual_entries=2 missing=none extra=none changed=none metadata=none"
    );
}

#[test]
fn unit_render_skills_lock_write_success_formats_path_and_entry_count() {
    let rendered = render_skills_lock_write_success(Path::new("skills.lock.json"), 3);
    assert_eq!(
        rendered,
        "skills lock write: path=skills.lock.json entries=3"
    );
}

#[test]
fn unit_render_skills_list_handles_empty_catalog() {
    let rendered = render_skills_list(Path::new(".tau/skills"), &[]);
    assert!(rendered.contains("skills list: path=.tau/skills count=0"));
    assert!(rendered.contains("skills: none"));
}

#[test]
fn unit_render_skills_show_includes_metadata_and_content() {
    let skill = crate::skills::Skill {
        name: "checklist".to_string(),
        content: "line one\nline two".to_string(),
        path: PathBuf::from("checklist.md"),
    };
    let rendered = render_skills_show(Path::new(".tau/skills"), &skill);
    assert!(rendered.contains("skills show: path=.tau/skills"));
    assert!(rendered.contains("name=checklist"));
    assert!(rendered.contains("file=checklist.md"));
    assert!(rendered.contains("content_bytes=17"));
    assert!(rendered.contains("---\nline one\nline two"));
}

#[test]
fn unit_parse_skills_search_args_defaults_and_supports_optional_limit() {
    assert_eq!(
        parse_skills_search_args("checklist").expect("parse default"),
        ("checklist".to_string(), 20)
    );
    assert_eq!(
        parse_skills_search_args("checklist 5").expect("parse explicit"),
        ("checklist".to_string(), 5)
    );
    assert_eq!(
        parse_skills_search_args("secure review 7").expect("parse multiword query"),
        ("secure review".to_string(), 7)
    );
}

#[test]
fn regression_parse_skills_search_args_rejects_missing_query_and_zero_limit() {
    let missing_query = parse_skills_search_args("").expect_err("empty query must fail");
    assert!(missing_query.to_string().contains("query is required"));

    let zero_limit = parse_skills_search_args("checklist 0").expect_err("zero limit must fail");
    assert!(zero_limit
        .to_string()
        .contains("max_results must be greater than zero"));
}

#[test]
fn unit_parse_skills_lock_diff_args_supports_defaults_path_override_and_json() {
    let default_lock = PathBuf::from(".tau/skills/skills.lock.json");
    assert_eq!(
        parse_skills_lock_diff_args("", &default_lock).expect("default parse"),
        (default_lock.clone(), false)
    );
    assert_eq!(
        parse_skills_lock_diff_args("--json", &default_lock).expect("json parse"),
        (default_lock.clone(), true)
    );
    assert_eq!(
        parse_skills_lock_diff_args("/tmp/custom.lock.json --json", &default_lock)
            .expect("path + json parse"),
        (PathBuf::from("/tmp/custom.lock.json"), true)
    );
}

#[test]
fn regression_parse_skills_lock_diff_args_rejects_extra_positional_args() {
    let default_lock = PathBuf::from(".tau/skills/skills.lock.json");
    let error = parse_skills_lock_diff_args("one two", &default_lock).expect_err("must fail");
    assert!(error
        .to_string()
        .contains("usage: /skills-lock-diff [lockfile_path] [--json]"));
}

#[test]
fn unit_parse_skills_prune_args_defaults_and_supports_mode_flags() {
    let default_lock = PathBuf::from(".tau/skills/skills.lock.json");
    assert_eq!(
        parse_skills_prune_args("", &default_lock).expect("default parse"),
        (default_lock.clone(), SkillsPruneMode::DryRun)
    );
    assert_eq!(
        parse_skills_prune_args("--apply", &default_lock).expect("apply parse"),
        (default_lock.clone(), SkillsPruneMode::Apply)
    );
    assert_eq!(
        parse_skills_prune_args("/tmp/custom.lock.json --dry-run", &default_lock)
            .expect("path + dry-run parse"),
        (
            PathBuf::from("/tmp/custom.lock.json"),
            SkillsPruneMode::DryRun
        )
    );
}

#[test]
fn regression_parse_skills_prune_args_rejects_conflicts_and_extra_positionals() {
    let default_lock = PathBuf::from(".tau/skills/skills.lock.json");

    let conflict = parse_skills_prune_args("--apply --dry-run", &default_lock)
        .expect_err("conflicting flags should fail");
    assert!(conflict.to_string().contains(SKILLS_PRUNE_USAGE));

    let extra = parse_skills_prune_args("one two", &default_lock)
        .expect_err("extra positional args should fail");
    assert!(extra.to_string().contains(SKILLS_PRUNE_USAGE));
}

#[test]
fn unit_validate_skills_prune_file_name_rejects_unsafe_paths() {
    validate_skills_prune_file_name("checklist.md").expect("simple markdown name should pass");
    assert!(validate_skills_prune_file_name("../checklist.md").is_err());
    assert!(validate_skills_prune_file_name("nested/checklist.md").is_err());
    assert!(validate_skills_prune_file_name(r"nested\checklist.md").is_err());
}

#[test]
fn unit_derive_skills_prune_candidates_filters_tracked_and_sorts() {
    let skills_dir = Path::new(".tau/skills");
    let catalog = vec![
        crate::skills::Skill {
            name: "zeta".to_string(),
            content: "zeta".to_string(),
            path: PathBuf::from(".tau/skills/zeta.md"),
        },
        crate::skills::Skill {
            name: "alpha".to_string(),
            content: "alpha".to_string(),
            path: PathBuf::from(".tau/skills/alpha.md"),
        },
        crate::skills::Skill {
            name: "beta".to_string(),
            content: "beta".to_string(),
            path: PathBuf::from(".tau/skills/beta.md"),
        },
    ];
    let tracked = HashSet::from([String::from("alpha.md")]);
    let candidates =
        derive_skills_prune_candidates(skills_dir, &catalog, &tracked).expect("derive candidates");
    let files = candidates
        .iter()
        .map(|candidate| candidate.file.as_str())
        .collect::<Vec<_>>();
    assert_eq!(files, vec!["beta.md", "zeta.md"]);
}

#[test]
fn regression_resolve_prunable_skill_file_name_rejects_nested_paths() {
    let skills_dir = Path::new(".tau/skills");
    let error = resolve_prunable_skill_file_name(skills_dir, Path::new(".tau/skills/nested/a.md"))
        .expect_err("nested path should fail");
    assert!(error.to_string().contains("nested paths are not allowed"));
}

#[test]
fn unit_parse_skills_trust_mutation_args_supports_configured_and_explicit_paths() {
    let configured = PathBuf::from("/tmp/trust-roots.json");
    assert_eq!(
        parse_skills_trust_mutation_args(
            "root=YQ==",
            Some(configured.as_path()),
            SKILLS_TRUST_ADD_USAGE
        )
        .expect("configured path should be used"),
        ("root=YQ==".to_string(), configured)
    );

    assert_eq!(
        parse_skills_trust_mutation_args(
            "root=YQ== /tmp/override.json",
            Some(Path::new("/tmp/default.json")),
            SKILLS_TRUST_ADD_USAGE
        )
        .expect("explicit path should override configured path"),
        ("root=YQ==".to_string(), PathBuf::from("/tmp/override.json"))
    );
}

#[test]
fn regression_parse_skills_trust_mutation_args_requires_path_without_configuration() {
    let missing = parse_skills_trust_mutation_args("root=YQ==", None, SKILLS_TRUST_ADD_USAGE)
        .expect_err("command should fail without configured/default path");
    assert!(missing.to_string().contains(SKILLS_TRUST_ADD_USAGE));

    let extra = parse_skills_trust_mutation_args(
        "one two three",
        Some(Path::new("/tmp/default.json")),
        SKILLS_TRUST_ADD_USAGE,
    )
    .expect_err("extra positional args should fail");
    assert!(extra.to_string().contains(SKILLS_TRUST_ADD_USAGE));
}

#[test]
fn unit_parse_skills_verify_args_supports_defaults_overrides_and_json() {
    let default_lock = Path::new("/tmp/default.lock.json");
    let default_trust = Path::new("/tmp/default-trust.json");

    let parsed =
        parse_skills_verify_args("", default_lock, Some(default_trust)).expect("parse defaults");
    assert_eq!(parsed.lock_path, PathBuf::from(default_lock));
    assert_eq!(parsed.trust_root_path, Some(PathBuf::from(default_trust)));
    assert!(!parsed.json_output);

    let parsed = parse_skills_verify_args(
        "/tmp/custom.lock.json /tmp/custom-trust.json --json",
        default_lock,
        Some(default_trust),
    )
    .expect("parse explicit args");
    assert_eq!(parsed.lock_path, PathBuf::from("/tmp/custom.lock.json"));
    assert_eq!(
        parsed.trust_root_path,
        Some(PathBuf::from("/tmp/custom-trust.json"))
    );
    assert!(parsed.json_output);
}

#[test]
fn regression_parse_skills_verify_args_rejects_unexpected_extra_positionals() {
    let error = parse_skills_verify_args(
        "a b c",
        Path::new("/tmp/default.lock.json"),
        Some(Path::new("/tmp/default-trust.json")),
    )
    .expect_err("unexpected positional arguments should fail");
    assert!(error.to_string().contains(SKILLS_VERIFY_USAGE));
}

#[test]
fn unit_parse_skills_trust_list_args_supports_configured_and_explicit_paths() {
    let configured = PathBuf::from("/tmp/trust-roots.json");
    assert_eq!(
        parse_skills_trust_list_args("", Some(configured.as_path()))
            .expect("configured path should be used"),
        configured
    );

    assert_eq!(
        parse_skills_trust_list_args("/tmp/override.json", Some(Path::new("/tmp/default.json")))
            .expect("explicit path should override configured path"),
        PathBuf::from("/tmp/override.json")
    );
}

#[test]
fn regression_parse_skills_trust_list_args_requires_path_without_configuration() {
    let missing = parse_skills_trust_list_args("", None)
        .expect_err("command should fail without configured/default path");
    assert!(missing.to_string().contains(SKILLS_TRUST_LIST_USAGE));

    let extra = parse_skills_trust_list_args("one two", Some(Path::new("/tmp/default.json")))
        .expect_err("extra positional args should fail");
    assert!(extra.to_string().contains(SKILLS_TRUST_LIST_USAGE));
}

#[test]
fn unit_trust_record_status_reports_active_revoked_and_expired() {
    let active = TrustedRootRecord {
        id: "active".to_string(),
        public_key: "YQ==".to_string(),
        revoked: false,
        expires_unix: None,
        rotated_from: None,
    };
    let revoked = TrustedRootRecord {
        id: "revoked".to_string(),
        public_key: "Yg==".to_string(),
        revoked: true,
        expires_unix: None,
        rotated_from: None,
    };
    let expired = TrustedRootRecord {
        id: "expired".to_string(),
        public_key: "Yw==".to_string(),
        revoked: false,
        expires_unix: Some(1),
        rotated_from: None,
    };

    assert_eq!(trust_record_status(&active, 10), "active");
    assert_eq!(trust_record_status(&revoked, 10), "revoked");
    assert_eq!(trust_record_status(&expired, 10), "expired");
}

#[test]
fn unit_render_skills_trust_list_handles_empty_records() {
    let rendered = render_skills_trust_list(Path::new(".tau/trust-roots.json"), &[]);
    assert!(rendered.contains("skills trust list: path=.tau/trust-roots.json count=0"));
    assert!(rendered.contains("roots: none"));
}

#[test]
fn unit_render_skills_lock_diff_helpers_include_expected_prefixes() {
    let report = crate::skills::SkillsSyncReport {
        expected_entries: 1,
        actual_entries: 1,
        ..crate::skills::SkillsSyncReport::default()
    };
    let in_sync = render_skills_lock_diff_in_sync(Path::new("skills.lock.json"), &report);
    assert!(in_sync.contains("skills lock diff: in-sync"));

    let drift = render_skills_lock_diff_drift(Path::new("skills.lock.json"), &report);
    assert!(drift.contains("skills lock diff: drift"));
}

#[test]
fn unit_render_skills_search_handles_empty_results() {
    let rendered = render_skills_search(Path::new(".tau/skills"), "missing", 10, &[], 0);
    assert!(rendered.contains("skills search: path=.tau/skills"));
    assert!(rendered.contains("query=\"missing\""));
    assert!(rendered.contains("matched=0"));
    assert!(rendered.contains("shown=0"));
    assert!(rendered.contains("skills: none"));
}

#[test]
fn functional_execute_skills_list_command_reports_sorted_inventory() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("zeta.md"), "zeta").expect("write zeta");
    std::fs::write(skills_dir.join("alpha.md"), "alpha").expect("write alpha");
    std::fs::write(skills_dir.join("ignored.txt"), "ignored").expect("write ignored");

    let output = execute_skills_list_command(&skills_dir);
    assert!(output.contains("count=2"));
    let alpha_index = output
        .find("skill: name=alpha file=alpha.md")
        .expect("alpha");
    let zeta_index = output.find("skill: name=zeta file=zeta.md").expect("zeta");
    assert!(alpha_index < zeta_index);
}

#[test]
fn regression_execute_skills_list_command_reports_errors_without_panicking() {
    let temp = tempdir().expect("tempdir");
    let not_a_dir = temp.path().join("skills.md");
    std::fs::write(&not_a_dir, "not a directory").expect("write file");

    let output = execute_skills_list_command(&not_a_dir);
    assert!(output.contains("skills list error: path="));
    assert!(output.contains("is not a directory"));
}

#[test]
fn functional_execute_skills_search_command_ranks_name_hits_before_content_hits() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("checklist.md"), "Always run tests").expect("write checklist");
    std::fs::write(skills_dir.join("quality.md"), "Use checklist for review")
        .expect("write quality");

    let output = execute_skills_search_command(&skills_dir, "checklist");
    assert!(output.contains("skills search: path="));
    assert!(output.contains("matched=2"));
    let checklist_index = output
        .find("skill: name=checklist file=checklist.md match=name")
        .expect("checklist row");
    let quality_index = output
        .find("skill: name=quality file=quality.md match=content")
        .expect("quality row");
    assert!(checklist_index < quality_index);
}

#[test]
fn regression_execute_skills_search_command_reports_invalid_args_without_panicking() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("checklist.md"), "Always run tests").expect("write skill");

    let output = execute_skills_search_command(&skills_dir, "checklist 0");
    assert!(output.contains("skills search error: path="));
    assert!(output.contains("max_results must be greater than zero"));
}

#[test]
fn functional_execute_skills_lock_diff_command_supports_human_and_json_output() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let lock_path = default_skills_lock_path(&skills_dir);
    let sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let lockfile = serde_json::json!({
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
    std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lock");

    let human = execute_skills_lock_diff_command(&skills_dir, &lock_path, "");
    assert!(human.contains("skills lock diff: in-sync"));
    assert!(human.contains("expected_entries=1"));

    let json_output = execute_skills_lock_diff_command(&skills_dir, &lock_path, "--json");
    let payload: serde_json::Value = serde_json::from_str(&json_output).expect("parse json output");
    assert_eq!(payload["status"], "in_sync");
    assert_eq!(payload["in_sync"], true);
    assert_eq!(payload["expected_entries"], 1);
    assert_eq!(payload["actual_entries"], 1);
}

#[test]
fn regression_execute_skills_lock_diff_command_reports_missing_lockfile_errors() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let missing_lock_path = temp.path().join("missing.lock.json");
    let output = execute_skills_lock_diff_command(
        &skills_dir,
        &default_skills_lock_path(&skills_dir),
        missing_lock_path.to_str().expect("utf8 path"),
    );
    assert!(output.contains("skills lock diff error: path="));
    assert!(output.contains("failed to read skills lockfile"));
}

#[test]
fn functional_execute_skills_prune_command_supports_dry_run_and_apply() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("tracked.md"), "tracked body").expect("write tracked");
    std::fs::write(skills_dir.join("stale.md"), "stale body").expect("write stale");

    let lock_path = default_skills_lock_path(&skills_dir);
    let tracked_sha = format!("{:x}", Sha256::digest("tracked body".as_bytes()));
    let lockfile = serde_json::json!({
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
    std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lockfile");

    let dry_run = execute_skills_prune_command(&skills_dir, &lock_path, "");
    assert!(dry_run.contains("skills prune: mode=dry-run"));
    assert!(dry_run.contains("prune: file=stale.md action=would_delete"));
    assert!(skills_dir.join("stale.md").exists());

    let apply = execute_skills_prune_command(&skills_dir, &lock_path, "--apply");
    assert!(apply.contains("skills prune: mode=apply"));
    assert!(apply.contains("prune: file=stale.md action=delete"));
    assert!(apply.contains("prune: file=stale.md status=deleted"));
    assert!(apply.contains("skills prune result: mode=apply deleted=1 failed=0"));
    assert!(skills_dir.join("tracked.md").exists());
    assert!(!skills_dir.join("stale.md").exists());
}

#[test]
fn regression_execute_skills_prune_command_reports_missing_lockfile_errors() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("stale.md"), "stale body").expect("write stale");

    let missing_lock_path = temp.path().join("missing.lock.json");
    let output = execute_skills_prune_command(
        &skills_dir,
        &default_skills_lock_path(&skills_dir),
        missing_lock_path.to_str().expect("utf8 path"),
    );
    assert!(output.contains("skills prune error: path="));
    assert!(output.contains("failed to read skills lockfile"));
}

#[test]
fn regression_execute_skills_prune_command_rejects_unsafe_lockfile_entries() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("stale.md"), "stale body").expect("write stale");

    let lock_path = default_skills_lock_path(&skills_dir);
    let lockfile = serde_json::json!({
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
    std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lockfile");

    let output = execute_skills_prune_command(&skills_dir, &lock_path, "");
    assert!(output.contains("skills prune error: path="));
    assert!(output.contains("unsafe lockfile entry '../escape.md'"));
}

#[test]
fn functional_execute_skills_trust_list_command_supports_default_and_explicit_paths() {
    let temp = tempdir().expect("tempdir");
    let default_trust_path = temp.path().join("trust-roots.json");
    let explicit_trust_path = temp.path().join("explicit-trust-roots.json");
    let payload = serde_json::json!({
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
    std::fs::write(&default_trust_path, format!("{payload}\n")).expect("write default trust");
    std::fs::write(&explicit_trust_path, format!("{payload}\n")).expect("write explicit trust");

    let default_output = execute_skills_trust_list_command(Some(default_trust_path.as_path()), "");
    assert!(default_output.contains("skills trust list: path="));
    assert!(default_output.contains("count=3"));
    let alpha_index = default_output.find("root: id=alpha").expect("alpha row");
    let beta_index = default_output.find("root: id=beta").expect("beta row");
    let zeta_index = default_output.find("root: id=zeta").expect("zeta row");
    assert!(alpha_index < beta_index);
    assert!(beta_index < zeta_index);
    assert!(default_output.contains(
        "root: id=beta revoked=true expires_unix=none rotated_from=alpha status=revoked"
    ));
    assert!(default_output
        .contains("root: id=zeta revoked=false expires_unix=1 rotated_from=none status=expired"));

    let explicit_output =
        execute_skills_trust_list_command(None, explicit_trust_path.to_str().expect("utf8 path"));
    assert!(explicit_output.contains("skills trust list: path="));
    assert!(explicit_output.contains("count=3"));
}

#[test]
fn functional_render_skills_verify_report_includes_summary_sync_and_entries() {
    let report = SkillsVerifyReport {
        lock_path: "/tmp/skills.lock.json".to_string(),
        trust_root_path: Some("/tmp/trust-roots.json".to_string()),
        expected_entries: 2,
        actual_entries: 2,
        missing: vec![],
        extra: vec![],
        changed: vec![],
        metadata_mismatch: vec![],
        trust: Some(SkillsVerifyTrustSummary {
            total: 1,
            active: 1,
            revoked: 0,
            expired: 0,
        }),
        summary: SkillsVerifySummary {
            entries: 2,
            pass: 2,
            warn: 0,
            fail: 0,
            status: SkillsVerifyStatus::Pass,
        },
        entries: vec![SkillsVerifyEntry {
            file: "focus.md".to_string(),
            name: "focus".to_string(),
            status: SkillsVerifyStatus::Pass,
            checks: vec![
                "sync=ok".to_string(),
                "signature=trusted key=root".to_string(),
            ],
        }],
    };

    let rendered = render_skills_verify_report(&report);
    assert!(rendered.contains(
            "skills verify: status=pass lock_path=/tmp/skills.lock.json trust_root_path=/tmp/trust-roots.json"
        ));
    assert!(rendered.contains(
            "sync: expected_entries=2 actual_entries=2 missing=none extra=none changed=none metadata=none"
        ));
    assert!(rendered.contains("trust: total=1 active=1 revoked=0 expired=0"));
    assert!(rendered.contains(
        "entry: file=focus.md name=focus status=pass checks=sync=ok;signature=trusted key=root"
    ));
}

#[test]
fn integration_execute_skills_verify_command_reports_pass_and_json_modes() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let lock_path = default_skills_lock_path(&skills_dir);
    let trust_path = temp.path().join("trust-roots.json");
    let skill_sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let signature = "c2ln";
    let signature_sha = format!("{:x}", Sha256::digest(signature.as_bytes()));
    let lockfile = serde_json::json!({
        "schema_version": 1,
        "entries": [{
            "name": "focus",
            "file": "focus.md",
            "sha256": skill_sha,
            "source": {
                "kind": "remote",
                "url": "https://example.com/focus.md",
                "expected_sha256": skill_sha,
                "signing_key_id": "root",
                "signature": signature,
                "signer_public_key": "YQ==",
                "signature_sha256": signature_sha
            }
        }]
    });
    std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lock");
    let trust = serde_json::json!({
        "roots": [{
            "id": "root",
            "public_key": "YQ==",
            "revoked": false,
            "expires_unix": null,
            "rotated_from": null
        }]
    });
    std::fs::write(&trust_path, format!("{trust}\n")).expect("write trust");

    let output =
        execute_skills_verify_command(&skills_dir, &lock_path, Some(trust_path.as_path()), "");
    assert!(output.contains("skills verify: status=pass"));
    assert!(output.contains("sync: expected_entries=1 actual_entries=1"));
    assert!(output.contains("entry: file=focus.md name=focus status=pass"));
    assert!(output.contains("signature=trusted key=root"));

    let json_output = execute_skills_verify_command(
        &skills_dir,
        &lock_path,
        Some(trust_path.as_path()),
        "--json",
    );
    let payload: serde_json::Value = serde_json::from_str(&json_output).expect("parse verify json");
    assert_eq!(payload["summary"]["status"], "pass");
    assert_eq!(payload["summary"]["fail"], 0);
    assert_eq!(payload["entries"][0]["status"], "pass");
}

#[test]
fn regression_execute_skills_verify_command_reports_untrusted_signing_key() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let lock_path = default_skills_lock_path(&skills_dir);
    let trust_path = temp.path().join("trust-roots.json");
    let skill_sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let signature = "c2ln";
    let signature_sha = format!("{:x}", Sha256::digest(signature.as_bytes()));
    let lockfile = serde_json::json!({
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
    std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lock");
    let trust = serde_json::json!({
        "roots": [{
            "id": "root",
            "public_key": "YQ==",
            "revoked": false,
            "expires_unix": null,
            "rotated_from": null
        }]
    });
    std::fs::write(&trust_path, format!("{trust}\n")).expect("write trust");

    let output =
        execute_skills_verify_command(&skills_dir, &lock_path, Some(trust_path.as_path()), "");
    assert!(output.contains("skills verify: status=fail"));
    assert!(output.contains("signature=untrusted key=unknown"));
}

#[test]
fn regression_execute_skills_verify_command_reports_missing_lockfile() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    let lock_path = temp.path().join("missing.lock.json");

    let output = execute_skills_verify_command(&skills_dir, &lock_path, None, "");
    assert!(output.contains("skills verify error: path="));
    assert!(output.contains("failed to read skills lockfile"));
}

#[test]
fn functional_execute_skills_trust_mutation_commands_round_trip_updates_store() {
    let temp = tempdir().expect("tempdir");
    let trust_path = temp.path().join("trust-roots.json");
    let payload = serde_json::json!({
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
    std::fs::write(&trust_path, format!("{payload}\n")).expect("write trust file");

    let add_output = execute_skills_trust_add_command(Some(trust_path.as_path()), "extra=Yg==");
    assert!(add_output.contains("skills trust add: path="));
    assert!(add_output.contains("id=extra"));
    assert!(add_output.contains("added=1"));

    let revoke_output = execute_skills_trust_revoke_command(Some(trust_path.as_path()), "extra");
    assert!(revoke_output.contains("skills trust revoke: path="));
    assert!(revoke_output.contains("id=extra"));
    assert!(revoke_output.contains("revoked=1"));

    let rotate_output =
        execute_skills_trust_rotate_command(Some(trust_path.as_path()), "old:new=Yw==");
    assert!(rotate_output.contains("skills trust rotate: path="));
    assert!(rotate_output.contains("old_id=old"));
    assert!(rotate_output.contains("new_id=new"));
    assert!(rotate_output.contains("rotated=1"));

    let list_output = execute_skills_trust_list_command(Some(trust_path.as_path()), "");
    assert!(list_output.contains("skills trust list: path="));
    assert!(list_output.contains("root: id=old"));
    assert!(list_output.contains("status=revoked"));
    assert!(list_output.contains("root: id=new"));
    assert!(list_output.contains("rotated_from=old status=active"));
    assert!(list_output.contains("root: id=extra"));
    assert!(list_output.contains("status=revoked"));
}

#[test]
fn regression_execute_skills_trust_add_command_requires_path_without_configuration() {
    let output = execute_skills_trust_add_command(None, "root=YQ==");
    assert!(output.contains("skills trust add error: path=none"));
    assert!(output.contains(SKILLS_TRUST_ADD_USAGE));
}

#[test]
fn regression_execute_skills_trust_revoke_command_reports_unknown_id() {
    let temp = tempdir().expect("tempdir");
    let trust_path = temp.path().join("trust-roots.json");
    std::fs::write(&trust_path, "[]\n").expect("write trust file");

    let output = execute_skills_trust_revoke_command(Some(trust_path.as_path()), "missing");
    assert!(output.contains("skills trust revoke error: path="));
    assert!(output.contains("cannot revoke unknown trust key id 'missing'"));
}

#[test]
fn regression_execute_skills_trust_rotate_command_reports_invalid_spec() {
    let temp = tempdir().expect("tempdir");
    let trust_path = temp.path().join("trust-roots.json");
    std::fs::write(&trust_path, "[]\n").expect("write trust file");

    let output = execute_skills_trust_rotate_command(Some(trust_path.as_path()), "bad-shape");
    assert!(output.contains("skills trust rotate error: path="));
    assert!(output.contains("expected old_id:new_id=base64_key"));
}

#[test]
fn regression_execute_skills_trust_list_command_reports_malformed_json() {
    let temp = tempdir().expect("tempdir");
    let trust_path = temp.path().join("trust-roots.json");
    std::fs::write(&trust_path, "{not-json").expect("write malformed trust file");

    let output = execute_skills_trust_list_command(None, trust_path.to_str().expect("utf8 path"));
    assert!(output.contains("skills trust list error: path="));
    assert!(output.contains("failed to parse trusted root file"));
}

#[test]
fn functional_execute_skills_show_command_displays_selected_skill() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("checklist.md"), "Always run tests").expect("write skill");

    let output = execute_skills_show_command(&skills_dir, "checklist");
    assert!(output.contains("skills show: path="));
    assert!(output.contains("name=checklist"));
    assert!(output.contains("file=checklist.md"));
    assert!(output.contains("Always run tests"));
}

#[test]
fn regression_execute_skills_show_command_reports_unknown_skill_without_panicking() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("known.md"), "Known").expect("write skill");

    let output = execute_skills_show_command(&skills_dir, "missing");
    assert!(output.contains("skills show error: path="));
    assert!(output.contains("error=unknown skill 'missing'"));
}

#[test]
fn functional_execute_skills_lock_write_command_writes_default_lockfile() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let lock_path = default_skills_lock_path(&skills_dir);
    let output = execute_skills_lock_write_command(&skills_dir, &lock_path, "");
    assert!(output.contains("skills lock write: path="));
    assert!(output.contains("entries=1"));

    let lock_raw = std::fs::read_to_string(lock_path).expect("read lockfile");
    assert!(lock_raw.contains("\"file\": \"focus.md\""));
}

#[test]
fn regression_execute_skills_lock_write_command_reports_write_errors_without_panicking() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let blocking_path = temp.path().join("lock-as-dir");
    std::fs::create_dir_all(&blocking_path).expect("create blocking dir");

    let output = execute_skills_lock_write_command(
        &skills_dir,
        &default_skills_lock_path(&skills_dir),
        blocking_path.to_str().expect("utf8 path"),
    );
    assert!(output.contains("skills lock write error: path="));
    assert!(
        output.contains("failed to read skills lockfile")
            || output.contains("failed to write skills lockfile")
    );
}

#[test]
fn functional_execute_skills_sync_command_reports_in_sync_for_default_lock_path() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let lock_path = default_skills_lock_path(&skills_dir);
    let sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
    let lockfile = serde_json::json!({
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
    std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lockfile");

    let output = execute_skills_sync_command(&skills_dir, &lock_path, "");
    assert!(output.contains("skills sync: in-sync"));
    assert!(output.contains("expected_entries=1"));
    assert!(output.contains("actual_entries=1"));
}

#[test]
fn regression_execute_skills_sync_command_reports_lockfile_errors_without_panicking() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

    let missing_lock_path = temp.path().join("missing.lock.json");
    let output = execute_skills_sync_command(
        &skills_dir,
        &default_skills_lock_path(&skills_dir),
        missing_lock_path.to_str().expect("utf8 path"),
    );

    assert!(output.contains("skills sync error: path="));
    assert!(output.contains("failed to read skills lockfile"));
}

#[test]
fn functional_help_command_returns_continue_action() {
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut runtime = None;
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command("/help branch", &mut agent, &mut runtime, &tool_policy_json)
        .expect("help should succeed");
    assert_eq!(action, CommandAction::Continue);
}

#[test]
fn functional_audit_summary_command_without_path_returns_continue_action() {
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut runtime = None;
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command(
        "/audit-summary",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
    )
    .expect("audit summary usage should not fail");
    assert_eq!(action, CommandAction::Continue);
}

#[test]
fn integration_skills_sync_command_preserves_session_runtime_on_drift() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "actual body").expect("write skill");
    let lock_path = default_skills_lock_path(&skills_dir);
    let lockfile = serde_json::json!({
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
    std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lock");

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        "/skills-sync",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills sync command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn integration_skills_lock_write_command_preserves_session_runtime_on_error() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("focus.md"), "actual body").expect("write skill");
    let lock_path = default_skills_lock_path(&skills_dir);
    let blocking_path = temp.path().join("lock-as-dir");
    std::fs::create_dir_all(&blocking_path).expect("blocking dir");

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        &format!("/skills-lock-write {}", blocking_path.display()),
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills lock write command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn integration_skills_list_command_preserves_session_runtime() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
    std::fs::write(skills_dir.join("beta.md"), "beta body").expect("write beta");
    let lock_path = default_skills_lock_path(&skills_dir);

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        "/skills-list",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills list command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn integration_skills_show_command_preserves_session_runtime_on_unknown_skill() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
    let lock_path = default_skills_lock_path(&skills_dir);

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        "/skills-show missing",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills show command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn integration_skills_search_command_preserves_session_runtime_on_invalid_args() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
    let lock_path = default_skills_lock_path(&skills_dir);

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        "/skills-search alpha 0",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills search command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn integration_skills_lock_diff_command_preserves_session_runtime_on_error() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
    let lock_path = default_skills_lock_path(&skills_dir);

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        "/skills-lock-diff /tmp/missing.lock.json",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills lock diff command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn integration_skills_verify_command_preserves_session_runtime_on_error() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
    let lock_path = default_skills_lock_path(&skills_dir);

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        "/skills-verify /tmp/missing.lock.json",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills verify command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn integration_skills_prune_command_preserves_session_runtime_on_error() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
    let lock_path = default_skills_lock_path(&skills_dir);

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        "/skills-prune /tmp/missing.lock.json --apply",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills prune command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn integration_skills_trust_list_command_preserves_session_runtime_on_error() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
    let lock_path = default_skills_lock_path(&skills_dir);
    let trust_path = temp.path().join("trust-roots.json");
    std::fs::write(&trust_path, "{invalid-json").expect("write malformed trust file");

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config =
        skills_command_config(&skills_dir, &lock_path, Some(trust_path.as_path()));

    let action = handle_command_with_session_import_mode(
        "/skills-trust-list",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills trust list command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn integration_skills_trust_mutation_commands_update_store_and_preserve_runtime() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
    let lock_path = default_skills_lock_path(&skills_dir);
    let trust_path = temp.path().join("trust-roots.json");
    std::fs::write(&trust_path, "[]\n").expect("write empty trust file");

    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[tau_ai::Message::user("hello")])
        .expect("append user")
        .expect("head id");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store.lineage_messages(Some(head)).expect("lineage");
    agent.replace_messages(lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_command_config =
        skills_command_config(&skills_dir, &lock_path, Some(trust_path.as_path()));

    let action = handle_command_with_session_import_mode(
        "/skills-trust-add root=YQ==",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills trust add command should continue");
    assert_eq!(action, CommandAction::Continue);

    let action = handle_command_with_session_import_mode(
        "/skills-trust-revoke root",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills trust revoke command should continue");
    assert_eq!(action, CommandAction::Continue);

    let action = handle_command_with_session_import_mode(
        "/skills-trust-rotate root:new=Yg==",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("skills trust rotate command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());

    let records = load_trust_root_records(&trust_path).expect("load trust records");
    let root_record = records
        .iter()
        .find(|record| record.id == "root")
        .expect("root");
    let new_record = records
        .iter()
        .find(|record| record.id == "new")
        .expect("new");
    assert!(root_record.revoked);
    assert!(!new_record.revoked);
    assert_eq!(new_record.rotated_from.as_deref(), Some("root"));
}

#[test]
fn functional_resolve_prompt_input_reads_prompt_file() {
    let temp = tempdir().expect("tempdir");
    let prompt_path = temp.path().join("prompt.txt");
    std::fs::write(&prompt_path, "file prompt\nline two").expect("write prompt");

    let mut cli = test_cli();
    cli.prompt_file = Some(prompt_path);

    let prompt = resolve_prompt_input(&cli).expect("resolve prompt from file");
    assert_eq!(prompt.as_deref(), Some("file prompt\nline two"));
}

#[test]
fn functional_resolve_prompt_input_renders_prompt_template_file() {
    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("prompt-template.txt");
    std::fs::write(
        &template_path,
        "Summarize {{module}} with focus on {{focus}}.",
    )
    .expect("write template");

    let mut cli = test_cli();
    cli.prompt_template_file = Some(template_path);
    cli.prompt_template_var = vec![
        "module=src/main.rs".to_string(),
        "focus=error handling".to_string(),
    ];

    let prompt = resolve_prompt_input(&cli).expect("resolve rendered template");
    assert_eq!(
        prompt.as_deref(),
        Some("Summarize src/main.rs with focus on error handling.")
    );
}

#[test]
fn regression_resolve_prompt_input_rejects_empty_prompt_file() {
    let temp = tempdir().expect("tempdir");
    let prompt_path = temp.path().join("prompt.txt");
    std::fs::write(&prompt_path, "   \n\t").expect("write prompt");

    let mut cli = test_cli();
    cli.prompt_file = Some(prompt_path.clone());

    let error = resolve_prompt_input(&cli).expect_err("empty prompt should fail");
    assert!(error
        .to_string()
        .contains(&format!("prompt file {} is empty", prompt_path.display())));
}

#[test]
fn regression_resolve_prompt_input_rejects_template_with_missing_variable() {
    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("prompt-template.txt");
    std::fs::write(&template_path, "Review {{path}} and {{goal}}").expect("write template");

    let mut cli = test_cli();
    cli.prompt_template_file = Some(template_path);
    cli.prompt_template_var = vec!["path=src/lib.rs".to_string()];

    let error = resolve_prompt_input(&cli).expect_err("missing template var should fail");
    assert!(error
        .to_string()
        .contains("missing a --prompt-template-var value"));
}

#[test]
fn regression_resolve_prompt_input_rejects_invalid_template_var_spec() {
    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("prompt-template.txt");
    std::fs::write(&template_path, "Review {{path}}").expect("write template");

    let mut cli = test_cli();
    cli.prompt_template_file = Some(template_path);
    cli.prompt_template_var = vec!["path".to_string()];

    let error = resolve_prompt_input(&cli).expect_err("invalid template var spec should fail");
    assert!(error.to_string().contains("invalid --prompt-template-var"));
}

#[test]
fn regression_resolve_prompt_input_rejects_unused_template_vars() {
    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("prompt-template.txt");
    std::fs::write(&template_path, "Review {{path}}").expect("write template");

    let mut cli = test_cli();
    cli.prompt_template_file = Some(template_path);
    cli.prompt_template_var = vec!["path=src/lib.rs".to_string(), "extra=unused".to_string()];

    let error = resolve_prompt_input(&cli).expect_err("unused template vars should fail");
    assert!(error
        .to_string()
        .contains("unused --prompt-template-var keys"));
}

#[test]
fn functional_resolve_secret_from_cli_or_store_id_reads_integration_secret() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("credentials.json");
    write_test_integration_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        "github-token",
        IntegrationCredentialStoreRecord {
            secret: Some("ghp_store_secret".to_string()),
            revoked: false,
            updated_unix: Some(current_unix_timestamp()),
        },
    );

    let mut cli = test_cli();
    cli.credential_store = store_path;
    let resolved =
        resolve_secret_from_cli_or_store_id(&cli, None, Some("github-token"), "--github-token-id")
            .expect("resolve secret")
            .expect("secret should be present");
    assert_eq!(resolved, "ghp_store_secret");
}

#[test]
fn regression_resolve_secret_from_cli_or_store_id_rejects_revoked_secret() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("credentials.json");
    write_test_integration_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        "slack-app-token",
        IntegrationCredentialStoreRecord {
            secret: Some("xapp_secret".to_string()),
            revoked: true,
            updated_unix: Some(current_unix_timestamp()),
        },
    );

    let mut cli = test_cli();
    cli.credential_store = store_path;
    let error = resolve_secret_from_cli_or_store_id(
        &cli,
        None,
        Some("slack-app-token"),
        "--slack-app-token-id",
    )
    .expect_err("revoked secret should fail");
    assert!(error.to_string().contains("is revoked"));
}

#[test]
fn unit_resolve_secret_from_cli_or_store_id_prefers_direct_secret() {
    let cli = test_cli();
    let resolved = resolve_secret_from_cli_or_store_id(
        &cli,
        Some("direct-token"),
        Some("missing-id"),
        "--github-token-id",
    )
    .expect("resolve direct secret")
    .expect("secret");
    assert_eq!(resolved, "direct-token");
}

#[test]
fn unit_validate_github_issues_bridge_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = Some("token".to_string());

    validate_github_issues_bridge_cli(&cli).expect("bridge config should validate");
}

#[test]
fn unit_validate_github_issues_bridge_cli_accepts_token_id_configuration() {
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token_id = Some("github-token".to_string());

    validate_github_issues_bridge_cli(&cli).expect("bridge config should validate");
}

#[test]
fn functional_validate_github_issues_bridge_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = Some("token".to_string());
    cli.prompt = Some("conflict".to_string());

    let error = validate_github_issues_bridge_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--github-issues-bridge cannot be combined"));
}

#[test]
fn regression_validate_github_issues_bridge_cli_rejects_prompt_template_conflicts() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = Some("token".to_string());
    cli.prompt_template_file = Some(temp.path().join("template.txt"));

    let error = validate_github_issues_bridge_cli(&cli).expect_err("template conflict");
    assert!(error.to_string().contains("--prompt-template-file"));
}

#[test]
fn regression_validate_github_issues_bridge_cli_requires_credentials() {
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = None;
    cli.github_token_id = None;

    let error = validate_github_issues_bridge_cli(&cli).expect_err("missing token");
    assert!(error
        .to_string()
        .contains("--github-token (or --github-token-id) is required"));
}

#[test]
fn regression_validate_github_issues_bridge_cli_rejects_empty_required_labels() {
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = Some("token".to_string());
    cli.github_required_label = vec!["  ".to_string()];

    let error = validate_github_issues_bridge_cli(&cli).expect_err("empty label should fail");
    assert!(error
        .to_string()
        .contains("--github-required-label cannot be empty"));
}

#[test]
fn regression_validate_github_issues_bridge_cli_rejects_zero_issue_number() {
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = Some("token".to_string());
    cli.github_issue_number = vec![0];

    let error = validate_github_issues_bridge_cli(&cli).expect_err("zero issue number");
    assert!(error
        .to_string()
        .contains("--github-issue-number must be greater than 0"));
}

#[test]
fn unit_validate_slack_bridge_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.slack_bridge = true;
    cli.slack_app_token = Some("xapp-test".to_string());
    cli.slack_bot_token = Some("xoxb-test".to_string());

    validate_slack_bridge_cli(&cli).expect("slack bridge config should validate");
}

#[test]
fn unit_validate_slack_bridge_cli_accepts_token_id_configuration() {
    let mut cli = test_cli();
    cli.slack_bridge = true;
    cli.slack_app_token_id = Some("slack-app-token".to_string());
    cli.slack_bot_token_id = Some("slack-bot-token".to_string());

    validate_slack_bridge_cli(&cli).expect("slack bridge config should validate");
}

#[test]
fn functional_validate_slack_bridge_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.slack_bridge = true;
    cli.slack_app_token = Some("xapp-test".to_string());
    cli.slack_bot_token = Some("xoxb-test".to_string());
    cli.prompt = Some("conflict".to_string());

    let error = validate_slack_bridge_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--slack-bridge cannot be combined"));
}

#[test]
fn regression_validate_slack_bridge_cli_rejects_prompt_template_conflicts() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.slack_bridge = true;
    cli.slack_app_token = Some("xapp-test".to_string());
    cli.slack_bot_token = Some("xoxb-test".to_string());
    cli.prompt_template_file = Some(temp.path().join("template.txt"));

    let error = validate_slack_bridge_cli(&cli).expect_err("template conflict");
    assert!(error.to_string().contains("--prompt-template-file"));
}

#[test]
fn regression_validate_slack_bridge_cli_rejects_missing_tokens() {
    let mut cli = test_cli();
    cli.slack_bridge = true;
    cli.slack_app_token = Some("xapp-test".to_string());
    cli.slack_bot_token = None;
    cli.slack_app_token_id = None;
    cli.slack_bot_token_id = None;

    let error = validate_slack_bridge_cli(&cli).expect_err("missing slack bot token");
    assert!(error
        .to_string()
        .contains("--slack-bot-token (or --slack-bot-token-id) is required"));
}

#[test]
fn unit_validate_events_runner_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.events_runner = true;
    validate_events_runner_cli(&cli).expect("events runner config should validate");
}

#[test]
fn functional_validate_events_runner_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.events_runner = true;
    cli.prompt = Some("conflict".to_string());
    let error = validate_events_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--events-runner cannot be combined"));
}

#[test]
fn regression_validate_events_runner_cli_rejects_prompt_template_conflicts() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.events_runner = true;
    cli.prompt_template_file = Some(temp.path().join("template.txt"));

    let error = validate_events_runner_cli(&cli).expect_err("template conflict");
    assert!(error.to_string().contains("--prompt-template-file"));
}

#[test]
fn unit_validate_multi_channel_contract_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "name": "single-event",
  "events": [
    {
      "schema_version": 1,
      "transport": "telegram",
      "event_kind": "message",
      "event_id": "telegram-1",
      "conversation_id": "telegram-chat-1",
      "actor_id": "telegram-user-1",
      "timestamp_ms": 1760000000000,
      "text": "hello",
      "metadata": {}
    }
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = test_cli();
    cli.multi_channel_contract_runner = true;
    cli.multi_channel_fixture = fixture_path;

    validate_multi_channel_contract_runner_cli(&cli)
        .expect("multi-channel runner config should validate");
}

#[test]
fn functional_validate_multi_channel_contract_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.multi_channel_contract_runner = true;
    cli.multi_channel_fixture = fixture_path;
    cli.prompt = Some("conflict".to_string());

    let error = validate_multi_channel_contract_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--multi-channel-contract-runner cannot be combined"));
}

#[test]
fn integration_validate_multi_channel_contract_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.multi_channel_contract_runner = true;
    cli.multi_channel_fixture = fixture_path;
    cli.events_runner = true;

    let error = validate_multi_channel_contract_runner_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, or --memory-contract-runner"
    ));
}

#[test]
fn regression_validate_multi_channel_contract_runner_cli_rejects_zero_limits() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.multi_channel_contract_runner = true;
    cli.multi_channel_fixture = fixture_path.clone();
    cli.multi_channel_queue_limit = 0;
    let queue_error =
        validate_multi_channel_contract_runner_cli(&cli).expect_err("zero queue limit");
    assert!(queue_error
        .to_string()
        .contains("--multi-channel-queue-limit must be greater than 0"));

    cli.multi_channel_queue_limit = 1;
    cli.multi_channel_processed_event_cap = 0;
    let processed_cap_error =
        validate_multi_channel_contract_runner_cli(&cli).expect_err("zero processed event cap");
    assert!(processed_cap_error
        .to_string()
        .contains("--multi-channel-processed-event-cap must be greater than 0"));

    cli.multi_channel_processed_event_cap = 1;
    cli.multi_channel_retry_max_attempts = 0;
    let retry_error =
        validate_multi_channel_contract_runner_cli(&cli).expect_err("zero retry max attempts");
    assert!(retry_error
        .to_string()
        .contains("--multi-channel-retry-max-attempts must be greater than 0"));

    cli.multi_channel_retry_max_attempts = 1;
    cli.multi_channel_outbound_max_chars = 0;
    let outbound_chunk_error =
        validate_multi_channel_contract_runner_cli(&cli).expect_err("zero outbound chunk size");
    assert!(outbound_chunk_error
        .to_string()
        .contains("--multi-channel-outbound-max-chars must be greater than 0"));

    cli.multi_channel_outbound_max_chars = 1;
    cli.multi_channel_outbound_http_timeout_ms = 0;
    let outbound_timeout_error =
        validate_multi_channel_contract_runner_cli(&cli).expect_err("zero outbound timeout");
    assert!(outbound_timeout_error
        .to_string()
        .contains("--multi-channel-outbound-http-timeout-ms must be greater than 0"));
}

#[test]
fn regression_validate_multi_channel_contract_runner_cli_requires_existing_fixture() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.multi_channel_contract_runner = true;
    cli.multi_channel_fixture = temp.path().join("missing.json");

    let error =
        validate_multi_channel_contract_runner_cli(&cli).expect_err("missing fixture should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_multi_channel_contract_runner_cli_requires_fixture_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.multi_channel_contract_runner = true;
    cli.multi_channel_fixture = temp.path().to_path_buf();

    let error = validate_multi_channel_contract_runner_cli(&cli)
        .expect_err("directory fixture should fail");
    assert!(error.to_string().contains("must point to a file"));
}

#[test]
fn unit_validate_multi_channel_live_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let ingress_dir = temp.path().join("live-ingress");
    std::fs::create_dir_all(&ingress_dir).expect("create ingress directory");

    let mut cli = test_cli();
    cli.multi_channel_live_runner = true;
    cli.multi_channel_live_ingress_dir = ingress_dir;

    validate_multi_channel_live_runner_cli(&cli)
        .expect("multi-channel live runner config should validate");
}

#[test]
fn functional_validate_multi_channel_live_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let ingress_dir = temp.path().join("live-ingress");
    std::fs::create_dir_all(&ingress_dir).expect("create ingress directory");

    let mut cli = test_cli();
    cli.multi_channel_live_runner = true;
    cli.multi_channel_live_ingress_dir = ingress_dir;
    cli.prompt = Some("conflict".to_string());

    let error = validate_multi_channel_live_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--multi-channel-live-runner cannot be combined"));
}

#[test]
fn integration_validate_multi_channel_live_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let ingress_dir = temp.path().join("live-ingress");
    std::fs::create_dir_all(&ingress_dir).expect("create ingress directory");

    let mut cli = test_cli();
    cli.multi_channel_live_runner = true;
    cli.multi_channel_live_ingress_dir = ingress_dir;
    cli.events_runner = true;

    let error = validate_multi_channel_live_runner_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, or --memory-contract-runner"
    ));
}

#[test]
fn regression_validate_multi_channel_live_runner_cli_rejects_missing_ingress_dir() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.multi_channel_live_runner = true;
    cli.multi_channel_live_ingress_dir = temp.path().join("missing-ingress");

    let error =
        validate_multi_channel_live_runner_cli(&cli).expect_err("missing ingress dir should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_multi_channel_live_runner_cli_requires_ingress_directory() {
    let temp = tempdir().expect("tempdir");
    let ingress_file = temp.path().join("ingress.ndjson");
    std::fs::write(&ingress_file, "{}\n").expect("write ingress file");

    let mut cli = test_cli();
    cli.multi_channel_live_runner = true;
    cli.multi_channel_live_ingress_dir = ingress_file;

    let error =
        validate_multi_channel_live_runner_cli(&cli).expect_err("ingress path file should fail");
    assert!(error.to_string().contains("must point to a directory"));
}

#[test]
fn regression_validate_multi_channel_live_runner_cli_rejects_zero_limits() {
    let temp = tempdir().expect("tempdir");
    let ingress_dir = temp.path().join("live-ingress");
    std::fs::create_dir_all(&ingress_dir).expect("create ingress directory");

    let mut cli = test_cli();
    cli.multi_channel_live_runner = true;
    cli.multi_channel_live_ingress_dir = ingress_dir;
    cli.multi_channel_queue_limit = 0;
    let queue_error = validate_multi_channel_live_runner_cli(&cli).expect_err("zero queue limit");
    assert!(queue_error
        .to_string()
        .contains("--multi-channel-queue-limit must be greater than 0"));

    cli.multi_channel_queue_limit = 1;
    cli.multi_channel_outbound_max_chars = 0;
    let chunk_error =
        validate_multi_channel_live_runner_cli(&cli).expect_err("zero outbound chunk size");
    assert!(chunk_error
        .to_string()
        .contains("--multi-channel-outbound-max-chars must be greater than 0"));

    cli.multi_channel_outbound_max_chars = 1;
    cli.multi_channel_outbound_http_timeout_ms = 0;
    let timeout_error =
        validate_multi_channel_live_runner_cli(&cli).expect_err("zero outbound timeout");
    assert!(timeout_error
        .to_string()
        .contains("--multi-channel-outbound-http-timeout-ms must be greater than 0"));
}

#[test]
fn unit_validate_multi_channel_live_connectors_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");

    let mut cli = test_cli();
    cli.multi_channel_live_connectors_runner = true;
    cli.multi_channel_live_ingress_dir = temp.path().join("live-ingress");
    cli.multi_channel_live_connectors_state_path = temp.path().join("connectors-state.json");
    cli.multi_channel_telegram_ingress_mode = CliMultiChannelLiveConnectorMode::Polling;

    validate_multi_channel_live_connectors_runner_cli(&cli)
        .expect("multi-channel live connectors config should validate");
}

#[test]
fn functional_validate_multi_channel_live_connectors_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");

    let mut cli = test_cli();
    cli.multi_channel_live_connectors_runner = true;
    cli.multi_channel_live_ingress_dir = temp.path().join("live-ingress");
    cli.multi_channel_live_connectors_state_path = temp.path().join("connectors-state.json");
    cli.multi_channel_telegram_ingress_mode = CliMultiChannelLiveConnectorMode::Polling;
    cli.prompt = Some("conflict".to_string());

    let error =
        validate_multi_channel_live_connectors_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--multi-channel-live-connectors-runner cannot be combined"));
}

#[test]
fn integration_validate_multi_channel_live_connectors_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");

    let mut cli = test_cli();
    cli.multi_channel_live_connectors_runner = true;
    cli.multi_channel_live_ingress_dir = temp.path().join("live-ingress");
    cli.multi_channel_live_connectors_state_path = temp.path().join("connectors-state.json");
    cli.multi_channel_telegram_ingress_mode = CliMultiChannelLiveConnectorMode::Polling;
    cli.events_runner = true;

    let error = validate_multi_channel_live_connectors_runner_cli(&cli)
        .expect_err("transport conflict should fail");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, or --memory-contract-runner"
    ));
}

#[test]
fn regression_validate_multi_channel_live_connectors_runner_cli_rejects_invalid_modes_and_bindings()
{
    let temp = tempdir().expect("tempdir");

    let mut cli = test_cli();
    cli.multi_channel_live_connectors_runner = true;
    cli.multi_channel_live_ingress_dir = temp.path().join("live-ingress");
    cli.multi_channel_live_connectors_state_path = temp.path().join("connectors-state.json");

    let no_mode_error = validate_multi_channel_live_connectors_runner_cli(&cli)
        .expect_err("missing mode should fail");
    assert!(no_mode_error
        .to_string()
        .contains("at least one connector mode must be enabled"));

    cli.multi_channel_discord_ingress_mode = CliMultiChannelLiveConnectorMode::Webhook;
    let discord_mode_error = validate_multi_channel_live_connectors_runner_cli(&cli)
        .expect_err("discord webhook should fail");
    assert!(discord_mode_error
        .to_string()
        .contains("--multi-channel-discord-ingress-mode=webhook is not supported"));

    cli.multi_channel_discord_ingress_mode = CliMultiChannelLiveConnectorMode::Polling;
    let discord_ids_error = validate_multi_channel_live_connectors_runner_cli(&cli)
        .expect_err("discord polling without channel ids should fail");
    assert!(discord_ids_error
        .to_string()
        .contains("--multi-channel-discord-ingress-channel-id is required"));

    cli.multi_channel_discord_ingress_channel_ids = vec!["ops-room".to_string()];
    cli.multi_channel_whatsapp_ingress_mode = CliMultiChannelLiveConnectorMode::Webhook;
    cli.multi_channel_live_connectors_poll_once = true;
    let poll_once_error = validate_multi_channel_live_connectors_runner_cli(&cli)
        .expect_err("poll once cannot pair with webhook mode");
    assert!(poll_once_error.to_string().contains(
        "--multi-channel-live-connectors-poll-once cannot be used with webhook connector modes"
    ));

    cli.multi_channel_live_connectors_poll_once = false;
    cli.multi_channel_live_webhook_bind = "invalid bind".to_string();
    let bind_error = validate_multi_channel_live_connectors_runner_cli(&cli)
        .expect_err("invalid bind should fail");
    assert!(bind_error
        .to_string()
        .contains("invalid --multi-channel-live-webhook-bind"));
}

#[test]
fn unit_validate_multi_channel_live_ingest_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let payload_file = temp.path().join("telegram-update.json");
    std::fs::write(
        &payload_file,
        r#"{"update_id":1,"message":{"message_id":2,"chat":{"id":"chat-1"},"from":{"id":"user-1"},"date":1760100000,"text":"hello"}}"#,
    )
    .expect("write payload");

    let mut cli = test_cli();
    cli.multi_channel_live_ingest_file = Some(payload_file);
    cli.multi_channel_live_ingest_transport = Some(CliMultiChannelTransport::Telegram);
    cli.multi_channel_live_ingest_dir = temp.path().join("live-ingress");

    validate_multi_channel_live_ingest_cli(&cli)
        .expect("multi-channel live ingest config should validate");
}

#[test]
fn functional_validate_multi_channel_live_ingest_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let payload_file = temp.path().join("discord-message.json");
    std::fs::write(
        &payload_file,
        r#"{"id":"m1","channel_id":"c1","timestamp":"2026-01-10T00:00:00Z","content":"hello","author":{"id":"u1"}}"#,
    )
    .expect("write payload");

    let mut cli = test_cli();
    cli.multi_channel_live_ingest_file = Some(payload_file);
    cli.multi_channel_live_ingest_transport = Some(CliMultiChannelTransport::Discord);
    cli.events_runner = true;

    let error =
        validate_multi_channel_live_ingest_cli(&cli).expect_err("transport conflict should fail");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, or --memory-contract-runner"
    ));
}

#[test]
fn integration_validate_multi_channel_live_ingest_cli_requires_existing_payload_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.multi_channel_live_ingest_file = Some(temp.path().join("missing.json"));
    cli.multi_channel_live_ingest_transport = Some(CliMultiChannelTransport::Whatsapp);
    cli.multi_channel_live_ingest_dir = temp.path().join("live-ingress");

    let error =
        validate_multi_channel_live_ingest_cli(&cli).expect_err("missing payload should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_multi_channel_live_ingest_cli_rejects_empty_provider() {
    let temp = tempdir().expect("tempdir");
    let payload_file = temp.path().join("whatsapp-message.json");
    std::fs::write(
        &payload_file,
        r#"{"metadata":{"phone_number_id":"p1"},"messages":[{"id":"mid","from":"15550001111","timestamp":"1760300000","text":{"body":"hello"}}]}"#,
    )
    .expect("write payload");

    let mut cli = test_cli();
    cli.multi_channel_live_ingest_file = Some(payload_file);
    cli.multi_channel_live_ingest_transport = Some(CliMultiChannelTransport::Whatsapp);
    cli.multi_channel_live_ingest_provider = "   ".to_string();
    cli.multi_channel_live_ingest_dir = temp.path().join("live-ingress");

    let error = validate_multi_channel_live_ingest_cli(&cli)
        .expect_err("empty provider should be rejected");
    assert!(error
        .to_string()
        .contains("--multi-channel-live-ingest-provider cannot be empty"));
}

#[test]
fn unit_validate_multi_channel_channel_lifecycle_cli_accepts_status_mode() {
    let mut cli = test_cli();
    cli.multi_channel_channel_status = Some(CliMultiChannelTransport::Telegram);
    validate_multi_channel_channel_lifecycle_cli(&cli)
        .expect("multi-channel lifecycle status config should validate");
}

#[test]
fn functional_validate_multi_channel_channel_lifecycle_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.multi_channel_channel_status = Some(CliMultiChannelTransport::Discord);
    cli.prompt = Some("conflict".to_string());
    let error =
        validate_multi_channel_channel_lifecycle_cli(&cli).expect_err("prompt conflict expected");
    assert!(error
        .to_string()
        .contains("--multi-channel-channel-* commands cannot be combined"));
}

#[test]
fn integration_validate_multi_channel_channel_lifecycle_cli_rejects_runtime_conflicts() {
    let mut cli = test_cli();
    cli.multi_channel_channel_probe = Some(CliMultiChannelTransport::Whatsapp);
    cli.events_runner = true;
    let error = validate_multi_channel_channel_lifecycle_cli(&cli)
        .expect_err("runtime conflict should fail");
    assert!(error
        .to_string()
        .contains("active transport/runtime commands"));
}

#[test]
fn regression_validate_multi_channel_channel_lifecycle_cli_rejects_multiple_actions() {
    let mut cli = test_cli();
    cli.multi_channel_channel_login = Some(CliMultiChannelTransport::Telegram);
    cli.multi_channel_channel_probe = Some(CliMultiChannelTransport::Telegram);
    let error = validate_multi_channel_channel_lifecycle_cli(&cli)
        .expect_err("multiple lifecycle actions should fail");
    assert!(error.to_string().contains("mutually exclusive"));
}

#[test]
fn regression_validate_multi_channel_channel_lifecycle_cli_rejects_file_state_dir() {
    let temp = tempdir().expect("tempdir");
    let state_file = temp.path().join("multi-channel-state-file");
    std::fs::write(&state_file, "{}").expect("write state file");

    let mut cli = test_cli();
    cli.multi_channel_channel_status = Some(CliMultiChannelTransport::Telegram);
    cli.multi_channel_state_dir = state_file;
    let error = validate_multi_channel_channel_lifecycle_cli(&cli)
        .expect_err("state-dir file path should fail");
    assert!(error.to_string().contains("--multi-channel-state-dir"));
}

#[test]
fn regression_validate_multi_channel_channel_lifecycle_cli_rejects_probe_online_without_probe() {
    let mut cli = test_cli();
    cli.multi_channel_channel_probe_online = true;

    let error = validate_multi_channel_channel_lifecycle_cli(&cli)
        .expect_err("probe online without probe action should fail");
    assert!(error
        .to_string()
        .contains("--multi-channel-channel-probe-online requires --multi-channel-channel-probe"));
}

#[test]
fn unit_validate_multi_channel_send_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.multi_channel_send = Some(CliMultiChannelTransport::Telegram);
    cli.multi_channel_send_target = Some("-100123456".to_string());
    cli.multi_channel_send_text = Some("hello".to_string());
    cli.multi_channel_outbound_mode = CliMultiChannelOutboundMode::DryRun;
    validate_multi_channel_send_cli(&cli).expect("multi-channel send config should validate");
}

#[test]
fn functional_validate_multi_channel_send_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.multi_channel_send = Some(CliMultiChannelTransport::Discord);
    cli.multi_channel_send_target = Some("1234567890123".to_string());
    cli.multi_channel_send_text = Some("hello".to_string());
    cli.multi_channel_outbound_mode = CliMultiChannelOutboundMode::DryRun;
    cli.prompt = Some("conflict".to_string());
    let error = validate_multi_channel_send_cli(&cli).expect_err("prompt conflict expected");
    assert!(error
        .to_string()
        .contains("--multi-channel-send cannot be combined"));
}

#[test]
fn integration_validate_multi_channel_send_cli_rejects_runtime_conflicts() {
    let mut cli = test_cli();
    cli.multi_channel_send = Some(CliMultiChannelTransport::Whatsapp);
    cli.multi_channel_send_target = Some("+15551230000".to_string());
    cli.multi_channel_send_text = Some("hello".to_string());
    cli.multi_channel_outbound_mode = CliMultiChannelOutboundMode::DryRun;
    cli.events_runner = true;
    let error = validate_multi_channel_send_cli(&cli).expect_err("runtime conflict should fail");
    assert!(error
        .to_string()
        .contains("active transport/runtime commands"));
}

#[test]
fn regression_validate_multi_channel_send_cli_rejects_channel_store_mode() {
    let mut cli = test_cli();
    cli.multi_channel_send = Some(CliMultiChannelTransport::Discord);
    cli.multi_channel_send_target = Some("1234567890123".to_string());
    cli.multi_channel_send_text = Some("hello".to_string());
    cli.multi_channel_outbound_mode = CliMultiChannelOutboundMode::ChannelStore;
    let error = validate_multi_channel_send_cli(&cli).expect_err("channel-store mode should fail");
    assert!(error.to_string().contains(
        "--multi-channel-send requires --multi-channel-outbound-mode=dry-run or provider"
    ));
}

#[test]
fn unit_validate_multi_channel_incident_timeline_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.multi_channel_incident_timeline = true;
    validate_multi_channel_incident_timeline_cli(&cli)
        .expect("incident timeline config should validate");
}

#[test]
fn functional_validate_multi_channel_incident_timeline_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.multi_channel_incident_timeline = true;
    cli.prompt = Some("conflict".to_string());
    let error = validate_multi_channel_incident_timeline_cli(&cli)
        .expect_err("prompt conflict should fail");
    assert!(error
        .to_string()
        .contains("--multi-channel-incident-timeline cannot be combined"));
}

#[test]
fn integration_validate_multi_channel_incident_timeline_cli_rejects_runtime_conflicts() {
    let mut cli = test_cli();
    cli.multi_channel_incident_timeline = true;
    cli.events_runner = true;
    let error = validate_multi_channel_incident_timeline_cli(&cli)
        .expect_err("runtime conflict should fail");
    assert!(error
        .to_string()
        .contains("active transport/runtime commands"));
}

#[test]
fn regression_validate_multi_channel_incident_timeline_cli_rejects_inverted_window() {
    let mut cli = test_cli();
    cli.multi_channel_incident_timeline = true;
    cli.multi_channel_incident_start_unix_ms = Some(200);
    cli.multi_channel_incident_end_unix_ms = Some(100);
    let error = validate_multi_channel_incident_timeline_cli(&cli)
        .expect_err("inverted window should fail");
    assert!(error.to_string().contains(
        "--multi-channel-incident-end-unix-ms must be greater than or equal to --multi-channel-incident-start-unix-ms"
    ));
}

#[test]
fn unit_validate_multi_agent_contract_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("multi-agent-fixture.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "name": "single-case",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "planner-success",
      "phase": "planner",
      "route_table": {
        "schema_version": 1,
        "roles": {
          "planner": {},
          "reviewer": {}
        },
        "planner": { "role": "planner" },
        "delegated": { "role": "planner" },
        "review": { "role": "reviewer" }
      },
      "expected": {
        "outcome": "success",
        "selected_role": "planner",
        "attempted_roles": ["planner"]
      }
    }
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = test_cli();
    cli.multi_agent_contract_runner = true;
    cli.multi_agent_fixture = fixture_path;

    validate_multi_agent_contract_runner_cli(&cli)
        .expect("multi-agent runner config should validate");
}

#[test]
fn functional_validate_multi_agent_contract_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.multi_agent_contract_runner = true;
    cli.multi_agent_fixture = fixture_path;
    cli.prompt = Some("conflict".to_string());

    let error = validate_multi_agent_contract_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--multi-agent-contract-runner cannot be combined"));
}

#[test]
fn integration_validate_multi_agent_contract_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.multi_agent_contract_runner = true;
    cli.multi_agent_fixture = fixture_path;
    cli.dashboard_contract_runner = true;

    let error = validate_multi_agent_contract_runner_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, --memory-contract-runner, or --dashboard-contract-runner"
    ));
}

#[test]
fn regression_validate_multi_agent_contract_runner_cli_rejects_zero_limits() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.multi_agent_contract_runner = true;
    cli.multi_agent_fixture = fixture_path.clone();
    cli.multi_agent_queue_limit = 0;
    let queue_error = validate_multi_agent_contract_runner_cli(&cli).expect_err("zero queue");
    assert!(queue_error
        .to_string()
        .contains("--multi-agent-queue-limit must be greater than 0"));

    cli.multi_agent_queue_limit = 1;
    cli.multi_agent_processed_case_cap = 0;
    let processed_error =
        validate_multi_agent_contract_runner_cli(&cli).expect_err("zero processed cap");
    assert!(processed_error
        .to_string()
        .contains("--multi-agent-processed-case-cap must be greater than 0"));

    cli.multi_agent_processed_case_cap = 1;
    cli.multi_agent_retry_max_attempts = 0;
    let retry_error = validate_multi_agent_contract_runner_cli(&cli).expect_err("zero retry max");
    assert!(retry_error
        .to_string()
        .contains("--multi-agent-retry-max-attempts must be greater than 0"));
}

#[test]
fn regression_validate_multi_agent_contract_runner_cli_requires_existing_fixture() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.multi_agent_contract_runner = true;
    cli.multi_agent_fixture = temp.path().join("missing.json");

    let error =
        validate_multi_agent_contract_runner_cli(&cli).expect_err("missing fixture should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_multi_agent_contract_runner_cli_requires_fixture_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.multi_agent_contract_runner = true;
    cli.multi_agent_fixture = temp.path().to_path_buf();

    let error =
        validate_multi_agent_contract_runner_cli(&cli).expect_err("directory fixture should fail");
    assert!(error.to_string().contains("must point to a file"));
}

#[test]
fn unit_validate_memory_contract_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("memory-fixture.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "name": "single-case",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "extract-basic",
      "mode": "extract",
      "scope": { "workspace_id": "tau-core" },
      "input_text": "Remember release checklist",
      "expected": {
        "outcome": "success",
        "entries": [
          {
            "memory_id": "mem-extract-basic",
            "summary": "Remember release checklist",
            "tags": [ "remember", "release", "checklist" ],
            "facts": [ "scope=tau-core" ],
            "source_event_key": "tau-core:extract:extract-basic",
            "recency_weight_bps": 9000,
            "confidence_bps": 8200
          }
        ]
      }
    }
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = test_cli();
    cli.memory_contract_runner = true;
    cli.memory_fixture = fixture_path;

    validate_memory_contract_runner_cli(&cli).expect("memory runner config should validate");
}

#[test]
fn functional_validate_memory_contract_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.memory_contract_runner = true;
    cli.memory_fixture = fixture_path;
    cli.prompt = Some("conflict".to_string());

    let error = validate_memory_contract_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--memory-contract-runner cannot be combined"));
}

#[test]
fn integration_validate_memory_contract_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.memory_contract_runner = true;
    cli.memory_fixture = fixture_path;
    cli.multi_channel_contract_runner = true;

    let error = validate_memory_contract_runner_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, or --multi-channel-live-runner"
    ));
}

#[test]
fn regression_validate_memory_contract_runner_cli_rejects_zero_limits() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.memory_contract_runner = true;
    cli.memory_fixture = fixture_path.clone();
    cli.memory_queue_limit = 0;
    let queue_error = validate_memory_contract_runner_cli(&cli).expect_err("zero queue limit");
    assert!(queue_error
        .to_string()
        .contains("--memory-queue-limit must be greater than 0"));

    cli.memory_queue_limit = 1;
    cli.memory_processed_case_cap = 0;
    let processed_case_error =
        validate_memory_contract_runner_cli(&cli).expect_err("zero processed case cap");
    assert!(processed_case_error
        .to_string()
        .contains("--memory-processed-case-cap must be greater than 0"));

    cli.memory_processed_case_cap = 1;
    cli.memory_retry_max_attempts = 0;
    let retry_error =
        validate_memory_contract_runner_cli(&cli).expect_err("zero retry max attempts");
    assert!(retry_error
        .to_string()
        .contains("--memory-retry-max-attempts must be greater than 0"));
}

#[test]
fn regression_validate_memory_contract_runner_cli_requires_existing_fixture() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.memory_contract_runner = true;
    cli.memory_fixture = temp.path().join("missing.json");

    let error = validate_memory_contract_runner_cli(&cli).expect_err("missing fixture should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_memory_contract_runner_cli_requires_fixture_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.memory_contract_runner = true;
    cli.memory_fixture = temp.path().to_path_buf();

    let error =
        validate_memory_contract_runner_cli(&cli).expect_err("directory fixture should fail");
    assert!(error.to_string().contains("must point to a file"));
}

#[test]
fn unit_validate_dashboard_contract_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("dashboard-fixture.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "name": "single-case",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "snapshot-basic",
      "mode": "snapshot",
      "scope": { "workspace_id": "tau-core", "operator_id": "ops-1" },
      "requested_widgets": [
        {
          "widget_id": "health-summary",
          "kind": "health_summary",
          "title": "Health Summary",
          "query_key": "health.summary",
          "refresh_interval_ms": 15000
        }
      ],
      "expected": {
        "outcome": "success",
        "widgets": [
          {
            "widget_id": "health-summary",
            "kind": "health_summary",
            "title": "Health Summary",
            "query_key": "health.summary",
            "refresh_interval_ms": 15000
          }
        ]
      }
    }
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = test_cli();
    cli.dashboard_contract_runner = true;
    cli.dashboard_fixture = fixture_path;

    validate_dashboard_contract_runner_cli(&cli).expect("dashboard runner config should validate");
}

#[test]
fn functional_validate_dashboard_contract_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.dashboard_contract_runner = true;
    cli.dashboard_fixture = fixture_path;
    cli.prompt = Some("conflict".to_string());

    let error = validate_dashboard_contract_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--dashboard-contract-runner cannot be combined"));
}

#[test]
fn integration_validate_dashboard_contract_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.dashboard_contract_runner = true;
    cli.dashboard_fixture = fixture_path;
    cli.memory_contract_runner = true;

    let error = validate_dashboard_contract_runner_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, or --memory-contract-runner"
    ));
}

#[test]
fn regression_validate_dashboard_contract_runner_cli_rejects_zero_limits() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.dashboard_contract_runner = true;
    cli.dashboard_fixture = fixture_path.clone();
    cli.dashboard_queue_limit = 0;
    let queue_error = validate_dashboard_contract_runner_cli(&cli).expect_err("zero queue limit");
    assert!(queue_error
        .to_string()
        .contains("--dashboard-queue-limit must be greater than 0"));

    cli.dashboard_queue_limit = 1;
    cli.dashboard_processed_case_cap = 0;
    let processed_case_error =
        validate_dashboard_contract_runner_cli(&cli).expect_err("zero processed case cap");
    assert!(processed_case_error
        .to_string()
        .contains("--dashboard-processed-case-cap must be greater than 0"));

    cli.dashboard_processed_case_cap = 1;
    cli.dashboard_retry_max_attempts = 0;
    let retry_error =
        validate_dashboard_contract_runner_cli(&cli).expect_err("zero retry max attempts");
    assert!(retry_error
        .to_string()
        .contains("--dashboard-retry-max-attempts must be greater than 0"));
}

#[test]
fn regression_validate_dashboard_contract_runner_cli_requires_existing_fixture() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.dashboard_contract_runner = true;
    cli.dashboard_fixture = temp.path().join("missing.json");

    let error =
        validate_dashboard_contract_runner_cli(&cli).expect_err("missing fixture should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_dashboard_contract_runner_cli_requires_fixture_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.dashboard_contract_runner = true;
    cli.dashboard_fixture = temp.path().to_path_buf();

    let error =
        validate_dashboard_contract_runner_cli(&cli).expect_err("directory fixture should fail");
    assert!(error.to_string().contains("must point to a file"));
}

#[test]
fn unit_validate_gateway_service_cli_accepts_status_mode() {
    let mut cli = test_cli();
    cli.gateway_service_status = true;
    cli.gateway_service_status_json = true;

    validate_gateway_service_cli(&cli).expect("gateway service status config should validate");
}

#[test]
fn functional_validate_gateway_service_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.gateway_service_start = true;
    cli.prompt = Some("conflict".to_string());

    let error = validate_gateway_service_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--gateway-service-* commands cannot be combined"));
}

#[test]
fn integration_validate_gateway_service_cli_rejects_transport_conflicts() {
    let mut cli = test_cli();
    cli.gateway_service_stop = true;
    cli.gateway_contract_runner = true;

    let error = validate_gateway_service_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--gateway-service-* commands cannot be combined with active transport runtime flags"
    ));
}

#[test]
fn regression_validate_gateway_service_cli_rejects_whitespace_stop_reason() {
    let mut cli = test_cli();
    cli.gateway_service_stop = true;
    cli.gateway_service_stop_reason = Some("   ".to_string());

    let error = validate_gateway_service_cli(&cli).expect_err("whitespace stop reason should fail");
    assert!(error
        .to_string()
        .contains("--gateway-service-stop-reason cannot be empty or whitespace"));
}

#[test]
fn unit_validate_daemon_cli_accepts_status_mode() {
    let mut cli = test_cli();
    cli.daemon_status = true;
    cli.daemon_status_json = true;

    validate_daemon_cli(&cli).expect("daemon status config should validate");
}

#[test]
fn functional_validate_daemon_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.daemon_install = true;
    cli.prompt = Some("conflict".to_string());

    let error = validate_daemon_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--daemon-* commands cannot be combined"));
}

#[test]
fn integration_validate_daemon_cli_rejects_transport_conflicts() {
    let mut cli = test_cli();
    cli.daemon_start = true;
    cli.gateway_contract_runner = true;

    let error = validate_daemon_cli(&cli).expect_err("transport conflict");
    assert!(error
        .to_string()
        .contains("--daemon-* commands cannot be combined with active transport/runtime flags"));
}

#[test]
fn integration_validate_daemon_cli_rejects_status_preflight_conflicts() {
    let mut cli = test_cli();
    cli.daemon_status = true;
    cli.gateway_status_inspect = true;

    let error = validate_daemon_cli(&cli).expect_err("status conflict");
    assert!(error.to_string().contains(
        "--daemon-* commands cannot be combined with status/inspection preflight commands"
    ));
}

#[test]
fn regression_validate_daemon_cli_rejects_whitespace_stop_reason() {
    let mut cli = test_cli();
    cli.daemon_stop = true;
    cli.daemon_stop_reason = Some("   ".to_string());

    let error = validate_daemon_cli(&cli).expect_err("whitespace stop reason should fail");
    assert!(error
        .to_string()
        .contains("--daemon-stop-reason cannot be empty or whitespace"));
}

#[test]
fn unit_validate_gateway_remote_profile_inspect_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.gateway_remote_profile_inspect = true;

    validate_gateway_remote_profile_inspect_cli(&cli)
        .expect("gateway remote profile inspect config should validate");
}

#[test]
fn functional_validate_gateway_remote_profile_inspect_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.gateway_remote_profile_inspect = true;
    cli.prompt = Some("conflict".to_string());

    let error =
        validate_gateway_remote_profile_inspect_cli(&cli).expect_err("prompt conflict should fail");
    assert!(error
        .to_string()
        .contains("--gateway-remote-profile-inspect cannot be combined"));
}

#[test]
fn unit_validate_gateway_remote_plan_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.gateway_remote_plan = true;

    validate_gateway_remote_plan_cli(&cli).expect("gateway remote plan config should validate");
}

#[test]
fn functional_validate_gateway_remote_plan_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.gateway_remote_plan = true;
    cli.prompt = Some("conflict".to_string());

    let error = validate_gateway_remote_plan_cli(&cli).expect_err("prompt conflict should fail");
    assert!(error
        .to_string()
        .contains("--gateway-remote-plan cannot be combined"));
}

#[test]
fn regression_validate_gateway_remote_plan_cli_rejects_hold_profile_configuration() {
    let mut cli = test_cli();
    cli.gateway_remote_plan = true;
    cli.gateway_openresponses_server = true;
    cli.gateway_remote_profile = CliGatewayRemoteProfile::TailscaleFunnel;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::PasswordSession;
    cli.gateway_openresponses_auth_password = None;
    cli.gateway_openresponses_bind = "127.0.0.1:8787".to_string();

    let error = validate_gateway_remote_plan_cli(&cli)
        .expect_err("invalid selected profile posture should fail closed");
    assert!(error
        .to_string()
        .contains("gateway remote plan rejected: profile=tailscale-funnel gate=hold"));
    assert!(error
        .to_string()
        .contains("tailscale_funnel_missing_password"));
}

#[test]
fn unit_validate_gateway_openresponses_server_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("secret-token".to_string());

    validate_gateway_openresponses_server_cli(&cli)
        .expect("gateway openresponses server config should validate");
}

#[test]
fn functional_validate_gateway_openresponses_server_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("secret-token".to_string());
    cli.prompt = Some("conflict".to_string());

    let error =
        validate_gateway_openresponses_server_cli(&cli).expect_err("prompt conflict should fail");
    assert!(error
        .to_string()
        .contains("--gateway-openresponses-server cannot be combined"));
}

#[test]
fn integration_validate_gateway_openresponses_server_cli_rejects_transport_conflicts() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("secret-token".to_string());
    cli.github_issues_bridge = true;

    let error = validate_gateway_openresponses_server_cli(&cli)
        .expect_err("transport conflict should fail");
    assert!(error.to_string().contains(
        "--gateway-openresponses-server cannot be combined with gateway service/daemon commands or other active transport runtime flags"
    ));
}

#[test]
fn regression_validate_gateway_openresponses_server_cli_requires_auth_token() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = None;

    let error = validate_gateway_openresponses_server_cli(&cli)
        .expect_err("missing auth token should fail");
    assert!(error.to_string().contains(
        "--gateway-openresponses-auth-token is required when --gateway-openresponses-auth-mode=token"
    ));
}

#[test]
fn regression_validate_gateway_openresponses_server_cli_rejects_whitespace_auth_token() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("   ".to_string());

    let error = validate_gateway_openresponses_server_cli(&cli)
        .expect_err("whitespace auth token should fail");
    assert!(error.to_string().contains(
        "--gateway-openresponses-auth-token is required when --gateway-openresponses-auth-mode=token"
    ));
}

#[test]
fn unit_validate_gateway_openresponses_server_cli_accepts_password_session_configuration() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::PasswordSession;
    cli.gateway_openresponses_auth_password = Some("pw-secret".to_string());

    validate_gateway_openresponses_server_cli(&cli)
        .expect("password-session config should validate");
}

#[test]
fn regression_validate_gateway_openresponses_server_cli_requires_password_for_password_mode() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::PasswordSession;
    cli.gateway_openresponses_auth_password = None;

    let error =
        validate_gateway_openresponses_server_cli(&cli).expect_err("missing password should fail");
    assert!(error.to_string().contains(
        "--gateway-openresponses-auth-password is required when --gateway-openresponses-auth-mode=password-session"
    ));
}

#[test]
fn regression_validate_gateway_openresponses_server_cli_rejects_non_loopback_localhost_dev() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::LocalhostDev;
    cli.gateway_openresponses_bind = "0.0.0.0:8787".to_string();

    let error =
        validate_gateway_openresponses_server_cli(&cli).expect_err("non-loopback bind should fail");
    assert!(error.to_string().contains(
        "--gateway-openresponses-auth-mode=localhost-dev requires loopback bind address"
    ));
}

#[test]
fn integration_validate_gateway_openresponses_server_cli_rejects_unsafe_local_only_remote_combo() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_remote_profile = CliGatewayRemoteProfile::LocalOnly;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("secret-token".to_string());
    cli.gateway_openresponses_bind = "0.0.0.0:8787".to_string();

    let error = validate_gateway_openresponses_server_cli(&cli)
        .expect_err("non-loopback local-only profile should fail");
    assert!(error
        .to_string()
        .contains("gateway remote profile rejected"));
    assert!(error.to_string().contains("local_only_non_loopback_bind"));
}

#[test]
fn integration_validate_gateway_openresponses_server_cli_accepts_tailscale_funnel_profile() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_remote_profile = CliGatewayRemoteProfile::TailscaleFunnel;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::PasswordSession;
    cli.gateway_openresponses_auth_password = Some("edge-password".to_string());
    cli.gateway_openresponses_bind = "127.0.0.1:8787".to_string();

    validate_gateway_openresponses_server_cli(&cli)
        .expect("tailscale funnel profile with password-session auth should pass");
}

#[test]
fn regression_validate_gateway_openresponses_server_cli_rejects_tailscale_serve_localhost_dev_auth()
{
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_remote_profile = CliGatewayRemoteProfile::TailscaleServe;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::LocalhostDev;
    cli.gateway_openresponses_bind = "127.0.0.1:8787".to_string();

    let error = validate_gateway_openresponses_server_cli(&cli)
        .expect_err("tailscale-serve should reject localhost-dev auth");
    assert!(error
        .to_string()
        .contains("tailscale_serve_localhost_dev_auth_unsupported"));
}

#[test]
fn regression_validate_gateway_openresponses_server_cli_rejects_zero_max_input_chars() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("secret-token".to_string());
    cli.gateway_openresponses_max_input_chars = 0;

    let error = validate_gateway_openresponses_server_cli(&cli)
        .expect_err("zero max input chars should fail");
    assert!(error
        .to_string()
        .contains("--gateway-openresponses-max-input-chars must be greater than 0"));
}

#[test]
fn regression_validate_gateway_openresponses_server_cli_rejects_invalid_bind() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("secret-token".to_string());
    cli.gateway_openresponses_bind = "invalid-bind".to_string();

    let error =
        validate_gateway_openresponses_server_cli(&cli).expect_err("invalid bind should fail");
    assert!(error
        .to_string()
        .contains("invalid gateway socket address 'invalid-bind'"));
}

#[test]
fn unit_validate_gateway_contract_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("gateway-fixture.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "name": "single-case",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "gateway-success-only",
      "method": "GET",
      "endpoint": "/v1/health",
      "actor_id": "ops-bot",
      "body": {},
      "expected": {
        "outcome": "success",
        "status_code": 200,
        "response_body": {
          "status":"accepted",
          "method":"GET",
          "endpoint":"/v1/health",
          "actor_id":"ops-bot"
        }
      }
    }
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = test_cli();
    cli.gateway_contract_runner = true;
    cli.gateway_fixture = fixture_path;

    validate_gateway_contract_runner_cli(&cli).expect("gateway runner config should validate");
}

#[test]
fn functional_validate_gateway_contract_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.gateway_contract_runner = true;
    cli.gateway_fixture = fixture_path;
    cli.prompt = Some("conflict".to_string());

    let error = validate_gateway_contract_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--gateway-contract-runner cannot be combined"));
}

#[test]
fn integration_validate_gateway_contract_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.gateway_contract_runner = true;
    cli.gateway_fixture = fixture_path;
    cli.multi_agent_contract_runner = true;

    let error = validate_gateway_contract_runner_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, --multi-agent-contract-runner, --memory-contract-runner, or --dashboard-contract-runner"
    ));
}

#[test]
fn regression_validate_gateway_contract_runner_cli_rejects_zero_guardrail_thresholds() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.gateway_contract_runner = true;
    cli.gateway_fixture = fixture_path.clone();
    cli.gateway_guardrail_failure_streak_threshold = 0;

    let failure_streak_error = validate_gateway_contract_runner_cli(&cli)
        .expect_err("zero failure-streak threshold should fail");
    assert!(failure_streak_error
        .to_string()
        .contains("--gateway-guardrail-failure-streak-threshold must be greater than 0"));

    cli.gateway_guardrail_failure_streak_threshold = 1;
    cli.gateway_guardrail_retryable_failures_threshold = 0;

    let retryable_error = validate_gateway_contract_runner_cli(&cli)
        .expect_err("zero retryable-failures threshold should fail");
    assert!(retryable_error
        .to_string()
        .contains("--gateway-guardrail-retryable-failures-threshold must be greater than 0"));
}

#[test]
fn regression_validate_gateway_contract_runner_cli_requires_existing_fixture() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.gateway_contract_runner = true;
    cli.gateway_fixture = temp.path().join("missing.json");

    let error =
        validate_gateway_contract_runner_cli(&cli).expect_err("missing fixture should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_gateway_contract_runner_cli_requires_fixture_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.gateway_contract_runner = true;
    cli.gateway_fixture = temp.path().to_path_buf();

    let error =
        validate_gateway_contract_runner_cli(&cli).expect_err("directory fixture should fail");
    assert!(error.to_string().contains("must point to a file"));
}

#[test]
fn unit_validate_deployment_contract_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("deployment-fixture.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "name": "single-case",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "deployment-success-only",
      "deploy_target": "container",
      "runtime_profile": "native",
      "blueprint_id": "staging-container",
      "environment": "staging",
      "region": "us-west-2",
      "container_image": "ghcr.io/njfio/tau:staging",
      "replicas": 1,
      "expected": {
        "outcome": "success",
        "status_code": 202,
        "response_body": {
          "status":"accepted",
          "blueprint_id":"staging-container",
          "deploy_target":"container",
          "runtime_profile":"native",
          "environment":"staging",
          "region":"us-west-2",
          "artifact":"ghcr.io/njfio/tau:staging",
          "replicas":1,
          "rollout_strategy":"recreate"
        }
      }
    }
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = test_cli();
    cli.deployment_contract_runner = true;
    cli.deployment_fixture = fixture_path;

    validate_deployment_contract_runner_cli(&cli)
        .expect("deployment runner config should validate");
}

#[test]
fn functional_validate_deployment_contract_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.deployment_contract_runner = true;
    cli.deployment_fixture = fixture_path;
    cli.prompt = Some("conflict".to_string());

    let error = validate_deployment_contract_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--deployment-contract-runner cannot be combined"));
}

#[test]
fn integration_validate_deployment_contract_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.deployment_contract_runner = true;
    cli.deployment_fixture = fixture_path;
    cli.voice_contract_runner = true;

    let error = validate_deployment_contract_runner_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, --multi-agent-contract-runner, --memory-contract-runner, --dashboard-contract-runner, --gateway-contract-runner, --custom-command-contract-runner, or --voice-contract-runner"
    ));
}

#[test]
fn regression_validate_deployment_contract_runner_cli_rejects_zero_limits() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.deployment_contract_runner = true;
    cli.deployment_fixture = fixture_path.clone();
    cli.deployment_queue_limit = 0;
    let queue_error = validate_deployment_contract_runner_cli(&cli).expect_err("zero queue limit");
    assert!(queue_error
        .to_string()
        .contains("--deployment-queue-limit must be greater than 0"));

    cli.deployment_queue_limit = 1;
    cli.deployment_processed_case_cap = 0;
    let processed_error =
        validate_deployment_contract_runner_cli(&cli).expect_err("zero processed case cap");
    assert!(processed_error
        .to_string()
        .contains("--deployment-processed-case-cap must be greater than 0"));

    cli.deployment_processed_case_cap = 1;
    cli.deployment_retry_max_attempts = 0;
    let retry_error =
        validate_deployment_contract_runner_cli(&cli).expect_err("zero retry max attempts");
    assert!(retry_error
        .to_string()
        .contains("--deployment-retry-max-attempts must be greater than 0"));
}

#[test]
fn regression_validate_deployment_contract_runner_cli_requires_existing_fixture() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.deployment_contract_runner = true;
    cli.deployment_fixture = temp.path().join("missing.json");

    let error =
        validate_deployment_contract_runner_cli(&cli).expect_err("missing fixture should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_deployment_contract_runner_cli_requires_fixture_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.deployment_contract_runner = true;
    cli.deployment_fixture = temp.path().to_path_buf();

    let error =
        validate_deployment_contract_runner_cli(&cli).expect_err("directory fixture should fail");
    assert!(error.to_string().contains("must point to a file"));
}

#[test]
fn unit_validate_deployment_wasm_package_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let module_path = temp.path().join("edge.wasm");
    std::fs::write(
        &module_path,
        [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
    )
    .expect("write wasm");

    let mut cli = test_cli();
    cli.deployment_wasm_package_module = Some(module_path);
    cli.deployment_wasm_package_output_dir = temp.path().join("out");
    cli.deployment_state_dir = temp.path().join(".tau/deployment");

    validate_deployment_wasm_package_cli(&cli)
        .expect("deployment wasm package config should validate");
}

#[test]
fn functional_validate_deployment_wasm_package_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let module_path = temp.path().join("edge.wasm");
    std::fs::write(
        &module_path,
        [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
    )
    .expect("write wasm");

    let mut cli = test_cli();
    cli.deployment_wasm_package_module = Some(module_path);
    cli.prompt = Some("conflict".to_string());
    let error =
        validate_deployment_wasm_package_cli(&cli).expect_err("prompt conflict should fail");
    assert!(error
        .to_string()
        .contains("--deployment-wasm-package-module cannot be combined"));
}

#[test]
fn integration_validate_deployment_wasm_package_cli_rejects_runtime_conflicts() {
    let temp = tempdir().expect("tempdir");
    let module_path = temp.path().join("edge.wasm");
    std::fs::write(
        &module_path,
        [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
    )
    .expect("write wasm");

    let mut cli = test_cli();
    cli.deployment_wasm_package_module = Some(module_path);
    cli.events_runner = true;
    let error =
        validate_deployment_wasm_package_cli(&cli).expect_err("runtime conflict should fail");
    assert!(error
        .to_string()
        .contains("active transport/runtime commands"));
}

#[test]
fn regression_validate_deployment_wasm_package_cli_requires_existing_module() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.deployment_wasm_package_module = Some(temp.path().join("missing.wasm"));
    let error = validate_deployment_wasm_package_cli(&cli).expect_err("missing module should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_deployment_wasm_package_cli_rejects_non_directory_output() {
    let temp = tempdir().expect("tempdir");
    let module_path = temp.path().join("edge.wasm");
    std::fs::write(
        &module_path,
        [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
    )
    .expect("write wasm");
    let output_file = temp.path().join("out-file");
    std::fs::write(&output_file, "{}").expect("write output file");

    let mut cli = test_cli();
    cli.deployment_wasm_package_module = Some(module_path);
    cli.deployment_wasm_package_output_dir = output_file;
    let error =
        validate_deployment_wasm_package_cli(&cli).expect_err("output file path should fail");
    assert!(error
        .to_string()
        .contains("--deployment-wasm-package-output-dir"));
}

#[test]
fn unit_validate_deployment_wasm_inspect_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("edge.manifest.json");
    std::fs::write(&manifest_path, "{}").expect("write manifest placeholder");

    let mut cli = test_cli();
    cli.deployment_wasm_inspect_manifest = Some(manifest_path);

    validate_deployment_wasm_inspect_cli(&cli)
        .expect("deployment wasm inspect config should validate");
}

#[test]
fn functional_validate_deployment_wasm_inspect_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("edge.manifest.json");
    std::fs::write(&manifest_path, "{}").expect("write manifest placeholder");

    let mut cli = test_cli();
    cli.deployment_wasm_inspect_manifest = Some(manifest_path);
    cli.prompt = Some("conflict".to_string());
    let error =
        validate_deployment_wasm_inspect_cli(&cli).expect_err("prompt conflict should fail");
    assert!(error
        .to_string()
        .contains("--deployment-wasm-inspect-manifest cannot be combined"));
}

#[test]
fn integration_validate_deployment_wasm_inspect_cli_rejects_runtime_conflicts() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("edge.manifest.json");
    std::fs::write(&manifest_path, "{}").expect("write manifest placeholder");

    let mut cli = test_cli();
    cli.deployment_wasm_inspect_manifest = Some(manifest_path);
    cli.events_runner = true;
    let error =
        validate_deployment_wasm_inspect_cli(&cli).expect_err("runtime conflict should fail");
    assert!(error
        .to_string()
        .contains("active transport/runtime commands"));
}

#[test]
fn regression_validate_deployment_wasm_inspect_cli_requires_existing_manifest() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.deployment_wasm_inspect_manifest = Some(temp.path().join("missing.manifest.json"));
    let error =
        validate_deployment_wasm_inspect_cli(&cli).expect_err("missing manifest should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_deployment_wasm_inspect_cli_rejects_directory_manifest_path() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.deployment_wasm_inspect_manifest = Some(temp.path().to_path_buf());
    let error = validate_deployment_wasm_inspect_cli(&cli)
        .expect_err("directory manifest path should fail");
    assert!(error.to_string().contains("must point to a file"));
}

#[test]
fn unit_validate_custom_command_contract_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("custom-command-fixture.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "name": "single-case",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "create-command",
      "operation": "create",
      "command_name": "deploy_release",
      "template": "deploy {{env}}",
      "arguments": { "env": "prod" },
      "expected": {
        "outcome": "success",
        "status_code": 201,
        "response_body": {
          "status": "accepted",
          "operation": "create",
          "command_name": "deploy_release"
        }
      }
    }
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = test_cli();
    cli.custom_command_contract_runner = true;
    cli.custom_command_fixture = fixture_path;

    validate_custom_command_contract_runner_cli(&cli)
        .expect("custom command runner config should validate");
}

#[test]
fn functional_validate_custom_command_contract_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.custom_command_contract_runner = true;
    cli.custom_command_fixture = fixture_path;
    cli.prompt = Some("conflict".to_string());

    let error = validate_custom_command_contract_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--custom-command-contract-runner cannot be combined"));
}

#[test]
fn integration_validate_custom_command_contract_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.custom_command_contract_runner = true;
    cli.custom_command_fixture = fixture_path;
    cli.gateway_contract_runner = true;

    let error = validate_custom_command_contract_runner_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, --multi-agent-contract-runner, --memory-contract-runner, --dashboard-contract-runner, or --gateway-contract-runner"
    ));
}

#[test]
fn regression_validate_custom_command_contract_runner_cli_rejects_zero_limits() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.custom_command_contract_runner = true;
    cli.custom_command_fixture = fixture_path.clone();
    cli.custom_command_queue_limit = 0;
    let queue_error =
        validate_custom_command_contract_runner_cli(&cli).expect_err("zero queue limit");
    assert!(queue_error
        .to_string()
        .contains("--custom-command-queue-limit must be greater than 0"));

    cli.custom_command_queue_limit = 1;
    cli.custom_command_processed_case_cap = 0;
    let processed_error =
        validate_custom_command_contract_runner_cli(&cli).expect_err("zero processed case cap");
    assert!(processed_error
        .to_string()
        .contains("--custom-command-processed-case-cap must be greater than 0"));

    cli.custom_command_processed_case_cap = 1;
    cli.custom_command_retry_max_attempts = 0;
    let retry_error =
        validate_custom_command_contract_runner_cli(&cli).expect_err("zero retry max attempts");
    assert!(retry_error
        .to_string()
        .contains("--custom-command-retry-max-attempts must be greater than 0"));
}

#[test]
fn regression_validate_custom_command_contract_runner_cli_requires_existing_fixture() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.custom_command_contract_runner = true;
    cli.custom_command_fixture = temp.path().join("missing.json");

    let error =
        validate_custom_command_contract_runner_cli(&cli).expect_err("missing fixture should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_custom_command_contract_runner_cli_requires_fixture_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.custom_command_contract_runner = true;
    cli.custom_command_fixture = temp.path().to_path_buf();

    let error = validate_custom_command_contract_runner_cli(&cli)
        .expect_err("directory fixture should fail");
    assert!(error.to_string().contains("must point to a file"));
}

#[test]
fn unit_validate_voice_contract_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("voice-fixture.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "name": "single-case",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "voice-success-only",
      "mode": "turn",
      "wake_word": "tau",
      "transcript": "tau open dashboard",
      "locale": "en-US",
      "speaker_id": "ops",
      "expected": {
        "outcome": "success",
        "status_code": 202,
        "response_body": {
          "status":"accepted",
          "mode":"turn",
          "wake_word":"tau",
          "utterance":"open dashboard",
          "locale":"en-US",
          "speaker_id":"ops"
        }
      }
    }
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = test_cli();
    cli.voice_contract_runner = true;
    cli.voice_fixture = fixture_path;

    validate_voice_contract_runner_cli(&cli).expect("voice runner config should validate");
}

#[test]
fn functional_validate_voice_contract_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.voice_contract_runner = true;
    cli.voice_fixture = fixture_path;
    cli.prompt = Some("conflict".to_string());

    let error = validate_voice_contract_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--voice-contract-runner cannot be combined"));
}

#[test]
fn integration_validate_voice_contract_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.voice_contract_runner = true;
    cli.voice_fixture = fixture_path;
    cli.custom_command_contract_runner = true;

    let error = validate_voice_contract_runner_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, --multi-agent-contract-runner, --memory-contract-runner, --dashboard-contract-runner, --gateway-contract-runner, or --custom-command-contract-runner"
    ));
}

#[test]
fn regression_validate_voice_contract_runner_cli_rejects_zero_limits() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.voice_contract_runner = true;
    cli.voice_fixture = fixture_path.clone();
    cli.voice_queue_limit = 0;
    let queue_error = validate_voice_contract_runner_cli(&cli).expect_err("zero queue limit");
    assert!(queue_error
        .to_string()
        .contains("--voice-queue-limit must be greater than 0"));

    cli.voice_queue_limit = 1;
    cli.voice_processed_case_cap = 0;
    let processed_error =
        validate_voice_contract_runner_cli(&cli).expect_err("zero processed case cap");
    assert!(processed_error
        .to_string()
        .contains("--voice-processed-case-cap must be greater than 0"));

    cli.voice_processed_case_cap = 1;
    cli.voice_retry_max_attempts = 0;
    let retry_error =
        validate_voice_contract_runner_cli(&cli).expect_err("zero retry max attempts");
    assert!(retry_error
        .to_string()
        .contains("--voice-retry-max-attempts must be greater than 0"));
}

#[test]
fn regression_validate_voice_contract_runner_cli_requires_existing_fixture() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.voice_contract_runner = true;
    cli.voice_fixture = temp.path().join("missing.json");

    let error = validate_voice_contract_runner_cli(&cli).expect_err("missing fixture should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_voice_contract_runner_cli_requires_fixture_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.voice_contract_runner = true;
    cli.voice_fixture = temp.path().to_path_buf();

    let error =
        validate_voice_contract_runner_cli(&cli).expect_err("directory fixture should fail");
    assert!(error.to_string().contains("must point to a file"));
}

#[test]
fn regression_validate_event_webhook_ingest_cli_requires_channel() {
    let mut cli = test_cli();
    cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
    cli.event_webhook_channel = None;
    let error = validate_event_webhook_ingest_cli(&cli).expect_err("missing channel");
    assert!(error
        .to_string()
        .contains("--event-webhook-channel is required"));
}

#[test]
fn functional_validate_event_webhook_ingest_cli_requires_signing_arguments_together() {
    let mut cli = test_cli();
    cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
    cli.event_webhook_channel = Some("slack/C123".to_string());
    cli.event_webhook_signature = Some("sha256=abcd".to_string());
    cli.event_webhook_secret = Some("secret".to_string());

    let error = validate_event_webhook_ingest_cli(&cli).expect_err("algorithm should be required");
    assert!(error
        .to_string()
        .contains("--event-webhook-signature-algorithm is required"));
}

#[test]
fn functional_validate_event_webhook_ingest_cli_accepts_secret_id_configuration() {
    let mut cli = test_cli();
    cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
    cli.event_webhook_channel = Some("slack/C123".to_string());
    cli.event_webhook_signature = Some("sha256=abcd".to_string());
    cli.event_webhook_secret_id = Some("event-webhook-secret".to_string());
    cli.event_webhook_signature_algorithm = Some(CliWebhookSignatureAlgorithm::GithubSha256);

    validate_event_webhook_ingest_cli(&cli).expect("webhook config should validate");
}

#[test]
fn regression_validate_event_webhook_ingest_cli_requires_timestamp_for_slack_v0() {
    let mut cli = test_cli();
    cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
    cli.event_webhook_channel = Some("slack/C123".to_string());
    cli.event_webhook_signature = Some("v0=abcd".to_string());
    cli.event_webhook_secret = Some("secret".to_string());
    cli.event_webhook_signature_algorithm = Some(CliWebhookSignatureAlgorithm::SlackV0);
    cli.event_webhook_timestamp = None;

    let error = validate_event_webhook_ingest_cli(&cli).expect_err("timestamp should be required");
    assert!(error
        .to_string()
        .contains("--event-webhook-timestamp is required"));
}

#[test]
fn unit_validate_event_webhook_ingest_cli_accepts_signed_github_configuration() {
    let mut cli = test_cli();
    cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
    cli.event_webhook_channel = Some("github/owner/repo#1".to_string());
    cli.event_webhook_signature = Some("sha256=abcd".to_string());
    cli.event_webhook_secret = Some("secret".to_string());
    cli.event_webhook_signature_algorithm = Some(CliWebhookSignatureAlgorithm::GithubSha256);

    validate_event_webhook_ingest_cli(&cli).expect("signed github webhook config should pass");
}

#[test]
fn functional_execute_channel_store_admin_inspect_succeeds() {
    let temp = tempdir().expect("tempdir");
    let store = crate::channel_store::ChannelStore::open(temp.path(), "github", "issue-1")
        .expect("open channel store");
    store
        .append_log_entry(&crate::channel_store::ChannelLogEntry {
            timestamp_unix_ms: 1,
            direction: "inbound".to_string(),
            event_key: Some("e1".to_string()),
            source: "github".to_string(),
            payload: serde_json::json!({"body":"hello"}),
        })
        .expect("append log");
    store
        .write_text_artifact(
            "run-active",
            "github-reply",
            "private",
            Some(30),
            "md",
            "artifact body",
        )
        .expect("write artifact");
    let mut artifact_index =
        std::fs::read_to_string(store.artifact_index_path()).expect("read artifact index");
    artifact_index.push_str("invalid-artifact-line\n");
    std::fs::write(store.artifact_index_path(), artifact_index).expect("seed invalid artifact");

    let mut cli = test_cli();
    cli.channel_store_root = temp.path().to_path_buf();
    cli.channel_store_inspect = Some("github/issue-1".to_string());

    execute_channel_store_admin_command(&cli).expect("inspect should succeed");
    let report = store.inspect().expect("inspect report");
    assert_eq!(report.artifact_records, 1);
    assert_eq!(report.invalid_artifact_lines, 1);
    assert_eq!(report.active_artifacts, 1);
    assert_eq!(report.expired_artifacts, 0);
}

#[test]
fn regression_execute_channel_store_admin_repair_removes_invalid_lines() {
    let temp = tempdir().expect("tempdir");
    let store = crate::channel_store::ChannelStore::open(temp.path(), "slack", "C123")
        .expect("open channel store");
    std::fs::write(store.log_path(), "{\"ok\":true}\ninvalid-json-line\n")
        .expect("seed invalid log");
    let expired = store
        .write_text_artifact(
            "run-expired",
            "slack-reply",
            "private",
            Some(0),
            "md",
            "expired artifact",
        )
        .expect("write expired artifact");
    let mut artifact_index =
        std::fs::read_to_string(store.artifact_index_path()).expect("read artifact index");
    artifact_index.push_str("invalid-artifact-line\n");
    std::fs::write(store.artifact_index_path(), artifact_index).expect("seed invalid artifact");

    let mut cli = test_cli();
    cli.channel_store_root = temp.path().to_path_buf();
    cli.channel_store_repair = Some("slack/C123".to_string());
    execute_channel_store_admin_command(&cli).expect("repair should succeed");

    let report = store.inspect().expect("inspect after repair");
    assert_eq!(report.invalid_log_lines, 0);
    assert_eq!(report.log_records, 1);
    assert_eq!(report.invalid_artifact_lines, 0);
    assert_eq!(report.expired_artifacts, 0);
    assert_eq!(report.active_artifacts, 0);
    assert!(!store.channel_dir().join(expired.relative_path).exists());
}

#[test]
fn functional_execute_channel_store_admin_github_status_inspect_succeeds() {
    let temp = tempdir().expect("tempdir");
    let github_state_dir = temp.path().join("github");
    let repo_state_dir = github_state_dir.join("owner__repo");
    std::fs::create_dir_all(&repo_state_dir).expect("create github repo dir");
    std::fs::write(
        repo_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "last_issue_scan_at": "2026-01-01T00:00:00Z",
  "processed_event_keys": ["issue-comment-created:1"],
  "issue_sessions": {
    "7": {
      "session_id": "issue-7",
      "last_run_id": "run-7",
      "last_event_key": "issue-comment-created:1",
      "last_event_kind": "issue_comment_created",
      "last_actor_login": "alice",
      "last_reason_code": "command_processed",
      "total_processed_events": 1
    }
  },
  "health": {
    "updated_unix_ms": 700,
    "cycle_duration_ms": 25,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write github state");
    std::fs::write(
        repo_state_dir.join("inbound-events.jsonl"),
        r#"{"kind":"issue_comment_created","event_key":"issue-comment-created:1"}
"#,
    )
    .expect("write inbound");
    std::fs::write(
        repo_state_dir.join("outbound-events.jsonl"),
        r#"{"event_key":"issue-comment-created:1","command":"chat-status","status":"reported","reason_code":"command_processed"}
"#,
    )
    .expect("write outbound");

    let mut cli = test_cli();
    cli.github_status_inspect = Some("owner/repo".to_string());
    cli.github_status_json = true;
    cli.github_state_dir = github_state_dir;

    execute_channel_store_admin_command(&cli).expect("github status inspect should succeed");
}

#[test]
fn regression_execute_channel_store_admin_github_status_inspect_requires_state_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.github_status_inspect = Some("owner/repo".to_string());
    cli.github_state_dir = temp.path().join("github");
    std::fs::create_dir_all(cli.github_state_dir.join("owner__repo")).expect("create repo dir");

    let error = execute_channel_store_admin_command(&cli)
        .expect_err("github status inspect should fail without state file");
    assert!(error.to_string().contains("failed to read"));
    assert!(error.to_string().contains("state.json"));
}

#[test]
fn functional_execute_channel_store_admin_dashboard_status_inspect_succeeds() {
    let temp = tempdir().expect("tempdir");
    let dashboard_state_dir = temp.path().join("dashboard");
    std::fs::create_dir_all(&dashboard_state_dir).expect("create dashboard state dir");
    std::fs::write(
        dashboard_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["snapshot:s1"],
  "widget_views": [{"widget_id":"health-summary"}],
  "control_audit": [{"case_id":"c1"}],
  "health": {
    "updated_unix_ms": 700,
    "cycle_duration_ms": 20,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write dashboard state");
    std::fs::write(
        dashboard_state_dir.join("runtime-events.jsonl"),
        r#"{"reason_codes":["widget_views_updated"],"health_reason":"no recent transport failures observed"}
"#,
    )
    .expect("write dashboard events");

    let mut cli = test_cli();
    cli.dashboard_status_inspect = true;
    cli.dashboard_status_json = true;
    cli.dashboard_state_dir = dashboard_state_dir;
    execute_channel_store_admin_command(&cli).expect("dashboard status inspect should succeed");
}

#[test]
fn regression_execute_channel_store_admin_dashboard_status_inspect_requires_state_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.dashboard_status_inspect = true;
    cli.dashboard_state_dir = temp.path().join("dashboard");
    std::fs::create_dir_all(&cli.dashboard_state_dir).expect("create dashboard dir");

    let error = execute_channel_store_admin_command(&cli)
        .expect_err("dashboard status inspect should fail without state file");
    assert!(error.to_string().contains("failed to read"));
    assert!(error.to_string().contains("state.json"));
}

#[test]
fn functional_execute_channel_store_admin_multi_channel_status_inspect_succeeds() {
    let temp = tempdir().expect("tempdir");
    let multi_channel_state_dir = temp.path().join("multi-channel");
    std::fs::create_dir_all(&multi_channel_state_dir).expect("create multi-channel state dir");
    std::fs::write(
        multi_channel_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_event_keys": ["telegram:tg-1", "discord:dc-1", "whatsapp:wa-1"],
  "health": {
    "updated_unix_ms": 701,
    "cycle_duration_ms": 16,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 3,
    "last_cycle_processed": 3,
    "last_cycle_completed": 3,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write multi-channel state");
    std::fs::write(
        multi_channel_state_dir.join("runtime-events.jsonl"),
        r#"{"reason_codes":["healthy_cycle","events_applied"],"health_reason":"no recent transport failures observed"}
"#,
    )
    .expect("write multi-channel events");

    let mut cli = test_cli();
    cli.multi_channel_status_inspect = true;
    cli.multi_channel_status_json = true;
    cli.multi_channel_state_dir = multi_channel_state_dir;
    execute_channel_store_admin_command(&cli).expect("multi-channel status inspect should succeed");
}

#[test]
fn regression_execute_channel_store_admin_multi_channel_status_inspect_requires_state_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.multi_channel_status_inspect = true;
    cli.multi_channel_state_dir = temp.path().join("multi-channel");
    std::fs::create_dir_all(&cli.multi_channel_state_dir).expect("create multi-channel dir");

    let error = execute_channel_store_admin_command(&cli)
        .expect_err("multi-channel status inspect should fail without state file");
    assert!(error.to_string().contains("failed to read"));
    assert!(error.to_string().contains("state.json"));
}

#[test]
fn functional_build_multi_channel_route_inspect_report_resolves_binding_and_role() {
    let temp = tempdir().expect("tempdir");
    let route_table_path = temp.path().join("route-table.json");
    let state_dir = temp.path().join("multi-channel");
    let security_dir = state_dir.join("security");
    std::fs::create_dir_all(&security_dir).expect("create security dir");
    std::fs::write(
        route_table_path.as_path(),
        r#"{
  "schema_version": 1,
  "roles": {
    "triage": {},
    "default": {}
  },
  "planner": { "role": "default" },
  "delegated": { "role": "default" },
  "delegated_categories": {
    "incident": { "role": "triage" }
  },
  "review": { "role": "default" }
}"#,
    )
    .expect("write route table");
    std::fs::write(
        security_dir.join("multi-channel-route-bindings.json"),
        r#"{
  "schema_version": 1,
  "bindings": [
    {
      "binding_id": "discord-ops",
      "transport": "discord",
      "account_id": "discord-main",
      "conversation_id": "ops-room",
      "actor_id": "*",
      "phase": "delegated_step",
      "category_hint": "incident",
      "session_key_template": "session-{role}"
    }
  ]
}"#,
    )
    .expect("write route bindings");
    let event_path = temp.path().join("event.json");
    std::fs::write(
        &event_path,
        r#"{
  "schema_version": 1,
  "transport": "discord",
  "event_kind": "message",
  "event_id": "dc-route-1",
  "conversation_id": "ops-room",
  "actor_id": "discord-user-1",
  "timestamp_ms": 1760200000000,
  "text": "please triage this incident",
  "metadata": {
    "account_id": "discord-main"
  }
}"#,
    )
    .expect("write event file");

    let mut cli = test_cli();
    cli.multi_channel_route_inspect_file = Some(event_path.clone());
    cli.multi_channel_state_dir = state_dir;
    cli.orchestrator_route_table = Some(route_table_path);
    let report = build_multi_channel_route_inspect_report(
        &tau_multi_channel::MultiChannelRouteInspectConfig {
            inspect_file: event_path.clone(),
            state_dir: cli.multi_channel_state_dir.clone(),
            orchestrator_route_table_path: cli.orchestrator_route_table.clone(),
        },
    )
    .expect("build report");
    assert_eq!(report.binding_id, "discord-ops");
    assert_eq!(report.selected_role, "triage");
    assert_eq!(report.phase, "delegated-step");
    assert_eq!(report.session_key, "session-triage");
}

#[test]
fn integration_execute_multi_channel_route_inspect_command_accepts_live_envelope_input() {
    let temp = tempdir().expect("tempdir");
    let state_dir = temp.path().join("multi-channel");
    std::fs::create_dir_all(state_dir.join("security")).expect("create security dir");
    let envelope_path = temp.path().join("telegram-envelope.json");
    std::fs::write(
        &envelope_path,
        r#"{
  "schema_version": 1,
  "transport": "telegram",
  "provider": "telegram-bot-api",
  "payload": {
    "update_id": 70001,
    "message": {
      "message_id": 501,
      "date": 1760200100,
      "text": "/status",
      "chat": { "id": "chat-100" },
      "from": { "id": "user-100", "username": "ops-user" }
    }
  }
}"#,
    )
    .expect("write envelope file");

    let mut cli = test_cli();
    cli.multi_channel_route_inspect_file = Some(envelope_path.clone());
    cli.multi_channel_route_inspect_json = true;
    cli.multi_channel_state_dir = state_dir;
    let report = build_multi_channel_route_inspect_report(
        &tau_multi_channel::MultiChannelRouteInspectConfig {
            inspect_file: envelope_path.clone(),
            state_dir: cli.multi_channel_state_dir.clone(),
            orchestrator_route_table_path: cli.orchestrator_route_table.clone(),
        },
    )
    .expect("build report");
    serde_json::to_string_pretty(&report).expect("render report");
}

#[test]
fn regression_build_multi_channel_route_inspect_report_rejects_empty_file() {
    let temp = tempdir().expect("tempdir");
    let event_path = temp.path().join("empty.json");
    std::fs::write(&event_path, "  \n").expect("write empty event file");

    let mut cli = test_cli();
    cli.multi_channel_route_inspect_file = Some(event_path.clone());
    let error = build_multi_channel_route_inspect_report(
        &tau_multi_channel::MultiChannelRouteInspectConfig {
            inspect_file: event_path.clone(),
            state_dir: cli.multi_channel_state_dir.clone(),
            orchestrator_route_table_path: cli.orchestrator_route_table.clone(),
        },
    )
    .expect_err("empty route inspect file should fail");
    assert!(error.to_string().contains("is empty"));
    assert!(error
        .to_string()
        .contains(event_path.to_string_lossy().as_ref()));
}

#[test]
fn unit_build_multi_channel_incident_timeline_report_aggregates_outcomes_and_reason_codes() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.multi_channel_incident_timeline = true;

    let channel_dir = cli
        .multi_channel_state_dir
        .join("channel-store/channels/discord/ops-room");
    std::fs::create_dir_all(&channel_dir).expect("create channel dir");
    std::fs::write(
        channel_dir.join("log.jsonl"),
        r#"{"timestamp_unix_ms":1760200000000,"direction":"inbound","event_key":"evt-allow","source":"discord","payload":{"transport":"discord","conversation_id":"ops-room","route_session_key":"ops-room","route":{"binding_id":"discord-ops","binding_matched":true},"channel_policy":{"reason_code":"allow_channel_policy_allow_from_any"}}}
{"timestamp_unix_ms":1760200000010,"direction":"outbound","event_key":"evt-allow","source":"tau-multi-channel-runner","payload":{"event_key":"evt-allow","response":"ok","delivery":{"mode":"dry_run","receipts":[{"status":"dry_run"}]}}}
{"timestamp_unix_ms":1760200000020,"direction":"inbound","event_key":"evt-denied","source":"discord","payload":{"transport":"discord","conversation_id":"ops-room","route_session_key":"ops-room","route":{"binding_id":"discord-ops","binding_matched":true},"channel_policy":{"reason_code":"deny_channel_policy_mention_required"}}}
{"timestamp_unix_ms":1760200000030,"direction":"outbound","event_key":"evt-denied","source":"tau-multi-channel-runner","payload":{"status":"denied","reason_code":"deny_channel_policy_mention_required"}}
{"timestamp_unix_ms":1760200000040,"direction":"inbound","event_key":"evt-retried","source":"discord","payload":{"transport":"discord","conversation_id":"ops-room","route_session_key":"ops-room","route":{"binding_id":"discord-ops","binding_matched":true},"channel_policy":{"reason_code":"allow_channel_policy_allow_from_any"}}}
{"timestamp_unix_ms":1760200000050,"direction":"outbound","event_key":"evt-retried","source":"tau-multi-channel-runner","payload":{"status":"delivery_failed","reason_code":"delivery_provider_unavailable","retryable":true}}
{"timestamp_unix_ms":1760200000060,"direction":"outbound","event_key":"evt-retried","source":"tau-multi-channel-runner","payload":{"event_key":"evt-retried","response":"after retry","delivery":{"mode":"provider","receipts":[{"status":"sent"}]}}}
{"timestamp_unix_ms":1760200000070,"direction":"inbound","event_key":"evt-failed","source":"discord","payload":{"transport":"discord","conversation_id":"ops-room","route_session_key":"ops-room","route":{"binding_id":"discord-ops","binding_matched":true},"channel_policy":{"reason_code":"allow_channel_policy_allow_from_any"}}}
{"timestamp_unix_ms":1760200000080,"direction":"outbound","event_key":"evt-failed","source":"tau-multi-channel-runner","payload":{"status":"delivery_failed","reason_code":"delivery_request_rejected","retryable":false}}
"#,
    )
    .expect("write channel log");

    let report = build_multi_channel_incident_timeline_report(
        &tau_multi_channel::MultiChannelIncidentTimelineQuery {
            state_dir: cli.multi_channel_state_dir.clone(),
            window_start_unix_ms: cli.multi_channel_incident_start_unix_ms,
            window_end_unix_ms: cli.multi_channel_incident_end_unix_ms,
            event_limit: cli.multi_channel_incident_event_limit.unwrap_or(200),
            replay_export_path: cli.multi_channel_incident_replay_export.clone(),
        },
    )
    .expect("build incident timeline report");
    assert_eq!(report.timeline.len(), 4);
    assert_eq!(report.outcomes.allowed, 1);
    assert_eq!(report.outcomes.denied, 1);
    assert_eq!(report.outcomes.retried, 1);
    assert_eq!(report.outcomes.failed, 1);
    assert_eq!(
        report
            .policy_reason_code_counts
            .get("allow_channel_policy_allow_from_any")
            .copied(),
        Some(3)
    );
    assert_eq!(
        report
            .policy_reason_code_counts
            .get("deny_channel_policy_mention_required")
            .copied(),
        Some(1)
    );
    assert_eq!(
        report
            .delivery_reason_code_counts
            .get("delivery_provider_unavailable")
            .copied(),
        Some(1)
    );
    assert_eq!(
        report
            .delivery_reason_code_counts
            .get("delivery_request_rejected")
            .copied(),
        Some(1)
    );
}

#[test]
fn functional_build_multi_channel_incident_timeline_report_writes_replay_export() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.multi_channel_incident_timeline = true;
    cli.multi_channel_incident_event_limit = Some(1);
    let replay_export_path = temp.path().join("artifacts/incident-replay.json");
    cli.multi_channel_incident_replay_export = Some(replay_export_path.clone());

    let channel_dir = cli
        .multi_channel_state_dir
        .join("channel-store/channels/telegram/ops-chat");
    std::fs::create_dir_all(&channel_dir).expect("create channel dir");
    std::fs::write(
        channel_dir.join("log.jsonl"),
        r#"{"timestamp_unix_ms":1760200100000,"direction":"inbound","event_key":"evt-1","source":"telegram","payload":{"transport":"telegram","conversation_id":"ops-chat","route_session_key":"ops-chat","route":{"binding_id":"telegram-ops","binding_matched":true},"channel_policy":{"reason_code":"allow_channel_policy_allow_from_any"}}}
{"timestamp_unix_ms":1760200100010,"direction":"outbound","event_key":"evt-1","source":"tau-multi-channel-runner","payload":{"event_key":"evt-1","response":"ok","delivery":{"mode":"dry_run","receipts":[{"status":"dry_run"}]}}}
{"timestamp_unix_ms":1760200100020,"direction":"inbound","event_key":"evt-2","source":"telegram","payload":{"transport":"telegram","conversation_id":"ops-chat","route_session_key":"ops-chat","route":{"binding_id":"telegram-ops","binding_matched":true},"channel_policy":{"reason_code":"allow_channel_policy_allow_from_any"}}}
{"timestamp_unix_ms":1760200100030,"direction":"outbound","event_key":"evt-2","source":"tau-multi-channel-runner","payload":{"status":"denied","reason_code":"deny_channel_policy_mention_required"}}
"#,
    )
    .expect("write channel log");

    let report = build_multi_channel_incident_timeline_report(
        &tau_multi_channel::MultiChannelIncidentTimelineQuery {
            state_dir: cli.multi_channel_state_dir.clone(),
            window_start_unix_ms: cli.multi_channel_incident_start_unix_ms,
            window_end_unix_ms: cli.multi_channel_incident_end_unix_ms,
            event_limit: cli.multi_channel_incident_event_limit.unwrap_or(200),
            replay_export_path: cli.multi_channel_incident_replay_export.clone(),
        },
    )
    .expect("build incident timeline report");
    assert_eq!(report.timeline.len(), 1);
    assert_eq!(report.truncated_event_count, 1);
    let replay_export = report.replay_export.expect("replay export summary");
    assert_eq!(replay_export.path, replay_export_path.display().to_string());
    assert!(!replay_export.checksum_sha256.is_empty());

    let replay_raw = std::fs::read_to_string(&replay_export_path).expect("read replay artifact");
    let replay_json: serde_json::Value =
        serde_json::from_str(&replay_raw).expect("parse replay artifact");
    assert_eq!(replay_json["schema_version"].as_u64(), Some(1));
    assert_eq!(
        replay_json["events"].as_array().map(|items| items.len()),
        Some(1)
    );
}

#[test]
fn integration_build_multi_channel_incident_timeline_report_reads_sample_channel_store_corpus() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.multi_channel_incident_timeline = true;

    let telegram_dir = cli
        .multi_channel_state_dir
        .join("channel-store/channels/telegram/tg-ops");
    let discord_dir = cli
        .multi_channel_state_dir
        .join("channel-store/channels/discord/dc-ops");
    let whatsapp_dir = cli
        .multi_channel_state_dir
        .join("channel-store/channels/whatsapp/wa-ops");
    std::fs::create_dir_all(&telegram_dir).expect("create telegram dir");
    std::fs::create_dir_all(&discord_dir).expect("create discord dir");
    std::fs::create_dir_all(&whatsapp_dir).expect("create whatsapp dir");
    std::fs::write(
        telegram_dir.join("log.jsonl"),
        r#"{"timestamp_unix_ms":1760200400000,"direction":"inbound","event_key":"evt-tg","source":"telegram","payload":{"transport":"telegram","conversation_id":"tg-ops","route_session_key":"tg-ops","route":{"binding_id":"tg-ops","binding_matched":true},"channel_policy":{"reason_code":"allow_channel_policy_allow_from_any"}}}
{"timestamp_unix_ms":1760200400010,"direction":"outbound","event_key":"evt-tg","source":"tau-multi-channel-runner","payload":{"event_key":"evt-tg","response":"ok","delivery":{"mode":"dry_run","receipts":[{"status":"dry_run"}]}}}
"#,
    )
    .expect("write telegram log");
    std::fs::write(
        discord_dir.join("log.jsonl"),
        r#"{"timestamp_unix_ms":1760200400100,"direction":"inbound","event_key":"evt-dc","source":"discord","payload":{"transport":"discord","conversation_id":"dc-ops","route_session_key":"dc-ops","route":{"binding_id":"dc-ops","binding_matched":true},"channel_policy":{"reason_code":"allow_channel_policy_allow_from_any"}}}
{"timestamp_unix_ms":1760200400110,"direction":"outbound","event_key":"evt-dc","source":"tau-multi-channel-runner","payload":{"status":"delivery_failed","reason_code":"delivery_provider_unavailable","retryable":true}}
{"timestamp_unix_ms":1760200400120,"direction":"outbound","event_key":"evt-dc","source":"tau-multi-channel-runner","payload":{"event_key":"evt-dc","response":"ok","delivery":{"mode":"provider","receipts":[{"status":"sent"}]}}}
"#,
    )
    .expect("write discord log");
    std::fs::write(
        whatsapp_dir.join("log.jsonl"),
        r#"{"timestamp_unix_ms":1760200400200,"direction":"inbound","event_key":"evt-wa","source":"whatsapp","payload":{"transport":"whatsapp","conversation_id":"wa-ops","route_session_key":"wa-ops","route":{"binding_id":"wa-ops","binding_matched":false},"channel_policy":{"reason_code":"deny_channel_policy_mention_required"}}}
{"timestamp_unix_ms":1760200400210,"direction":"outbound","event_key":"evt-wa","source":"tau-multi-channel-runner","payload":{"status":"denied","reason_code":"deny_channel_policy_mention_required"}}
"#,
    )
    .expect("write whatsapp log");

    let report = build_multi_channel_incident_timeline_report(
        &tau_multi_channel::MultiChannelIncidentTimelineQuery {
            state_dir: cli.multi_channel_state_dir.clone(),
            window_start_unix_ms: cli.multi_channel_incident_start_unix_ms,
            window_end_unix_ms: cli.multi_channel_incident_end_unix_ms,
            event_limit: cli.multi_channel_incident_event_limit.unwrap_or(200),
            replay_export_path: cli.multi_channel_incident_replay_export.clone(),
        },
    )
    .expect("build incident timeline report");
    assert_eq!(report.timeline.len(), 3);
    assert_eq!(report.scanned_channel_count, 3);
    assert_eq!(report.outcomes.allowed, 1);
    assert_eq!(report.outcomes.retried, 1);
    assert_eq!(report.outcomes.denied, 1);
    assert_eq!(
        report
            .route_reason_code_counts
            .get("route_binding_matched")
            .copied(),
        Some(2)
    );
    assert_eq!(
        report
            .route_reason_code_counts
            .get("route_binding_default")
            .copied(),
        Some(1)
    );
}

#[test]
fn regression_build_multi_channel_incident_timeline_report_tolerates_malformed_log_lines() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.multi_channel_incident_timeline = true;

    let channel_dir = cli
        .multi_channel_state_dir
        .join("channel-store/channels/whatsapp/ops-room");
    std::fs::create_dir_all(&channel_dir).expect("create channel dir");
    std::fs::write(
        channel_dir.join("log.jsonl"),
        r#"{"timestamp_unix_ms":1760200200000,"direction":"inbound","event_key":"evt-1","source":"whatsapp","payload":{"transport":"whatsapp","conversation_id":"ops-room","route_session_key":"ops-room","route":{"binding_id":"wa-ops","binding_matched":false},"channel_policy":{"reason_code":"allow_channel_policy_allow_from_any"}}}
{bad-json-line
{"timestamp_unix_ms":1760200200010,"direction":"outbound","event_key":"evt-1","source":"tau-multi-channel-runner","payload":{"event_key":"evt-1","response":"ok","delivery":{"mode":"dry_run","receipts":[{"status":"dry_run"}]}}}
"#,
    )
    .expect("write channel log");

    let report = build_multi_channel_incident_timeline_report(
        &tau_multi_channel::MultiChannelIncidentTimelineQuery {
            state_dir: cli.multi_channel_state_dir.clone(),
            window_start_unix_ms: cli.multi_channel_incident_start_unix_ms,
            window_end_unix_ms: cli.multi_channel_incident_end_unix_ms,
            event_limit: cli.multi_channel_incident_event_limit.unwrap_or(200),
            replay_export_path: cli.multi_channel_incident_replay_export.clone(),
        },
    )
    .expect("build incident timeline report");
    assert_eq!(report.timeline.len(), 1);
    assert_eq!(report.invalid_line_count, 1);
    assert!(
        !report.diagnostics.is_empty(),
        "malformed line should surface diagnostic message"
    );
}

#[test]
fn functional_execute_channel_store_admin_multi_agent_status_inspect_succeeds() {
    let temp = tempdir().expect("tempdir");
    let multi_agent_state_dir = temp.path().join("multi-agent");
    std::fs::create_dir_all(&multi_agent_state_dir).expect("create multi-agent state dir");
    std::fs::write(
        multi_agent_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["planner:planner-success"],
  "routed_cases": [
    {
      "case_key": "planner:planner-success",
      "case_id": "planner-success",
      "phase": "planner",
      "selected_role": "planner",
      "attempted_roles": ["planner"],
      "category": "planning",
      "updated_unix_ms": 1
    }
  ],
  "health": {
    "updated_unix_ms": 702,
    "cycle_duration_ms": 19,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write multi-agent state");
    std::fs::write(
        multi_agent_state_dir.join("runtime-events.jsonl"),
        r#"{"reason_codes":["routed_cases_updated"],"health_reason":"no recent transport failures observed"}
"#,
    )
    .expect("write multi-agent events");

    let mut cli = test_cli();
    cli.multi_agent_status_inspect = true;
    cli.multi_agent_status_json = true;
    cli.multi_agent_state_dir = multi_agent_state_dir;
    execute_channel_store_admin_command(&cli).expect("multi-agent status inspect should succeed");
}

#[test]
fn regression_execute_channel_store_admin_multi_agent_status_inspect_requires_state_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.multi_agent_status_inspect = true;
    cli.multi_agent_state_dir = temp.path().join("multi-agent");
    std::fs::create_dir_all(&cli.multi_agent_state_dir).expect("create multi-agent dir");

    let error = execute_channel_store_admin_command(&cli)
        .expect_err("multi-agent status inspect should fail without state file");
    assert!(error.to_string().contains("failed to read"));
    assert!(error.to_string().contains("state.json"));
}

#[test]
fn functional_execute_channel_store_admin_gateway_status_inspect_succeeds() {
    let temp = tempdir().expect("tempdir");
    let gateway_state_dir = temp.path().join("gateway");
    std::fs::create_dir_all(&gateway_state_dir).expect("create gateway state dir");
    std::fs::write(
        gateway_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["POST:/v1/tasks:gateway-success"],
  "requests": [
    {
      "case_key": "POST:/v1/tasks:gateway-success",
      "case_id": "gateway-success",
      "method": "POST",
      "endpoint": "/v1/tasks",
      "actor_id": "ops-bot",
      "status_code": 201,
      "outcome": "success",
      "error_code": "",
      "response_body": {"status":"accepted"},
      "updated_unix_ms": 1
    }
  ],
  "health": {
    "updated_unix_ms": 705,
    "cycle_duration_ms": 15,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write gateway state");
    std::fs::write(
        gateway_state_dir.join("runtime-events.jsonl"),
        r#"{"reason_codes":["healthy_cycle"],"health_reason":"no recent transport failures observed"}
"#,
    )
    .expect("write gateway events");

    let mut cli = test_cli();
    cli.gateway_status_inspect = true;
    cli.gateway_status_json = true;
    cli.gateway_state_dir = gateway_state_dir;
    execute_channel_store_admin_command(&cli).expect("gateway status inspect should succeed");
}

#[test]
fn regression_execute_channel_store_admin_gateway_status_inspect_requires_state_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.gateway_status_inspect = true;
    cli.gateway_state_dir = temp.path().join("gateway");
    std::fs::create_dir_all(&cli.gateway_state_dir).expect("create gateway dir");

    let error = execute_channel_store_admin_command(&cli)
        .expect_err("gateway status inspect should fail without state file");
    assert!(error.to_string().contains("failed to read"));
    assert!(error.to_string().contains("state.json"));
}

#[test]
fn functional_execute_channel_store_admin_custom_command_status_inspect_succeeds() {
    let temp = tempdir().expect("tempdir");
    let custom_command_state_dir = temp.path().join("custom-command");
    std::fs::create_dir_all(&custom_command_state_dir).expect("create custom-command state dir");
    std::fs::write(
        custom_command_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["CREATE:deploy_release:create-1"],
  "commands": [
    {
      "case_key": "CREATE:deploy_release:create-1",
      "case_id": "create-1",
      "command_name": "deploy_release",
      "template": "deploy {{env}}",
      "operation": "CREATE",
      "last_status_code": 201,
      "last_outcome": "success",
      "run_count": 1,
      "updated_unix_ms": 1
    }
  ],
  "health": {
    "updated_unix_ms": 710,
    "cycle_duration_ms": 14,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write custom-command state");
    std::fs::write(
        custom_command_state_dir.join("runtime-events.jsonl"),
        r#"{"reason_codes":["command_registry_mutated"],"health_reason":"no recent transport failures observed"}
"#,
    )
    .expect("write custom-command events");

    let mut cli = test_cli();
    cli.custom_command_status_inspect = true;
    cli.custom_command_status_json = true;
    cli.custom_command_state_dir = custom_command_state_dir;
    execute_channel_store_admin_command(&cli)
        .expect("custom-command status inspect should succeed");
}

#[test]
fn regression_execute_channel_store_admin_custom_command_status_inspect_requires_state_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.custom_command_status_inspect = true;
    cli.custom_command_state_dir = temp.path().join("custom-command");
    std::fs::create_dir_all(&cli.custom_command_state_dir).expect("create custom-command dir");

    let error = execute_channel_store_admin_command(&cli)
        .expect_err("custom-command status inspect should fail without state file");
    assert!(error.to_string().contains("failed to read"));
    assert!(error.to_string().contains("state.json"));
}

#[test]
fn functional_execute_channel_store_admin_voice_status_inspect_succeeds() {
    let temp = tempdir().expect("tempdir");
    let voice_state_dir = temp.path().join("voice");
    std::fs::create_dir_all(&voice_state_dir).expect("create voice state dir");
    std::fs::write(
        voice_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["turn:tau:ops-1:voice-success-turn"],
  "interactions": [
    {
      "case_key": "turn:tau:ops-1:voice-success-turn",
      "case_id": "voice-success-turn",
      "mode": "turn",
      "wake_word": "tau",
      "locale": "en-US",
      "speaker_id": "ops-1",
      "utterance": "open dashboard",
      "last_status_code": 202,
      "last_outcome": "success",
      "run_count": 1,
      "updated_unix_ms": 1
    }
  ],
  "health": {
    "updated_unix_ms": 720,
    "cycle_duration_ms": 12,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write voice state");
    std::fs::write(
        voice_state_dir.join("runtime-events.jsonl"),
        r#"{"reason_codes":["turns_handled"],"health_reason":"no recent transport failures observed"}
"#,
    )
    .expect("write voice events");

    let mut cli = test_cli();
    cli.voice_status_inspect = true;
    cli.voice_status_json = true;
    cli.voice_state_dir = voice_state_dir;
    execute_channel_store_admin_command(&cli).expect("voice status inspect should succeed");
}

#[test]
fn regression_execute_channel_store_admin_voice_status_inspect_requires_state_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.voice_status_inspect = true;
    cli.voice_state_dir = temp.path().join("voice");
    std::fs::create_dir_all(&cli.voice_state_dir).expect("create voice dir");

    let error = execute_channel_store_admin_command(&cli)
        .expect_err("voice status inspect should fail without state file");
    assert!(error.to_string().contains("failed to read"));
    assert!(error.to_string().contains("state.json"));
}

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
