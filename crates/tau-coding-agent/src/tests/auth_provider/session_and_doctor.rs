//! Session and doctor command tests for auth/provider diagnostics and report rendering.

use super::super::{
    build_doctor_command_config, compute_session_entry_depths, compute_session_stats,
    current_unix_timestamp, default_skills_lock_path, ensure_non_empty_text, escape_graph_label,
    evaluate_multi_channel_live_readiness, execute_doctor_cli_command, execute_doctor_command,
    execute_doctor_command_with_options, execute_session_diff_command,
    execute_session_graph_export_command, execute_session_search_command,
    execute_session_stats_command, handle_command_with_session_import_mode, parse_command,
    parse_doctor_command_args, parse_session_diff_args, parse_session_search_args,
    parse_session_stats_args, render_doctor_report, render_doctor_report_json, render_session_diff,
    render_session_graph_dot, render_session_graph_mermaid, render_session_stats,
    render_session_stats_json, resolve_prompt_input, resolve_session_graph_format,
    run_doctor_checks, run_doctor_checks_with_lookup, save_credential_store,
    search_session_entries, session_message_preview, shared_lineage_prefix_depth,
    skills_command_config, tempdir, test_auth_command_config, test_cli, test_profile_defaults,
    test_tool_policy_json, Agent, AgentConfig, Arc, BTreeMap, CommandAction,
    CommandExecutionContext, CredentialStoreData, CredentialStoreEncryptionMode,
    DoctorCheckOptions, DoctorCheckResult, DoctorCommandArgs, DoctorCommandConfig,
    DoctorCommandOutputFormat, DoctorMultiChannelReadinessConfig, DoctorProviderKeyStatus,
    DoctorStatus, HashMap, IntegrationCredentialStoreRecord, Message, ModelCatalog, ModelRef,
    NoopClient, Path, PathBuf, Provider, ProviderAuthMethod, SessionDiffEntry, SessionDiffReport,
    SessionGraphFormat, SessionImportMode, SessionRuntime, SessionSearchArgs, SessionStats,
    SessionStatsOutputFormat, SessionStore, SESSION_SEARCH_DEFAULT_RESULTS,
    SESSION_SEARCH_PREVIEW_CHARS,
};

#[test]
fn unit_resolve_prompt_input_uses_inline_prompt() {
    let mut cli = test_cli();
    cli.prompt = Some("inline prompt".to_string());

    let prompt = resolve_prompt_input(&cli).expect("resolve prompt");
    assert_eq!(prompt.as_deref(), Some("inline prompt"));
}

#[test]
fn unit_ensure_non_empty_text_returns_original_content() {
    let text = ensure_non_empty_text("hello".to_string(), "prompt".to_string())
        .expect("non-empty text should pass");
    assert_eq!(text, "hello");
}

#[test]
fn regression_ensure_non_empty_text_rejects_blank_content() {
    let error = ensure_non_empty_text(" \n\t".to_string(), "prompt".to_string())
        .expect_err("blank text should fail");
    assert!(error.to_string().contains("prompt is empty"));
}

#[test]
fn unit_parse_command_splits_name_and_args_with_extra_whitespace() {
    let parsed = parse_command("   /branch    42   ").expect("parse command");
    assert_eq!(parsed.name, "/branch");
    assert_eq!(parsed.args, "42");
}

#[test]
fn regression_parse_command_rejects_non_slash_input() {
    assert!(parse_command("help").is_none());
}

#[test]
fn unit_parse_session_search_args_supports_query_role_and_limit() {
    assert_eq!(
        parse_session_search_args("  retry budget  ").expect("parse query"),
        SessionSearchArgs {
            query: "retry budget".to_string(),
            role: None,
            limit: SESSION_SEARCH_DEFAULT_RESULTS,
        }
    );
    assert_eq!(
        parse_session_search_args("target --role user --limit 5").expect("parse flags"),
        SessionSearchArgs {
            query: "target".to_string(),
            role: Some("user".to_string()),
            limit: 5,
        }
    );
    assert_eq!(
        parse_session_search_args("--role=assistant --limit=9 delta").expect("parse inline"),
        SessionSearchArgs {
            query: "delta".to_string(),
            role: Some("assistant".to_string()),
            limit: 9,
        }
    );
}

#[test]
fn regression_parse_session_search_args_rejects_invalid_role_limit_and_flags() {
    let empty = parse_session_search_args(" \n\t ").expect_err("empty query should fail");
    assert!(empty.to_string().contains("query is required"));

    let invalid_role =
        parse_session_search_args("retry --role owner").expect_err("invalid role should fail");
    assert!(invalid_role.to_string().contains("invalid role"));

    let invalid_limit =
        parse_session_search_args("retry --limit 0").expect_err("limit zero should fail");
    assert!(invalid_limit
        .to_string()
        .contains("limit must be greater than 0"));

    let too_large =
        parse_session_search_args("retry --limit 9999").expect_err("too large limit should fail");
    assert!(too_large.to_string().contains("exceeds maximum"));

    let missing_role =
        parse_session_search_args("retry --role").expect_err("missing role value should fail");
    assert!(missing_role
        .to_string()
        .contains("missing value for --role"));

    let unknown_flag =
        parse_session_search_args("retry --unknown").expect_err("unknown flag should fail");
    assert!(unknown_flag.to_string().contains("unknown flag"));
}

#[test]
fn unit_session_message_preview_normalizes_whitespace_and_truncates() {
    let message = Message::user(format!(
        "line one\nline two {}",
        "x".repeat(SESSION_SEARCH_PREVIEW_CHARS)
    ));
    let preview = session_message_preview(&message);
    assert!(preview.starts_with("line one line two"));
    assert!(preview.ends_with("..."));
}

#[test]
fn unit_search_session_entries_matches_role_and_text_case_insensitively() {
    let entries = vec![
        tau_session::SessionEntry {
            id: 2,
            parent_id: Some(1),
            message: Message::assistant_text("Budget stabilized"),
        },
        tau_session::SessionEntry {
            id: 1,
            parent_id: None,
            message: Message::user("Root question"),
        },
    ];

    let (role_matches, role_total) = search_session_entries(&entries, "USER", None, 10);
    assert_eq!(role_total, 1);
    assert_eq!(role_matches[0].id, 1);
    assert_eq!(role_matches[0].role, "user");

    let (text_matches, text_total) = search_session_entries(&entries, "budget", None, 10);
    assert_eq!(text_total, 1);
    assert_eq!(text_matches[0].id, 2);
    assert_eq!(text_matches[0].role, "assistant");

    let (assistant_only, assistant_total) =
        search_session_entries(&entries, "budget", Some("assistant"), 10);
    assert_eq!(assistant_total, 1);
    assert_eq!(assistant_only[0].id, 2);
}

#[test]
fn functional_execute_session_search_command_renders_result_rows() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let head = store
        .append_messages(None, &[Message::system("sys")])
        .expect("append root");
    let head = store
        .append_messages(head, &[Message::user("Retry budget fix in progress")])
        .expect("append user");
    let runtime = SessionRuntime {
        store,
        active_head: head,
    };

    let output = execute_session_search_command(&runtime, "retry");
    assert!(output.contains("session search: query=\"retry\" role=any"));
    assert!(output.contains("matches=1"));
    assert!(output.contains("shown=1"));
    assert!(output.contains("limit=50"));
    assert!(output.contains("result: id=2 parent=1 role=user"));
    assert!(output.contains("preview=Retry budget fix in progress"));
}

#[test]
fn regression_search_session_entries_caps_huge_result_sets() {
    let entries = (1..=200)
        .map(|id| tau_session::SessionEntry {
            id,
            parent_id: if id == 1 { None } else { Some(id - 1) },
            message: Message::user(format!("needle-{id}")),
        })
        .collect::<Vec<_>>();
    let (matches, total_matches) =
        search_session_entries(&entries, "needle", None, SESSION_SEARCH_DEFAULT_RESULTS);
    assert_eq!(total_matches, 200);
    assert_eq!(matches.len(), SESSION_SEARCH_DEFAULT_RESULTS);
    assert_eq!(matches[0].id, 1);
    assert_eq!(
        matches.last().map(|item| item.id),
        Some(SESSION_SEARCH_DEFAULT_RESULTS as u64)
    );
}

