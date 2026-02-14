//! Tests for auth provider backend launches, CLI flag validation, and fallback routing behavior.

use super::*;

#[test]
fn regression_execute_auth_command_login_rejects_unsupported_google_session_mode() {
    let config = test_auth_command_config();
    let output = execute_auth_command(&config, "login google --mode session-token --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse output");
    assert_eq!(payload["status"], "error");
    assert!(payload["reason"]
        .as_str()
        .unwrap_or_default()
        .contains("not supported"));
    assert_eq!(
        payload["supported_modes"],
        serde_json::json!(["api_key", "oauth_token", "adc"])
    );
}

#[cfg(unix)]
#[test]
fn functional_execute_auth_command_login_openai_launch_executes_codex_login_command() {
    let temp = tempdir().expect("tempdir");
    let args_file = temp.path().join("codex-login-args.txt");
    let script = write_mock_codex_script(
        temp.path(),
        &format!("printf '%s' \"$*\" > \"{}\"", args_file.display()),
    );

    let mut config = test_auth_command_config();
    config.openai_codex_backend = true;
    config.openai_codex_cli = script.display().to_string();

    let output = execute_auth_command(&config, "login openai --mode oauth-token --launch --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse output");
    assert_eq!(payload["status"], "launched");
    assert_eq!(payload["source"], "codex_cli");
    assert_eq!(payload["launch_requested"], true);
    assert_eq!(payload["launch_executed"], true);
    assert_eq!(
        payload["launch_command"],
        format!("{} --login", script.display())
    );

    let launched_args = std::fs::read_to_string(&args_file).expect("read codex login args");
    assert_eq!(launched_args, "--login");
}

#[cfg(unix)]
#[test]
fn integration_execute_auth_command_login_google_adc_launch_executes_gcloud_flow() {
    let temp = tempdir().expect("tempdir");
    let gcloud_args = temp.path().join("gcloud-login-args.txt");
    let gemini = write_mock_gemini_script(temp.path(), "printf 'ok'");
    let gcloud = write_mock_gcloud_script(
        temp.path(),
        &format!("printf '%s' \"$*\" > \"{}\"", gcloud_args.display()),
    );

    let mut config = test_auth_command_config();
    config.google_gemini_backend = true;
    config.google_gemini_cli = gemini.display().to_string();
    config.google_gcloud_cli = gcloud.display().to_string();

    let output = execute_auth_command(&config, "login google --mode adc --launch --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse output");
    assert_eq!(payload["status"], "launched");
    assert_eq!(payload["source"], "gemini_cli");
    assert_eq!(payload["launch_requested"], true);
    assert_eq!(payload["launch_executed"], true);
    assert_eq!(
        payload["launch_command"],
        format!("{} auth application-default login", gcloud.display())
    );

    let launched_args = std::fs::read_to_string(&gcloud_args).expect("read gcloud args");
    assert_eq!(launched_args, "auth application-default login");
}

#[test]
fn regression_execute_auth_command_login_launch_rejects_unsupported_api_key_mode() {
    let config = test_auth_command_config();
    let output = execute_auth_command(&config, "login openai --mode api-key --launch --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse output");
    assert_eq!(payload["status"], "error");
    assert_eq!(payload["launch_requested"], true);
    assert_eq!(payload["launch_executed"], false);
    assert!(payload["reason"]
        .as_str()
        .unwrap_or_default()
        .contains("--launch is only supported"));
}

#[cfg(unix)]
#[test]
fn regression_execute_auth_command_login_launch_reports_non_zero_exit() {
    let temp = tempdir().expect("tempdir");
    let script = write_mock_claude_script(temp.path(), "exit 9");

    let mut config = test_auth_command_config();
    config.anthropic_claude_backend = true;
    config.anthropic_claude_cli = script.display().to_string();

    let output = execute_auth_command(
        &config,
        "login anthropic --mode oauth-token --launch --json",
    );
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse output");
    assert_eq!(payload["status"], "error");
    assert_eq!(payload["launch_requested"], true);
    assert_eq!(payload["launch_executed"], false);
    assert_eq!(payload["launch_command"], script.display().to_string());
    assert!(payload["reason"]
        .as_str()
        .unwrap_or_default()
        .contains("exited with status 9"));
}