#[test]
fn integration_execute_session_search_command_scans_entries_across_branches() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("sys")])
        .expect("append root");
    let main_head = store
        .append_messages(root, &[Message::user("main branch target")])
        .expect("append main");
    let _branch_head = store
        .append_messages(root, &[Message::user("branch target")])
        .expect("append branch");
    let runtime = SessionRuntime {
        store,
        active_head: main_head,
    };

    let output = execute_session_search_command(&runtime, "target");
    let main_index = output.find("result: id=2").expect("main result");
    let branch_index = output.find("result: id=3").expect("branch result");
    assert!(main_index < branch_index);
}

#[test]
fn integration_execute_session_search_command_applies_role_filter_and_limit() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root target")])
        .expect("append root");
    let user_id = store
        .append_messages(root, &[Message::user("target user message")])
        .expect("append user");
    let _assistant_id = store
        .append_messages(
            user_id,
            &[Message::assistant_text("target assistant message")],
        )
        .expect("append assistant");
    let _tool_id = store
        .append_messages(
            user_id,
            &[Message::tool_result(
                "tool-call-1",
                "tool_call",
                "{}",
                false,
            )],
        )
        .expect("append tool");
    let runtime = SessionRuntime {
        store,
        active_head: user_id,
    };

    let output = execute_session_search_command(&runtime, "target --role user --limit 1");
    assert!(output.contains("role=user"));
    assert!(output.contains("matches=1"));
    assert!(output.contains("shown=1"));
    assert!(output.contains("limit=1"));
    assert!(output.contains("result: id=2 parent=1 role=user"));
    assert!(!output.contains("role=assistant"));
    assert!(!output.contains("role=tool"));
}

#[test]
fn unit_parse_session_diff_args_supports_default_and_explicit_heads() {
    assert_eq!(parse_session_diff_args("").expect("default heads"), None);
    assert_eq!(
        parse_session_diff_args(" 12  24 ").expect("explicit heads"),
        Some((12, 24))
    );
}

#[test]
fn regression_parse_session_diff_args_rejects_invalid_shapes() {
    let usage = parse_session_diff_args("12").expect_err("single head should fail");
    assert!(usage
        .to_string()
        .contains("usage: /session-diff [<left-id> <right-id>]"));

    let left_error = parse_session_diff_args("left 2").expect_err("invalid left head");
    assert!(left_error
        .to_string()
        .contains("invalid left session id 'left'"));
}

#[test]
fn unit_shared_lineage_prefix_depth_returns_common_ancestor_depth() {
    let left = vec![
        tau_session::SessionEntry {
            id: 1,
            parent_id: None,
            message: Message::system("root"),
        },
        tau_session::SessionEntry {
            id: 2,
            parent_id: Some(1),
            message: Message::user("shared"),
        },
        tau_session::SessionEntry {
            id: 4,
            parent_id: Some(2),
            message: Message::assistant_text("left"),
        },
    ];
    let right = vec![
        tau_session::SessionEntry {
            id: 1,
            parent_id: None,
            message: Message::system("root"),
        },
        tau_session::SessionEntry {
            id: 2,
            parent_id: Some(1),
            message: Message::user("shared"),
        },
        tau_session::SessionEntry {
            id: 5,
            parent_id: Some(2),
            message: Message::assistant_text("right"),
        },
    ];

    assert_eq!(shared_lineage_prefix_depth(&left, &right), 2);
}

#[test]
fn functional_render_session_diff_includes_summary_and_lineage_rows() {
    let report = SessionDiffReport {
        source: "explicit",
        left_id: 4,
        right_id: 5,
        shared_depth: 2,
        left_depth: 3,
        right_depth: 3,
        shared_entries: vec![SessionDiffEntry {
            id: 1,
            parent_id: None,
            role: "system".to_string(),
            preview: "root".to_string(),
        }],
        left_only_entries: vec![SessionDiffEntry {
            id: 4,
            parent_id: Some(2),
            role: "assistant".to_string(),
            preview: "left path".to_string(),
        }],
        right_only_entries: vec![SessionDiffEntry {
            id: 5,
            parent_id: Some(2),
            role: "assistant".to_string(),
            preview: "right path".to_string(),
        }],
    };

    let output = render_session_diff(&report);
    assert!(output.contains("session diff: source=explicit left=4 right=5"));
    assert!(output
        .contains("summary: shared_depth=2 left_depth=3 right_depth=3 left_only=1 right_only=1"));
    assert!(output.contains("shared: id=1 parent=none role=system preview=root"));
    assert!(output.contains("left-only: id=4 parent=2 role=assistant preview=left path"));
    assert!(output.contains("right-only: id=5 parent=2 role=assistant preview=right path"));
}

#[test]
fn integration_execute_session_diff_command_defaults_to_active_and_latest_heads() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let main_head = store
        .append_messages(Some(root), &[Message::user("main user")])
        .expect("append main")
        .expect("main head");
    let latest_head = store
        .append_messages(Some(root), &[Message::user("branch user")])
        .expect("append branch")
        .expect("branch head");
    let runtime = SessionRuntime {
        store,
        active_head: Some(main_head),
    };

    let output = execute_session_diff_command(&runtime, None);
    assert!(output.contains(&format!(
        "session diff: source=default left={} right={}",
        main_head, latest_head
    )));
    assert!(output
        .contains("summary: shared_depth=1 left_depth=2 right_depth=2 left_only=1 right_only=1"));
    assert!(output.contains("shared: id=1 parent=none role=system preview=sys"));
    assert!(output.contains("left-only: id=2 parent=1 role=user preview=main user"));
    assert!(output.contains("right-only: id=3 parent=1 role=user preview=branch user"));
}

#[test]
fn integration_execute_session_diff_command_supports_explicit_identical_heads() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let head = store
        .append_messages(Some(root), &[Message::user("user")])
        .expect("append user")
        .expect("head id");
    let runtime = SessionRuntime {
        store,
        active_head: Some(head),
    };

    let output = execute_session_diff_command(&runtime, Some((head, head)));
    assert!(output.contains("summary: shared_depth=2 left_depth=2 right_depth=2"));
    assert!(output.contains("left-only: none"));
    assert!(output.contains("right-only: none"));
}

#[test]
fn regression_execute_session_diff_command_reports_unknown_ids() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("sys")])
        .expect("append")
        .expect("root");
    let runtime = SessionRuntime {
        store,
        active_head: Some(root),
    };

    let output = execute_session_diff_command(&runtime, Some((999, root)));
    assert!(output.contains("session diff error: unknown left session id 999"));
}

#[test]
fn regression_execute_session_diff_command_reports_empty_session_default_heads() {
    let temp = tempdir().expect("tempdir");
    let store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let runtime = SessionRuntime {
        store,
        active_head: None,
    };

    let output = execute_session_diff_command(&runtime, None);
    assert!(output.contains("session diff error: active head is not set"));
}

#[test]
fn regression_execute_session_diff_command_reports_malformed_graph() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("malformed-session.jsonl");
    let raw = [
        serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
        serde_json::json!({
            "record_type":"entry",
            "id":1,
            "parent_id":2,
            "message": Message::system("orphan")
        })
        .to_string(),
    ]
    .join("\n");
    std::fs::write(&session_path, format!("{raw}\n")).expect("write session");
    let store = SessionStore::load(&session_path).expect("load session");
    let runtime = SessionRuntime {
        store,
        active_head: Some(1),
    };

    let output = execute_session_diff_command(&runtime, None);
    assert!(output.contains("session diff error: unknown session id 2"));
}

#[test]
fn unit_compute_session_entry_depths_calculates_branch_depths() {
    let entries = vec![
        tau_session::SessionEntry {
            id: 3,
            parent_id: Some(2),
            message: Message::assistant_text("leaf"),
        },
        tau_session::SessionEntry {
            id: 1,
            parent_id: None,
            message: Message::system("root"),
        },
        tau_session::SessionEntry {
            id: 2,
            parent_id: Some(1),
            message: Message::user("middle"),
        },
    ];
    let depths = compute_session_entry_depths(&entries).expect("depth computation");
    assert_eq!(depths.get(&1), Some(&1));
    assert_eq!(depths.get(&2), Some(&2));
    assert_eq!(depths.get(&3), Some(&3));
}