#[test]
fn functional_execute_auth_command_login_anthropic_oauth_reports_backend_ready() {
    let mut config = test_auth_command_config();
    config.anthropic_claude_backend = true;
    config.anthropic_claude_cli = std::env::current_exe()
        .expect("current executable path")
        .display()
        .to_string();

    let output = execute_auth_command(&config, "login anthropic --mode oauth-token --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse output");
    assert_eq!(payload["status"], "ready");
    assert_eq!(payload["source"], "claude_cli");
    assert_eq!(payload["persisted"], false);
    assert_eq!(payload["launch_requested"], false);
    assert_eq!(payload["launch_executed"], false);
    assert!(payload["action"]
        .as_str()
        .unwrap_or_default()
        .contains("enter /login in the Claude prompt"));
}

#[test]
fn regression_execute_auth_command_status_anthropic_oauth_reports_backend_disabled() {
    let mut config = test_auth_command_config();
    config.anthropic_auth_mode = ProviderAuthMethod::OauthToken;
    config.anthropic_claude_backend = false;

    let output = execute_auth_command(&config, "status anthropic --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse status payload");
    let entry = payload["entries"]
        .as_array()
        .and_then(|entries| entries.first())
        .expect("anthropic status entry");
    assert_eq!(entry["provider"], "anthropic");
    assert_eq!(entry["mode"], "oauth_token");
    assert_eq!(entry["mode_supported"], true);
    assert_eq!(entry["available"], false);
    assert_eq!(entry["state"], "backend_disabled");
    assert_eq!(entry["reason_code"], "backend_disabled");
    assert_eq!(entry["backend_required"], true);
    assert_eq!(entry["backend"], "claude_cli");
    assert_eq!(entry["backend_health"], "disabled");
    assert_eq!(entry["backend_reason_code"], "backend_disabled");
    assert_eq!(entry["reauth_required"], false);
}

#[test]
fn regression_execute_auth_command_status_anthropic_oauth_reports_backend_unavailable() {
    let mut config = test_auth_command_config();
    config.anthropic_auth_mode = ProviderAuthMethod::OauthToken;
    config.anthropic_claude_backend = true;
    config.anthropic_claude_cli = "__missing_claude_backend_for_test__".to_string();

    let output = execute_auth_command(&config, "status anthropic --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse status payload");
    let entry = payload["entries"]
        .as_array()
        .and_then(|entries| entries.first())
        .expect("anthropic status entry");
    assert_eq!(entry["provider"], "anthropic");
    assert_eq!(entry["mode"], "oauth_token");
    assert_eq!(entry["mode_supported"], true);
    assert_eq!(entry["available"], false);
    assert_eq!(entry["state"], "backend_unavailable");
    assert_eq!(entry["reason_code"], "backend_unavailable");
    assert_eq!(entry["backend_required"], true);
    assert_eq!(entry["backend"], "claude_cli");
    assert_eq!(entry["backend_health"], "unavailable");
    assert_eq!(entry["backend_reason_code"], "backend_unavailable");
    assert_eq!(entry["reauth_required"], false);
}

#[test]
fn functional_execute_auth_command_login_google_oauth_reports_backend_ready() {
    let mut config = test_auth_command_config();
    config.google_gemini_backend = true;
    config.google_gemini_cli = std::env::current_exe()
        .expect("current executable path")
        .display()
        .to_string();

    let output = execute_auth_command(&config, "login google --mode oauth-token --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse output");
    assert_eq!(payload["status"], "ready");
    assert_eq!(payload["source"], "gemini_cli");
    assert_eq!(payload["persisted"], false);
    assert_eq!(payload["launch_requested"], false);
    assert_eq!(payload["launch_executed"], false);
    assert!(payload["action"]
        .as_str()
        .unwrap_or_default()
        .contains("Login with Google"));
}

#[test]
fn regression_execute_auth_command_status_google_oauth_reports_backend_disabled() {
    let mut config = test_auth_command_config();
    config.google_auth_mode = ProviderAuthMethod::OauthToken;
    config.google_gemini_backend = false;

    let output = execute_auth_command(&config, "status google --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse status payload");
    let entry = payload["entries"]
        .as_array()
        .and_then(|entries| entries.first())
        .expect("google status entry");
    assert_eq!(entry["provider"], "google");
    assert_eq!(entry["mode"], "oauth_token");
    assert_eq!(entry["mode_supported"], true);
    assert_eq!(entry["available"], false);
    assert_eq!(entry["state"], "backend_disabled");
    assert_eq!(entry["reason_code"], "backend_disabled");
    assert_eq!(entry["backend_required"], true);
    assert_eq!(entry["backend"], "gemini_cli");
    assert_eq!(entry["backend_health"], "disabled");
    assert_eq!(entry["backend_reason_code"], "backend_disabled");
    assert_eq!(entry["reauth_required"], false);
}

#[test]
fn regression_execute_auth_command_status_google_oauth_reports_backend_unavailable() {
    let mut config = test_auth_command_config();
    config.google_auth_mode = ProviderAuthMethod::OauthToken;
    config.google_gemini_backend = true;
    config.google_gemini_cli = "__missing_gemini_backend_for_test__".to_string();

    let output = execute_auth_command(&config, "status google --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse status payload");
    let entry = payload["entries"]
        .as_array()
        .and_then(|entries| entries.first())
        .expect("google status entry");
    assert_eq!(entry["provider"], "google");
    assert_eq!(entry["mode"], "oauth_token");
    assert_eq!(entry["mode_supported"], true);
    assert_eq!(entry["available"], false);
    assert_eq!(entry["state"], "backend_unavailable");
    assert_eq!(entry["reason_code"], "backend_unavailable");
    assert_eq!(entry["backend_required"], true);
    assert_eq!(entry["backend"], "gemini_cli");
    assert_eq!(entry["backend_health"], "unavailable");
    assert_eq!(entry["backend_reason_code"], "backend_unavailable");
    assert_eq!(entry["reauth_required"], false);
}

#[test]
fn unit_cli_skills_lock_flags_default_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.skills_lock_write);
    assert!(!cli.skills_sync);
    assert!(cli.skills_lock_file.is_none());
}

#[test]
fn functional_cli_skills_lock_flags_accept_explicit_values() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--skills-lock-write",
        "--skills-sync",
        "--skills-lock-file",
        "custom/skills.lock.json",
    ]);
    assert!(cli.skills_lock_write);
    assert!(cli.skills_sync);
    assert_eq!(
        cli.skills_lock_file,
        Some(PathBuf::from("custom/skills.lock.json"))
    );
}

#[test]
fn unit_cli_skills_cache_flags_default_to_online_mode() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.skills_offline);
    assert!(cli.skills_cache_dir.is_none());
}

#[test]
fn functional_cli_skills_cache_flags_accept_explicit_values() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--skills-offline",
        "--skills-cache-dir",
        "custom/skills-cache",
    ]);
    assert!(cli.skills_offline);
    assert_eq!(
        cli.skills_cache_dir,
        Some(PathBuf::from("custom/skills-cache"))
    );
}

#[test]
fn unit_cli_command_file_flags_default_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(cli.command_file.is_none());
    assert_eq!(
        cli.command_file_error_mode,
        CliCommandFileErrorMode::FailFast
    );
}

#[test]
fn functional_cli_command_file_flags_accept_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--command-file",
        "automation.commands",
        "--command-file-error-mode",
        "continue-on-error",
    ]);
    assert_eq!(cli.command_file, Some(PathBuf::from("automation.commands")));
    assert_eq!(
        cli.command_file_error_mode,
        CliCommandFileErrorMode::ContinueOnError
    );
}

#[test]
fn unit_cli_onboarding_flags_default_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.onboard);
    assert!(!cli.onboard_non_interactive);
    assert_eq!(cli.onboard_profile, "default");
    assert_eq!(cli.onboard_release_channel, None);
    assert!(!cli.onboard_install_daemon);
    assert!(!cli.onboard_start_daemon);
}