#[test]
fn unit_compute_session_stats_calculates_core_counts() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let active_head = store
        .append_messages(Some(root), &[Message::user("user one")])
        .expect("append user")
        .expect("active head");
    let runtime = SessionRuntime {
        store,
        active_head: Some(active_head),
    };

    let stats = compute_session_stats(&runtime).expect("compute stats");
    assert_eq!(stats.entries, 2);
    assert_eq!(stats.branch_tips, 1);
    assert_eq!(stats.roots, 1);
    assert_eq!(stats.max_depth, 2);
    assert_eq!(stats.active_depth, Some(2));
    assert_eq!(stats.latest_depth, Some(2));
    assert!(stats.active_is_latest);
    assert_eq!(stats.role_counts.get("system"), Some(&1));
    assert_eq!(stats.role_counts.get("user"), Some(&1));
}

#[test]
fn functional_render_session_stats_includes_heads_depths_and_roles() {
    let mut role_counts = BTreeMap::new();
    role_counts.insert("assistant".to_string(), 2);
    role_counts.insert("user".to_string(), 1);
    let stats = SessionStats {
        entries: 3,
        branch_tips: 1,
        roots: 1,
        max_depth: 3,
        active_depth: Some(3),
        latest_depth: Some(3),
        active_head: Some(3),
        latest_head: Some(3),
        active_is_latest: true,
        role_counts,
    };

    let rendered = render_session_stats(&stats);
    assert!(rendered.contains("session stats: entries=3 branch_tips=1 roots=1 max_depth=3"));
    assert!(rendered.contains("heads: active=3 latest=3 active_is_latest=true"));
    assert!(rendered.contains("depth: active=3 latest=3"));
    assert!(rendered.contains("role: assistant=2"));
    assert!(rendered.contains("role: user=1"));
}

#[test]
fn unit_parse_session_stats_args_supports_default_and_json_modes() {
    assert_eq!(
        parse_session_stats_args("").expect("empty args"),
        SessionStatsOutputFormat::Text
    );
    assert_eq!(
        parse_session_stats_args("--json").expect("json flag"),
        SessionStatsOutputFormat::Json
    );
    let error = parse_session_stats_args("--bad").expect_err("invalid flag should fail");
    assert!(error.to_string().contains("usage: /session-stats [--json]"));
}

#[test]
fn unit_render_session_stats_json_includes_counts_and_roles() {
    let mut role_counts = BTreeMap::new();
    role_counts.insert("assistant".to_string(), 2);
    role_counts.insert("user".to_string(), 1);
    let stats = SessionStats {
        entries: 3,
        branch_tips: 1,
        roots: 1,
        max_depth: 3,
        active_depth: Some(3),
        latest_depth: Some(3),
        active_head: Some(3),
        latest_head: Some(3),
        active_is_latest: true,
        role_counts,
    };

    let json = render_session_stats_json(&stats);
    let value = serde_json::from_str::<serde_json::Value>(&json).expect("parse json");
    assert_eq!(value["entries"], 3);
    assert_eq!(value["branch_tips"], 1);
    assert_eq!(value["active_head"], 3);
    assert_eq!(value["role_counts"]["assistant"], 2);
    assert_eq!(value["role_counts"]["user"], 1);
}

#[test]
fn integration_execute_session_stats_command_summarizes_branched_session() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("sys")])
        .expect("append root")
        .expect("root id");
    let main_head = store
        .append_messages(Some(root), &[Message::user("main user")])
        .expect("append main")
        .expect("main head");
    let branch_head = store
        .append_messages(Some(root), &[Message::user("branch user")])
        .expect("append branch")
        .expect("branch head");
    let latest_head = store
        .append_messages(
            Some(branch_head),
            &[Message::assistant_text("branch assistant")],
        )
        .expect("append branch assistant")
        .expect("latest head");
    let runtime = SessionRuntime {
        store,
        active_head: Some(main_head),
    };

    let output = execute_session_stats_command(&runtime, SessionStatsOutputFormat::Text);
    assert!(output.contains("session stats: entries=4"));
    assert!(output.contains("branch_tips=2"));
    assert!(output.contains("roots=1"));
    assert!(output.contains("max_depth=3"));
    assert!(output.contains(&format!(
        "heads: active={} latest={} active_is_latest=false",
        main_head, latest_head
    )));
    assert!(output.contains("role: assistant=1"));
    assert!(output.contains("role: system=1"));
    assert!(output.contains("role: user=2"));

    let json_output = execute_session_stats_command(&runtime, SessionStatsOutputFormat::Json);
    let value = serde_json::from_str::<serde_json::Value>(&json_output).expect("parse json");
    assert_eq!(value["entries"], 4);
    assert_eq!(value["branch_tips"], 2);
    assert_eq!(value["roots"], 1);
    assert_eq!(value["max_depth"], 3);
    assert_eq!(value["active_head"], main_head);
    assert_eq!(value["latest_head"], latest_head);
    assert_eq!(value["role_counts"]["assistant"], 1);
    assert_eq!(value["role_counts"]["system"], 1);
    assert_eq!(value["role_counts"]["user"], 2);
}

#[test]
fn regression_execute_session_stats_command_handles_empty_session() {
    let temp = tempdir().expect("tempdir");
    let store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let runtime = SessionRuntime {
        store,
        active_head: None,
    };

    let output = execute_session_stats_command(&runtime, SessionStatsOutputFormat::Text);
    assert!(output.contains("session stats: entries=0 branch_tips=0 roots=0 max_depth=0"));
    assert!(output.contains("heads: active=none latest=none active_is_latest=true"));
    assert!(output.contains("depth: active=none latest=none"));
    assert!(output.contains("roles: none"));

    let json_output = execute_session_stats_command(&runtime, SessionStatsOutputFormat::Json);
    let value = serde_json::from_str::<serde_json::Value>(&json_output).expect("parse json");
    assert_eq!(value["entries"], 0);
    assert_eq!(value["branch_tips"], 0);
    assert_eq!(value["roots"], 0);
    assert_eq!(value["max_depth"], 0);
    assert_eq!(value["active_head"], serde_json::Value::Null);
}

#[test]
fn regression_execute_session_stats_command_reports_malformed_graph() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("malformed-session.jsonl");
    let raw = [
        serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
        serde_json::json!({
            "record_type":"entry",
            "id":1,
            "parent_id":2,
            "message": Message::system("orphan")
        })
        .to_string(),
    ]
    .join("\n");
    std::fs::write(&session_path, format!("{raw}\n")).expect("write session");
    let store = SessionStore::load(&session_path).expect("load session");
    let runtime = SessionRuntime {
        store,
        active_head: Some(1),
    };

    let output = execute_session_stats_command(&runtime, SessionStatsOutputFormat::Text);
    assert!(output.contains("session stats error:"));
    assert!(output.contains("missing parent id 2"));

    let json_output = execute_session_stats_command(&runtime, SessionStatsOutputFormat::Json);
    let value = serde_json::from_str::<serde_json::Value>(&json_output).expect("parse json error");
    assert!(value["error"]
        .as_str()
        .expect("error string")
        .contains("missing parent id 2"));
}

#[test]
fn unit_build_doctor_command_config_collects_sorted_unique_provider_states() {
    let mut cli = test_cli();
    cli.no_session = true;
    cli.session = PathBuf::from("/tmp/session.jsonl");
    cli.skills_dir = PathBuf::from("/tmp/skills");
    cli.skills_lock_file = Some(PathBuf::from("/tmp/custom.lock.json"));
    cli.skill_trust_root_file = Some(PathBuf::from("/tmp/trust-roots.json"));
    cli.doctor_release_cache_file = PathBuf::from("/tmp/doctor-cache.json");
    cli.doctor_release_cache_ttl_ms = 180_000;
    cli.openai_api_key = Some("openai-key".to_string());
    cli.anthropic_api_key = Some("anthropic-key".to_string());
    cli.google_api_key = None;

    let primary = ModelRef {
        provider: Provider::OpenAi,
        model: "gpt-4o-mini".to_string(),
    };
    let fallbacks = vec![
        ModelRef {
            provider: Provider::Google,
            model: "gemini-2.5-pro".to_string(),
        },
        ModelRef {
            provider: Provider::Anthropic,
            model: "claude-sonnet-4".to_string(),
        },
        ModelRef {
            provider: Provider::OpenAi,
            model: "gpt-4.1-mini".to_string(),
        },
    ];
    let lock_path = PathBuf::from("/tmp/skills.lock.json");

    let config = build_doctor_command_config(&cli, &primary, &fallbacks, &lock_path);
    assert_eq!(config.model, "openai/gpt-4o-mini");
    assert!(!config.session_enabled);
    assert_eq!(config.session_path, PathBuf::from("/tmp/session.jsonl"));
    assert_eq!(config.skills_dir, PathBuf::from("/tmp/skills"));
    assert_eq!(config.skills_lock_path, lock_path);
    assert_eq!(
        config.trust_root_path,
        Some(PathBuf::from("/tmp/trust-roots.json"))
    );
    assert!(config
        .release_channel_path
        .ends_with(Path::new(".tau").join("release-channel.json")));
    assert_eq!(
        config.release_lookup_cache_path,
        PathBuf::from("/tmp/doctor-cache.json")
    );
    assert_eq!(config.release_lookup_cache_ttl_ms, 180_000);

    let provider_rows = config
        .provider_keys
        .iter()
        .map(|item| {
            (
                item.provider.clone(),
                item.key_env_var.clone(),
                item.present,
                item.auth_mode.as_str().to_string(),
                item.mode_supported,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        provider_rows,
        vec![
            (
                "anthropic".to_string(),
                "ANTHROPIC_API_KEY".to_string(),
                true,
                "api_key".to_string(),
                true
            ),
            (
                "google".to_string(),
                "GEMINI_API_KEY".to_string(),
                false,
                "api_key".to_string(),
                true
            ),
            (
                "openai".to_string(),
                "OPENAI_API_KEY".to_string(),
                true,
                "api_key".to_string(),
                true
            ),
        ]
    );
}

#[test]
fn unit_render_doctor_report_summarizes_counts_and_rows() {
    let report = render_doctor_report(&[
        DoctorCheckResult {
            key: "model".to_string(),
            status: DoctorStatus::Pass,
            code: "openai/gpt-4o-mini".to_string(),
            path: None,
            action: None,
        },
        DoctorCheckResult {
            key: "provider_key.openai".to_string(),
            status: DoctorStatus::Fail,
            code: "missing".to_string(),
            path: None,
            action: Some("set OPENAI_API_KEY".to_string()),
        },
        DoctorCheckResult {
            key: "skills_lock".to_string(),
            status: DoctorStatus::Warn,
            code: "missing".to_string(),
            path: Some("/tmp/skills.lock.json".to_string()),
            action: Some("run /skills-lock-write to generate lockfile".to_string()),
        },
    ]);

    assert!(report.contains("doctor summary: checks=3 pass=1 warn=1 fail=1"));
    assert!(report.contains(
        "doctor check: key=model status=pass code=openai/gpt-4o-mini path=none action=none"
    ));
    assert!(report.contains(
            "doctor check: key=provider_key.openai status=fail code=missing path=none action=set OPENAI_API_KEY"
        ));
    assert!(report.contains("doctor check: key=skills_lock status=warn code=missing path=/tmp/skills.lock.json action=run /skills-lock-write to generate lockfile"));
}

#[test]
fn unit_parse_doctor_command_args_supports_default_and_json_modes() {
    assert_eq!(
        parse_doctor_command_args("").expect("parse empty"),
        DoctorCommandArgs {
            output_format: DoctorCommandOutputFormat::Text,
            online: false,
        }
    );
    assert_eq!(
        parse_doctor_command_args("--json").expect("parse json"),
        DoctorCommandArgs {
            output_format: DoctorCommandOutputFormat::Json,
            online: false,
        }
    );
    assert_eq!(
        parse_doctor_command_args("--online").expect("parse online"),
        DoctorCommandArgs {
            output_format: DoctorCommandOutputFormat::Text,
            online: true,
        }
    );
    assert_eq!(
        parse_doctor_command_args("--online --json").expect("parse online + json"),
        DoctorCommandArgs {
            output_format: DoctorCommandOutputFormat::Json,
            online: true,
        }
    );

    let error = parse_doctor_command_args("--json --extra").expect_err("extra args should fail");
    assert!(error
        .to_string()
        .contains("usage: /doctor [--json] [--online]"));
    let duplicate = parse_doctor_command_args("--online --online")
        .expect_err("duplicate online flag should fail");
    assert!(duplicate
        .to_string()
        .contains("usage: /doctor [--json] [--online]"));
}

#[test]
fn unit_render_doctor_report_json_contains_summary_and_rows() {
    let report = render_doctor_report_json(&[
        DoctorCheckResult {
            key: "model".to_string(),
            status: DoctorStatus::Pass,
            code: "openai/gpt-4o-mini".to_string(),
            path: None,
            action: None,
        },
        DoctorCheckResult {
            key: "provider_key.openai".to_string(),
            status: DoctorStatus::Fail,
            code: "missing".to_string(),
            path: None,
            action: Some("set OPENAI_API_KEY".to_string()),
        },
    ]);
    let value = serde_json::from_str::<serde_json::Value>(&report).expect("parse json");
    assert_eq!(value["summary"]["checks"], 2);
    assert_eq!(value["summary"]["pass"], 1);
    assert_eq!(value["summary"]["warn"], 0);
    assert_eq!(value["summary"]["fail"], 1);
    assert_eq!(value["checks"][0]["key"], "model");
    assert_eq!(value["checks"][0]["status"], "pass");
    assert_eq!(value["checks"][1]["key"], "provider_key.openai");
    assert_eq!(value["checks"][1]["status"], "fail");
}

#[test]
fn functional_execute_doctor_command_supports_text_and_json_modes() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");
    let skills_dir = temp.path().join("skills");
    let lock_path = temp.path().join("skills.lock.json");
    let trust_root_path = temp.path().join("trust-roots.json");
    let ingress_dir = temp.path().join("multi-channel-live-ingress");
    let missing_credential_store = temp.path().join("missing-credentials.json");
    std::fs::create_dir_all(&skills_dir).expect("mkdir skills");
    std::fs::create_dir_all(&ingress_dir).expect("mkdir ingress dir");
    std::fs::write(ingress_dir.join("telegram.ndjson"), "").expect("write telegram inbox");
    std::fs::write(ingress_dir.join("discord.ndjson"), "").expect("write discord inbox");
    std::fs::write(ingress_dir.join("whatsapp.ndjson"), "").expect("write whatsapp inbox");
    std::fs::write(&session_path, "{}\n").expect("write session");
    std::fs::write(&lock_path, "{}\n").expect("write lock");
    std::fs::write(&trust_root_path, "[]\n").expect("write trust");

    let config = DoctorCommandConfig {
        model: "openai/gpt-4o-mini".to_string(),
        provider_keys: vec![
            DoctorProviderKeyStatus {
                provider_kind: Provider::Anthropic,
                provider: "anthropic".to_string(),
                key_env_var: "ANTHROPIC_API_KEY".to_string(),
                present: false,
                auth_mode: ProviderAuthMethod::ApiKey,
                mode_supported: true,
                login_backend_enabled: false,
                login_backend_executable: None,
                login_backend_available: false,
            },
            DoctorProviderKeyStatus {
                provider_kind: Provider::OpenAi,
                provider: "openai".to_string(),
                key_env_var: "OPENAI_API_KEY".to_string(),
                present: true,
                auth_mode: ProviderAuthMethod::ApiKey,
                mode_supported: true,
                login_backend_enabled: false,
                login_backend_executable: None,
                login_backend_available: false,
            },
        ],
        release_channel_path: temp.path().join("release-channel.json"),
        release_lookup_cache_path: temp.path().join("release-lookup-cache.json"),
        release_lookup_cache_ttl_ms: 900_000,
        browser_automation_playwright_cli: "playwright-cli".to_string(),
        session_enabled: true,
        session_path,
        skills_dir,
        skills_lock_path: lock_path,
        trust_root_path: Some(trust_root_path),
        multi_channel_live_readiness: DoctorMultiChannelReadinessConfig {
            ingress_dir,
            credential_store_path: missing_credential_store,
            credential_store_encryption: CredentialStoreEncryptionMode::None,
            credential_store_key: None,
            telegram_bot_token: Some("telegram-token".to_string()),
            discord_bot_token: Some("discord-token".to_string()),
            whatsapp_access_token: Some("whatsapp-access-token".to_string()),
            whatsapp_phone_number_id: Some("15551234567".to_string()),
        },
    };

    let report = execute_doctor_command(&config, DoctorCommandOutputFormat::Text);
    assert!(report.contains("doctor summary: checks=20"));

    let keys = report
        .lines()
        .skip(1)
        .map(|line| {
            line.split("key=")
                .nth(1)
                .expect("key section")
                .split(" status=")
                .next()
                .expect("key value")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        vec![
            "model".to_string(),
            "release_channel".to_string(),
            "release_update".to_string(),
            "provider_auth_mode.anthropic".to_string(),
            "provider_key.anthropic".to_string(),
            "provider_auth_mode.openai".to_string(),
            "provider_key.openai".to_string(),
            "session_path".to_string(),
            "skills_dir".to_string(),
            "skills_lock".to_string(),
            "trust_root".to_string(),
            "browser_automation.npx".to_string(),
            "browser_automation.playwright_cli".to_string(),
            "multi_channel_live.credential_store".to_string(),
            "multi_channel_live.ingress_dir".to_string(),
            "multi_channel_live.channel_policy".to_string(),
            "multi_channel_live.channel_policy.risk".to_string(),
            "multi_channel_live.channel.telegram".to_string(),
            "multi_channel_live.channel.discord".to_string(),
            "multi_channel_live.channel.whatsapp".to_string(),
        ]
    );

    let json_report = execute_doctor_command(&config, DoctorCommandOutputFormat::Json);
    let value = serde_json::from_str::<serde_json::Value>(&json_report).expect("parse json report");
    assert_eq!(value["summary"]["checks"], 20);
    assert_eq!(value["checks"].as_array().map(|rows| rows.len()), Some(20));
    assert_eq!(
        value["summary"]["pass"].as_u64().unwrap_or_default()
            + value["summary"]["warn"].as_u64().unwrap_or_default()
            + value["summary"]["fail"].as_u64().unwrap_or_default(),
        20
    );
    assert_eq!(value["checks"][0]["key"], "model");
    assert_eq!(value["checks"][1]["key"], "release_channel");
    assert_eq!(value["checks"][2]["key"], "release_update");
}

#[test]
fn functional_execute_doctor_cli_command_accepts_online_without_network_when_store_is_invalid() {
    let temp = tempdir().expect("tempdir");
    let release_channel_path = temp.path().join("release-channel.json");
    std::fs::write(&release_channel_path, "{invalid-json").expect("write malformed release file");
    let config = DoctorCommandConfig {
        model: "openai/gpt-4o-mini".to_string(),
        provider_keys: vec![],
        release_channel_path,
        release_lookup_cache_path: temp.path().join("release-lookup-cache.json"),
        release_lookup_cache_ttl_ms: 900_000,
        browser_automation_playwright_cli: "playwright-cli".to_string(),
        session_enabled: false,
        session_path: temp.path().join("session.jsonl"),
        skills_dir: temp.path().join("skills"),
        skills_lock_path: temp.path().join("skills.lock.json"),
        trust_root_path: None,
        multi_channel_live_readiness: DoctorMultiChannelReadinessConfig::default(),
    };

    let report = execute_doctor_cli_command(&config, "--online");
    assert!(report.contains("doctor summary:"));
    assert!(report.contains("key=release_channel"));
    assert!(report.contains("code=invalid_store:"));
    assert!(report.contains("key=release_update"));
    assert!(report.contains("code=lookup_skipped_invalid_store"));
}

#[test]
fn integration_run_doctor_checks_identifies_missing_runtime_prerequisites() {
    let temp = tempdir().expect("tempdir");
    let config = DoctorCommandConfig {
        model: "openai/gpt-4o-mini".to_string(),
        provider_keys: vec![DoctorProviderKeyStatus {
            provider_kind: Provider::OpenAi,
            provider: "openai".to_string(),
            key_env_var: "OPENAI_API_KEY".to_string(),
            present: false,
            auth_mode: ProviderAuthMethod::ApiKey,
            mode_supported: true,
            login_backend_enabled: false,
            login_backend_executable: None,
            login_backend_available: false,
        }],
        release_channel_path: temp.path().join("release-channel.json"),
        release_lookup_cache_path: temp.path().join("release-lookup-cache.json"),
        release_lookup_cache_ttl_ms: 900_000,
        browser_automation_playwright_cli: "playwright-cli".to_string(),
        session_enabled: true,
        session_path: temp.path().join("missing-parent").join("session.jsonl"),
        skills_dir: temp.path().join("missing-skills"),
        skills_lock_path: temp.path().join("missing-lock.json"),
        trust_root_path: Some(temp.path().join("missing-trust-roots.json")),
        multi_channel_live_readiness: DoctorMultiChannelReadinessConfig::default(),
    };

    let checks = run_doctor_checks(&config);
    let by_key = checks
        .into_iter()
        .map(|check| (check.key.clone(), check))
        .collect::<HashMap<_, _>>();

    assert_eq!(
        by_key.get("model").map(|item| item.status),
        Some(DoctorStatus::Pass)
    );
    assert_eq!(
        by_key
            .get("provider_auth_mode.openai")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Pass, "api_key".to_string()))
    );
    assert_eq!(
        by_key
            .get("provider_key.openai")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Fail, "missing".to_string()))
    );
    assert_eq!(
        by_key
            .get("release_update")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Warn, "skipped_offline".to_string()))
    );
    assert_eq!(
        by_key
            .get("session_path")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Fail, "missing_parent".to_string()))
    );
    assert_eq!(
        by_key
            .get("skills_dir")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Warn, "missing".to_string()))
    );
    assert_eq!(
        by_key
            .get("skills_lock")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Warn, "missing".to_string()))
    );
    assert_eq!(
        by_key
            .get("trust_root")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Warn, "missing".to_string()))
    );
}