#[test]
fn functional_cli_onboarding_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--onboard",
        "--onboard-non-interactive",
        "--onboard-profile",
        "team_default",
        "--onboard-release-channel",
        "beta",
        "--onboard-install-daemon",
        "--onboard-start-daemon",
    ]);
    assert!(cli.onboard);
    assert!(cli.onboard_non_interactive);
    assert_eq!(cli.onboard_profile, "team_default");
    assert_eq!(cli.onboard_release_channel, Some("beta".to_string()));
    assert!(cli.onboard_install_daemon);
    assert!(cli.onboard_start_daemon);
}

#[test]
fn regression_cli_onboarding_non_interactive_requires_onboard() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--onboard-non-interactive"]);
    let error = parse.expect_err("non-interactive onboarding should require --onboard");
    assert!(error.to_string().contains("--onboard"));
}

#[test]
fn regression_cli_onboarding_profile_requires_onboard() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--onboard-profile", "team"]);
    let error = parse.expect_err("onboarding profile should require --onboard");
    assert!(error.to_string().contains("--onboard"));
}

#[test]
fn regression_cli_onboarding_release_channel_requires_onboard() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--onboard-release-channel", "beta"]);
    let error = parse.expect_err("onboarding release channel should require --onboard");
    assert!(error.to_string().contains("--onboard"));
}

#[test]
fn regression_cli_onboarding_install_daemon_requires_onboard() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--onboard-install-daemon"]);
    let error = parse.expect_err("onboarding daemon install should require --onboard");
    assert!(error.to_string().contains("--onboard"));
}

#[test]
fn regression_cli_onboarding_start_daemon_requires_onboarding_install_flag() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--onboard", "--onboard-start-daemon"]);
    let error = parse.expect_err("onboarding daemon start should require daemon install flag");
    assert!(error.to_string().contains("--onboard-install-daemon"));
}

#[test]
fn unit_cli_doctor_release_cache_flags_default_values_are_stable() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert_eq!(
        cli.doctor_release_cache_file,
        PathBuf::from(".tau/release-lookup-cache.json")
    );
    assert_eq!(cli.doctor_release_cache_ttl_ms, 900_000);
}

#[test]
fn functional_cli_doctor_release_cache_flags_accept_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--doctor-release-cache-file",
        "/tmp/custom-doctor-cache.json",
        "--doctor-release-cache-ttl-ms",
        "120000",
    ]);
    assert_eq!(
        cli.doctor_release_cache_file,
        PathBuf::from("/tmp/custom-doctor-cache.json")
    );
    assert_eq!(cli.doctor_release_cache_ttl_ms, 120_000);
}

#[test]
fn regression_cli_doctor_release_cache_ttl_rejects_zero() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--doctor-release-cache-ttl-ms", "0"]);
    let error = parse.expect_err("zero ttl should be rejected");
    assert!(error.to_string().contains("value must be greater than 0"));
}

#[test]
fn unit_is_retryable_provider_error_classifies_status_errors() {
    assert!(is_retryable_provider_error(&TauAiError::HttpStatus {
        status: 429,
        body: "rate limited".to_string(),
    }));
    assert!(is_retryable_provider_error(&TauAiError::HttpStatus {
        status: 503,
        body: "unavailable".to_string(),
    }));
    assert!(!is_retryable_provider_error(&TauAiError::HttpStatus {
        status: 401,
        body: "unauthorized".to_string(),
    }));
    assert!(!is_retryable_provider_error(&TauAiError::InvalidResponse(
        "bad payload".to_string(),
    )));
}

#[test]
fn functional_resolve_fallback_models_parses_deduplicates_and_skips_primary() {
    let primary = ModelRef::parse("openai/gpt-4o-mini").expect("primary model parse");
    let mut cli = test_cli();
    cli.fallback_model = vec![
        "openai/gpt-4o-mini".to_string(),
        "google/gemini-2.5-pro".to_string(),
        "google/gemini-2.5-pro".to_string(),
        "anthropic/claude-sonnet-4-20250514".to_string(),
    ];

    let resolved = resolve_fallback_models(&cli, &primary).expect("resolve fallbacks");
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].provider, Provider::Google);
    assert_eq!(resolved[0].model, "gemini-2.5-pro");
    assert_eq!(resolved[1].provider, Provider::Anthropic);
}