#[test]
fn integration_run_doctor_checks_reports_google_backend_status_for_oauth_mode() {
    let temp = tempdir().expect("tempdir");
    let config = DoctorCommandConfig {
        model: "google/gemini-2.5-pro".to_string(),
        provider_keys: vec![DoctorProviderKeyStatus {
            provider_kind: Provider::Google,
            provider: "google".to_string(),
            key_env_var: "GEMINI_API_KEY".to_string(),
            present: false,
            auth_mode: ProviderAuthMethod::OauthToken,
            mode_supported: true,
            login_backend_enabled: false,
            login_backend_executable: Some("gemini".to_string()),
            login_backend_available: false,
        }],
        release_channel_path: temp.path().join("release-channel.json"),
        release_lookup_cache_path: temp.path().join("release-lookup-cache.json"),
        release_lookup_cache_ttl_ms: 900_000,
        browser_automation_playwright_cli: "playwright-cli".to_string(),
        session_enabled: false,
        session_path: temp.path().join("session.jsonl"),
        skills_dir: temp.path().join("skills"),
        skills_lock_path: temp.path().join("skills.lock.json"),
        trust_root_path: None,
        multi_channel_live_readiness: DoctorMultiChannelReadinessConfig::default(),
    };

    let checks = run_doctor_checks(&config);
    let by_key = checks
        .into_iter()
        .map(|check| (check.key.clone(), check))
        .collect::<HashMap<_, _>>();

    assert_eq!(
        by_key
            .get("provider_auth_mode.google")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Pass, "oauth_token".to_string()))
    );
    assert_eq!(
        by_key
            .get("provider_backend.google")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Fail, "backend_disabled".to_string()))
    );
}

#[test]
fn integration_run_doctor_checks_reports_anthropic_backend_status_for_oauth_mode() {
    let temp = tempdir().expect("tempdir");
    let config = DoctorCommandConfig {
        model: "anthropic/claude-sonnet-4-20250514".to_string(),
        provider_keys: vec![DoctorProviderKeyStatus {
            provider_kind: Provider::Anthropic,
            provider: "anthropic".to_string(),
            key_env_var: "ANTHROPIC_API_KEY".to_string(),
            present: false,
            auth_mode: ProviderAuthMethod::OauthToken,
            mode_supported: true,
            login_backend_enabled: false,
            login_backend_executable: Some("claude".to_string()),
            login_backend_available: false,
        }],
        release_channel_path: temp.path().join("release-channel.json"),
        release_lookup_cache_path: temp.path().join("release-lookup-cache.json"),
        release_lookup_cache_ttl_ms: 900_000,
        browser_automation_playwright_cli: "playwright-cli".to_string(),
        session_enabled: false,
        session_path: temp.path().join("session.jsonl"),
        skills_dir: temp.path().join("skills"),
        skills_lock_path: temp.path().join("skills.lock.json"),
        trust_root_path: None,
        multi_channel_live_readiness: DoctorMultiChannelReadinessConfig::default(),
    };

    let checks = run_doctor_checks(&config);
    let by_key = checks
        .into_iter()
        .map(|check| (check.key.clone(), check))
        .collect::<HashMap<_, _>>();

    assert_eq!(
        by_key
            .get("provider_auth_mode.anthropic")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Pass, "oauth_token".to_string()))
    );
    assert_eq!(
        by_key
            .get("provider_backend.anthropic")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Fail, "backend_disabled".to_string()))
    );
}

#[test]
fn integration_execute_doctor_command_with_online_lookup_reports_update_available() {
    let temp = tempdir().expect("tempdir");
    let config = DoctorCommandConfig {
        model: "openai/gpt-4o-mini".to_string(),
        provider_keys: vec![],
        release_channel_path: temp.path().join("release-channel.json"),
        release_lookup_cache_path: temp.path().join("release-lookup-cache.json"),
        release_lookup_cache_ttl_ms: 900_000,
        browser_automation_playwright_cli: "playwright-cli".to_string(),
        session_enabled: false,
        session_path: temp.path().join("session.jsonl"),
        skills_dir: temp.path().join("skills"),
        skills_lock_path: temp.path().join("skills.lock.json"),
        trust_root_path: None,
        multi_channel_live_readiness: DoctorMultiChannelReadinessConfig::default(),
    };
    let checks =
        run_doctor_checks_with_lookup(&config, DoctorCheckOptions { online: true }, |_| {
            Ok(Some("v999.0.0".to_string()))
        });
    let by_key = checks
        .into_iter()
        .map(|check| (check.key.clone(), check))
        .collect::<HashMap<_, _>>();
    let release_update = by_key.get("release_update").expect("release update check");
    assert_eq!(release_update.status, DoctorStatus::Warn);
    assert_eq!(release_update.code, "update_available");
    assert!(release_update
        .action
        .as_ref()
        .expect("release update action")
        .contains("latest=v999.0.0"));

    let report = execute_doctor_command_with_options(
        &config,
        DoctorCommandOutputFormat::Text,
        DoctorCheckOptions { online: true },
    );
    assert!(report.contains("doctor summary:"));
}

#[test]
fn integration_doctor_command_preserves_session_runtime() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("mkdir");
    std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write skill");
    let lock_path = default_skills_lock_path(&skills_dir);
    std::fs::write(&lock_path, "{}\n").expect("write lock");
    let trust_root_path = temp.path().join("trust-roots.json");
    std::fs::write(&trust_root_path, "[]\n").expect("write trust");

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
    let auth_command_config = test_auth_command_config();
    let mut skills_command_config =
        skills_command_config(&skills_dir, &lock_path, Some(&trust_root_path));
    skills_command_config.doctor_config.session_path = temp.path().join("session.jsonl");

    let action = handle_command_with_session_import_mode(
        "/doctor",
        &mut agent,
        &mut runtime,
        CommandExecutionContext {
            tool_policy_json: &tool_policy_json,
            session_import_mode: SessionImportMode::Merge,
            profile_defaults: &profile_defaults,
            skills_command_config: &skills_command_config,
            auth_command_config: &auth_command_config,
            model_catalog: &ModelCatalog::built_in(),
            extension_commands: &[],
        },
    )
    .expect("doctor command should continue");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.active_head, Some(head));
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(agent.messages().len(), lineage.len());
}

#[test]
fn regression_run_doctor_checks_reports_type_and_readability_errors() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session-as-dir");
    std::fs::create_dir_all(&session_path).expect("mkdir session dir");
    let skills_dir = temp.path().join("skills-as-file");
    std::fs::write(&skills_dir, "not a directory").expect("write skills file");
    let lock_path = temp.path().join("lock-as-dir");
    std::fs::create_dir_all(&lock_path).expect("mkdir lock dir");
    let trust_root_path = temp.path().join("trust-as-dir");
    std::fs::create_dir_all(&trust_root_path).expect("mkdir trust dir");

    let config = DoctorCommandConfig {
        model: "openai/gpt-4o-mini".to_string(),
        provider_keys: vec![DoctorProviderKeyStatus {
            provider_kind: Provider::OpenAi,
            provider: "openai".to_string(),
            key_env_var: "OPENAI_API_KEY".to_string(),
            present: true,
            auth_mode: ProviderAuthMethod::ApiKey,
            mode_supported: true,
            login_backend_enabled: false,
            login_backend_executable: None,
            login_backend_available: false,
        }],
        release_channel_path: temp.path().join("release-channel.json"),
        release_lookup_cache_path: temp.path().join("release-lookup-cache.json"),
        release_lookup_cache_ttl_ms: 900_000,
        browser_automation_playwright_cli: "playwright-cli".to_string(),
        session_enabled: true,
        session_path,
        skills_dir,
        skills_lock_path: lock_path,
        trust_root_path: Some(trust_root_path),
        multi_channel_live_readiness: DoctorMultiChannelReadinessConfig::default(),
    };

    let checks = run_doctor_checks(&config);
    let by_key = checks
        .into_iter()
        .map(|check| (check.key.clone(), check))
        .collect::<HashMap<_, _>>();

    assert_eq!(
        by_key
            .get("provider_auth_mode.openai")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Pass, "api_key".to_string()))
    );

    assert_eq!(
        by_key
            .get("session_path")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Fail, "not_file".to_string()))
    );
    assert_eq!(
        by_key
            .get("skills_dir")
            .map(|item| (item.status, item.code.clone())),
        Some((DoctorStatus::Fail, "not_dir".to_string()))
    );
    let lock = by_key.get("skills_lock").expect("skills lock check");
    assert_eq!(lock.status, DoctorStatus::Fail);
    assert!(lock.code.starts_with("read_error:"));
    let trust = by_key.get("trust_root").expect("trust root check");
    assert_eq!(trust.status, DoctorStatus::Fail);
    assert!(trust.code.starts_with("read_error:"));
}

#[test]
fn regression_run_doctor_checks_reports_invalid_release_channel_store() {
    let temp = tempdir().expect("tempdir");
    let release_channel_path = temp.path().join("release-channel.json");
    std::fs::write(&release_channel_path, "{invalid-json").expect("write malformed release file");

    let config = DoctorCommandConfig {
        model: "openai/gpt-4o-mini".to_string(),
        provider_keys: vec![DoctorProviderKeyStatus {
            provider_kind: Provider::OpenAi,
            provider: "openai".to_string(),
            key_env_var: "OPENAI_API_KEY".to_string(),
            present: true,
            auth_mode: ProviderAuthMethod::ApiKey,
            mode_supported: true,
            login_backend_enabled: false,
            login_backend_executable: None,
            login_backend_available: false,
        }],
        release_channel_path,
        release_lookup_cache_path: temp.path().join("release-lookup-cache.json"),
        release_lookup_cache_ttl_ms: 900_000,
        browser_automation_playwright_cli: "playwright-cli".to_string(),
        session_enabled: false,
        session_path: temp.path().join("session.jsonl"),
        skills_dir: temp.path().join("skills"),
        skills_lock_path: temp.path().join("skills.lock.json"),
        trust_root_path: None,
        multi_channel_live_readiness: DoctorMultiChannelReadinessConfig::default(),
    };

    let checks = run_doctor_checks(&config);
    let by_key = checks
        .into_iter()
        .map(|check| (check.key.clone(), check))
        .collect::<HashMap<_, _>>();
    let release_channel = by_key
        .get("release_channel")
        .expect("release channel check should exist");
    assert_eq!(release_channel.status, DoctorStatus::Fail);
    assert!(release_channel.code.starts_with("invalid_store:"));
    let release_update = by_key
        .get("release_update")
        .expect("release update check should exist");
    assert_eq!(release_update.status, DoctorStatus::Warn);
    assert_eq!(release_update.code, "skipped_offline");
}