#[tokio::test]
async fn functional_fallback_routing_client_uses_next_route_for_retryable_error() {
    let primary = Arc::new(SequenceClient {
        outcomes: AsyncMutex::new(VecDeque::from([Err(TauAiError::HttpStatus {
            status: 503,
            body: "unavailable".to_string(),
        })])),
    });
    let fallback = Arc::new(SequenceClient {
        outcomes: AsyncMutex::new(VecDeque::from([Ok(ChatResponse {
            message: Message::assistant_text("fallback success"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        })])),
    });

    let client = FallbackRoutingClient::new(
        vec![
            ClientRoute {
                provider: Provider::OpenAi,
                model: "gpt-primary".to_string(),
                client: primary as Arc<dyn LlmClient>,
            },
            ClientRoute {
                provider: Provider::Anthropic,
                model: "claude-fallback".to_string(),
                client: fallback as Arc<dyn LlmClient>,
            },
        ],
        None,
    );

    let response = client
        .complete(test_chat_request())
        .await
        .expect("fallback should recover request");
    assert_eq!(response.message.text_content(), "fallback success");
}

#[tokio::test]
async fn regression_fallback_routing_client_skips_fallback_on_non_retryable_error() {
    let primary = Arc::new(SequenceClient {
        outcomes: AsyncMutex::new(VecDeque::from([Err(TauAiError::HttpStatus {
            status: 400,
            body: "bad request".to_string(),
        })])),
    });
    let fallback = Arc::new(SequenceClient {
        outcomes: AsyncMutex::new(VecDeque::from([Ok(ChatResponse {
            message: Message::assistant_text("should not run"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        })])),
    });

    let client = FallbackRoutingClient::new(
        vec![
            ClientRoute {
                provider: Provider::OpenAi,
                model: "gpt-primary".to_string(),
                client: primary as Arc<dyn LlmClient>,
            },
            ClientRoute {
                provider: Provider::Google,
                model: "gemini-fallback".to_string(),
                client: fallback.clone() as Arc<dyn LlmClient>,
            },
        ],
        None,
    );

    let error = client
        .complete(test_chat_request())
        .await
        .expect_err("non-retryable error should return immediately");
    match error {
        TauAiError::HttpStatus { status, body } => {
            assert_eq!(status, 400);
            assert!(body.contains("bad request"));
        }
        other => panic!("expected HttpStatus error, got {other:?}"),
    }

    let fallback_remaining = fallback.outcomes.lock().await.len();
    assert_eq!(
        fallback_remaining, 1,
        "fallback route should not be invoked"
    );
}

#[tokio::test]
async fn integration_fallback_routing_client_emits_json_event_on_failover() {
    let primary = Arc::new(SequenceClient {
        outcomes: AsyncMutex::new(VecDeque::from([Err(TauAiError::HttpStatus {
            status: 429,
            body: "rate limited".to_string(),
        })])),
    });
    let fallback = Arc::new(SequenceClient {
        outcomes: AsyncMutex::new(VecDeque::from([Ok(ChatResponse {
            message: Message::assistant_text("fallback ok"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        })])),
    });
    let events = Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
    let sink_events = events.clone();
    let sink = Arc::new(move |event: serde_json::Value| {
        sink_events.lock().expect("event lock").push(event);
    });

    let client = FallbackRoutingClient::new(
        vec![
            ClientRoute {
                provider: Provider::OpenAi,
                model: "gpt-primary".to_string(),
                client: primary as Arc<dyn LlmClient>,
            },
            ClientRoute {
                provider: Provider::OpenAi,
                model: "gpt-fallback".to_string(),
                client: fallback as Arc<dyn LlmClient>,
            },
        ],
        Some(sink),
    );

    let _ = client
        .complete(test_chat_request())
        .await
        .expect("fallback should succeed");

    let events = events.lock().expect("event lock");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["type"], "provider_fallback");
    assert_eq!(events[0]["from_model"], "openai/gpt-primary");
    assert_eq!(events[0]["to_model"], "openai/gpt-fallback");
    assert_eq!(events[0]["error_kind"], "http_status");
    assert_eq!(events[0]["status"], 429);
    assert_eq!(events[0]["fallback_index"], 1);
}