#[test]
fn regression_run_doctor_checks_with_online_lookup_surfaces_lookup_errors() {
    let temp = tempdir().expect("tempdir");
    let config = DoctorCommandConfig {
        model: "openai/gpt-4o-mini".to_string(),
        provider_keys: vec![],
        release_channel_path: temp.path().join("release-channel.json"),
        release_lookup_cache_path: temp.path().join("release-lookup-cache.json"),
        release_lookup_cache_ttl_ms: 900_000,
        browser_automation_playwright_cli: "playwright-cli".to_string(),
        session_enabled: false,
        session_path: temp.path().join("session.jsonl"),
        skills_dir: temp.path().join("skills"),
        skills_lock_path: temp.path().join("skills.lock.json"),
        trust_root_path: None,
        multi_channel_live_readiness: DoctorMultiChannelReadinessConfig::default(),
    };
    let checks =
        run_doctor_checks_with_lookup(&config, DoctorCheckOptions { online: true }, |_| {
            Err(anyhow::anyhow!("lookup backend unavailable"))
        });
    let by_key = checks
        .into_iter()
        .map(|check| (check.key.clone(), check))
        .collect::<HashMap<_, _>>();
    let release_update = by_key
        .get("release_update")
        .expect("release update check should exist");
    assert_eq!(release_update.status, DoctorStatus::Warn);
    assert!(release_update.code.starts_with("lookup_error:"));
}

#[test]
fn unit_evaluate_multi_channel_live_readiness_reports_missing_prerequisites() {
    let temp = tempdir().expect("tempdir");
    let ingress_dir = temp.path().join("missing-ingress");
    let credential_store_path = temp.path().join("missing-credentials.json");
    let config = DoctorMultiChannelReadinessConfig {
        ingress_dir: ingress_dir.clone(),
        credential_store_path: credential_store_path.clone(),
        credential_store_encryption: CredentialStoreEncryptionMode::None,
        credential_store_key: None,
        telegram_bot_token: None,
        discord_bot_token: None,
        whatsapp_access_token: None,
        whatsapp_phone_number_id: None,
    };

    let report = evaluate_multi_channel_live_readiness(&config);
    assert_eq!(report.checks.len(), 7);
    assert_eq!(report.pass, 0);
    assert_eq!(report.warn, 3);
    assert_eq!(report.fail, 4);
    assert_eq!(report.gate, "fail");
    assert!(!report.reason_codes.is_empty());

    let by_key = report
        .checks
        .iter()
        .map(|check| (check.key.clone(), check))
        .collect::<HashMap<_, _>>();

    assert_eq!(
        by_key
            .get("multi_channel_live.credential_store")
            .map(|check| (check.status, check.code.clone(), check.path.clone())),
        Some((
            DoctorStatus::Warn,
            "missing".to_string(),
            Some(credential_store_path.display().to_string()),
        ))
    );
    assert_eq!(
        by_key.get("multi_channel_live.ingress_dir").map(|check| (
            check.status,
            check.code.clone(),
            check.path.clone()
        )),
        Some((
            DoctorStatus::Fail,
            "missing".to_string(),
            Some(ingress_dir.display().to_string()),
        ))
    );
    assert_eq!(
        by_key
            .get("multi_channel_live.channel.telegram")
            .map(|check| (check.status, check.code.clone())),
        Some((DoctorStatus::Fail, "missing_prerequisites".to_string()))
    );
    assert_eq!(
        by_key
            .get("multi_channel_live.channel.discord")
            .map(|check| (check.status, check.code.clone())),
        Some((DoctorStatus::Fail, "missing_prerequisites".to_string()))
    );
    assert_eq!(
        by_key
            .get("multi_channel_live.channel.whatsapp")
            .map(|check| (check.status, check.code.clone())),
        Some((DoctorStatus::Fail, "missing_prerequisites".to_string()))
    );
    assert_eq!(
        by_key
            .get("multi_channel_live.channel_policy")
            .map(|check| (check.status, check.code.clone())),
        Some((DoctorStatus::Warn, "missing".to_string()))
    );
    assert_eq!(
        by_key
            .get("multi_channel_live.channel_policy.risk")
            .map(|check| (check.status, check.code.clone())),
        Some((
            DoctorStatus::Warn,
            "unknown_without_policy_file".to_string()
        ))
    );
}

#[test]
fn functional_evaluate_multi_channel_live_readiness_uses_store_backed_secrets() {
    let temp = tempdir().expect("tempdir");
    let ingress_dir = temp.path().join("live-ingress");
    let credential_store_path = temp.path().join("credentials.json");
    std::fs::create_dir_all(&ingress_dir).expect("create ingress dir");
    std::fs::write(ingress_dir.join("telegram.ndjson"), "").expect("write telegram inbox");
    std::fs::write(ingress_dir.join("discord.ndjson"), "").expect("write discord inbox");
    std::fs::write(ingress_dir.join("whatsapp.ndjson"), "").expect("write whatsapp inbox");

    let mut store = CredentialStoreData {
        encryption: CredentialStoreEncryptionMode::None,
        providers: BTreeMap::new(),
        integrations: BTreeMap::new(),
    };
    let timestamp = current_unix_timestamp();
    store.integrations.insert(
        "telegram-bot-token".to_string(),
        IntegrationCredentialStoreRecord {
            secret: Some("telegram-token".to_string()),
            revoked: false,
            updated_unix: Some(timestamp),
        },
    );
    store.integrations.insert(
        "discord-bot-token".to_string(),
        IntegrationCredentialStoreRecord {
            secret: Some("discord-token".to_string()),
            revoked: false,
            updated_unix: Some(timestamp),
        },
    );
    store.integrations.insert(
        "whatsapp-access-token".to_string(),
        IntegrationCredentialStoreRecord {
            secret: Some("whatsapp-access-token".to_string()),
            revoked: false,
            updated_unix: Some(timestamp),
        },
    );
    store.integrations.insert(
        "whatsapp-phone-number-id".to_string(),
        IntegrationCredentialStoreRecord {
            secret: Some("15551234567".to_string()),
            revoked: false,
            updated_unix: Some(timestamp),
        },
    );
    save_credential_store(&credential_store_path, &store, None).expect("save credential store");

    let config = DoctorMultiChannelReadinessConfig {
        ingress_dir,
        credential_store_path,
        credential_store_encryption: CredentialStoreEncryptionMode::None,
        credential_store_key: None,
        telegram_bot_token: None,
        discord_bot_token: None,
        whatsapp_access_token: None,
        whatsapp_phone_number_id: None,
    };

    let report = evaluate_multi_channel_live_readiness(&config);
    assert_eq!(report.checks.len(), 7);
    assert_eq!(report.pass, 5);
    assert_eq!(report.warn, 2);
    assert_eq!(report.fail, 0);
    assert_eq!(report.gate, "pass");
    assert!(report.reason_codes.is_empty());
}

#[test]
fn integration_evaluate_multi_channel_live_readiness_warns_on_unsafe_open_dm_when_not_strict() {
    let temp = tempdir().expect("tempdir");
    let ingress_dir = temp.path().join("live-ingress");
    let credential_store_path = temp.path().join("credentials.json");
    std::fs::create_dir_all(&ingress_dir).expect("create ingress dir");
    std::fs::write(ingress_dir.join("telegram.ndjson"), "").expect("write telegram inbox");
    std::fs::write(ingress_dir.join("discord.ndjson"), "").expect("write discord inbox");
    std::fs::write(ingress_dir.join("whatsapp.ndjson"), "").expect("write whatsapp inbox");

    let mut store = CredentialStoreData {
        encryption: CredentialStoreEncryptionMode::None,
        providers: BTreeMap::new(),
        integrations: BTreeMap::new(),
    };
    let timestamp = current_unix_timestamp();
    for (id, secret) in [
        ("telegram-bot-token", "telegram-token"),
        ("discord-bot-token", "discord-token"),
        ("whatsapp-access-token", "whatsapp-token"),
        ("whatsapp-phone-number-id", "15551234567"),
    ] {
        store.integrations.insert(
            id.to_string(),
            IntegrationCredentialStoreRecord {
                secret: Some(secret.to_string()),
                revoked: false,
                updated_unix: Some(timestamp),
            },
        );
    }
    save_credential_store(&credential_store_path, &store, None).expect("save credential store");

    let security_dir = temp.path().join("security");
    std::fs::create_dir_all(&security_dir).expect("create security dir");
    std::fs::write(
        security_dir.join("channel-policy.json"),
        r#"{
  "schema_version": 1,
  "strictMode": false,
  "defaultPolicy": {
    "dmPolicy": "allow",
    "allowFrom": "any",
    "groupPolicy": "allow",
    "requireMention": false
  }
}
"#,
    )
    .expect("write policy");

    let config = DoctorMultiChannelReadinessConfig {
        ingress_dir,
        credential_store_path,
        credential_store_encryption: CredentialStoreEncryptionMode::None,
        credential_store_key: None,
        telegram_bot_token: None,
        discord_bot_token: None,
        whatsapp_access_token: None,
        whatsapp_phone_number_id: None,
    };
    let report = evaluate_multi_channel_live_readiness(&config);
    let by_key = report
        .checks
        .iter()
        .map(|check| (check.key.clone(), check))
        .collect::<HashMap<_, _>>();
    assert_eq!(
        by_key
            .get("multi_channel_live.channel_policy")
            .map(|check| (check.status, check.code.clone())),
        Some((DoctorStatus::Pass, "ready".to_string()))
    );
    assert_eq!(
        by_key
            .get("multi_channel_live.channel_policy.risk")
            .map(|check| (check.status, check.code.clone())),
        Some((DoctorStatus::Warn, "unsafe_open_dm_warn".to_string()))
    );
    assert_eq!(report.fail, 0);
    assert_eq!(report.gate, "pass");
}

#[test]
fn regression_evaluate_multi_channel_live_readiness_fails_on_unsafe_open_dm_when_strict() {
    let temp = tempdir().expect("tempdir");
    let ingress_dir = temp.path().join("live-ingress");
    let credential_store_path = temp.path().join("credentials.json");
    std::fs::create_dir_all(&ingress_dir).expect("create ingress dir");
    std::fs::write(ingress_dir.join("telegram.ndjson"), "").expect("write telegram inbox");
    std::fs::write(ingress_dir.join("discord.ndjson"), "").expect("write discord inbox");
    std::fs::write(ingress_dir.join("whatsapp.ndjson"), "").expect("write whatsapp inbox");

    let mut store = CredentialStoreData {
        encryption: CredentialStoreEncryptionMode::None,
        providers: BTreeMap::new(),
        integrations: BTreeMap::new(),
    };
    let timestamp = current_unix_timestamp();
    for (id, secret) in [
        ("telegram-bot-token", "telegram-token"),
        ("discord-bot-token", "discord-token"),
        ("whatsapp-access-token", "whatsapp-token"),
        ("whatsapp-phone-number-id", "15551234567"),
    ] {
        store.integrations.insert(
            id.to_string(),
            IntegrationCredentialStoreRecord {
                secret: Some(secret.to_string()),
                revoked: false,
                updated_unix: Some(timestamp),
            },
        );
    }
    save_credential_store(&credential_store_path, &store, None).expect("save credential store");

    let security_dir = temp.path().join("security");
    std::fs::create_dir_all(&security_dir).expect("create security dir");
    std::fs::write(
        security_dir.join("channel-policy.json"),
        r#"{
  "schema_version": 1,
  "strictMode": true,
  "defaultPolicy": {
    "dmPolicy": "allow",
    "allowFrom": "any",
    "groupPolicy": "allow",
    "requireMention": false
  }
}
"#,
    )
    .expect("write policy");

    let config = DoctorMultiChannelReadinessConfig {
        ingress_dir,
        credential_store_path,
        credential_store_encryption: CredentialStoreEncryptionMode::None,
        credential_store_key: None,
        telegram_bot_token: None,
        discord_bot_token: None,
        whatsapp_access_token: None,
        whatsapp_phone_number_id: None,
    };
    let report = evaluate_multi_channel_live_readiness(&config);
    let by_key = report
        .checks
        .iter()
        .map(|check| (check.key.clone(), check))
        .collect::<HashMap<_, _>>();
    assert_eq!(
        by_key
            .get("multi_channel_live.channel_policy.risk")
            .map(|check| (check.status, check.code.clone())),
        Some((DoctorStatus::Fail, "unsafe_open_dm_fail".to_string()))
    );
    assert_eq!(report.fail, 1);
    assert_eq!(report.gate, "fail");
    assert!(report
        .reason_codes
        .contains(&"multi_channel_live.channel_policy.risk:unsafe_open_dm_fail".to_string()));
}

#[test]
fn unit_resolve_session_graph_format_and_escape_label_behaviors() {
    assert_eq!(
        resolve_session_graph_format(Path::new("/tmp/graph.dot")),
        SessionGraphFormat::Dot
    );
    assert_eq!(
        resolve_session_graph_format(Path::new("/tmp/graph.mmd")),
        SessionGraphFormat::Mermaid
    );
    assert_eq!(escape_graph_label("a\"b\\c"), "a\\\"b\\\\c".to_string());
}

#[test]
fn unit_render_session_graph_mermaid_and_dot_include_deterministic_edges() {
    let entries = vec![
        tau_session::SessionEntry {
            id: 2,
            parent_id: Some(1),
            message: Message::user("child"),
        },
        tau_session::SessionEntry {
            id: 1,
            parent_id: None,
            message: Message::system("root"),
        },
    ];

    let mermaid = render_session_graph_mermaid(&entries);
    assert!(mermaid.contains("graph TD"));
    let root_index = mermaid.find("n1[\"1: system | root\"]").expect("root node");
    let child_index = mermaid.find("n2[\"2: user | child\"]").expect("child node");
    assert!(root_index < child_index);
    assert!(mermaid.contains("n1 --> n2"));

    let dot = render_session_graph_dot(&entries);
    assert!(dot.contains("digraph session"));
    assert!(dot.contains("n1 [label=\"1: system | root\"];"));
    assert!(dot.contains("n2 [label=\"2: user | child\"];"));
    assert!(dot.contains("n1 -> n2;"));
}

#[test]
fn functional_execute_session_graph_export_command_writes_mermaid_file() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let _head = store
        .append_messages(Some(root), &[Message::user("child")])
        .expect("append child")
        .expect("head id");
    let runtime = SessionRuntime {
        store,
        active_head: Some(root + 1),
    };
    let destination = temp.path().join("session-graph.mmd");

    let output =
        execute_session_graph_export_command(&runtime, destination.to_str().expect("utf8 path"));
    assert!(output.contains("session graph export: path="));
    assert!(output.contains("format=mermaid"));
    assert!(output.contains("nodes=2"));
    assert!(output.contains("edges=1"));

    let raw = std::fs::read_to_string(destination).expect("read graph");
    assert!(raw.contains("graph TD"));
    assert!(raw.contains("n1 --> n2"));
}

#[test]
fn integration_execute_session_graph_export_command_supports_dot_for_branched_session() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let _main = store
        .append_messages(Some(root), &[Message::user("main")])
        .expect("append main")
        .expect("main id");
    let _branch = store
        .append_messages(Some(root), &[Message::user("branch")])
        .expect("append branch")
        .expect("branch id");
    let runtime = SessionRuntime {
        store,
        active_head: Some(root + 2),
    };
    let destination = temp.path().join("session-graph.dot");

    let output =
        execute_session_graph_export_command(&runtime, destination.to_str().expect("utf8 path"));
    assert!(output.contains("format=dot"));
    assert!(output.contains("nodes=3"));
    assert!(output.contains("edges=2"));

    let raw = std::fs::read_to_string(destination).expect("read graph");
    assert!(raw.contains("digraph session"));
    assert!(raw.contains("n1 -> n2;"));
    assert!(raw.contains("n1 -> n3;"));
}

#[test]
fn regression_execute_session_graph_export_command_rejects_directory_destination() {
    let temp = tempdir().expect("tempdir");
    let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
    let root = store
        .append_messages(None, &[Message::system("root")])
        .expect("append root")
        .expect("root id");
    let runtime = SessionRuntime {
        store,
        active_head: Some(root),
    };
    let destination_dir = temp.path().join("graph-dir");
    std::fs::create_dir_all(&destination_dir).expect("mkdir");

    let output = execute_session_graph_export_command(
        &runtime,
        destination_dir.to_str().expect("utf8 path"),
    );
    assert!(output.contains("session graph export error: path="));
    assert!(output.contains("is a directory"));
}
