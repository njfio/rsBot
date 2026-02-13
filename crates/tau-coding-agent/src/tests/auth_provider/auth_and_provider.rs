use super::*;

#[test]
fn resolve_api_key_uses_first_non_empty_candidate() {
    let key = resolve_api_key(vec![
        Some("".to_string()),
        Some("  ".to_string()),
        Some("abc".to_string()),
        Some("def".to_string()),
    ]);

    assert_eq!(key, Some("abc".to_string()));
}

#[test]
fn unit_parse_auth_command_supports_login_status_logout_and_json() {
    let login = parse_auth_command("login openai --mode oauth-token --launch --json")
        .expect("parse auth login");
    assert_eq!(
        login,
        AuthCommand::Login {
            provider: Provider::OpenAi,
            mode: Some(ProviderAuthMethod::OauthToken),
            launch: true,
            json_output: true,
        }
    );

    let status = parse_auth_command("status anthropic --json").expect("parse auth status");
    assert_eq!(
        status,
        AuthCommand::Status {
            provider: Some(Provider::Anthropic),
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let filtered_status =
        parse_auth_command("status --availability unavailable openai --state Ready --json")
            .expect("parse filtered auth status");
    assert_eq!(
        filtered_status,
        AuthCommand::Status {
            provider: Some(Provider::OpenAi),
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::Unavailable,
            state: Some("ready".to_string()),
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let mode_filtered_status = parse_auth_command("status openai --mode session-token --json")
        .expect("parse mode-filtered auth status");
    assert_eq!(
        mode_filtered_status,
        AuthCommand::Status {
            provider: Some(Provider::OpenAi),
            mode: Some(ProviderAuthMethod::SessionToken),
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let supported_status = parse_auth_command("status --mode-support supported --json")
        .expect("parse mode-support filtered auth status");
    assert_eq!(
        supported_status,
        AuthCommand::Status {
            provider: None,
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::Supported,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let source_kind_filtered_status = parse_auth_command("status --source-kind env --json")
        .expect("parse source-kind filtered auth status");
    assert_eq!(
        source_kind_filtered_status,
        AuthCommand::Status {
            provider: None,
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::Env,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let revoked_filtered_status = parse_auth_command("status --revoked revoked --json")
        .expect("parse revoked filtered auth status");
    assert_eq!(
        revoked_filtered_status,
        AuthCommand::Status {
            provider: None,
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::Revoked,
            json_output: true,
        }
    );

    let logout = parse_auth_command("logout google").expect("parse auth logout");
    assert_eq!(
        logout,
        AuthCommand::Logout {
            provider: Provider::Google,
            json_output: false,
        }
    );

    let reauth = parse_auth_command("reauth openai --mode oauth-token --launch --json")
        .expect("parse auth reauth");
    assert_eq!(
        reauth,
        AuthCommand::Reauth {
            provider: Provider::OpenAi,
            mode: Some(ProviderAuthMethod::OauthToken),
            launch: true,
            json_output: true,
        }
    );

    let openrouter_login =
        parse_auth_command("login openrouter --mode api-key").expect("parse openrouter login");
    assert_eq!(
        openrouter_login,
        AuthCommand::Login {
            provider: Provider::OpenAi,
            mode: Some(ProviderAuthMethod::ApiKey),
            launch: false,
            json_output: false,
        }
    );

    let groq_login = parse_auth_command("login groq --mode api-key").expect("parse groq login");
    assert_eq!(
        groq_login,
        AuthCommand::Login {
            provider: Provider::OpenAi,
            mode: Some(ProviderAuthMethod::ApiKey),
            launch: false,
            json_output: false,
        }
    );

    let xai_login = parse_auth_command("login xai --mode api-key").expect("parse xai login");
    assert_eq!(
        xai_login,
        AuthCommand::Login {
            provider: Provider::OpenAi,
            mode: Some(ProviderAuthMethod::ApiKey),
            launch: false,
            json_output: false,
        }
    );

    let mistral_login =
        parse_auth_command("login mistral --mode api-key").expect("parse mistral login");
    assert_eq!(
        mistral_login,
        AuthCommand::Login {
            provider: Provider::OpenAi,
            mode: Some(ProviderAuthMethod::ApiKey),
            launch: false,
            json_output: false,
        }
    );

    let azure_login = parse_auth_command("login azure --mode api-key").expect("parse azure login");
    assert_eq!(
        azure_login,
        AuthCommand::Login {
            provider: Provider::OpenAi,
            mode: Some(ProviderAuthMethod::ApiKey),
            launch: false,
            json_output: false,
        }
    );

    let matrix = parse_auth_command("matrix --json").expect("parse auth matrix");
    assert_eq!(
        matrix,
        AuthCommand::Matrix {
            provider: None,
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let filtered_matrix = parse_auth_command("matrix openai --mode oauth-token --json")
        .expect("parse filtered auth matrix");
    assert_eq!(
        filtered_matrix,
        AuthCommand::Matrix {
            provider: Some(Provider::OpenAi),
            mode: Some(ProviderAuthMethod::OauthToken),
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let available_only_matrix = parse_auth_command("matrix --availability available --json")
        .expect("parse available-only auth matrix");
    assert_eq!(
        available_only_matrix,
        AuthCommand::Matrix {
            provider: None,
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::Available,
            state: None,
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let state_filtered_matrix = parse_auth_command("matrix --state ready --json")
        .expect("parse state-filtered auth matrix");
    assert_eq!(
        state_filtered_matrix,
        AuthCommand::Matrix {
            provider: None,
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::All,
            state: Some("ready".to_string()),
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let supported_only_matrix = parse_auth_command("matrix --mode-support supported --json")
        .expect("parse supported-only auth matrix");
    assert_eq!(
        supported_only_matrix,
        AuthCommand::Matrix {
            provider: None,
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::Supported,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let source_kind_filtered_matrix =
        parse_auth_command("matrix --source-kind credential-store --json")
            .expect("parse source-kind filtered auth matrix");
    assert_eq!(
        source_kind_filtered_matrix,
        AuthCommand::Matrix {
            provider: None,
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::CredentialStore,
            revoked: AuthRevokedFilter::All,
            json_output: true,
        }
    );

    let revoked_filtered_matrix = parse_auth_command("matrix --revoked not-revoked --json")
        .expect("parse revoked filtered auth matrix");
    assert_eq!(
        revoked_filtered_matrix,
        AuthCommand::Matrix {
            provider: None,
            mode: None,
            mode_support: AuthMatrixModeSupportFilter::All,
            availability: AuthMatrixAvailabilityFilter::All,
            state: None,
            source_kind: AuthSourceKindFilter::All,
            revoked: AuthRevokedFilter::NotRevoked,
            json_output: true,
        }
    );
}

#[test]
fn regression_parse_auth_command_rejects_unknown_provider_mode_and_usage_errors() {
    let unknown_provider =
        parse_auth_command("login mystery --mode oauth-token").expect_err("provider fail");
    assert!(unknown_provider.to_string().contains("unknown provider"));

    let unknown_mode = parse_auth_command("login openai --mode unknown").expect_err("mode fail");
    assert!(unknown_mode.to_string().contains("unknown auth mode"));

    let missing_login_provider = parse_auth_command("login").expect_err("usage fail for login");
    assert!(missing_login_provider
        .to_string()
        .contains("usage: /auth login"));

    let duplicate_login_launch =
        parse_auth_command("login openai --launch --launch").expect_err("duplicate launch flag");
    assert!(duplicate_login_launch
        .to_string()
        .contains("usage: /auth login"));

    let missing_reauth_provider = parse_auth_command("reauth").expect_err("usage fail for reauth");
    assert!(missing_reauth_provider
        .to_string()
        .contains("usage: /auth reauth"));

    let duplicate_reauth_launch =
        parse_auth_command("reauth openai --launch --launch").expect_err("duplicate reauth launch");
    assert!(duplicate_reauth_launch
        .to_string()
        .contains("usage: /auth reauth"));

    let invalid_matrix_args =
        parse_auth_command("matrix openai anthropic").expect_err("matrix args fail");
    assert!(invalid_matrix_args
        .to_string()
        .contains("usage: /auth matrix"));

    let missing_matrix_mode = parse_auth_command("matrix --mode").expect_err("missing matrix mode");
    assert!(missing_matrix_mode
        .to_string()
        .contains("usage: /auth matrix"));

    let missing_matrix_mode_support =
        parse_auth_command("matrix --mode-support").expect_err("missing matrix mode-support");
    assert!(missing_matrix_mode_support
        .to_string()
        .contains("usage: /auth matrix"));

    let duplicate_matrix_mode_support =
        parse_auth_command("matrix --mode-support all --mode-support supported")
            .expect_err("duplicate matrix mode-support");
    assert!(duplicate_matrix_mode_support
        .to_string()
        .contains("usage: /auth matrix"));

    let unknown_matrix_mode_support =
        parse_auth_command("matrix --mode-support maybe").expect_err("unknown matrix mode-support");
    assert!(unknown_matrix_mode_support
        .to_string()
        .contains("unknown mode-support filter"));

    let missing_matrix_availability =
        parse_auth_command("matrix --availability").expect_err("missing matrix availability");
    assert!(missing_matrix_availability
        .to_string()
        .contains("usage: /auth matrix"));

    let missing_status_availability =
        parse_auth_command("status --availability").expect_err("missing status availability");
    assert!(missing_status_availability
        .to_string()
        .contains("usage: /auth status"));

    let missing_status_mode_support =
        parse_auth_command("status --mode-support").expect_err("missing status mode-support");
    assert!(missing_status_mode_support
        .to_string()
        .contains("usage: /auth status"));

    let missing_status_mode = parse_auth_command("status --mode").expect_err("missing status mode");
    assert!(missing_status_mode
        .to_string()
        .contains("usage: /auth status"));

    let duplicate_status_availability =
        parse_auth_command("status --availability all --availability unavailable")
            .expect_err("duplicate status availability");
    assert!(duplicate_status_availability
        .to_string()
        .contains("usage: /auth status"));

    let duplicate_status_mode =
        parse_auth_command("status --mode api-key --mode adc").expect_err("duplicate status mode");
    assert!(duplicate_status_mode
        .to_string()
        .contains("usage: /auth status"));

    let duplicate_status_mode_support =
        parse_auth_command("status --mode-support all --mode-support supported")
            .expect_err("duplicate status mode-support");
    assert!(duplicate_status_mode_support
        .to_string()
        .contains("usage: /auth status"));

    let unknown_matrix_availability = parse_auth_command("matrix --availability sometimes")
        .expect_err("unknown matrix availability");
    assert!(unknown_matrix_availability
        .to_string()
        .contains("unknown availability filter"));

    let unknown_status_availability = parse_auth_command("status --availability sometimes")
        .expect_err("unknown status availability");
    assert!(unknown_status_availability
        .to_string()
        .contains("unknown availability filter"));

    let unknown_status_mode_support =
        parse_auth_command("status --mode-support maybe").expect_err("unknown status mode-support");
    assert!(unknown_status_mode_support
        .to_string()
        .contains("unknown mode-support filter"));

    let unknown_status_mode =
        parse_auth_command("status --mode impossible").expect_err("unknown status mode");
    assert!(unknown_status_mode
        .to_string()
        .contains("unknown auth mode"));

    let missing_matrix_state =
        parse_auth_command("matrix --state").expect_err("missing matrix state filter");
    assert!(missing_matrix_state
        .to_string()
        .contains("usage: /auth matrix"));

    let missing_status_state =
        parse_auth_command("status --state").expect_err("missing status state filter");
    assert!(missing_status_state
        .to_string()
        .contains("usage: /auth status"));

    let duplicate_status_state = parse_auth_command("status --state ready --state revoked")
        .expect_err("duplicate status state filter");
    assert!(duplicate_status_state
        .to_string()
        .contains("usage: /auth status"));

    let duplicate_matrix_state = parse_auth_command("matrix --state ready --state revoked")
        .expect_err("duplicate matrix state filter");
    assert!(duplicate_matrix_state
        .to_string()
        .contains("usage: /auth matrix"));

    let missing_status_source_kind =
        parse_auth_command("status --source-kind").expect_err("missing status source-kind");
    assert!(missing_status_source_kind
        .to_string()
        .contains("usage: /auth status"));

    let duplicate_status_source_kind =
        parse_auth_command("status --source-kind all --source-kind env")
            .expect_err("duplicate status source-kind");
    assert!(duplicate_status_source_kind
        .to_string()
        .contains("usage: /auth status"));

    let unknown_status_source_kind = parse_auth_command("status --source-kind wildcard")
        .expect_err("unknown status source-kind");
    assert!(unknown_status_source_kind
        .to_string()
        .contains("unknown source-kind filter"));

    let missing_matrix_source_kind =
        parse_auth_command("matrix --source-kind").expect_err("missing matrix source-kind");
    assert!(missing_matrix_source_kind
        .to_string()
        .contains("usage: /auth matrix"));

    let duplicate_matrix_source_kind =
        parse_auth_command("matrix --source-kind all --source-kind env")
            .expect_err("duplicate matrix source-kind");
    assert!(duplicate_matrix_source_kind
        .to_string()
        .contains("usage: /auth matrix"));

    let unknown_matrix_source_kind = parse_auth_command("matrix --source-kind wildcard")
        .expect_err("unknown matrix source-kind");
    assert!(unknown_matrix_source_kind
        .to_string()
        .contains("unknown source-kind filter"));

    let missing_status_revoked =
        parse_auth_command("status --revoked").expect_err("missing status revoked filter");
    assert!(missing_status_revoked
        .to_string()
        .contains("usage: /auth status"));

    let duplicate_status_revoked = parse_auth_command("status --revoked all --revoked revoked")
        .expect_err("duplicate status revoked filter");
    assert!(duplicate_status_revoked
        .to_string()
        .contains("usage: /auth status"));

    let unknown_status_revoked =
        parse_auth_command("status --revoked maybe").expect_err("unknown status revoked filter");
    assert!(unknown_status_revoked
        .to_string()
        .contains("unknown revoked filter"));

    let missing_matrix_revoked =
        parse_auth_command("matrix --revoked").expect_err("missing matrix revoked filter");
    assert!(missing_matrix_revoked
        .to_string()
        .contains("usage: /auth matrix"));

    let duplicate_matrix_revoked = parse_auth_command("matrix --revoked all --revoked revoked")
        .expect_err("duplicate matrix revoked filter");
    assert!(duplicate_matrix_revoked
        .to_string()
        .contains("usage: /auth matrix"));

    let unknown_matrix_revoked =
        parse_auth_command("matrix --revoked maybe").expect_err("unknown matrix revoked filter");
    assert!(unknown_matrix_revoked
        .to_string()
        .contains("unknown revoked filter"));

    let unknown_subcommand = parse_auth_command("noop").expect_err("subcommand fail");
    assert!(unknown_subcommand.to_string().contains("usage: /auth"));
}

#[test]
fn unit_parse_integration_auth_command_supports_set_status_rotate_revoke_and_json() {
    let set = parse_integration_auth_command("set github-token ghp_token --json")
        .expect("parse integration set");
    assert_eq!(
        set,
        IntegrationAuthCommand::Set {
            integration_id: "github-token".to_string(),
            secret: "ghp_token".to_string(),
            json_output: true,
        }
    );

    let status = parse_integration_auth_command("status slack-app-token --json")
        .expect("parse integration status");
    assert_eq!(
        status,
        IntegrationAuthCommand::Status {
            integration_id: Some("slack-app-token".to_string()),
            json_output: true,
        }
    );

    let rotate = parse_integration_auth_command("rotate slack-bot-token next_secret")
        .expect("parse integration rotate");
    assert_eq!(
        rotate,
        IntegrationAuthCommand::Rotate {
            integration_id: "slack-bot-token".to_string(),
            secret: "next_secret".to_string(),
            json_output: false,
        }
    );

    let revoke = parse_integration_auth_command("revoke event-webhook-secret")
        .expect("parse integration revoke");
    assert_eq!(
        revoke,
        IntegrationAuthCommand::Revoke {
            integration_id: "event-webhook-secret".to_string(),
            json_output: false,
        }
    );
}

#[test]
fn regression_parse_integration_auth_command_rejects_usage_and_invalid_ids() {
    let error = parse_integration_auth_command("set github-token").expect_err("missing secret");
    assert!(error
        .to_string()
        .contains("usage: /integration-auth set <integration-id> <secret> [--json]"));

    let error = parse_integration_auth_command("status bad$id").expect_err("invalid id");
    assert!(error.to_string().contains("contains unsupported character"));

    let error = parse_integration_auth_command("unknown").expect_err("unknown subcommand");
    assert!(error
        .to_string()
        .contains("usage: /integration-auth <set|status|rotate|revoke> ..."));
}

#[test]
fn unit_auth_conformance_provider_capability_matrix_matches_expected_support() {
    let cases = vec![
        (
            Provider::OpenAi,
            ProviderAuthMethod::ApiKey,
            true,
            "supported",
        ),
        (
            Provider::OpenAi,
            ProviderAuthMethod::OauthToken,
            true,
            "supported",
        ),
        (
            Provider::OpenAi,
            ProviderAuthMethod::SessionToken,
            true,
            "supported",
        ),
        (
            Provider::OpenAi,
            ProviderAuthMethod::Adc,
            false,
            "not_implemented",
        ),
        (
            Provider::Anthropic,
            ProviderAuthMethod::ApiKey,
            true,
            "supported",
        ),
        (
            Provider::Anthropic,
            ProviderAuthMethod::OauthToken,
            true,
            "supported",
        ),
        (
            Provider::Anthropic,
            ProviderAuthMethod::SessionToken,
            true,
            "supported",
        ),
        (
            Provider::Anthropic,
            ProviderAuthMethod::Adc,
            false,
            "not_implemented",
        ),
        (
            Provider::Google,
            ProviderAuthMethod::ApiKey,
            true,
            "supported",
        ),
        (
            Provider::Google,
            ProviderAuthMethod::OauthToken,
            true,
            "supported",
        ),
        (
            Provider::Google,
            ProviderAuthMethod::SessionToken,
            false,
            "unsupported",
        ),
        (Provider::Google, ProviderAuthMethod::Adc, true, "supported"),
    ];

    for (provider, mode, expected_supported, expected_reason) in cases {
        let capability = provider_auth_capability(provider, mode);
        assert_eq!(capability.supported, expected_supported);
        assert_eq!(capability.reason, expected_reason);
    }
}

#[test]
fn unit_auth_state_count_helpers_are_deterministic() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let snapshot = snapshot_env_vars(&[
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
        "GROQ_API_KEY",
        "XAI_API_KEY",
        "MISTRAL_API_KEY",
        "AZURE_OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        "TAU_API_KEY",
    ]);
    for key in [
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
        "GROQ_API_KEY",
        "XAI_API_KEY",
        "MISTRAL_API_KEY",
        "AZURE_OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        "TAU_API_KEY",
    ] {
        std::env::remove_var(key);
    }

    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-state-helper-counts.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::SessionToken;
    config.openai_api_key = None;
    config.anthropic_api_key = None;
    config.google_api_key = None;

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("helper-revoked-access".to_string()),
            refresh_token: Some("helper-revoked-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(300)),
            revoked: true,
        },
    );
    let store = load_credential_store(
        &config.credential_store,
        config.credential_store_encryption,
        config.credential_store_key.as_deref(),
    )
    .expect("load helper credential store");

    let rows = vec![
        auth_status_row_for_provider(&config, Provider::OpenAi, Some(&store), None),
        auth_status_row_for_provider(&config, Provider::Anthropic, None, None),
        auth_status_row_for_provider(&config, Provider::Google, None, None),
    ];
    let mode_counts = auth_mode_counts(&rows);
    assert_eq!(mode_counts.get("session_token"), Some(&1));
    assert_eq!(mode_counts.get("api_key"), Some(&2));
    assert_eq!(
        format_auth_state_counts(&mode_counts),
        "api_key:2,session_token:1"
    );
    let provider_counts = auth_provider_counts(&rows);
    assert_eq!(provider_counts.get("openai"), Some(&1));
    assert_eq!(provider_counts.get("anthropic"), Some(&1));
    assert_eq!(provider_counts.get("google"), Some(&1));
    assert_eq!(
        format_auth_state_counts(&provider_counts),
        "anthropic:1,google:1,openai:1"
    );
    let availability_counts = auth_availability_counts(&rows);
    assert_eq!(availability_counts.get("available"), None);
    assert_eq!(availability_counts.get("unavailable"), Some(&3));
    assert_eq!(
        format_auth_state_counts(&availability_counts),
        "unavailable:3"
    );
    let counts = auth_state_counts(&rows);
    assert_eq!(counts.get("revoked"), Some(&1));
    assert_eq!(counts.get("missing_api_key"), Some(&2));
    let source_kind_counts = auth_source_kind_counts(&rows);
    assert_eq!(source_kind_counts.get("credential_store"), Some(&1));
    assert_eq!(source_kind_counts.get("none"), Some(&2));
    let revoked_counts = auth_revoked_counts(&rows);
    assert_eq!(revoked_counts.get("revoked"), Some(&1));
    assert_eq!(revoked_counts.get("not_revoked"), Some(&2));
    assert_eq!(
        format_auth_state_counts(&revoked_counts),
        "not_revoked:2,revoked:1"
    );
    assert_eq!(
        format_auth_state_counts(&source_kind_counts),
        "credential_store:1,none:2"
    );
    assert_eq!(auth_source_kind("--api-key"), "flag");
    assert_eq!(auth_source_kind("OPENAI_API_KEY"), "env");
    assert_eq!(auth_source_kind("credential_store"), "credential_store");
    assert_eq!(auth_source_kind("none"), "none");

    assert_eq!(
        format_auth_state_counts(&counts),
        "missing_api_key:2,revoked:1"
    );
    assert_eq!(
        format_auth_state_counts(&std::collections::BTreeMap::new()),
        "none"
    );

    restore_env_vars(snapshot);
}

#[test]
fn unit_provider_auth_snapshot_reports_refreshable_and_expiration() {
    let temp = tempdir().expect("tempdir");
    let now = current_unix_timestamp();
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-snapshot-ready.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = None;
    config.openai_api_key = None;
    set_provider_auth_mode(
        &mut config,
        Provider::OpenAi,
        ProviderAuthMethod::OauthToken,
    );

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("oauth-access-snapshot".to_string()),
            refresh_token: Some("oauth-refresh-snapshot".to_string()),
            expires_unix: Some(now.saturating_add(120)),
            revoked: false,
        },
    );
    let store = load_credential_store(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
    )
    .expect("load snapshot store");

    let snapshot = provider_auth_snapshot_for_status(&config, Provider::OpenAi, Some(&store), None);
    assert_eq!(snapshot.method, ProviderAuthMethod::OauthToken);
    assert!(snapshot.available);
    assert_eq!(snapshot.state, "ready");
    assert_eq!(snapshot.source, "credential_store");
    assert_eq!(snapshot.expires_unix, Some(now.saturating_add(120)));
    assert!(snapshot.refreshable);
    assert!(!snapshot.revoked);
    assert_eq!(snapshot.secret.as_deref(), Some("oauth-access-snapshot"));
}

#[test]
fn unit_provider_auth_snapshot_marks_revoked_not_refreshable() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-snapshot-revoked.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = None;
    config.openai_api_key = None;
    set_provider_auth_mode(
        &mut config,
        Provider::OpenAi,
        ProviderAuthMethod::SessionToken,
    );

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("revoked-access".to_string()),
            refresh_token: Some("revoked-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(300)),
            revoked: true,
        },
    );
    let store = load_credential_store(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
    )
    .expect("load revoked snapshot store");

    let snapshot = provider_auth_snapshot_for_status(&config, Provider::OpenAi, Some(&store), None);
    assert!(!snapshot.available);
    assert_eq!(snapshot.state, "revoked");
    assert_eq!(snapshot.source, "credential_store");
    assert!(snapshot.revoked);
    assert!(!snapshot.refreshable);
    assert!(snapshot.secret.is_none());
}

#[test]
fn functional_auth_conformance_status_matrix_reports_expected_rows() {
    #[derive(Debug)]
    struct AuthConformanceCase {
        provider: Provider,
        mode: ProviderAuthMethod,
        api_key: Option<&'static str>,
        store_record: Option<ProviderCredentialStoreRecord>,
        expected_state: &'static str,
        expected_available: bool,
        expected_source: &'static str,
    }

    let temp = tempdir().expect("tempdir");
    let future_expiry = current_unix_timestamp().saturating_add(600);
    let cases = vec![
        AuthConformanceCase {
            provider: Provider::OpenAi,
            mode: ProviderAuthMethod::ApiKey,
            api_key: Some("openai-conformance-key"),
            store_record: None,
            expected_state: "ready",
            expected_available: true,
            expected_source: "--openai-api-key",
        },
        AuthConformanceCase {
            provider: Provider::Anthropic,
            mode: ProviderAuthMethod::ApiKey,
            api_key: Some("anthropic-conformance-key"),
            store_record: None,
            expected_state: "ready",
            expected_available: true,
            expected_source: "--anthropic-api-key",
        },
        AuthConformanceCase {
            provider: Provider::Google,
            mode: ProviderAuthMethod::ApiKey,
            api_key: Some("google-conformance-key"),
            store_record: None,
            expected_state: "ready",
            expected_available: true,
            expected_source: "--google-api-key",
        },
        AuthConformanceCase {
            provider: Provider::OpenAi,
            mode: ProviderAuthMethod::OauthToken,
            api_key: None,
            store_record: Some(ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::OauthToken,
                access_token: Some("oauth-access".to_string()),
                refresh_token: Some("oauth-refresh".to_string()),
                expires_unix: Some(future_expiry),
                revoked: false,
            }),
            expected_state: "ready",
            expected_available: true,
            expected_source: "credential_store",
        },
        AuthConformanceCase {
            provider: Provider::OpenAi,
            mode: ProviderAuthMethod::SessionToken,
            api_key: None,
            store_record: Some(ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::SessionToken,
                access_token: Some("session-access".to_string()),
                refresh_token: Some("session-refresh".to_string()),
                expires_unix: Some(future_expiry),
                revoked: false,
            }),
            expected_state: "ready",
            expected_available: true,
            expected_source: "credential_store",
        },
        AuthConformanceCase {
            provider: Provider::Anthropic,
            mode: ProviderAuthMethod::OauthToken,
            api_key: None,
            store_record: None,
            expected_state: "ready",
            expected_available: true,
            expected_source: "claude_cli",
        },
        AuthConformanceCase {
            provider: Provider::Google,
            mode: ProviderAuthMethod::SessionToken,
            api_key: None,
            store_record: None,
            expected_state: "unsupported_mode",
            expected_available: false,
            expected_source: "none",
        },
    ];

    let mut matrix_rows = Vec::new();
    for (index, case) in cases.into_iter().enumerate() {
        let mut config = test_auth_command_config();
        config.credential_store = temp.path().join(format!("auth-conformance-{index}.json"));
        config.credential_store_encryption = CredentialStoreEncryptionMode::None;
        config.api_key = None;
        config.openai_api_key = None;
        config.anthropic_api_key = None;
        config.google_api_key = None;
        set_provider_auth_mode(&mut config, case.provider, case.mode);
        if let Some(api_key) = case.api_key {
            set_provider_api_key(&mut config, case.provider, api_key);
        }
        if let Some(record) = case.store_record {
            write_test_provider_credential(
                &config.credential_store,
                CredentialStoreEncryptionMode::None,
                None,
                case.provider,
                record,
            );
        }

        let output = execute_auth_command(
            &config,
            &format!("status {} --json", case.provider.as_str()),
        );
        let payload: serde_json::Value = serde_json::from_str(&output).expect("parse status");
        let row = &payload["entries"][0];
        matrix_rows.push(format!(
            "{}:{}:{}:{}",
            case.provider.as_str(),
            case.mode.as_str(),
            row["state"].as_str().unwrap_or("unknown"),
            row["available"].as_bool().unwrap_or(false)
        ));
        assert_eq!(row["provider"], case.provider.as_str());
        assert_eq!(row["mode"], case.mode.as_str());
        assert_eq!(row["state"], case.expected_state);
        assert_eq!(row["available"], case.expected_available);
        assert_eq!(row["source"], case.expected_source);
    }

    assert_eq!(
        matrix_rows,
        vec![
            "openai:api_key:ready:true",
            "anthropic:api_key:ready:true",
            "google:api_key:ready:true",
            "openai:oauth_token:ready:true",
            "openai:session_token:ready:true",
            "anthropic:oauth_token:ready:true",
            "google:session_token:unsupported_mode:false",
        ]
    );
}

#[test]
fn functional_execute_auth_command_matrix_reports_provider_mode_inventory() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-matrix.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());
    config.openai_api_key = None;
    config.anthropic_api_key = None;
    config.google_api_key = None;

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("oauth-access-secret".to_string()),
            refresh_token: Some("oauth-refresh-secret".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: false,
        },
    );

    let output = execute_auth_command(&config, "matrix --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse matrix payload");
    assert_eq!(payload["command"], "auth.matrix");
    assert_eq!(payload["provider_filter"], "all");
    assert_eq!(payload["mode_filter"], "all");
    assert_eq!(payload["source_kind_filter"], "all");
    assert_eq!(payload["revoked_filter"], "all");
    assert_eq!(payload["subscription_strict"], false);
    assert_eq!(payload["providers"], 3);
    assert_eq!(payload["modes"], 4);
    assert_eq!(payload["rows_total"], 12);
    assert_eq!(payload["rows"], 12);
    assert_eq!(payload["mode_supported"], 9);
    assert_eq!(payload["mode_unsupported"], 3);
    assert_eq!(payload["mode_supported_total"], 9);
    assert_eq!(payload["mode_unsupported_total"], 3);
    assert_eq!(payload["mode_counts_total"]["api_key"], 3);
    assert_eq!(payload["mode_counts_total"]["oauth_token"], 3);
    assert_eq!(payload["mode_counts_total"]["adc"], 3);
    assert_eq!(payload["mode_counts_total"]["session_token"], 3);
    assert_eq!(payload["mode_counts"]["api_key"], 3);
    assert_eq!(payload["mode_counts"]["oauth_token"], 3);
    assert_eq!(payload["mode_counts"]["adc"], 3);
    assert_eq!(payload["mode_counts"]["session_token"], 3);
    assert_eq!(payload["provider_counts_total"]["openai"], 4);
    assert_eq!(payload["provider_counts_total"]["anthropic"], 4);
    assert_eq!(payload["provider_counts_total"]["google"], 4);
    assert_eq!(payload["provider_counts"]["openai"], 4);
    assert_eq!(payload["provider_counts"]["anthropic"], 4);
    assert_eq!(payload["provider_counts"]["google"], 4);
    assert_eq!(payload["available"], 8);
    assert_eq!(payload["unavailable"], 4);
    assert_eq!(payload["availability_counts_total"]["available"], 8);
    assert_eq!(payload["availability_counts_total"]["unavailable"], 4);
    assert_eq!(payload["availability_counts"]["available"], 8);
    assert_eq!(payload["availability_counts"]["unavailable"], 4);
    assert_eq!(payload["state_counts_total"]["ready"], 8);
    assert_eq!(payload["state_counts_total"]["mode_mismatch"], 1);
    assert_eq!(payload["state_counts_total"]["unsupported_mode"], 3);
    assert_eq!(payload["state_counts"]["ready"], 8);
    assert_eq!(payload["state_counts"]["mode_mismatch"], 1);
    assert_eq!(payload["state_counts"]["unsupported_mode"], 3);
    assert_eq!(payload["source_kind_counts_total"]["flag"], 3);
    assert_eq!(payload["source_kind_counts_total"]["credential_store"], 2);
    assert_eq!(payload["source_kind_counts_total"]["env"], 4);
    assert_eq!(payload["source_kind_counts_total"]["none"], 3);
    assert_eq!(payload["source_kind_counts"]["flag"], 3);
    assert_eq!(payload["source_kind_counts"]["credential_store"], 2);
    assert_eq!(payload["source_kind_counts"]["env"], 4);
    assert_eq!(payload["source_kind_counts"]["none"], 3);
    assert_eq!(payload["revoked_counts_total"]["not_revoked"], 12);
    assert_eq!(payload["revoked_counts"]["not_revoked"], 12);

    let entries = payload["entries"].as_array().expect("matrix entries");
    assert_eq!(entries.len(), 12);
    let row_for = |provider: &str, mode: &str| -> &serde_json::Value {
        entries
            .iter()
            .find(|row| row["provider"] == provider && row["mode"] == mode)
            .unwrap_or_else(|| panic!("missing matrix row provider={provider} mode={mode}"))
    };

    let openai_api = row_for("openai", "api_key");
    assert_eq!(openai_api["mode_supported"], true);
    assert_eq!(openai_api["available"], true);
    assert_eq!(openai_api["state"], "ready");
    assert_eq!(openai_api["reason_code"], "ready");
    assert_eq!(openai_api["backend_required"], false);
    assert_eq!(openai_api["backend_health"], "not_required");
    assert_eq!(openai_api["backend_reason_code"], "backend_not_required");
    assert_eq!(
        openai_api["fallback_order"],
        "oauth_token>session_token>api_key"
    );
    assert_eq!(openai_api["fallback_mode"], "oauth_token");
    assert_eq!(openai_api["fallback_available"], true);
    assert_eq!(openai_api["fallback_reason_code"], "fallback_ready");

    let openai_oauth = row_for("openai", "oauth_token");
    assert_eq!(openai_oauth["mode_supported"], true);
    assert_eq!(openai_oauth["available"], true);
    assert_eq!(openai_oauth["state"], "ready");
    assert_eq!(openai_oauth["source"], "credential_store");
    assert_eq!(openai_oauth["reason_code"], "ready");
    assert_eq!(openai_oauth["expiry_state"], "expiring_soon");
    assert_eq!(openai_oauth["backend_required"], true);
    assert_eq!(openai_oauth["backend"], "codex_cli");
    assert_eq!(openai_oauth["backend_health"], "ready");
    assert_eq!(openai_oauth["backend_reason_code"], "backend_ready");
    assert_eq!(
        openai_oauth["fallback_order"],
        "oauth_token>session_token>api_key"
    );
    assert_eq!(openai_oauth["fallback_mode"], "session_token");
    assert_eq!(openai_oauth["fallback_available"], false);
    assert_eq!(openai_oauth["fallback_reason_code"], "fallback_unavailable");
    assert_eq!(openai_oauth["reauth_required"], false);
    assert_eq!(openai_oauth["reauth_hint"], "none");
    assert!(openai_oauth["fallback_hint"]
        .as_str()
        .unwrap_or_default()
        .contains("--openai-auth-mode session_token"));

    let anthropic_oauth = row_for("anthropic", "oauth_token");
    assert_eq!(anthropic_oauth["mode_supported"], true);
    assert_eq!(anthropic_oauth["available"], true);
    assert_eq!(anthropic_oauth["state"], "ready");
    assert_eq!(anthropic_oauth["source"], "claude_cli");

    let google_oauth = row_for("google", "oauth_token");
    assert_eq!(google_oauth["mode_supported"], true);
    assert_eq!(google_oauth["available"], true);
    assert_eq!(google_oauth["state"], "ready");
    assert_eq!(google_oauth["source"], "gemini_cli");

    let text_output = execute_auth_command(&config, "matrix");
    assert!(text_output.contains("auth matrix: providers=3 modes=4 rows=12"));
    assert!(text_output.contains("mode_supported_total=9"));
    assert!(text_output.contains("mode_unsupported_total=3"));
    assert!(text_output.contains("mode_counts=adc:3,api_key:3,oauth_token:3,session_token:3"));
    assert!(text_output.contains("mode_counts_total=adc:3,api_key:3,oauth_token:3,session_token:3"));
    assert!(text_output.contains("provider_counts=anthropic:4,google:4,openai:4"));
    assert!(text_output.contains("provider_counts_total=anthropic:4,google:4,openai:4"));
    assert!(text_output.contains("availability_counts=available:8,unavailable:4"));
    assert!(text_output.contains("availability_counts_total=available:8,unavailable:4"));
    assert!(text_output.contains("provider_filter=all"));
    assert!(text_output.contains("mode_filter=all"));
    assert!(text_output.contains("source_kind_filter=all"));
    assert!(text_output.contains("revoked_filter=all"));
    assert!(text_output.contains("subscription_strict=false"));
    assert!(text_output.contains("source_kind_counts=credential_store:2,env:4,flag:3,none:3"));
    assert!(text_output.contains("source_kind_counts_total=credential_store:2,env:4,flag:3,none:3"));
    assert!(text_output.contains("revoked_counts=not_revoked:12"));
    assert!(text_output.contains("revoked_counts_total=not_revoked:12"));
    assert!(text_output.contains("state_counts=mode_mismatch:1,ready:8,unsupported_mode:3"));
    assert!(text_output.contains("state_counts_total=mode_mismatch:1,ready:8,unsupported_mode:3"));
    assert!(text_output.contains("auth matrix row: provider=openai mode=oauth_token"));
    assert!(text_output.contains("backend_health=ready"));
    assert!(text_output.contains("reason_code=ready"));
    assert!(text_output.contains("fallback_order=oauth_token>session_token>api_key"));
    assert!(text_output.contains("fallback_reason_code=fallback_"));
    assert!(!text_output.contains("oauth-access-secret"));
}

#[test]
fn functional_execute_auth_command_matrix_supports_provider_and_mode_filters() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-matrix-filtered.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("filtered-oauth-access".to_string()),
            refresh_token: Some("filtered-oauth-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: false,
        },
    );

    let json_output = execute_auth_command(&config, "matrix openai --mode oauth-token --json");
    let payload: serde_json::Value =
        serde_json::from_str(&json_output).expect("parse filtered matrix payload");
    assert_eq!(payload["command"], "auth.matrix");
    assert_eq!(payload["provider_filter"], "openai");
    assert_eq!(payload["mode_filter"], "oauth_token");
    assert_eq!(payload["source_kind_filter"], "all");
    assert_eq!(payload["revoked_filter"], "all");
    assert_eq!(payload["providers"], 1);
    assert_eq!(payload["modes"], 1);
    assert_eq!(payload["rows_total"], 1);
    assert_eq!(payload["rows"], 1);
    assert_eq!(payload["mode_supported"], 1);
    assert_eq!(payload["mode_unsupported"], 0);
    assert_eq!(payload["mode_supported_total"], 1);
    assert_eq!(payload["mode_unsupported_total"], 0);
    assert_eq!(payload["mode_counts_total"]["oauth_token"], 1);
    assert_eq!(payload["mode_counts"]["oauth_token"], 1);
    assert_eq!(payload["provider_counts_total"]["openai"], 1);
    assert_eq!(payload["provider_counts"]["openai"], 1);
    assert_eq!(payload["available"], 1);
    assert_eq!(payload["source_kind_counts_total"]["credential_store"], 1);
    assert_eq!(payload["source_kind_counts"]["credential_store"], 1);
    let entries = payload["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["provider"], "openai");
    assert_eq!(entries[0]["mode"], "oauth_token");
    assert_eq!(entries[0]["state"], "ready");
    assert_eq!(entries[0]["available"], true);

    let text_output = execute_auth_command(&config, "matrix openai --mode oauth-token");
    assert!(text_output.contains("auth matrix: providers=1 modes=1 rows=1"));
    assert!(text_output.contains("provider_filter=openai"));
    assert!(text_output.contains("mode_filter=oauth_token"));
    assert!(text_output.contains("source_kind_filter=all"));
    assert!(text_output.contains("revoked_filter=all"));
    assert!(text_output.contains("provider_counts=openai:1"));
    assert!(text_output.contains("provider_counts_total=openai:1"));
    assert!(text_output.contains(
        "auth matrix row: provider=openai mode=oauth_token mode_supported=true available=true state=ready"
    ));
}

#[test]
fn functional_execute_auth_command_matrix_supports_availability_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-matrix-availability.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("availability-access".to_string()),
            refresh_token: Some("availability-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: false,
        },
    );

    let available_output = execute_auth_command(&config, "matrix --availability available --json");
    let available_payload: serde_json::Value =
        serde_json::from_str(&available_output).expect("parse available matrix payload");
    assert_eq!(available_payload["provider_filter"], "all");
    assert_eq!(available_payload["mode_filter"], "all");
    assert_eq!(available_payload["source_kind_filter"], "all");
    assert_eq!(available_payload["revoked_filter"], "all");
    assert_eq!(available_payload["availability_filter"], "available");
    assert_eq!(available_payload["rows_total"], 12);
    assert_eq!(available_payload["rows"], 8);
    assert_eq!(available_payload["mode_supported_total"], 9);
    assert_eq!(available_payload["mode_unsupported_total"], 3);
    assert_eq!(available_payload["mode_counts_total"]["api_key"], 3);
    assert_eq!(available_payload["mode_counts_total"]["oauth_token"], 3);
    assert_eq!(available_payload["mode_counts_total"]["adc"], 3);
    assert_eq!(available_payload["mode_counts_total"]["session_token"], 3);
    assert_eq!(available_payload["mode_counts"]["api_key"], 3);
    assert_eq!(available_payload["mode_counts"]["oauth_token"], 3);
    assert_eq!(available_payload["mode_counts"]["adc"], 1);
    assert_eq!(available_payload["mode_counts"]["session_token"], 1);
    assert_eq!(available_payload["provider_counts_total"]["openai"], 4);
    assert_eq!(available_payload["provider_counts_total"]["anthropic"], 4);
    assert_eq!(available_payload["provider_counts_total"]["google"], 4);
    assert_eq!(available_payload["provider_counts"]["openai"], 2);
    assert_eq!(available_payload["provider_counts"]["anthropic"], 3);
    assert_eq!(available_payload["provider_counts"]["google"], 3);
    assert_eq!(
        available_payload["availability_counts_total"]["available"],
        8
    );
    assert_eq!(
        available_payload["availability_counts_total"]["unavailable"],
        4
    );
    assert_eq!(available_payload["availability_counts"]["available"], 8);
    assert_eq!(available_payload["source_kind_counts_total"]["flag"], 3);
    assert_eq!(
        available_payload["source_kind_counts_total"]["credential_store"],
        2
    );
    assert_eq!(available_payload["source_kind_counts_total"]["env"], 4);
    assert_eq!(available_payload["source_kind_counts_total"]["none"], 3);
    assert_eq!(available_payload["source_kind_counts"]["flag"], 3);
    assert_eq!(
        available_payload["source_kind_counts"]["credential_store"],
        1
    );
    assert_eq!(available_payload["source_kind_counts"]["env"], 4);
    assert_eq!(available_payload["revoked_counts_total"]["not_revoked"], 12);
    assert_eq!(available_payload["revoked_counts"]["not_revoked"], 8);
    assert_eq!(available_payload["available"], 8);
    assert_eq!(available_payload["unavailable"], 0);
    assert_eq!(available_payload["state_counts_total"]["ready"], 8);
    assert_eq!(available_payload["state_counts_total"]["mode_mismatch"], 1);
    assert_eq!(
        available_payload["state_counts_total"]["unsupported_mode"],
        3
    );
    assert_eq!(available_payload["state_counts"]["ready"], 8);
    let available_entries = available_payload["entries"]
        .as_array()
        .expect("available entries");
    assert_eq!(available_entries.len(), 8);
    assert!(available_entries
        .iter()
        .all(|entry| entry["available"].as_bool() == Some(true)));

    let unavailable_output =
        execute_auth_command(&config, "matrix --availability unavailable --json");
    let unavailable_payload: serde_json::Value =
        serde_json::from_str(&unavailable_output).expect("parse unavailable matrix payload");
    assert_eq!(unavailable_payload["provider_filter"], "all");
    assert_eq!(unavailable_payload["mode_filter"], "all");
    assert_eq!(unavailable_payload["source_kind_filter"], "all");
    assert_eq!(unavailable_payload["revoked_filter"], "all");
    assert_eq!(unavailable_payload["availability_filter"], "unavailable");
    assert_eq!(unavailable_payload["rows_total"], 12);
    assert_eq!(unavailable_payload["rows"], 4);
    assert_eq!(unavailable_payload["mode_supported_total"], 9);
    assert_eq!(unavailable_payload["mode_unsupported_total"], 3);
    assert_eq!(unavailable_payload["mode_counts_total"]["api_key"], 3);
    assert_eq!(unavailable_payload["mode_counts_total"]["oauth_token"], 3);
    assert_eq!(unavailable_payload["mode_counts_total"]["adc"], 3);
    assert_eq!(unavailable_payload["mode_counts_total"]["session_token"], 3);
    assert!(unavailable_payload["mode_counts"]["oauth_token"].is_null());
    assert_eq!(unavailable_payload["mode_counts"]["adc"], 2);
    assert_eq!(unavailable_payload["mode_counts"]["session_token"], 2);
    assert_eq!(unavailable_payload["provider_counts_total"]["openai"], 4);
    assert_eq!(unavailable_payload["provider_counts_total"]["anthropic"], 4);
    assert_eq!(unavailable_payload["provider_counts_total"]["google"], 4);
    assert_eq!(unavailable_payload["provider_counts"]["openai"], 2);
    assert_eq!(unavailable_payload["provider_counts"]["anthropic"], 1);
    assert_eq!(unavailable_payload["provider_counts"]["google"], 1);
    assert_eq!(
        unavailable_payload["availability_counts_total"]["available"],
        8
    );
    assert_eq!(
        unavailable_payload["availability_counts_total"]["unavailable"],
        4
    );
    assert_eq!(unavailable_payload["availability_counts"]["unavailable"], 4);
    assert_eq!(unavailable_payload["source_kind_counts_total"]["flag"], 3);
    assert_eq!(
        unavailable_payload["source_kind_counts_total"]["credential_store"],
        2
    );
    assert_eq!(unavailable_payload["source_kind_counts_total"]["env"], 4);
    assert_eq!(unavailable_payload["source_kind_counts_total"]["none"], 3);
    assert_eq!(
        unavailable_payload["source_kind_counts"]["credential_store"],
        1
    );
    assert_eq!(unavailable_payload["source_kind_counts"]["none"], 3);
    assert_eq!(
        unavailable_payload["revoked_counts_total"]["not_revoked"],
        12
    );
    assert_eq!(unavailable_payload["revoked_counts"]["not_revoked"], 4);
    assert_eq!(unavailable_payload["available"], 0);
    assert_eq!(unavailable_payload["unavailable"], 4);
    assert_eq!(unavailable_payload["state_counts_total"]["ready"], 8);
    assert_eq!(
        unavailable_payload["state_counts_total"]["mode_mismatch"],
        1
    );
    assert_eq!(
        unavailable_payload["state_counts_total"]["unsupported_mode"],
        3
    );
    assert_eq!(unavailable_payload["state_counts"]["mode_mismatch"], 1);
    assert_eq!(unavailable_payload["state_counts"]["unsupported_mode"], 3);
    let unavailable_entries = unavailable_payload["entries"]
        .as_array()
        .expect("unavailable entries");
    assert_eq!(unavailable_entries.len(), 4);
    assert!(unavailable_entries
        .iter()
        .all(|entry| entry["available"].as_bool() == Some(false)));
}

#[test]
fn functional_execute_auth_command_matrix_supports_state_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-matrix-state-filter.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("state-filter-access".to_string()),
            refresh_token: Some("state-filter-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: false,
        },
    );

    let ready_output = execute_auth_command(&config, "matrix --state ready --json");
    let ready_payload: serde_json::Value =
        serde_json::from_str(&ready_output).expect("parse state-filtered payload");
    assert_eq!(ready_payload["command"], "auth.matrix");
    assert_eq!(ready_payload["provider_filter"], "all");
    assert_eq!(ready_payload["mode_filter"], "all");
    assert_eq!(ready_payload["state_filter"], "ready");
    assert_eq!(ready_payload["source_kind_filter"], "all");
    assert_eq!(ready_payload["revoked_filter"], "all");
    assert_eq!(ready_payload["rows_total"], 12);
    assert_eq!(ready_payload["rows"], 8);
    assert_eq!(ready_payload["mode_supported_total"], 9);
    assert_eq!(ready_payload["mode_unsupported_total"], 3);
    assert_eq!(ready_payload["provider_counts_total"]["openai"], 4);
    assert_eq!(ready_payload["provider_counts_total"]["anthropic"], 4);
    assert_eq!(ready_payload["provider_counts_total"]["google"], 4);
    assert_eq!(ready_payload["provider_counts"]["openai"], 2);
    assert_eq!(ready_payload["provider_counts"]["anthropic"], 3);
    assert_eq!(ready_payload["provider_counts"]["google"], 3);
    assert_eq!(ready_payload["source_kind_counts_total"]["flag"], 3);
    assert_eq!(
        ready_payload["source_kind_counts_total"]["credential_store"],
        2
    );
    assert_eq!(ready_payload["source_kind_counts_total"]["env"], 4);
    assert_eq!(ready_payload["source_kind_counts_total"]["none"], 3);
    assert_eq!(ready_payload["source_kind_counts"]["flag"], 3);
    assert_eq!(ready_payload["source_kind_counts"]["credential_store"], 1);
    assert_eq!(ready_payload["source_kind_counts"]["env"], 4);
    assert_eq!(ready_payload["state_counts_total"]["ready"], 8);
    assert_eq!(ready_payload["state_counts_total"]["mode_mismatch"], 1);
    assert_eq!(ready_payload["state_counts_total"]["unsupported_mode"], 3);
    assert_eq!(ready_payload["state_counts"]["ready"], 8);
    let ready_entries = ready_payload["entries"].as_array().expect("ready entries");
    assert_eq!(ready_entries.len(), 8);
    assert!(ready_entries.iter().all(|entry| entry["state"] == "ready"));

    let text_output = execute_auth_command(&config, "matrix --state ready");
    assert!(text_output.contains("provider_filter=all"));
    assert!(text_output.contains("mode_filter=all"));
    assert!(text_output.contains("state_filter=ready"));
    assert!(text_output.contains("source_kind_filter=all"));
    assert!(text_output.contains("revoked_filter=all"));
    assert!(text_output.contains("provider_counts=anthropic:3,google:3,openai:2"));
    assert!(text_output.contains("provider_counts_total=anthropic:4,google:4,openai:4"));
    assert!(text_output.contains("source_kind_counts=credential_store:1,env:4,flag:3"));
    assert!(text_output.contains("source_kind_counts_total=credential_store:2,env:4,flag:3,none:3"));
    assert!(text_output.contains("state_counts=ready:8"));
    assert!(text_output.contains("state_counts_total=mode_mismatch:1,ready:8,unsupported_mode:3"));
    assert!(!text_output.contains("state=unsupported_mode"));
}

#[test]
fn functional_execute_auth_command_matrix_supports_mode_support_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-matrix-mode-support-filter.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("mode-support-access".to_string()),
            refresh_token: Some("mode-support-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: false,
        },
    );

    let supported_output = execute_auth_command(&config, "matrix --mode-support supported --json");
    let supported_payload: serde_json::Value =
        serde_json::from_str(&supported_output).expect("parse supported-only matrix payload");
    assert_eq!(supported_payload["provider_filter"], "all");
    assert_eq!(supported_payload["mode_filter"], "all");
    assert_eq!(supported_payload["mode_support_filter"], "supported");
    assert_eq!(supported_payload["source_kind_filter"], "all");
    assert_eq!(supported_payload["revoked_filter"], "all");
    assert_eq!(supported_payload["rows_total"], 12);
    assert_eq!(supported_payload["rows"], 9);
    assert_eq!(supported_payload["mode_supported"], 9);
    assert_eq!(supported_payload["mode_unsupported"], 0);
    assert_eq!(supported_payload["mode_supported_total"], 9);
    assert_eq!(supported_payload["mode_unsupported_total"], 3);
    assert_eq!(supported_payload["provider_counts_total"]["openai"], 4);
    assert_eq!(supported_payload["provider_counts_total"]["anthropic"], 4);
    assert_eq!(supported_payload["provider_counts_total"]["google"], 4);
    assert_eq!(supported_payload["provider_counts"]["openai"], 3);
    assert_eq!(supported_payload["provider_counts"]["anthropic"], 3);
    assert_eq!(supported_payload["provider_counts"]["google"], 3);
    assert_eq!(supported_payload["source_kind_counts_total"]["flag"], 3);
    assert_eq!(
        supported_payload["source_kind_counts_total"]["credential_store"],
        2
    );
    assert_eq!(supported_payload["source_kind_counts_total"]["env"], 4);
    assert_eq!(supported_payload["source_kind_counts_total"]["none"], 3);
    assert_eq!(supported_payload["source_kind_counts"]["flag"], 3);
    assert_eq!(
        supported_payload["source_kind_counts"]["credential_store"],
        2
    );
    assert_eq!(supported_payload["source_kind_counts"]["env"], 4);
    assert_eq!(supported_payload["state_counts_total"]["ready"], 8);
    assert_eq!(supported_payload["state_counts_total"]["mode_mismatch"], 1);
    assert_eq!(
        supported_payload["state_counts_total"]["unsupported_mode"],
        3
    );
    assert_eq!(supported_payload["state_counts"]["ready"], 8);
    assert_eq!(supported_payload["state_counts"]["mode_mismatch"], 1);
    let supported_entries = supported_payload["entries"]
        .as_array()
        .expect("supported entries");
    assert_eq!(supported_entries.len(), 9);
    assert!(supported_entries
        .iter()
        .all(|entry| entry["mode_supported"].as_bool() == Some(true)));

    let unsupported_output =
        execute_auth_command(&config, "matrix --mode-support unsupported --json");
    let unsupported_payload: serde_json::Value =
        serde_json::from_str(&unsupported_output).expect("parse unsupported-only matrix payload");
    assert_eq!(unsupported_payload["provider_filter"], "all");
    assert_eq!(unsupported_payload["mode_filter"], "all");
    assert_eq!(unsupported_payload["mode_support_filter"], "unsupported");
    assert_eq!(unsupported_payload["source_kind_filter"], "all");
    assert_eq!(unsupported_payload["revoked_filter"], "all");
    assert_eq!(unsupported_payload["rows_total"], 12);
    assert_eq!(unsupported_payload["rows"], 3);
    assert_eq!(unsupported_payload["mode_supported"], 0);
    assert_eq!(unsupported_payload["mode_unsupported"], 3);
    assert_eq!(unsupported_payload["mode_supported_total"], 9);
    assert_eq!(unsupported_payload["mode_unsupported_total"], 3);
    assert_eq!(unsupported_payload["provider_counts_total"]["openai"], 4);
    assert_eq!(unsupported_payload["provider_counts_total"]["anthropic"], 4);
    assert_eq!(unsupported_payload["provider_counts_total"]["google"], 4);
    assert_eq!(unsupported_payload["provider_counts"]["openai"], 1);
    assert_eq!(unsupported_payload["provider_counts"]["anthropic"], 1);
    assert_eq!(unsupported_payload["provider_counts"]["google"], 1);
    assert_eq!(unsupported_payload["source_kind_counts_total"]["flag"], 3);
    assert_eq!(
        unsupported_payload["source_kind_counts_total"]["credential_store"],
        2
    );
    assert_eq!(unsupported_payload["source_kind_counts_total"]["env"], 4);
    assert_eq!(unsupported_payload["source_kind_counts_total"]["none"], 3);
    assert_eq!(unsupported_payload["source_kind_counts"]["none"], 3);
    assert_eq!(unsupported_payload["state_counts_total"]["ready"], 8);
    assert_eq!(
        unsupported_payload["state_counts_total"]["mode_mismatch"],
        1
    );
    assert_eq!(
        unsupported_payload["state_counts_total"]["unsupported_mode"],
        3
    );
    assert_eq!(unsupported_payload["state_counts"]["unsupported_mode"], 3);
    let unsupported_entries = unsupported_payload["entries"]
        .as_array()
        .expect("unsupported entries");
    assert_eq!(unsupported_entries.len(), 3);
    assert!(unsupported_entries
        .iter()
        .all(|entry| entry["mode_supported"].as_bool() == Some(false)));

    let text_output = execute_auth_command(&config, "matrix --mode-support supported");
    assert!(text_output.contains("provider_filter=all"));
    assert!(text_output.contains("mode_filter=all"));
    assert!(text_output.contains("mode_support_filter=supported"));
    assert!(text_output.contains("source_kind_filter=all"));
    assert!(text_output.contains("revoked_filter=all"));
    assert!(text_output.contains("mode_supported_total=9"));
    assert!(text_output.contains("mode_unsupported_total=3"));
    assert!(text_output.contains("source_kind_counts=credential_store:2,env:4,flag:3"));
    assert!(text_output.contains("source_kind_counts_total=credential_store:2,env:4,flag:3,none:3"));
    assert!(text_output.contains("state_counts=mode_mismatch:1,ready:8"));
    assert!(text_output.contains("state_counts_total=mode_mismatch:1,ready:8,unsupported_mode:3"));
    assert!(!text_output.contains("mode_supported=false"));
}

#[test]
fn functional_execute_auth_command_matrix_supports_source_kind_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-matrix-source-kind-filter.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("source-kind-access".to_string()),
            refresh_token: Some("source-kind-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: false,
        },
    );

    let filtered_output =
        execute_auth_command(&config, "matrix --source-kind credential-store --json");
    let filtered_payload: serde_json::Value =
        serde_json::from_str(&filtered_output).expect("parse source-kind filtered matrix payload");
    assert_eq!(filtered_payload["provider_filter"], "all");
    assert_eq!(filtered_payload["mode_filter"], "all");
    assert_eq!(filtered_payload["source_kind_filter"], "credential_store");
    assert_eq!(filtered_payload["revoked_filter"], "all");
    assert_eq!(filtered_payload["rows_total"], 12);
    assert_eq!(filtered_payload["rows"], 2);
    assert_eq!(filtered_payload["mode_supported"], 2);
    assert_eq!(filtered_payload["mode_unsupported"], 0);
    assert_eq!(filtered_payload["available"], 1);
    assert_eq!(filtered_payload["unavailable"], 1);
    assert_eq!(filtered_payload["provider_counts_total"]["openai"], 4);
    assert_eq!(filtered_payload["provider_counts_total"]["anthropic"], 4);
    assert_eq!(filtered_payload["provider_counts_total"]["google"], 4);
    assert_eq!(filtered_payload["provider_counts"]["openai"], 2);
    assert_eq!(
        filtered_payload["source_kind_counts_total"]["credential_store"],
        2
    );
    assert_eq!(filtered_payload["source_kind_counts_total"]["flag"], 3);
    assert_eq!(filtered_payload["source_kind_counts_total"]["env"], 4);
    assert_eq!(filtered_payload["source_kind_counts_total"]["none"], 3);
    assert_eq!(
        filtered_payload["source_kind_counts"]["credential_store"],
        2
    );
    assert_eq!(filtered_payload["state_counts"]["ready"], 1);
    assert_eq!(filtered_payload["state_counts"]["mode_mismatch"], 1);
    let filtered_entries = filtered_payload["entries"]
        .as_array()
        .expect("source-kind filtered entries");
    assert_eq!(filtered_entries.len(), 2);
    assert!(filtered_entries
        .iter()
        .all(|entry| entry["source"] == "credential_store"));

    let text_output = execute_auth_command(&config, "matrix --source-kind credential-store");
    assert!(text_output.contains("source_kind_filter=credential_store"));
    assert!(text_output.contains("revoked_filter=all"));
    assert!(text_output.contains("rows=2"));
    assert!(text_output.contains("provider_counts=openai:2"));
    assert!(text_output.contains("provider_counts_total=anthropic:4,google:4,openai:4"));
    assert!(text_output.contains("source_kind_counts=credential_store:2"));
}

#[test]
fn functional_execute_auth_command_matrix_supports_revoked_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-matrix-revoked-filter.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("matrix-revoked-access".to_string()),
            refresh_token: Some("matrix-revoked-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: true,
        },
    );

    let revoked_output = execute_auth_command(&config, "matrix openai --revoked revoked --json");
    let revoked_payload: serde_json::Value =
        serde_json::from_str(&revoked_output).expect("parse revoked matrix payload");
    assert_eq!(revoked_payload["provider_filter"], "openai");
    assert_eq!(revoked_payload["mode_filter"], "all");
    assert_eq!(revoked_payload["revoked_filter"], "revoked");
    assert_eq!(revoked_payload["rows_total"], 4);
    assert_eq!(revoked_payload["rows"], 2);
    assert_eq!(revoked_payload["mode_supported"], 2);
    assert_eq!(revoked_payload["mode_unsupported"], 0);
    assert_eq!(revoked_payload["mode_counts_total"]["api_key"], 1);
    assert_eq!(revoked_payload["mode_counts_total"]["oauth_token"], 1);
    assert_eq!(revoked_payload["mode_counts_total"]["adc"], 1);
    assert_eq!(revoked_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(revoked_payload["mode_counts"]["oauth_token"], 1);
    assert_eq!(revoked_payload["mode_counts"]["session_token"], 1);
    assert_eq!(revoked_payload["provider_counts_total"]["openai"], 4);
    assert_eq!(revoked_payload["provider_counts"]["openai"], 2);
    assert_eq!(revoked_payload["available"], 0);
    assert_eq!(revoked_payload["unavailable"], 2);
    assert_eq!(revoked_payload["state_counts"]["mode_mismatch"], 1);
    assert_eq!(revoked_payload["state_counts"]["revoked"], 1);
    assert_eq!(revoked_payload["source_kind_counts"]["credential_store"], 2);
    assert_eq!(revoked_payload["revoked_counts_total"]["not_revoked"], 2);
    assert_eq!(revoked_payload["revoked_counts_total"]["revoked"], 2);
    assert_eq!(revoked_payload["revoked_counts"]["revoked"], 2);
    let revoked_entries = revoked_payload["entries"]
        .as_array()
        .expect("revoked matrix entries");
    assert_eq!(revoked_entries.len(), 2);
    assert!(revoked_entries.iter().all(|entry| entry["revoked"] == true));

    let not_revoked_output =
        execute_auth_command(&config, "matrix openai --revoked not-revoked --json");
    let not_revoked_payload: serde_json::Value =
        serde_json::from_str(&not_revoked_output).expect("parse non-revoked matrix payload");
    assert_eq!(not_revoked_payload["revoked_filter"], "not_revoked");
    assert_eq!(not_revoked_payload["rows_total"], 4);
    assert_eq!(not_revoked_payload["rows"], 2);
    assert_eq!(not_revoked_payload["mode_counts_total"]["api_key"], 1);
    assert_eq!(not_revoked_payload["mode_counts_total"]["oauth_token"], 1);
    assert_eq!(not_revoked_payload["mode_counts_total"]["adc"], 1);
    assert_eq!(not_revoked_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(not_revoked_payload["mode_counts"]["api_key"], 1);
    assert_eq!(not_revoked_payload["mode_counts"]["adc"], 1);
    assert_eq!(not_revoked_payload["provider_counts_total"]["openai"], 4);
    assert_eq!(not_revoked_payload["provider_counts"]["openai"], 2);
    assert_eq!(
        not_revoked_payload["revoked_counts_total"]["not_revoked"],
        2
    );
    assert_eq!(not_revoked_payload["revoked_counts_total"]["revoked"], 2);
    assert_eq!(not_revoked_payload["revoked_counts"]["not_revoked"], 2);
    let not_revoked_entries = not_revoked_payload["entries"]
        .as_array()
        .expect("non-revoked matrix entries");
    assert_eq!(not_revoked_entries.len(), 2);
    assert!(not_revoked_entries
        .iter()
        .all(|entry| entry["revoked"] == false));

    let text_output = execute_auth_command(&config, "matrix openai --revoked revoked");
    assert!(text_output.contains("revoked_filter=revoked"));
    assert!(text_output.contains("rows=2"));
    assert!(text_output.contains("mode_counts=oauth_token:1,session_token:1"));
    assert!(text_output.contains("mode_counts_total=adc:1,api_key:1,oauth_token:1,session_token:1"));
    assert!(text_output.contains("provider_counts=openai:2"));
    assert!(text_output.contains("provider_counts_total=openai:4"));
    assert!(text_output.contains("revoked_counts=revoked:2"));
    assert!(text_output.contains("revoked_counts_total=not_revoked:2,revoked:2"));
}

#[test]
fn integration_execute_auth_command_matrix_state_filter_composes_with_other_filters() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-matrix-state-composition.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("state-composition-access".to_string()),
            refresh_token: Some("state-composition-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: false,
        },
    );

    let filtered_output = execute_auth_command(
        &config,
        "matrix openai --mode oauth-token --availability available --state ready --source-kind credential-store --json",
    );
    let filtered_payload: serde_json::Value =
        serde_json::from_str(&filtered_output).expect("parse composed filter payload");
    assert_eq!(filtered_payload["provider_filter"], "openai");
    assert_eq!(filtered_payload["mode_filter"], "oauth_token");
    assert_eq!(filtered_payload["availability_filter"], "available");
    assert_eq!(filtered_payload["state_filter"], "ready");
    assert_eq!(filtered_payload["source_kind_filter"], "credential_store");
    assert_eq!(filtered_payload["revoked_filter"], "all");
    assert_eq!(filtered_payload["providers"], 1);
    assert_eq!(filtered_payload["modes"], 1);
    assert_eq!(filtered_payload["rows_total"], 1);
    assert_eq!(filtered_payload["rows"], 1);
    assert_eq!(filtered_payload["mode_supported_total"], 1);
    assert_eq!(filtered_payload["mode_unsupported_total"], 0);
    assert_eq!(filtered_payload["mode_counts_total"]["oauth_token"], 1);
    assert_eq!(filtered_payload["mode_counts"]["oauth_token"], 1);
    assert_eq!(filtered_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(filtered_payload["provider_counts"]["openai"], 1);
    assert_eq!(
        filtered_payload["source_kind_counts_total"]["credential_store"],
        1
    );
    assert_eq!(
        filtered_payload["source_kind_counts"]["credential_store"],
        1
    );
    assert_eq!(filtered_payload["state_counts_total"]["ready"], 1);
    assert_eq!(filtered_payload["state_counts"]["ready"], 1);
    assert_eq!(filtered_payload["revoked_counts_total"]["not_revoked"], 1);
    assert_eq!(filtered_payload["revoked_counts"]["not_revoked"], 1);
    assert_eq!(filtered_payload["entries"][0]["provider"], "openai");
    assert_eq!(filtered_payload["entries"][0]["mode"], "oauth_token");
    assert_eq!(filtered_payload["entries"][0]["state"], "ready");
    assert_eq!(filtered_payload["entries"][0]["available"], true);

    let mismatch_output = execute_auth_command(
        &config,
        "matrix openai --mode session-token --state mode_mismatch --source-kind credential-store --json",
    );
    let mismatch_payload: serde_json::Value =
        serde_json::from_str(&mismatch_output).expect("parse mismatch filter payload");
    assert_eq!(mismatch_payload["provider_filter"], "openai");
    assert_eq!(mismatch_payload["mode_filter"], "session_token");
    assert_eq!(mismatch_payload["state_filter"], "mode_mismatch");
    assert_eq!(mismatch_payload["source_kind_filter"], "credential_store");
    assert_eq!(mismatch_payload["revoked_filter"], "all");
    assert_eq!(mismatch_payload["rows_total"], 1);
    assert_eq!(mismatch_payload["rows"], 1);
    assert_eq!(mismatch_payload["mode_supported_total"], 1);
    assert_eq!(mismatch_payload["mode_unsupported_total"], 0);
    assert_eq!(mismatch_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(mismatch_payload["provider_counts"]["openai"], 1);
    assert_eq!(
        mismatch_payload["source_kind_counts_total"]["credential_store"],
        1
    );
    assert_eq!(
        mismatch_payload["source_kind_counts"]["credential_store"],
        1
    );
    assert_eq!(mismatch_payload["state_counts_total"]["mode_mismatch"], 1);
    assert_eq!(mismatch_payload["state_counts"]["mode_mismatch"], 1);
    assert_eq!(mismatch_payload["entries"][0]["state"], "mode_mismatch");
}

#[test]
fn integration_execute_auth_command_matrix_mode_support_filter_composes_with_other_filters() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp
        .path()
        .join("auth-matrix-mode-support-composition.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("mode-support-composition-access".to_string()),
            refresh_token: Some("mode-support-composition-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: false,
        },
    );

    let filtered_output = execute_auth_command(
        &config,
        "matrix openai --mode oauth-token --mode-support supported --availability available --state ready --source-kind credential-store --json",
    );
    let filtered_payload: serde_json::Value =
        serde_json::from_str(&filtered_output).expect("parse mode-support composed filter payload");
    assert_eq!(filtered_payload["provider_filter"], "openai");
    assert_eq!(filtered_payload["mode_filter"], "oauth_token");
    assert_eq!(filtered_payload["mode_support_filter"], "supported");
    assert_eq!(filtered_payload["availability_filter"], "available");
    assert_eq!(filtered_payload["state_filter"], "ready");
    assert_eq!(filtered_payload["source_kind_filter"], "credential_store");
    assert_eq!(filtered_payload["revoked_filter"], "all");
    assert_eq!(filtered_payload["providers"], 1);
    assert_eq!(filtered_payload["modes"], 1);
    assert_eq!(filtered_payload["rows_total"], 1);
    assert_eq!(filtered_payload["rows"], 1);
    assert_eq!(filtered_payload["mode_supported_total"], 1);
    assert_eq!(filtered_payload["mode_unsupported_total"], 0);
    assert_eq!(filtered_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(filtered_payload["provider_counts"]["openai"], 1);
    assert_eq!(
        filtered_payload["source_kind_counts_total"]["credential_store"],
        1
    );
    assert_eq!(
        filtered_payload["source_kind_counts"]["credential_store"],
        1
    );
    assert_eq!(filtered_payload["state_counts_total"]["ready"], 1);
    assert_eq!(filtered_payload["state_counts"]["ready"], 1);
    assert_eq!(filtered_payload["entries"][0]["provider"], "openai");
    assert_eq!(filtered_payload["entries"][0]["mode"], "oauth_token");
    assert_eq!(filtered_payload["entries"][0]["mode_supported"], true);
    assert_eq!(filtered_payload["entries"][0]["state"], "ready");

    let zero_row_output = execute_auth_command(
        &config,
        "matrix openai --mode oauth-token --mode-support unsupported --source-kind credential-store --json",
    );
    let zero_row_payload: serde_json::Value =
        serde_json::from_str(&zero_row_output).expect("parse zero-row mode-support payload");
    assert_eq!(zero_row_payload["provider_filter"], "openai");
    assert_eq!(zero_row_payload["mode_filter"], "oauth_token");
    assert_eq!(zero_row_payload["mode_support_filter"], "unsupported");
    assert_eq!(zero_row_payload["source_kind_filter"], "credential_store");
    assert_eq!(zero_row_payload["revoked_filter"], "all");
    assert_eq!(zero_row_payload["rows_total"], 1);
    assert_eq!(zero_row_payload["rows"], 0);
    assert_eq!(zero_row_payload["mode_supported"], 0);
    assert_eq!(zero_row_payload["mode_unsupported"], 0);
    assert_eq!(zero_row_payload["mode_supported_total"], 1);
    assert_eq!(zero_row_payload["mode_unsupported_total"], 0);
    assert_eq!(zero_row_payload["mode_counts_total"]["oauth_token"], 1);
    assert_eq!(
        zero_row_payload["mode_counts"]
            .as_object()
            .expect("zero-row mode counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(
        zero_row_payload["provider_counts"]
            .as_object()
            .expect("zero-row provider counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["source_kind_counts_total"]["credential_store"],
        1
    );
    assert_eq!(
        zero_row_payload["source_kind_counts"]
            .as_object()
            .expect("zero-row source-kind counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["state_counts_total"]["ready"], 1);
    assert_eq!(zero_row_payload["revoked_counts_total"]["not_revoked"], 1);
    assert_eq!(
        zero_row_payload["revoked_counts"]
            .as_object()
            .expect("zero-row revoked counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["state_counts"]
            .as_object()
            .expect("zero-row state counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["entries"]
            .as_array()
            .expect("zero-row entries")
            .len(),
        0
    );

    let zero_row_text = execute_auth_command(
        &config,
        "matrix openai --mode oauth-token --mode-support unsupported --source-kind credential-store",
    );
    assert!(zero_row_text.contains("rows=0"));
    assert!(zero_row_text.contains("mode_supported_total=1"));
    assert!(zero_row_text.contains("mode_unsupported_total=0"));
    assert!(zero_row_text.contains("mode_counts=none"));
    assert!(zero_row_text.contains("mode_counts_total=oauth_token:1"));
    assert!(zero_row_text.contains("provider_counts=none"));
    assert!(zero_row_text.contains("provider_counts_total=openai:1"));
    assert!(zero_row_text.contains("provider_filter=openai"));
    assert!(zero_row_text.contains("mode_filter=oauth_token"));
    assert!(zero_row_text.contains("source_kind_filter=credential_store"));
    assert!(zero_row_text.contains("revoked_filter=all"));
    assert!(zero_row_text.contains("source_kind_counts=none"));
    assert!(zero_row_text.contains("source_kind_counts_total=credential_store:1"));
    assert!(zero_row_text.contains("revoked_counts=none"));
    assert!(zero_row_text.contains("revoked_counts_total=not_revoked:1"));
    assert!(zero_row_text.contains("state_counts=none"));
    assert!(zero_row_text.contains("state_counts_total=ready:1"));
}

#[test]
fn integration_execute_auth_command_matrix_revoked_filter_composes_with_other_filters() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-matrix-revoked-composition.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = Some("shared-api-key".to_string());

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("matrix-revoked-composition-access".to_string()),
            refresh_token: Some("matrix-revoked-composition-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: true,
        },
    );

    let revoked_output = execute_auth_command(
        &config,
        "matrix openai --mode session-token --mode-support supported --availability unavailable --state revoked --source-kind credential-store --revoked revoked --json",
    );
    let revoked_payload: serde_json::Value =
        serde_json::from_str(&revoked_output).expect("parse revoked composed matrix payload");
    assert_eq!(revoked_payload["provider_filter"], "openai");
    assert_eq!(revoked_payload["mode_filter"], "session_token");
    assert_eq!(revoked_payload["mode_support_filter"], "supported");
    assert_eq!(revoked_payload["availability_filter"], "unavailable");
    assert_eq!(revoked_payload["state_filter"], "revoked");
    assert_eq!(revoked_payload["source_kind_filter"], "credential_store");
    assert_eq!(revoked_payload["revoked_filter"], "revoked");
    assert_eq!(revoked_payload["rows_total"], 1);
    assert_eq!(revoked_payload["rows"], 1);
    assert_eq!(revoked_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(revoked_payload["mode_counts"]["session_token"], 1);
    assert_eq!(revoked_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(revoked_payload["provider_counts"]["openai"], 1);
    assert_eq!(revoked_payload["revoked_counts_total"]["revoked"], 1);
    assert_eq!(revoked_payload["revoked_counts"]["revoked"], 1);
    assert_eq!(revoked_payload["entries"][0]["revoked"], true);

    let zero_row_output = execute_auth_command(
        &config,
        "matrix openai --mode session-token --mode-support supported --availability unavailable --state revoked --source-kind credential-store --revoked not-revoked --json",
    );
    let zero_row_payload: serde_json::Value =
        serde_json::from_str(&zero_row_output).expect("parse zero-row revoked composed payload");
    assert_eq!(zero_row_payload["revoked_filter"], "not_revoked");
    assert_eq!(zero_row_payload["rows_total"], 1);
    assert_eq!(zero_row_payload["rows"], 0);
    assert_eq!(zero_row_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(
        zero_row_payload["mode_counts"]
            .as_object()
            .expect("zero-row revoked mode counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(
        zero_row_payload["provider_counts"]
            .as_object()
            .expect("zero-row revoked provider counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["revoked_counts_total"]["revoked"], 1);
    assert_eq!(
        zero_row_payload["revoked_counts"]
            .as_object()
            .expect("zero-row revoked counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["entries"]
            .as_array()
            .expect("zero-row revoked entries")
            .len(),
        0
    );
}

#[test]
fn regression_execute_auth_command_matrix_rejects_missing_and_duplicate_mode_support_flags() {
    let config = test_auth_command_config();

    let missing_mode_support = execute_auth_command(&config, "matrix --mode-support");
    assert!(missing_mode_support
        .contains("auth error: missing mode-support filter after --mode-support"));
    assert!(missing_mode_support.contains("usage: /auth matrix"));

    let duplicate_mode_support = execute_auth_command(
        &config,
        "matrix --mode-support all --mode-support supported",
    );
    assert!(duplicate_mode_support.contains("auth error: duplicate --mode-support flag"));
    assert!(duplicate_mode_support.contains("usage: /auth matrix"));

    let missing_source_kind = execute_auth_command(&config, "matrix --source-kind");
    assert!(
        missing_source_kind.contains("auth error: missing source-kind filter after --source-kind")
    );
    assert!(missing_source_kind.contains("usage: /auth matrix"));

    let duplicate_source_kind =
        execute_auth_command(&config, "matrix --source-kind all --source-kind env");
    assert!(duplicate_source_kind.contains("auth error: duplicate --source-kind flag"));
    assert!(duplicate_source_kind.contains("usage: /auth matrix"));

    let missing_revoked = execute_auth_command(&config, "matrix --revoked");
    assert!(missing_revoked.contains("auth error: missing revoked filter after --revoked"));
    assert!(missing_revoked.contains("usage: /auth matrix"));

    let duplicate_revoked = execute_auth_command(&config, "matrix --revoked all --revoked revoked");
    assert!(duplicate_revoked.contains("auth error: duplicate --revoked flag"));
    assert!(duplicate_revoked.contains("usage: /auth matrix"));
}

#[test]
fn integration_auth_conformance_store_backed_status_matrix_handles_stale_token_scenarios() {
    #[derive(Debug)]
    struct StaleCase {
        mode: ProviderAuthMethod,
        record: ProviderCredentialStoreRecord,
        expected_state: &'static str,
        expected_refreshable: bool,
        access_secret: &'static str,
        refresh_secret: Option<&'static str>,
    }

    let temp = tempdir().expect("tempdir");
    let now = current_unix_timestamp();
    let cases = vec![
        StaleCase {
            mode: ProviderAuthMethod::OauthToken,
            record: ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::OauthToken,
                access_token: Some("oauth-access-secret".to_string()),
                refresh_token: Some("oauth-refresh-secret".to_string()),
                expires_unix: Some(now.saturating_sub(1)),
                revoked: false,
            },
            expected_state: "expired_refresh_pending",
            expected_refreshable: true,
            access_secret: "oauth-access-secret",
            refresh_secret: Some("oauth-refresh-secret"),
        },
        StaleCase {
            mode: ProviderAuthMethod::SessionToken,
            record: ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::SessionToken,
                access_token: Some("session-access-secret".to_string()),
                refresh_token: None,
                expires_unix: Some(now.saturating_sub(1)),
                revoked: false,
            },
            expected_state: "expired",
            expected_refreshable: false,
            access_secret: "session-access-secret",
            refresh_secret: None,
        },
        StaleCase {
            mode: ProviderAuthMethod::SessionToken,
            record: ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::SessionToken,
                access_token: Some("revoked-access-secret".to_string()),
                refresh_token: Some("revoked-refresh-secret".to_string()),
                expires_unix: Some(now.saturating_add(60)),
                revoked: true,
            },
            expected_state: "revoked",
            expected_refreshable: false,
            access_secret: "revoked-access-secret",
            refresh_secret: Some("revoked-refresh-secret"),
        },
        StaleCase {
            mode: ProviderAuthMethod::OauthToken,
            record: ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::OauthToken,
                access_token: None,
                refresh_token: Some("missing-access-refresh-secret".to_string()),
                expires_unix: Some(now.saturating_add(60)),
                revoked: false,
            },
            expected_state: "missing_access_token",
            expected_refreshable: true,
            access_secret: "not-present-access-secret",
            refresh_secret: Some("missing-access-refresh-secret"),
        },
    ];

    for (index, case) in cases.into_iter().enumerate() {
        let mut config = test_auth_command_config();
        config.credential_store = temp.path().join(format!("auth-stale-{index}.json"));
        config.credential_store_encryption = CredentialStoreEncryptionMode::None;
        config.api_key = None;
        config.openai_api_key = None;
        set_provider_auth_mode(&mut config, Provider::OpenAi, case.mode);
        write_test_provider_credential(
            &config.credential_store,
            CredentialStoreEncryptionMode::None,
            None,
            Provider::OpenAi,
            case.record,
        );

        let json_output = execute_auth_command(&config, "status openai --json");
        let payload: serde_json::Value =
            serde_json::from_str(&json_output).expect("parse status json");
        let row = &payload["entries"][0];
        assert_eq!(row["provider"], "openai");
        assert_eq!(row["mode"], case.mode.as_str());
        assert_eq!(row["state"], case.expected_state);
        assert_eq!(row["available"], false);
        assert_eq!(row["refreshable"], case.expected_refreshable);
        assert!(!json_output.contains(case.access_secret));
        if let Some(refresh_secret) = case.refresh_secret {
            assert!(!json_output.contains(refresh_secret));
        }

        let text_output = execute_auth_command(&config, "status openai");
        assert!(!text_output.contains(case.access_secret));
        if let Some(refresh_secret) = case.refresh_secret {
            assert!(!text_output.contains(refresh_secret));
        }
    }
}

#[test]
fn integration_execute_auth_command_matrix_reports_store_error_for_supported_non_api_modes() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("broken-auth-store.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    std::fs::write(&config.credential_store, "{not-json").expect("write broken store");

    let output = execute_auth_command(&config, "matrix --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse matrix payload");
    let entries = payload["entries"].as_array().expect("matrix entries");
    let openai_oauth = entries
        .iter()
        .find(|row| row["provider"] == "openai" && row["mode"] == "oauth_token")
        .expect("openai oauth row");
    assert_eq!(openai_oauth["mode_supported"], true);
    assert_eq!(openai_oauth["available"], false);
    assert_eq!(openai_oauth["state"], "store_error");
    assert!(openai_oauth["reason"]
        .as_str()
        .unwrap_or("missing reason")
        .contains("failed to parse"));

    let anthropic_oauth = entries
        .iter()
        .find(|row| row["provider"] == "anthropic" && row["mode"] == "oauth_token")
        .expect("anthropic oauth row");
    assert_eq!(anthropic_oauth["mode_supported"], true);
    assert_eq!(anthropic_oauth["available"], true);
    assert_eq!(anthropic_oauth["state"], "ready");
    assert_eq!(anthropic_oauth["source"], "claude_cli");
}

#[test]
fn regression_execute_auth_command_matrix_rejects_invalid_filter_combinations() {
    let config = test_auth_command_config();

    let missing_mode = execute_auth_command(&config, "matrix --mode");
    assert!(missing_mode.contains("auth error:"));
    assert!(missing_mode.contains("usage: /auth matrix"));

    let duplicate_provider = execute_auth_command(&config, "matrix openai anthropic");
    assert!(duplicate_provider.contains("auth error:"));
    assert!(duplicate_provider.contains("usage: /auth matrix"));

    let duplicate_availability = execute_auth_command(
        &config,
        "matrix --availability available --availability unavailable",
    );
    assert!(duplicate_availability.contains("auth error:"));
    assert!(duplicate_availability.contains("usage: /auth matrix"));

    let missing_state = execute_auth_command(&config, "matrix --state");
    assert!(missing_state.contains("auth error:"));
    assert!(missing_state.contains("usage: /auth matrix"));

    let duplicate_state = execute_auth_command(&config, "matrix --state ready --state revoked");
    assert!(duplicate_state.contains("auth error:"));
    assert!(duplicate_state.contains("usage: /auth matrix"));
}

#[test]
fn regression_auth_security_matrix_blocks_unsupported_mode_bypass_attempts() {
    let unsupported_cases = vec![
        (Provider::OpenAi, ProviderAuthMethod::Adc),
        (Provider::Anthropic, ProviderAuthMethod::Adc),
        (Provider::Google, ProviderAuthMethod::SessionToken),
    ];

    for (provider, mode) in unsupported_cases {
        let capability = provider_auth_capability(provider, mode);
        assert!(!capability.supported);

        let output = execute_auth_command(
            &test_auth_command_config(),
            &format!(
                "login {} --mode {} --json",
                provider.as_str(),
                mode.as_str()
            ),
        );
        let payload: serde_json::Value = serde_json::from_str(&output).expect("parse login output");
        assert_eq!(payload["command"], "auth.login");
        assert_eq!(payload["provider"], provider.as_str());
        assert_eq!(payload["mode"], mode.as_str());
        assert_eq!(payload["status"], "error");
        assert!(payload["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("not supported"));
    }
}

#[test]
fn functional_execute_auth_command_login_status_logout_lifecycle() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("credentials.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::OauthToken;

    let expires_unix = current_unix_timestamp().saturating_add(3600);
    let snapshot = snapshot_env_vars(&[
        "OPENAI_ACCESS_TOKEN",
        "OPENAI_REFRESH_TOKEN",
        "OPENAI_AUTH_EXPIRES_UNIX",
    ]);
    std::env::set_var("OPENAI_ACCESS_TOKEN", "openai-access-token");
    std::env::set_var("OPENAI_REFRESH_TOKEN", "openai-refresh-token");
    std::env::set_var("OPENAI_AUTH_EXPIRES_UNIX", expires_unix.to_string());

    let login_output = execute_auth_command(&config, "login openai --json");
    let login_json: serde_json::Value =
        serde_json::from_str(&login_output).expect("parse login output");
    assert_eq!(login_json["status"], "saved");
    assert_eq!(login_json["provider"], "openai");
    assert_eq!(login_json["mode"], "oauth_token");
    assert_eq!(login_json["expires_unix"], expires_unix);
    assert!(!login_output.contains("openai-access-token"));
    assert!(!login_output.contains("openai-refresh-token"));

    let status_output = execute_auth_command(&config, "status openai --json");
    let status_json: serde_json::Value =
        serde_json::from_str(&status_output).expect("parse status output");
    assert_eq!(status_json["available"], 1);
    assert_eq!(status_json["entries"][0]["provider"], "openai");
    assert_eq!(status_json["entries"][0]["state"], "ready");
    assert_eq!(status_json["entries"][0]["source"], "credential_store");
    assert!(!status_output.contains("openai-access-token"));
    assert!(!status_output.contains("openai-refresh-token"));

    let logout_output = execute_auth_command(&config, "logout openai --json");
    let logout_json: serde_json::Value =
        serde_json::from_str(&logout_output).expect("parse logout output");
    assert_eq!(logout_json["status"], "revoked");

    let post_logout_status = execute_auth_command(&config, "status openai --json");
    let post_logout_json: serde_json::Value =
        serde_json::from_str(&post_logout_status).expect("parse post logout status");
    assert_eq!(post_logout_json["entries"][0]["state"], "revoked");
    assert_eq!(post_logout_json["entries"][0]["available"], false);

    restore_env_vars(snapshot);
}

#[test]
fn functional_execute_auth_command_reauth_reports_guidance_and_recovery_plan() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("reauth-guidance-credentials.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::OauthToken;

    let snapshot = snapshot_env_vars(&[
        "TAU_AUTH_ACCESS_TOKEN",
        "TAU_AUTH_EXPIRES_UNIX",
        "OPENAI_ACCESS_TOKEN",
        "OPENAI_AUTH_EXPIRES_UNIX",
    ]);
    std::env::remove_var("TAU_AUTH_ACCESS_TOKEN");
    std::env::remove_var("TAU_AUTH_EXPIRES_UNIX");
    std::env::set_var("OPENAI_ACCESS_TOKEN", "openai-reauth-expired-access");
    std::env::set_var(
        "OPENAI_AUTH_EXPIRES_UNIX",
        current_unix_timestamp().saturating_sub(2).to_string(),
    );

    let output = execute_auth_command(&config, "reauth openai --mode oauth-token --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse reauth payload");
    assert_eq!(payload["command"], "auth.reauth");
    assert_eq!(payload["provider"], "openai");
    assert_eq!(payload["mode"], "oauth_token");
    assert_eq!(payload["status"], "reauth_required");
    assert_eq!(payload["launch_requested"], false);
    assert_eq!(payload["launch_executed"], false);
    assert_eq!(payload["launch_supported"], true);
    assert!(payload["reauth_command"]
        .as_str()
        .unwrap_or_default()
        .contains("/auth reauth openai --mode oauth_token"));
    assert_eq!(
        payload["fallback_order"],
        "oauth_token>session_token>api_key"
    );
    assert_eq!(payload["fallback_mode"], "session_token");
    assert_eq!(payload["fallback_available"], false);
    assert_eq!(payload["entry"]["state"], "expired_env_access_token");
    assert!(!output.contains("openai-reauth-expired-access"));

    restore_env_vars(snapshot);
}

#[test]
fn integration_execute_auth_command_reauth_supports_mode_transition_after_expiration() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("reauth-transition-credentials.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::OauthToken;
    config.api_key = Some("transition-api-key".to_string());
    config.openai_api_key = None;

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("transition-expired-access".to_string()),
            refresh_token: None,
            expires_unix: Some(current_unix_timestamp().saturating_sub(10)),
            revoked: false,
        },
    );

    let expired_status = execute_auth_command(&config, "status openai --json");
    let expired_payload: serde_json::Value =
        serde_json::from_str(&expired_status).expect("parse expired status payload");
    assert_eq!(expired_payload["entries"][0]["state"], "expired");
    assert_eq!(expired_payload["entries"][0]["available"], false);

    let reauth_output = execute_auth_command(&config, "reauth openai --json");
    let reauth_payload: serde_json::Value =
        serde_json::from_str(&reauth_output).expect("parse reauth transition payload");
    assert_eq!(reauth_payload["status"], "reauth_required");
    assert_eq!(reauth_payload["entry"]["state"], "expired");
    assert!(reauth_payload["next_action"]
        .as_str()
        .unwrap_or_default()
        .contains("/auth reauth openai --mode oauth_token"));

    set_provider_auth_mode(&mut config, Provider::OpenAi, ProviderAuthMethod::ApiKey);
    let api_key_status = execute_auth_command(&config, "status openai --json");
    let api_key_payload: serde_json::Value =
        serde_json::from_str(&api_key_status).expect("parse api-key status payload");
    assert_eq!(api_key_payload["entries"][0]["state"], "ready");
    assert_eq!(api_key_payload["entries"][0]["available"], true);
}

#[test]
fn regression_execute_auth_command_reauth_launch_rejects_unsupported_api_key_mode() {
    let config = test_auth_command_config();
    let output = execute_auth_command(&config, "reauth openai --mode api-key --launch --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse reauth payload");
    assert_eq!(payload["command"], "auth.reauth");
    assert_eq!(payload["provider"], "openai");
    assert_eq!(payload["mode"], "api_key");
    assert_eq!(payload["launch_requested"], true);
    assert_eq!(payload["launch_supported"], false);
    assert_eq!(payload["launch_executed"], false);
    assert_eq!(payload["login"]["command"], "auth.login");
    assert_eq!(payload["login"]["status"], "error");
    assert!(payload["login"]["reason"]
        .as_str()
        .unwrap_or_default()
        .contains("--launch is only supported"));
}

#[test]
fn unit_execute_auth_command_status_marks_expired_env_access_token_unavailable() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("env-expired-credentials.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::OauthToken;

    let snapshot = snapshot_env_vars(&[
        "TAU_AUTH_ACCESS_TOKEN",
        "TAU_AUTH_EXPIRES_UNIX",
        "OPENAI_ACCESS_TOKEN",
        "OPENAI_AUTH_EXPIRES_UNIX",
    ]);
    std::env::remove_var("TAU_AUTH_ACCESS_TOKEN");
    std::env::remove_var("TAU_AUTH_EXPIRES_UNIX");
    std::env::set_var("OPENAI_ACCESS_TOKEN", "openai-expired-env-access");
    std::env::set_var(
        "OPENAI_AUTH_EXPIRES_UNIX",
        current_unix_timestamp().saturating_sub(5).to_string(),
    );

    let output = execute_auth_command(&config, "status openai --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse status payload");
    assert_eq!(payload["command"], "auth.status");
    assert_eq!(payload["available"], 0);
    assert_eq!(payload["unavailable"], 1);
    assert_eq!(payload["entries"][0]["provider"], "openai");
    assert_eq!(payload["entries"][0]["mode"], "oauth_token");
    assert_eq!(payload["entries"][0]["state"], "expired_env_access_token");
    assert_eq!(
        payload["entries"][0]["reason_code"],
        "expired_env_access_token"
    );
    assert_eq!(payload["entries"][0]["available"], false);
    assert_eq!(payload["entries"][0]["source"], "OPENAI_ACCESS_TOKEN");
    assert_eq!(payload["entries"][0]["expiry_state"], "expired");
    assert_eq!(payload["entries"][0]["reauth_required"], true);
    assert_eq!(payload["entries"][0]["backend_required"], true);
    assert_eq!(payload["entries"][0]["backend"], "codex_cli");
    assert_eq!(payload["entries"][0]["backend_health"], "ready");
    assert_eq!(
        payload["entries"][0]["backend_reason_code"],
        "backend_ready"
    );
    assert_eq!(
        payload["entries"][0]["fallback_order"],
        "oauth_token>session_token>api_key"
    );
    assert_eq!(payload["entries"][0]["fallback_mode"], "session_token");
    assert_eq!(payload["entries"][0]["fallback_available"], false);
    assert_eq!(
        payload["entries"][0]["fallback_reason_code"],
        "fallback_unavailable"
    );
    assert!(payload["entries"][0]["reauth_hint"]
        .as_str()
        .unwrap_or_default()
        .contains("/auth reauth openai --mode oauth_token"));
    assert!(!output.contains("openai-expired-env-access"));

    restore_env_vars(snapshot);
}

#[test]
fn functional_execute_auth_command_status_uses_env_access_token_when_store_entry_missing() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("env-fallback-credentials.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::OauthToken;

    let snapshot = snapshot_env_vars(&[
        "TAU_AUTH_ACCESS_TOKEN",
        "TAU_AUTH_EXPIRES_UNIX",
        "OPENAI_ACCESS_TOKEN",
        "OPENAI_AUTH_EXPIRES_UNIX",
    ]);
    std::env::remove_var("TAU_AUTH_ACCESS_TOKEN");
    std::env::remove_var("TAU_AUTH_EXPIRES_UNIX");
    std::env::set_var("OPENAI_ACCESS_TOKEN", "openai-env-fallback-access");
    std::env::set_var(
        "OPENAI_AUTH_EXPIRES_UNIX",
        current_unix_timestamp().saturating_add(300).to_string(),
    );

    let output = execute_auth_command(&config, "status openai --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse status payload");
    assert_eq!(payload["command"], "auth.status");
    assert_eq!(payload["available"], 1);
    assert_eq!(payload["unavailable"], 0);
    assert_eq!(payload["source_kind_counts_total"]["env"], 1);
    assert_eq!(payload["source_kind_counts"]["env"], 1);
    assert_eq!(payload["entries"][0]["provider"], "openai");
    assert_eq!(payload["entries"][0]["mode"], "oauth_token");
    assert_eq!(payload["entries"][0]["state"], "ready");
    assert_eq!(payload["entries"][0]["available"], true);
    assert_eq!(payload["entries"][0]["source"], "OPENAI_ACCESS_TOKEN");
    assert_eq!(
        payload["entries"][0]["reason"],
        "env_access_token_available"
    );
    assert!(!output.contains("openai-env-fallback-access"));

    let text_output = execute_auth_command(&config, "status openai");
    assert!(text_output.contains("source=OPENAI_ACCESS_TOKEN"));
    assert!(text_output.contains("source_kind_counts=env:1"));
    assert!(text_output.contains("source_kind_counts_total=env:1"));
    assert!(!text_output.contains("openai-env-fallback-access"));

    restore_env_vars(snapshot);
}

#[test]
fn integration_build_provider_client_supports_openai_oauth_from_env_when_store_entry_missing() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.openai_auth_mode = CliProviderAuthMode::OauthToken;
    cli.credential_store = temp.path().join("missing-store-entry.json");
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::None;

    let snapshot = snapshot_env_vars(&[
        "TAU_AUTH_ACCESS_TOKEN",
        "TAU_AUTH_EXPIRES_UNIX",
        "OPENAI_ACCESS_TOKEN",
        "OPENAI_AUTH_EXPIRES_UNIX",
    ]);
    std::env::remove_var("TAU_AUTH_ACCESS_TOKEN");
    std::env::remove_var("TAU_AUTH_EXPIRES_UNIX");
    std::env::set_var("OPENAI_ACCESS_TOKEN", "openai-env-client-access");
    std::env::set_var(
        "OPENAI_AUTH_EXPIRES_UNIX",
        current_unix_timestamp().saturating_add(300).to_string(),
    );

    let client = build_provider_client(&cli, Provider::OpenAi).expect("build env oauth client");
    let ptr = Arc::as_ptr(&client);
    assert!(!ptr.is_null());

    restore_env_vars(snapshot);
}

#[cfg(unix)]
#[test]
fn integration_build_provider_client_uses_codex_backend_when_oauth_store_entry_missing() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let temp = tempdir().expect("tempdir");
    let script = write_mock_codex_script(
        temp.path(),
        r#"
out=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output-last-message) out="$2"; shift 2;;
    *) shift;;
  esac
done
cat >/dev/null
printf "codex fallback response" > "$out"
"#,
    );

    let mut cli = test_cli();
    cli.openai_auth_mode = CliProviderAuthMode::OauthToken;
    cli.credential_store = temp.path().join("missing-store-entry.json");
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::None;
    cli.openai_codex_backend = true;
    cli.openai_codex_cli = script.display().to_string();

    let snapshot = snapshot_env_vars(&[
        "TAU_AUTH_ACCESS_TOKEN",
        "TAU_AUTH_EXPIRES_UNIX",
        "OPENAI_ACCESS_TOKEN",
        "OPENAI_AUTH_EXPIRES_UNIX",
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
        "GROQ_API_KEY",
        "XAI_API_KEY",
        "MISTRAL_API_KEY",
        "AZURE_OPENAI_API_KEY",
        "TAU_API_KEY",
    ]);
    std::env::remove_var("TAU_AUTH_ACCESS_TOKEN");
    std::env::remove_var("TAU_AUTH_EXPIRES_UNIX");
    std::env::remove_var("OPENAI_ACCESS_TOKEN");
    std::env::remove_var("OPENAI_AUTH_EXPIRES_UNIX");
    std::env::remove_var("OPENAI_API_KEY");
    std::env::remove_var("OPENROUTER_API_KEY");
    std::env::remove_var("GROQ_API_KEY");
    std::env::remove_var("XAI_API_KEY");
    std::env::remove_var("MISTRAL_API_KEY");
    std::env::remove_var("AZURE_OPENAI_API_KEY");
    std::env::remove_var("TAU_API_KEY");

    let client = build_provider_client(&cli, Provider::OpenAi).expect("build codex backend client");
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let response = runtime
        .block_on(client.complete(test_chat_request()))
        .expect("codex backend completion");
    assert_eq!(response.message.text_content(), "codex fallback response");

    restore_env_vars(snapshot);
}

#[test]
fn regression_build_provider_client_does_not_bypass_revoked_store_with_env_token() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("revoked-store.json");
    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("revoked-store-access".to_string()),
            refresh_token: Some("revoked-store-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(300)),
            revoked: true,
        },
    );

    let mut cli = test_cli();
    cli.openai_auth_mode = CliProviderAuthMode::OauthToken;
    cli.credential_store = store_path;
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::None;

    let snapshot = snapshot_env_vars(&[
        "TAU_AUTH_ACCESS_TOKEN",
        "TAU_AUTH_EXPIRES_UNIX",
        "OPENAI_ACCESS_TOKEN",
        "OPENAI_AUTH_EXPIRES_UNIX",
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
        "GROQ_API_KEY",
        "XAI_API_KEY",
        "MISTRAL_API_KEY",
        "AZURE_OPENAI_API_KEY",
        "TAU_API_KEY",
    ]);
    std::env::remove_var("TAU_AUTH_ACCESS_TOKEN");
    std::env::remove_var("TAU_AUTH_EXPIRES_UNIX");
    std::env::set_var("OPENAI_ACCESS_TOKEN", "openai-env-should-not-bypass");
    std::env::set_var(
        "OPENAI_AUTH_EXPIRES_UNIX",
        current_unix_timestamp().saturating_add(300).to_string(),
    );
    std::env::remove_var("OPENAI_API_KEY");
    std::env::remove_var("OPENROUTER_API_KEY");
    std::env::remove_var("GROQ_API_KEY");
    std::env::remove_var("XAI_API_KEY");
    std::env::remove_var("MISTRAL_API_KEY");
    std::env::remove_var("AZURE_OPENAI_API_KEY");
    std::env::remove_var("TAU_API_KEY");

    let error = match build_provider_client(&cli, Provider::OpenAi) {
        Ok(_) => panic!("revoked store should remain fail-closed"),
        Err(error) => error,
    };
    let message = error.to_string();
    assert!(message.contains("requires re-authentication"));
    assert!(message.contains("revoked"));
    assert!(!message.contains("openai-env-should-not-bypass"));
    assert!(!message.contains("revoked-store-access"));

    restore_env_vars(snapshot);
}

#[test]
fn integration_execute_auth_command_status_reports_store_backed_state() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("credentials.json");
    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("session-access".to_string()),
            refresh_token: Some("session-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(1200)),
            revoked: false,
        },
    );

    let mut config = test_auth_command_config();
    config.credential_store = store_path;
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::SessionToken;

    let output = execute_auth_command(&config, "status openai --json");
    let payload: serde_json::Value = serde_json::from_str(&output).expect("parse status");
    assert_eq!(payload["provider_filter"], "openai");
    assert_eq!(payload["mode_filter"], "all");
    assert_eq!(payload["subscription_strict"], false);
    assert_eq!(payload["entries"][0]["provider"], "openai");
    assert_eq!(payload["entries"][0]["mode"], "session_token");
    assert_eq!(payload["entries"][0]["state"], "ready");
    assert_eq!(payload["entries"][0]["available"], true);
    assert_eq!(payload["mode_supported"], 1);
    assert_eq!(payload["mode_unsupported"], 0);
    assert_eq!(payload["mode_supported_total"], 1);
    assert_eq!(payload["mode_unsupported_total"], 0);
    assert_eq!(payload["provider_counts_total"]["openai"], 1);
    assert_eq!(payload["provider_counts"]["openai"], 1);
    assert_eq!(payload["state_counts"]["ready"], 1);
    assert_eq!(payload["state_counts_total"]["ready"], 1);
    assert_eq!(payload["source_kind_counts_total"]["credential_store"], 1);
    assert_eq!(payload["source_kind_counts"]["credential_store"], 1);
}

#[test]
fn functional_execute_auth_command_status_and_matrix_report_subscription_strict() {
    let mut config = test_auth_command_config();
    config.provider_subscription_strict = true;

    let status_json = execute_auth_command(&config, "status --json");
    let status_payload: serde_json::Value =
        serde_json::from_str(&status_json).expect("parse strict status payload");
    assert_eq!(status_payload["command"], "auth.status");
    assert_eq!(status_payload["subscription_strict"], true);

    let status_text = execute_auth_command(&config, "status");
    assert!(status_text.contains("subscription_strict=true"));

    let matrix_json = execute_auth_command(&config, "matrix --json");
    let matrix_payload: serde_json::Value =
        serde_json::from_str(&matrix_json).expect("parse strict matrix payload");
    assert_eq!(matrix_payload["command"], "auth.matrix");
    assert_eq!(matrix_payload["subscription_strict"], true);

    let matrix_text = execute_auth_command(&config, "matrix");
    assert!(matrix_text.contains("subscription_strict=true"));
}

#[test]
fn functional_execute_auth_command_status_supports_availability_and_state_filters() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-status-filters.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = None;
    config.openai_api_key = Some("openai-status-filter-key".to_string());
    config.anthropic_api_key = Some("anthropic-status-filter-key".to_string());
    config.google_api_key = None;

    let available_output = execute_auth_command(&config, "status --availability available --json");
    let available_payload: serde_json::Value =
        serde_json::from_str(&available_output).expect("parse available status payload");
    assert_eq!(available_payload["command"], "auth.status");
    assert_eq!(available_payload["provider_filter"], "all");
    assert_eq!(available_payload["mode_filter"], "all");
    assert_eq!(available_payload["availability_filter"], "available");
    assert_eq!(available_payload["state_filter"], "all");
    assert_eq!(available_payload["source_kind_filter"], "all");
    assert_eq!(available_payload["revoked_filter"], "all");
    assert_eq!(available_payload["providers"], 3);
    assert_eq!(available_payload["rows_total"], 3);
    assert_eq!(available_payload["rows"], 2);
    assert_eq!(available_payload["mode_supported_total"], 3);
    assert_eq!(available_payload["mode_unsupported_total"], 0);
    assert_eq!(available_payload["mode_counts_total"]["api_key"], 3);
    assert_eq!(available_payload["mode_counts"]["api_key"], 2);
    assert_eq!(available_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(available_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(available_payload["provider_counts_total"]["google"], 1);
    assert_eq!(available_payload["provider_counts"]["openai"], 1);
    assert_eq!(available_payload["provider_counts"]["anthropic"], 1);
    assert_eq!(
        available_payload["availability_counts_total"]["available"],
        2
    );
    assert_eq!(
        available_payload["availability_counts_total"]["unavailable"],
        1
    );
    assert_eq!(available_payload["availability_counts"]["available"], 2);
    assert_eq!(available_payload["available"], 2);
    assert_eq!(available_payload["unavailable"], 0);
    assert_eq!(available_payload["source_kind_counts_total"]["flag"], 2);
    assert_eq!(available_payload["source_kind_counts_total"]["none"], 1);
    assert_eq!(available_payload["source_kind_counts"]["flag"], 2);
    assert_eq!(available_payload["revoked_counts_total"]["not_revoked"], 3);
    assert_eq!(available_payload["revoked_counts"]["not_revoked"], 2);
    assert_eq!(available_payload["state_counts"]["ready"], 2);
    assert_eq!(available_payload["state_counts_total"]["ready"], 2);
    assert_eq!(
        available_payload["state_counts_total"]["missing_api_key"],
        1
    );
    let available_entries = available_payload["entries"]
        .as_array()
        .expect("available status entries");
    assert_eq!(available_entries.len(), 2);
    assert!(available_entries
        .iter()
        .all(|entry| entry["available"].as_bool() == Some(true)));
    assert!(available_entries
        .iter()
        .all(|entry| entry["state"] == "ready"));

    let unavailable_output =
        execute_auth_command(&config, "status --availability unavailable --json");
    let unavailable_payload: serde_json::Value =
        serde_json::from_str(&unavailable_output).expect("parse unavailable status payload");
    assert_eq!(unavailable_payload["provider_filter"], "all");
    assert_eq!(unavailable_payload["availability_filter"], "unavailable");
    assert_eq!(unavailable_payload["mode_filter"], "all");
    assert_eq!(unavailable_payload["state_filter"], "all");
    assert_eq!(unavailable_payload["source_kind_filter"], "all");
    assert_eq!(unavailable_payload["revoked_filter"], "all");
    assert_eq!(unavailable_payload["providers"], 3);
    assert_eq!(unavailable_payload["rows_total"], 3);
    assert_eq!(unavailable_payload["rows"], 1);
    assert_eq!(unavailable_payload["mode_supported_total"], 3);
    assert_eq!(unavailable_payload["mode_unsupported_total"], 0);
    assert_eq!(unavailable_payload["mode_counts_total"]["api_key"], 3);
    assert_eq!(unavailable_payload["mode_counts"]["api_key"], 1);
    assert_eq!(unavailable_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(unavailable_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(unavailable_payload["provider_counts_total"]["google"], 1);
    assert_eq!(unavailable_payload["provider_counts"]["google"], 1);
    assert_eq!(
        unavailable_payload["availability_counts_total"]["available"],
        2
    );
    assert_eq!(
        unavailable_payload["availability_counts_total"]["unavailable"],
        1
    );
    assert_eq!(unavailable_payload["availability_counts"]["unavailable"], 1);
    assert_eq!(unavailable_payload["available"], 0);
    assert_eq!(unavailable_payload["unavailable"], 1);
    assert_eq!(unavailable_payload["source_kind_counts_total"]["flag"], 2);
    assert_eq!(unavailable_payload["source_kind_counts_total"]["none"], 1);
    assert_eq!(unavailable_payload["source_kind_counts"]["none"], 1);
    assert_eq!(
        unavailable_payload["revoked_counts_total"]["not_revoked"],
        3
    );
    assert_eq!(unavailable_payload["revoked_counts"]["not_revoked"], 1);
    assert_eq!(unavailable_payload["state_counts"]["missing_api_key"], 1);
    assert_eq!(unavailable_payload["state_counts_total"]["ready"], 2);
    assert_eq!(
        unavailable_payload["state_counts_total"]["missing_api_key"],
        1
    );
    assert_eq!(unavailable_payload["entries"][0]["provider"], "google");
    assert_eq!(
        unavailable_payload["entries"][0]["state"],
        "missing_api_key"
    );
    assert_eq!(unavailable_payload["entries"][0]["available"], false);

    let state_output = execute_auth_command(&config, "status --state missing_api_key --json");
    let state_payload: serde_json::Value =
        serde_json::from_str(&state_output).expect("parse state-filtered status payload");
    assert_eq!(state_payload["provider_filter"], "all");
    assert_eq!(state_payload["availability_filter"], "all");
    assert_eq!(state_payload["mode_filter"], "all");
    assert_eq!(state_payload["state_filter"], "missing_api_key");
    assert_eq!(state_payload["source_kind_filter"], "all");
    assert_eq!(state_payload["revoked_filter"], "all");
    assert_eq!(state_payload["providers"], 3);
    assert_eq!(state_payload["rows_total"], 3);
    assert_eq!(state_payload["rows"], 1);
    assert_eq!(state_payload["mode_supported_total"], 3);
    assert_eq!(state_payload["mode_unsupported_total"], 0);
    assert_eq!(state_payload["mode_counts_total"]["api_key"], 3);
    assert_eq!(state_payload["mode_counts"]["api_key"], 1);
    assert_eq!(state_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(state_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(state_payload["provider_counts_total"]["google"], 1);
    assert_eq!(state_payload["provider_counts"]["google"], 1);
    assert_eq!(state_payload["availability_counts_total"]["available"], 2);
    assert_eq!(state_payload["availability_counts_total"]["unavailable"], 1);
    assert_eq!(state_payload["availability_counts"]["unavailable"], 1);
    assert_eq!(state_payload["state_counts"]["missing_api_key"], 1);
    assert_eq!(state_payload["source_kind_counts_total"]["flag"], 2);
    assert_eq!(state_payload["source_kind_counts_total"]["none"], 1);
    assert_eq!(state_payload["source_kind_counts"]["none"], 1);
    assert_eq!(state_payload["revoked_counts_total"]["not_revoked"], 3);
    assert_eq!(state_payload["revoked_counts"]["not_revoked"], 1);
    assert_eq!(state_payload["state_counts_total"]["ready"], 2);
    assert_eq!(state_payload["state_counts_total"]["missing_api_key"], 1);
    assert_eq!(state_payload["entries"][0]["provider"], "google");
    assert_eq!(state_payload["entries"][0]["state"], "missing_api_key");

    let text_output = execute_auth_command(
        &config,
        "status --availability unavailable --state missing_api_key",
    );
    assert!(text_output.contains("provider_filter=all"));
    assert!(text_output.contains("mode_supported_total=3"));
    assert!(text_output.contains("mode_unsupported_total=0"));
    assert!(text_output.contains("mode_counts=api_key:1"));
    assert!(text_output.contains("mode_counts_total=api_key:3"));
    assert!(text_output.contains("provider_counts=google:1"));
    assert!(text_output.contains("provider_counts_total=anthropic:1,google:1,openai:1"));
    assert!(text_output.contains("availability_counts=unavailable:1"));
    assert!(text_output.contains("availability_counts_total=available:2,unavailable:1"));
    assert!(text_output.contains("source_kind_counts=none:1"));
    assert!(text_output.contains("source_kind_counts_total=flag:2,none:1"));
    assert!(text_output.contains("revoked_counts=not_revoked:1"));
    assert!(text_output.contains("revoked_counts_total=not_revoked:3"));
    assert!(text_output.contains("availability_filter=unavailable"));
    assert!(text_output.contains("state_filter=missing_api_key"));
    assert!(text_output.contains("source_kind_filter=all"));
    assert!(text_output.contains("revoked_filter=all"));
    assert!(text_output.contains("rows_total=3"));
    assert!(text_output.contains("state_counts=missing_api_key:1"));
    assert!(text_output.contains("state_counts_total=missing_api_key:1,ready:2"));
    assert!(text_output.contains("auth provider: name=google"));
    assert!(!text_output.contains("auth provider: name=openai"));
}

#[test]
fn functional_execute_auth_command_status_supports_mode_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-status-mode-filter.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = None;
    config.openai_api_key = Some("openai-mode-filter-key".to_string());
    config.anthropic_api_key = None;
    config.google_api_key = None;

    let api_key_output = execute_auth_command(&config, "status --mode api-key --json");
    let api_key_payload: serde_json::Value =
        serde_json::from_str(&api_key_output).expect("parse api-key mode-filtered status payload");
    assert_eq!(api_key_payload["command"], "auth.status");
    assert_eq!(api_key_payload["provider_filter"], "all");
    assert_eq!(api_key_payload["mode_filter"], "api_key");
    assert_eq!(api_key_payload["mode_support_filter"], "all");
    assert_eq!(api_key_payload["source_kind_filter"], "all");
    assert_eq!(api_key_payload["revoked_filter"], "all");
    assert_eq!(api_key_payload["providers"], 3);
    assert_eq!(api_key_payload["rows_total"], 3);
    assert_eq!(api_key_payload["rows"], 3);
    assert_eq!(api_key_payload["mode_counts_total"]["api_key"], 3);
    assert_eq!(api_key_payload["mode_counts"]["api_key"], 3);
    assert_eq!(api_key_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(api_key_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(api_key_payload["provider_counts_total"]["google"], 1);
    assert_eq!(api_key_payload["provider_counts"]["openai"], 1);
    assert_eq!(api_key_payload["provider_counts"]["anthropic"], 1);
    assert_eq!(api_key_payload["provider_counts"]["google"], 1);
    assert_eq!(
        api_key_payload["mode_supported_total"]
            .as_u64()
            .unwrap_or(0)
            + api_key_payload["mode_unsupported_total"]
                .as_u64()
                .unwrap_or(0),
        3
    );
    assert_eq!(api_key_payload["source_kind_counts_total"]["flag"], 1);
    assert_eq!(api_key_payload["source_kind_counts"]["flag"], 1);
    assert_eq!(
        api_key_payload["source_kind_counts_total"]
            .as_object()
            .expect("api-key source-kind counts total")
            .values()
            .map(|value| value.as_u64().unwrap_or(0))
            .sum::<u64>(),
        3
    );
    assert_eq!(
        api_key_payload["source_kind_counts"]
            .as_object()
            .expect("api-key source-kind counts")
            .values()
            .map(|value| value.as_u64().unwrap_or(0))
            .sum::<u64>(),
        3
    );
    assert_eq!(
        api_key_payload["available"].as_u64().unwrap_or(0)
            + api_key_payload["unavailable"].as_u64().unwrap_or(0),
        3
    );
    let api_key_entries = api_key_payload["entries"]
        .as_array()
        .expect("api-key mode-filtered status entries");
    assert_eq!(api_key_entries.len(), 3);
    assert!(api_key_entries
        .iter()
        .all(|entry| entry["mode"] == "api_key"));
    let openai_entry = api_key_entries
        .iter()
        .find(|entry| entry["provider"] == "openai")
        .expect("openai status row");
    assert_eq!(openai_entry["available"], true);
    assert_eq!(openai_entry["state"], "ready");

    let oauth_output = execute_auth_command(&config, "status --mode oauth-token --json");
    let oauth_payload: serde_json::Value =
        serde_json::from_str(&oauth_output).expect("parse oauth mode-filtered status payload");
    assert_eq!(oauth_payload["provider_filter"], "all");
    assert_eq!(oauth_payload["mode_filter"], "oauth_token");
    assert_eq!(oauth_payload["source_kind_filter"], "all");
    assert_eq!(oauth_payload["revoked_filter"], "all");
    assert_eq!(oauth_payload["providers"], 3);
    assert_eq!(oauth_payload["rows_total"], 3);
    assert_eq!(oauth_payload["rows"], 3);
    assert_eq!(oauth_payload["mode_counts_total"]["oauth_token"], 3);
    assert_eq!(oauth_payload["mode_counts"]["oauth_token"], 3);
    assert_eq!(oauth_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(oauth_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(oauth_payload["provider_counts_total"]["google"], 1);
    assert_eq!(oauth_payload["provider_counts"]["openai"], 1);
    assert_eq!(oauth_payload["provider_counts"]["anthropic"], 1);
    assert_eq!(oauth_payload["provider_counts"]["google"], 1);
    assert_eq!(
        oauth_payload["mode_supported_total"].as_u64().unwrap_or(0)
            + oauth_payload["mode_unsupported_total"]
                .as_u64()
                .unwrap_or(0),
        3
    );
    assert_eq!(
        oauth_payload["source_kind_counts_total"]
            .as_object()
            .expect("oauth source-kind counts total")
            .values()
            .map(|value| value.as_u64().unwrap_or(0))
            .sum::<u64>(),
        3
    );
    let oauth_entries = oauth_payload["entries"]
        .as_array()
        .expect("oauth mode-filtered status entries");
    assert_eq!(oauth_entries.len(), 3);
    assert!(oauth_entries
        .iter()
        .all(|entry| entry["mode"] == "oauth_token"));

    let text_output = execute_auth_command(&config, "status --mode api-key");
    assert!(text_output.contains("provider_filter=all"));
    assert!(text_output.contains("mode_filter=api_key"));
    assert!(text_output.contains("source_kind_filter=all"));
    assert!(text_output.contains("revoked_filter=all"));
    assert!(text_output.contains("mode_counts=api_key:3"));
    assert!(text_output.contains("mode_counts_total=api_key:3"));
    assert!(text_output.contains("provider_counts=anthropic:1,google:1,openai:1"));
    assert!(text_output.contains("provider_counts_total=anthropic:1,google:1,openai:1"));
    assert!(text_output.contains("source_kind_counts="));
    assert!(text_output.contains("source_kind_counts_total="));
    assert!(text_output.contains("flag:1"));
    assert!(text_output.contains("auth provider: name=openai mode=api_key"));
}

#[test]
fn functional_execute_auth_command_status_supports_mode_support_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-status-mode-support-filter.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = None;
    config.openai_api_key = Some("openai-mode-support-key".to_string());
    config.google_api_key = None;
    config.anthropic_auth_mode = ProviderAuthMethod::OauthToken;

    let supported_output = execute_auth_command(&config, "status --mode-support supported --json");
    let supported_payload: serde_json::Value =
        serde_json::from_str(&supported_output).expect("parse supported-only status payload");
    assert_eq!(supported_payload["provider_filter"], "all");
    assert_eq!(supported_payload["mode_support_filter"], "supported");
    assert_eq!(supported_payload["mode_filter"], "all");
    assert_eq!(supported_payload["source_kind_filter"], "all");
    assert_eq!(supported_payload["revoked_filter"], "all");
    assert_eq!(supported_payload["providers"], 3);
    assert_eq!(supported_payload["rows_total"], 3);
    assert_eq!(supported_payload["rows"], 3);
    assert_eq!(supported_payload["mode_supported"], 3);
    assert_eq!(supported_payload["mode_unsupported"], 0);
    assert_eq!(supported_payload["mode_supported_total"], 3);
    assert_eq!(supported_payload["mode_unsupported_total"], 0);
    assert_eq!(supported_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(supported_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(supported_payload["provider_counts_total"]["google"], 1);
    assert_eq!(supported_payload["provider_counts"]["openai"], 1);
    assert_eq!(supported_payload["provider_counts"]["anthropic"], 1);
    assert_eq!(supported_payload["provider_counts"]["google"], 1);
    assert_eq!(supported_payload["source_kind_counts_total"]["flag"], 1);
    assert_eq!(supported_payload["source_kind_counts_total"]["env"], 1);
    assert_eq!(supported_payload["source_kind_counts_total"]["none"], 1);
    assert_eq!(supported_payload["source_kind_counts"]["flag"], 1);
    assert_eq!(supported_payload["source_kind_counts"]["env"], 1);
    assert_eq!(supported_payload["source_kind_counts"]["none"], 1);
    assert_eq!(
        supported_payload["source_kind_counts_total"]
            .as_object()
            .expect("supported source-kind counts total")
            .values()
            .map(|value| value.as_u64().unwrap_or(0))
            .sum::<u64>(),
        3
    );
    assert_eq!(
        supported_payload["source_kind_counts"]
            .as_object()
            .expect("supported source-kind counts")
            .values()
            .map(|value| value.as_u64().unwrap_or(0))
            .sum::<u64>(),
        3
    );
    assert_eq!(supported_payload["state_counts"]["missing_api_key"], 1);
    assert_eq!(supported_payload["state_counts"]["ready"], 2);
    assert_eq!(
        supported_payload["state_counts_total"]["missing_api_key"],
        1
    );
    assert_eq!(supported_payload["state_counts_total"]["ready"], 2);
    assert!(supported_payload["state_counts_total"]["unsupported_mode"].is_null());
    let supported_entries = supported_payload["entries"]
        .as_array()
        .expect("supported status entries");
    assert_eq!(supported_entries.len(), 3);
    assert!(supported_entries
        .iter()
        .all(|entry| entry["mode_supported"] == true));

    let unsupported_output =
        execute_auth_command(&config, "status --mode-support unsupported --json");
    let unsupported_payload: serde_json::Value =
        serde_json::from_str(&unsupported_output).expect("parse unsupported-only status payload");
    assert_eq!(unsupported_payload["provider_filter"], "all");
    assert_eq!(unsupported_payload["mode_support_filter"], "unsupported");
    assert_eq!(unsupported_payload["mode_filter"], "all");
    assert_eq!(unsupported_payload["source_kind_filter"], "all");
    assert_eq!(unsupported_payload["revoked_filter"], "all");
    assert_eq!(unsupported_payload["rows_total"], 3);
    assert_eq!(unsupported_payload["rows"], 0);
    assert_eq!(unsupported_payload["mode_supported"], 0);
    assert_eq!(unsupported_payload["mode_unsupported"], 0);
    assert_eq!(unsupported_payload["mode_supported_total"], 3);
    assert_eq!(unsupported_payload["mode_unsupported_total"], 0);
    assert_eq!(unsupported_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(unsupported_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(unsupported_payload["provider_counts_total"]["google"], 1);
    assert_eq!(unsupported_payload["source_kind_counts_total"]["flag"], 1);
    assert_eq!(unsupported_payload["source_kind_counts_total"]["env"], 1);
    assert_eq!(unsupported_payload["source_kind_counts_total"]["none"], 1);
    assert_eq!(
        unsupported_payload["source_kind_counts"]
            .as_object()
            .expect("unsupported source-kind counts")
            .len(),
        0
    );
    assert_eq!(
        unsupported_payload["source_kind_counts_total"]
            .as_object()
            .expect("unsupported source-kind counts total")
            .values()
            .map(|value| value.as_u64().unwrap_or(0))
            .sum::<u64>(),
        3
    );
    assert_eq!(
        unsupported_payload["state_counts"]
            .as_object()
            .expect("unsupported state counts")
            .len(),
        0
    );
    assert_eq!(
        unsupported_payload["entries"]
            .as_array()
            .expect("unsupported entries")
            .len(),
        0
    );

    let text_output = execute_auth_command(&config, "status --mode-support unsupported");
    assert!(text_output.contains("provider_filter=all"));
    assert!(text_output.contains("mode_support_filter=unsupported"));
    assert!(text_output.contains("source_kind_filter=all"));
    assert!(text_output.contains("revoked_filter=all"));
    assert!(text_output.contains("mode_supported_total=3"));
    assert!(text_output.contains("mode_unsupported_total=0"));
    assert!(text_output.contains("provider_counts=none"));
    assert!(text_output.contains("provider_counts_total=anthropic:1,google:1,openai:1"));
    assert!(text_output.contains("source_kind_counts=none"));
    assert!(text_output.contains("source_kind_counts_total="));
    assert!(text_output.contains("flag:1"));
    assert!(text_output.contains("state_counts=none"));
    assert!(!text_output.contains("auth provider: name=openai"));
}

#[test]
fn functional_execute_auth_command_status_supports_source_kind_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-status-source-kind-filter.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.api_key = None;
    config.openai_api_key = Some("openai-source-kind-key".to_string());
    config.anthropic_api_key = Some("anthropic-source-kind-key".to_string());
    config.google_api_key = None;

    let flag_output = execute_auth_command(&config, "status --source-kind flag --json");
    let flag_payload: serde_json::Value =
        serde_json::from_str(&flag_output).expect("parse flag source-kind status payload");
    assert_eq!(flag_payload["provider_filter"], "all");
    assert_eq!(flag_payload["mode_filter"], "all");
    assert_eq!(flag_payload["source_kind_filter"], "flag");
    assert_eq!(flag_payload["revoked_filter"], "all");
    assert_eq!(flag_payload["rows_total"], 3);
    assert_eq!(flag_payload["rows"], 2);
    assert_eq!(flag_payload["available"], 2);
    assert_eq!(flag_payload["unavailable"], 0);
    assert_eq!(flag_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(flag_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(flag_payload["provider_counts_total"]["google"], 1);
    assert_eq!(flag_payload["provider_counts"]["openai"], 1);
    assert_eq!(flag_payload["provider_counts"]["anthropic"], 1);
    assert_eq!(flag_payload["source_kind_counts_total"]["flag"], 2);
    assert_eq!(flag_payload["source_kind_counts_total"]["none"], 1);
    assert_eq!(flag_payload["source_kind_counts"]["flag"], 2);
    let flag_entries = flag_payload["entries"]
        .as_array()
        .expect("flag source-kind entries");
    assert_eq!(flag_entries.len(), 2);
    assert!(flag_entries
        .iter()
        .all(|entry| entry["source"].as_str().unwrap_or("").starts_with("--")));

    let none_output = execute_auth_command(&config, "status --source-kind none --json");
    let none_payload: serde_json::Value =
        serde_json::from_str(&none_output).expect("parse none source-kind status payload");
    assert_eq!(none_payload["source_kind_filter"], "none");
    assert_eq!(none_payload["revoked_filter"], "all");
    assert_eq!(none_payload["rows_total"], 3);
    assert_eq!(none_payload["rows"], 1);
    assert_eq!(none_payload["available"], 0);
    assert_eq!(none_payload["unavailable"], 1);
    assert_eq!(none_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(none_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(none_payload["provider_counts_total"]["google"], 1);
    assert_eq!(none_payload["provider_counts"]["google"], 1);
    assert_eq!(none_payload["source_kind_counts"]["none"], 1);
    assert_eq!(none_payload["entries"][0]["provider"], "google");
    assert_eq!(none_payload["entries"][0]["state"], "missing_api_key");

    let text_output = execute_auth_command(&config, "status --source-kind none");
    assert!(text_output.contains("source_kind_filter=none"));
    assert!(text_output.contains("revoked_filter=all"));
    assert!(text_output.contains("rows=1"));
    assert!(text_output.contains("provider_counts=google:1"));
    assert!(text_output.contains("provider_counts_total=anthropic:1,google:1,openai:1"));
    assert!(text_output.contains("source_kind_counts=none:1"));
}

#[test]
fn functional_execute_auth_command_status_supports_revoked_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("auth-status-revoked-filter.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::SessionToken;

    write_test_provider_credential(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("status-revoked-access".to_string()),
            refresh_token: Some("status-revoked-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(600)),
            revoked: true,
        },
    );

    let revoked_output = execute_auth_command(&config, "status --revoked revoked --json");
    let revoked_payload: serde_json::Value =
        serde_json::from_str(&revoked_output).expect("parse revoked status payload");
    assert_eq!(revoked_payload["provider_filter"], "all");
    assert_eq!(revoked_payload["mode_filter"], "all");
    assert_eq!(revoked_payload["revoked_filter"], "revoked");
    assert_eq!(revoked_payload["rows_total"], 3);
    assert_eq!(revoked_payload["rows"], 1);
    assert_eq!(revoked_payload["mode_counts_total"]["api_key"], 2);
    assert_eq!(revoked_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(revoked_payload["mode_counts"]["session_token"], 1);
    assert_eq!(revoked_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(revoked_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(revoked_payload["provider_counts_total"]["google"], 1);
    assert_eq!(revoked_payload["provider_counts"]["openai"], 1);
    assert_eq!(revoked_payload["state_counts"]["revoked"], 1);
    assert_eq!(revoked_payload["source_kind_counts"]["credential_store"], 1);
    assert_eq!(revoked_payload["revoked_counts_total"]["not_revoked"], 2);
    assert_eq!(revoked_payload["revoked_counts_total"]["revoked"], 1);
    assert_eq!(revoked_payload["revoked_counts"]["revoked"], 1);
    assert_eq!(revoked_payload["entries"][0]["provider"], "openai");
    assert_eq!(revoked_payload["entries"][0]["revoked"], true);

    let not_revoked_output = execute_auth_command(&config, "status --revoked not-revoked --json");
    let not_revoked_payload: serde_json::Value =
        serde_json::from_str(&not_revoked_output).expect("parse non-revoked status payload");
    assert_eq!(not_revoked_payload["revoked_filter"], "not_revoked");
    assert_eq!(not_revoked_payload["rows_total"], 3);
    assert_eq!(not_revoked_payload["rows"], 2);
    assert_eq!(not_revoked_payload["mode_counts_total"]["api_key"], 2);
    assert_eq!(not_revoked_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(not_revoked_payload["mode_counts"]["api_key"], 2);
    assert_eq!(not_revoked_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(not_revoked_payload["provider_counts_total"]["anthropic"], 1);
    assert_eq!(not_revoked_payload["provider_counts_total"]["google"], 1);
    assert_eq!(not_revoked_payload["provider_counts"]["anthropic"], 1);
    assert_eq!(not_revoked_payload["provider_counts"]["google"], 1);
    assert_eq!(
        not_revoked_payload["revoked_counts_total"]["not_revoked"],
        2
    );
    assert_eq!(not_revoked_payload["revoked_counts_total"]["revoked"], 1);
    assert_eq!(not_revoked_payload["revoked_counts"]["not_revoked"], 2);
    let not_revoked_entries = not_revoked_payload["entries"]
        .as_array()
        .expect("non-revoked status entries");
    assert_eq!(not_revoked_entries.len(), 2);
    assert!(not_revoked_entries
        .iter()
        .all(|entry| entry["revoked"] == false));

    let text_output = execute_auth_command(&config, "status --revoked revoked");
    assert!(text_output.contains("revoked_filter=revoked"));
    assert!(text_output.contains("rows=1"));
    assert!(text_output.contains("mode_counts=session_token:1"));
    assert!(text_output.contains("mode_counts_total=api_key:2,session_token:1"));
    assert!(text_output.contains("provider_counts=openai:1"));
    assert!(text_output.contains("provider_counts_total=anthropic:1,google:1,openai:1"));
    assert!(text_output.contains("revoked_counts=revoked:1"));
    assert!(text_output.contains("revoked_counts_total=not_revoked:2,revoked:1"));
}

#[test]
fn integration_execute_auth_command_status_filters_compose_with_provider_and_zero_rows() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("auth-status-composition.json");
    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("composition-session-access".to_string()),
            refresh_token: Some("composition-session-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(1200)),
            revoked: false,
        },
    );

    let mut config = test_auth_command_config();
    config.credential_store = store_path;
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::SessionToken;

    let filtered_output = execute_auth_command(
        &config,
        "status openai --availability available --state ready --source-kind credential-store --json",
    );
    let filtered_payload: serde_json::Value =
        serde_json::from_str(&filtered_output).expect("parse composed status payload");
    assert_eq!(filtered_payload["provider_filter"], "openai");
    assert_eq!(filtered_payload["availability_filter"], "available");
    assert_eq!(filtered_payload["mode_filter"], "all");
    assert_eq!(filtered_payload["state_filter"], "ready");
    assert_eq!(filtered_payload["source_kind_filter"], "credential_store");
    assert_eq!(filtered_payload["revoked_filter"], "all");
    assert_eq!(filtered_payload["providers"], 1);
    assert_eq!(filtered_payload["rows_total"], 1);
    assert_eq!(filtered_payload["rows"], 1);
    assert_eq!(filtered_payload["mode_supported"], 1);
    assert_eq!(filtered_payload["mode_unsupported"], 0);
    assert_eq!(filtered_payload["mode_supported_total"], 1);
    assert_eq!(filtered_payload["mode_unsupported_total"], 0);
    assert_eq!(filtered_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(filtered_payload["mode_counts"]["session_token"], 1);
    assert_eq!(filtered_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(filtered_payload["provider_counts"]["openai"], 1);
    assert_eq!(
        filtered_payload["availability_counts_total"]["available"],
        1
    );
    assert_eq!(filtered_payload["availability_counts"]["available"], 1);
    assert_eq!(
        filtered_payload["source_kind_counts_total"]["credential_store"],
        1
    );
    assert_eq!(
        filtered_payload["source_kind_counts"]["credential_store"],
        1
    );
    assert_eq!(filtered_payload["available"], 1);
    assert_eq!(filtered_payload["unavailable"], 0);
    assert_eq!(filtered_payload["state_counts"]["ready"], 1);
    assert_eq!(filtered_payload["state_counts_total"]["ready"], 1);
    assert_eq!(filtered_payload["revoked_counts_total"]["not_revoked"], 1);
    assert_eq!(filtered_payload["revoked_counts"]["not_revoked"], 1);
    assert_eq!(filtered_payload["entries"][0]["provider"], "openai");
    assert_eq!(filtered_payload["entries"][0]["state"], "ready");
    assert_eq!(filtered_payload["entries"][0]["available"], true);

    let zero_row_output = execute_auth_command(
        &config,
        "status openai --availability unavailable --state ready --source-kind credential-store --json",
    );
    let zero_row_payload: serde_json::Value =
        serde_json::from_str(&zero_row_output).expect("parse zero-row composed status payload");
    assert_eq!(zero_row_payload["provider_filter"], "openai");
    assert_eq!(zero_row_payload["availability_filter"], "unavailable");
    assert_eq!(zero_row_payload["mode_filter"], "all");
    assert_eq!(zero_row_payload["state_filter"], "ready");
    assert_eq!(zero_row_payload["source_kind_filter"], "credential_store");
    assert_eq!(zero_row_payload["revoked_filter"], "all");
    assert_eq!(zero_row_payload["providers"], 1);
    assert_eq!(zero_row_payload["rows_total"], 1);
    assert_eq!(zero_row_payload["rows"], 0);
    assert_eq!(zero_row_payload["mode_supported"], 0);
    assert_eq!(zero_row_payload["mode_unsupported"], 0);
    assert_eq!(zero_row_payload["mode_supported_total"], 1);
    assert_eq!(zero_row_payload["mode_unsupported_total"], 0);
    assert_eq!(zero_row_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(
        zero_row_payload["mode_counts"]
            .as_object()
            .expect("zero-row status mode counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(
        zero_row_payload["provider_counts"]
            .as_object()
            .expect("zero-row status provider counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["availability_counts_total"]["available"],
        1
    );
    assert_eq!(
        zero_row_payload["availability_counts"]
            .as_object()
            .expect("zero-row availability counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["source_kind_counts_total"]["credential_store"],
        1
    );
    assert_eq!(
        zero_row_payload["source_kind_counts"]
            .as_object()
            .expect("zero-row source-kind counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["available"], 0);
    assert_eq!(zero_row_payload["unavailable"], 0);
    assert_eq!(
        zero_row_payload["state_counts"]
            .as_object()
            .expect("zero-row state counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["state_counts_total"]["ready"], 1);
    assert_eq!(zero_row_payload["revoked_counts_total"]["not_revoked"], 1);
    assert_eq!(
        zero_row_payload["revoked_counts"]
            .as_object()
            .expect("zero-row revoked status counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["entries"]
            .as_array()
            .expect("zero-row entries")
            .len(),
        0
    );

    let zero_row_text = execute_auth_command(
        &config,
        "status openai --availability unavailable --state ready --source-kind credential-store",
    );
    assert!(zero_row_text.contains("providers=1 rows=0"));
    assert!(zero_row_text.contains("provider_filter=openai"));
    assert!(zero_row_text.contains("source_kind_filter=credential_store"));
    assert!(zero_row_text.contains("revoked_filter=all"));
    assert!(zero_row_text.contains("mode_supported_total=1"));
    assert!(zero_row_text.contains("mode_unsupported_total=0"));
    assert!(zero_row_text.contains("mode_counts=none"));
    assert!(zero_row_text.contains("mode_counts_total=session_token:1"));
    assert!(zero_row_text.contains("provider_counts=none"));
    assert!(zero_row_text.contains("provider_counts_total=openai:1"));
    assert!(zero_row_text.contains("availability_counts=none"));
    assert!(zero_row_text.contains("availability_counts_total=available:1"));
    assert!(zero_row_text.contains("source_kind_counts=none"));
    assert!(zero_row_text.contains("source_kind_counts_total=credential_store:1"));
    assert!(zero_row_text.contains("revoked_counts=none"));
    assert!(zero_row_text.contains("revoked_counts_total=not_revoked:1"));
    assert!(zero_row_text.contains("rows_total=1"));
    assert!(zero_row_text.contains("state_counts=none"));
    assert!(zero_row_text.contains("state_counts_total=ready:1"));
}

#[test]
fn integration_execute_auth_command_status_mode_support_filter_composes_with_other_filters() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp
        .path()
        .join("auth-status-mode-support-composition.json");
    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("status-mode-support-access".to_string()),
            refresh_token: Some("status-mode-support-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(1200)),
            revoked: false,
        },
    );

    let mut config = test_auth_command_config();
    config.credential_store = store_path;
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::SessionToken;

    let filtered_output = execute_auth_command(
        &config,
        "status openai --mode session-token --mode-support supported --availability available --state ready --source-kind credential-store --json",
    );
    let filtered_payload: serde_json::Value =
        serde_json::from_str(&filtered_output).expect("parse composed mode-support payload");
    assert_eq!(filtered_payload["provider_filter"], "openai");
    assert_eq!(filtered_payload["mode_filter"], "session_token");
    assert_eq!(filtered_payload["mode_support_filter"], "supported");
    assert_eq!(filtered_payload["availability_filter"], "available");
    assert_eq!(filtered_payload["state_filter"], "ready");
    assert_eq!(filtered_payload["source_kind_filter"], "credential_store");
    assert_eq!(filtered_payload["revoked_filter"], "all");
    assert_eq!(filtered_payload["providers"], 1);
    assert_eq!(filtered_payload["rows_total"], 1);
    assert_eq!(filtered_payload["rows"], 1);
    assert_eq!(filtered_payload["mode_supported"], 1);
    assert_eq!(filtered_payload["mode_unsupported"], 0);
    assert_eq!(filtered_payload["mode_supported_total"], 1);
    assert_eq!(filtered_payload["mode_unsupported_total"], 0);
    assert_eq!(filtered_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(filtered_payload["mode_counts"]["session_token"], 1);
    assert_eq!(filtered_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(filtered_payload["provider_counts"]["openai"], 1);
    assert_eq!(
        filtered_payload["source_kind_counts_total"]["credential_store"],
        1
    );
    assert_eq!(
        filtered_payload["source_kind_counts"]["credential_store"],
        1
    );
    assert_eq!(filtered_payload["revoked_counts_total"]["not_revoked"], 1);
    assert_eq!(filtered_payload["revoked_counts"]["not_revoked"], 1);
    assert_eq!(filtered_payload["entries"][0]["provider"], "openai");
    assert_eq!(filtered_payload["entries"][0]["mode_supported"], true);
    assert_eq!(filtered_payload["state_counts"]["ready"], 1);
    assert_eq!(filtered_payload["state_counts_total"]["ready"], 1);

    let zero_row_output = execute_auth_command(
        &config,
        "status openai --mode session-token --mode-support unsupported --source-kind credential-store --json",
    );
    let zero_row_payload: serde_json::Value =
        serde_json::from_str(&zero_row_output).expect("parse zero-row mode-support payload");
    assert_eq!(zero_row_payload["provider_filter"], "openai");
    assert_eq!(zero_row_payload["mode_filter"], "session_token");
    assert_eq!(zero_row_payload["mode_support_filter"], "unsupported");
    assert_eq!(zero_row_payload["source_kind_filter"], "credential_store");
    assert_eq!(zero_row_payload["revoked_filter"], "all");
    assert_eq!(zero_row_payload["rows_total"], 1);
    assert_eq!(zero_row_payload["rows"], 0);
    assert_eq!(zero_row_payload["mode_supported"], 0);
    assert_eq!(zero_row_payload["mode_unsupported"], 0);
    assert_eq!(zero_row_payload["mode_supported_total"], 1);
    assert_eq!(zero_row_payload["mode_unsupported_total"], 0);
    assert_eq!(zero_row_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(
        zero_row_payload["mode_counts"]
            .as_object()
            .expect("zero-row status mode-support mode counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(
        zero_row_payload["provider_counts"]
            .as_object()
            .expect("zero-row status mode-support provider counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["source_kind_counts_total"]["credential_store"],
        1
    );
    assert_eq!(
        zero_row_payload["source_kind_counts"]
            .as_object()
            .expect("zero-row source-kind counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["state_counts"]
            .as_object()
            .expect("zero-row state counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["state_counts_total"]["ready"], 1);
    assert_eq!(zero_row_payload["revoked_counts_total"]["not_revoked"], 1);
    assert_eq!(
        zero_row_payload["revoked_counts"]
            .as_object()
            .expect("zero-row revoked status mode-support counts")
            .len(),
        0
    );

    let zero_row_text = execute_auth_command(
        &config,
        "status openai --mode session-token --mode-support unsupported --source-kind credential-store",
    );
    assert!(zero_row_text.contains("rows=0"));
    assert!(zero_row_text.contains("provider_filter=openai"));
    assert!(zero_row_text.contains("mode_filter=session_token"));
    assert!(zero_row_text.contains("source_kind_filter=credential_store"));
    assert!(zero_row_text.contains("revoked_filter=all"));
    assert!(zero_row_text.contains("mode_supported_total=1"));
    assert!(zero_row_text.contains("mode_unsupported_total=0"));
    assert!(zero_row_text.contains("mode_counts=none"));
    assert!(zero_row_text.contains("mode_counts_total=session_token:1"));
    assert!(zero_row_text.contains("provider_counts=none"));
    assert!(zero_row_text.contains("provider_counts_total=openai:1"));
    assert!(zero_row_text.contains("source_kind_counts=none"));
    assert!(zero_row_text.contains("source_kind_counts_total=credential_store:1"));
    assert!(zero_row_text.contains("revoked_counts=none"));
    assert!(zero_row_text.contains("revoked_counts_total=not_revoked:1"));
    assert!(zero_row_text.contains("mode_support_filter=unsupported"));
    assert!(zero_row_text.contains("state_counts=none"));
    assert!(zero_row_text.contains("state_counts_total=ready:1"));
}

#[test]
fn integration_execute_auth_command_status_revoked_filter_composes_with_other_filters() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("auth-status-revoked-composition.json");
    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("status-revoked-composition-access".to_string()),
            refresh_token: Some("status-revoked-composition-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(1200)),
            revoked: true,
        },
    );

    let mut config = test_auth_command_config();
    config.credential_store = store_path;
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::SessionToken;

    let revoked_output = execute_auth_command(
        &config,
        "status openai --mode session-token --mode-support supported --availability unavailable --state revoked --source-kind credential-store --revoked revoked --json",
    );
    let revoked_payload: serde_json::Value =
        serde_json::from_str(&revoked_output).expect("parse revoked composed status payload");
    assert_eq!(revoked_payload["provider_filter"], "openai");
    assert_eq!(revoked_payload["mode_filter"], "session_token");
    assert_eq!(revoked_payload["mode_support_filter"], "supported");
    assert_eq!(revoked_payload["availability_filter"], "unavailable");
    assert_eq!(revoked_payload["state_filter"], "revoked");
    assert_eq!(revoked_payload["source_kind_filter"], "credential_store");
    assert_eq!(revoked_payload["revoked_filter"], "revoked");
    assert_eq!(revoked_payload["rows_total"], 1);
    assert_eq!(revoked_payload["rows"], 1);
    assert_eq!(revoked_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(revoked_payload["mode_counts"]["session_token"], 1);
    assert_eq!(revoked_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(revoked_payload["provider_counts"]["openai"], 1);
    assert_eq!(revoked_payload["revoked_counts_total"]["revoked"], 1);
    assert_eq!(revoked_payload["revoked_counts"]["revoked"], 1);
    assert_eq!(revoked_payload["entries"][0]["revoked"], true);

    let zero_row_output = execute_auth_command(
        &config,
        "status openai --mode session-token --mode-support supported --availability unavailable --state revoked --source-kind credential-store --revoked not-revoked --json",
    );
    let zero_row_payload: serde_json::Value =
        serde_json::from_str(&zero_row_output).expect("parse zero-row revoked status payload");
    assert_eq!(zero_row_payload["revoked_filter"], "not_revoked");
    assert_eq!(zero_row_payload["rows_total"], 1);
    assert_eq!(zero_row_payload["rows"], 0);
    assert_eq!(zero_row_payload["mode_counts_total"]["session_token"], 1);
    assert_eq!(
        zero_row_payload["mode_counts"]
            .as_object()
            .expect("zero-row revoked status mode counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["provider_counts_total"]["openai"], 1);
    assert_eq!(
        zero_row_payload["provider_counts"]
            .as_object()
            .expect("zero-row revoked status provider counts")
            .len(),
        0
    );
    assert_eq!(zero_row_payload["revoked_counts_total"]["revoked"], 1);
    assert_eq!(
        zero_row_payload["revoked_counts"]
            .as_object()
            .expect("zero-row revoked status counts")
            .len(),
        0
    );
    assert_eq!(
        zero_row_payload["entries"]
            .as_array()
            .expect("zero-row revoked status entries")
            .len(),
        0
    );
}

#[test]
fn regression_execute_auth_command_availability_counts_zero_row_outputs_remain_explicit() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp
        .path()
        .join("auth-availability-zero-row-regression.json");
    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("availability-zero-row-access".to_string()),
            refresh_token: Some("availability-zero-row-refresh".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(1200)),
            revoked: false,
        },
    );

    let mut config = test_auth_command_config();
    config.credential_store = store_path;
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;
    config.openai_auth_mode = ProviderAuthMethod::SessionToken;

    let status_json = execute_auth_command(
        &config,
        "status openai --availability unavailable --state ready --source-kind credential-store --json",
    );
    let status_payload: serde_json::Value =
        serde_json::from_str(&status_json).expect("parse zero-row availability status payload");
    assert_eq!(status_payload["rows"], 0);
    assert_eq!(status_payload["availability_counts_total"]["available"], 1);
    assert_eq!(
        status_payload["availability_counts"]
            .as_object()
            .expect("zero-row status availability counts")
            .len(),
        0
    );
    let status_text = execute_auth_command(
        &config,
        "status openai --availability unavailable --state ready --source-kind credential-store",
    );
    assert!(status_text.contains("availability_counts=none"));
    assert!(status_text.contains("availability_counts_total=available:1"));

    let matrix_json = execute_auth_command(
        &config,
        "matrix openai --mode session-token --availability unavailable --state ready --source-kind credential-store --json",
    );
    let matrix_payload: serde_json::Value =
        serde_json::from_str(&matrix_json).expect("parse zero-row availability matrix payload");
    assert_eq!(matrix_payload["rows"], 0);
    assert_eq!(matrix_payload["availability_counts_total"]["available"], 1);
    assert_eq!(
        matrix_payload["availability_counts"]
            .as_object()
            .expect("zero-row matrix availability counts")
            .len(),
        0
    );
    let matrix_text = execute_auth_command(
        &config,
        "matrix openai --mode session-token --availability unavailable --state ready --source-kind credential-store",
    );
    assert!(matrix_text.contains("availability_counts=none"));
    assert!(matrix_text.contains("availability_counts_total=available:1"));
}

#[test]
fn regression_execute_auth_command_status_rejects_missing_and_duplicate_filter_flags() {
    let config = test_auth_command_config();

    let missing_mode = execute_auth_command(&config, "status --mode");
    assert!(missing_mode.contains("auth error: missing auth mode after --mode"));
    assert!(missing_mode.contains("usage: /auth status"));

    let missing_mode_support = execute_auth_command(&config, "status --mode-support");
    assert!(missing_mode_support
        .contains("auth error: missing mode-support filter after --mode-support"));
    assert!(missing_mode_support.contains("usage: /auth status"));

    let missing_availability = execute_auth_command(&config, "status --availability");
    assert!(missing_availability
        .contains("auth error: missing availability filter after --availability"));
    assert!(missing_availability.contains("usage: /auth status"));

    let duplicate_mode = execute_auth_command(&config, "status --mode api-key --mode adc");
    assert!(duplicate_mode.contains("auth error: duplicate --mode flag"));
    assert!(duplicate_mode.contains("usage: /auth status"));

    let duplicate_mode_support = execute_auth_command(
        &config,
        "status --mode-support all --mode-support supported",
    );
    assert!(duplicate_mode_support.contains("auth error: duplicate --mode-support flag"));
    assert!(duplicate_mode_support.contains("usage: /auth status"));

    let duplicate_state = execute_auth_command(&config, "status --state ready --state revoked");
    assert!(duplicate_state.contains("auth error: duplicate --state flag"));
    assert!(duplicate_state.contains("usage: /auth status"));

    let missing_source_kind = execute_auth_command(&config, "status --source-kind");
    assert!(
        missing_source_kind.contains("auth error: missing source-kind filter after --source-kind")
    );
    assert!(missing_source_kind.contains("usage: /auth status"));

    let duplicate_source_kind =
        execute_auth_command(&config, "status --source-kind all --source-kind env");
    assert!(duplicate_source_kind.contains("auth error: duplicate --source-kind flag"));
    assert!(duplicate_source_kind.contains("usage: /auth status"));

    let missing_revoked = execute_auth_command(&config, "status --revoked");
    assert!(missing_revoked.contains("auth error: missing revoked filter after --revoked"));
    assert!(missing_revoked.contains("usage: /auth status"));

    let duplicate_revoked = execute_auth_command(&config, "status --revoked all --revoked revoked");
    assert!(duplicate_revoked.contains("auth error: duplicate --revoked flag"));
    assert!(duplicate_revoked.contains("usage: /auth status"));
}

#[test]
fn functional_execute_integration_auth_command_set_status_rotate_revoke_lifecycle() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("integration-credentials.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;

    let set_output =
        execute_integration_auth_command(&config, "set github-token ghp_secret --json");
    let set_json: serde_json::Value = serde_json::from_str(&set_output).expect("parse set");
    assert_eq!(set_json["command"], "integration_auth.set");
    assert_eq!(set_json["integration_id"], "github-token");
    assert_eq!(set_json["status"], "saved");
    assert!(!set_output.contains("ghp_secret"));

    let status_output = execute_integration_auth_command(&config, "status github-token --json");
    let status_json: serde_json::Value =
        serde_json::from_str(&status_output).expect("parse status");
    assert_eq!(status_json["integrations_total"], 1);
    assert_eq!(status_json["integrations"], 1);
    assert_eq!(status_json["available_total"], 1);
    assert_eq!(status_json["unavailable_total"], 0);
    assert_eq!(status_json["available"], 1);
    assert_eq!(status_json["unavailable"], 0);
    assert_eq!(status_json["state_counts_total"]["ready"], 1);
    assert_eq!(status_json["state_counts"]["ready"], 1);
    assert_eq!(status_json["revoked_counts_total"]["not_revoked"], 1);
    assert_eq!(status_json["revoked_counts"]["not_revoked"], 1);
    assert_eq!(status_json["entries"][0]["integration_id"], "github-token");
    assert_eq!(status_json["entries"][0]["state"], "ready");
    assert_eq!(status_json["entries"][0]["revoked"], false);
    assert!(!status_output.contains("ghp_secret"));

    let rotate_output =
        execute_integration_auth_command(&config, "rotate github-token ghp_rotated --json");
    let rotate_json: serde_json::Value =
        serde_json::from_str(&rotate_output).expect("parse rotate");
    assert_eq!(rotate_json["command"], "integration_auth.rotate");
    assert_eq!(rotate_json["status"], "rotated");
    assert!(!rotate_output.contains("ghp_rotated"));

    let revoke_output = execute_integration_auth_command(&config, "revoke github-token --json");
    let revoke_json: serde_json::Value =
        serde_json::from_str(&revoke_output).expect("parse revoke");
    assert_eq!(revoke_json["command"], "integration_auth.revoke");
    assert_eq!(revoke_json["status"], "revoked");

    let post_revoke_status =
        execute_integration_auth_command(&config, "status github-token --json");
    let post_revoke_json: serde_json::Value =
        serde_json::from_str(&post_revoke_status).expect("parse status");
    assert_eq!(post_revoke_json["available_total"], 0);
    assert_eq!(post_revoke_json["unavailable_total"], 1);
    assert_eq!(post_revoke_json["available"], 0);
    assert_eq!(post_revoke_json["unavailable"], 1);
    assert_eq!(post_revoke_json["state_counts_total"]["revoked"], 1);
    assert_eq!(post_revoke_json["state_counts"]["revoked"], 1);
    assert_eq!(post_revoke_json["revoked_counts_total"]["revoked"], 1);
    assert_eq!(post_revoke_json["revoked_counts"]["revoked"], 1);
    assert_eq!(post_revoke_json["entries"][0]["state"], "revoked");
    assert_eq!(post_revoke_json["entries"][0]["available"], false);

    let store = load_credential_store(
        &config.credential_store,
        CredentialStoreEncryptionMode::None,
        None,
    )
    .expect("load credential store");
    let entry = store
        .integrations
        .get("github-token")
        .expect("github integration entry");
    assert!(entry.secret.is_none());
    assert!(entry.revoked);
}

#[test]
fn functional_execute_integration_auth_command_status_reports_totals_with_filter() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("integration-status-totals.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;

    execute_integration_auth_command(&config, "set github-token ghp_ready --json");
    execute_integration_auth_command(&config, "set slack-token slack_ready --json");
    execute_integration_auth_command(&config, "revoke slack-token --json");

    let status_output = execute_integration_auth_command(&config, "status github-token --json");
    let status_json: serde_json::Value =
        serde_json::from_str(&status_output).expect("parse status totals");
    assert_eq!(status_json["integrations_total"], 2);
    assert_eq!(status_json["integrations"], 1);
    assert_eq!(status_json["available_total"], 1);
    assert_eq!(status_json["unavailable_total"], 1);
    assert_eq!(status_json["available"], 1);
    assert_eq!(status_json["unavailable"], 0);
    assert_eq!(status_json["state_counts_total"]["ready"], 1);
    assert_eq!(status_json["state_counts_total"]["revoked"], 1);
    assert_eq!(status_json["state_counts"]["ready"], 1);
    assert_eq!(status_json["revoked_counts_total"]["not_revoked"], 1);
    assert_eq!(status_json["revoked_counts_total"]["revoked"], 1);
    assert_eq!(status_json["revoked_counts"]["not_revoked"], 1);

    let text_output = execute_integration_auth_command(&config, "status github-token");
    assert!(text_output.contains("integrations=1"));
    assert!(text_output.contains("integrations_total=2"));
    assert!(text_output.contains("available=1"));
    assert!(text_output.contains("unavailable=0"));
    assert!(text_output.contains("available_total=1"));
    assert!(text_output.contains("unavailable_total=1"));
    assert!(text_output.contains("state_counts=ready:1"));
    assert!(text_output.contains("state_counts_total=ready:1,revoked:1"));
    assert!(text_output.contains("revoked_counts=not_revoked:1"));
    assert!(text_output.contains("revoked_counts_total=not_revoked:1,revoked:1"));
}

#[test]
fn regression_execute_integration_auth_command_status_handles_empty_store() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_auth_command_config();
    config.credential_store = temp.path().join("integration-status-empty.json");
    config.credential_store_encryption = CredentialStoreEncryptionMode::None;

    let store = CredentialStoreData {
        encryption: CredentialStoreEncryptionMode::None,
        providers: BTreeMap::new(),
        integrations: BTreeMap::new(),
    };
    save_credential_store(&config.credential_store, &store, None)
        .expect("save empty credential store");

    let output = execute_integration_auth_command(&config, "status --json");
    let payload: serde_json::Value =
        serde_json::from_str(&output).expect("parse empty integration status");
    assert_eq!(payload["integrations_total"], 0);
    assert_eq!(payload["integrations"], 0);
    assert_eq!(payload["available_total"], 0);
    assert_eq!(payload["unavailable_total"], 0);
    assert_eq!(payload["available"], 0);
    assert_eq!(payload["unavailable"], 0);
    assert_eq!(
        payload["state_counts_total"]
            .as_object()
            .expect("empty state counts total")
            .len(),
        0
    );
    assert_eq!(
        payload["state_counts"]
            .as_object()
            .expect("empty state counts")
            .len(),
        0
    );
    assert_eq!(
        payload["revoked_counts_total"]
            .as_object()
            .expect("empty revoked counts total")
            .len(),
        0
    );
    assert_eq!(
        payload["revoked_counts"]
            .as_object()
            .expect("empty revoked counts")
            .len(),
        0
    );

    let text_output = execute_integration_auth_command(&config, "status");
    assert!(text_output.contains("integrations=0"));
    assert!(text_output.contains("integrations_total=0"));
    assert!(text_output.contains("available=0"));
    assert!(text_output.contains("unavailable=0"));
    assert!(text_output.contains("available_total=0"));
    assert!(text_output.contains("unavailable_total=0"));
    assert!(text_output.contains("state_counts=none"));
    assert!(text_output.contains("state_counts_total=none"));
    assert!(text_output.contains("revoked_counts=none"));
    assert!(text_output.contains("revoked_counts_total=none"));
}

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

#[test]
fn resolve_api_key_returns_none_when_all_candidates_are_empty() {
    let key = resolve_api_key(vec![None, Some("".to_string())]);
    assert!(key.is_none());
}

#[test]
fn functional_openai_api_key_candidates_include_openrouter_groq_xai_mistral_and_azure_env_slots() {
    let candidates =
        provider_api_key_candidates_with_inputs(Provider::OpenAi, None, None, None, None);
    assert!(candidates
        .iter()
        .any(|(source, _)| *source == "OPENROUTER_API_KEY"));
    assert!(candidates
        .iter()
        .any(|(source, _)| *source == "GROQ_API_KEY"));
    assert!(candidates
        .iter()
        .any(|(source, _)| *source == "XAI_API_KEY"));
    assert!(candidates
        .iter()
        .any(|(source, _)| *source == "MISTRAL_API_KEY"));
    assert!(candidates
        .iter()
        .any(|(source, _)| *source == "AZURE_OPENAI_API_KEY"));
}

#[test]
fn unit_provider_auth_capability_reports_api_key_support() {
    let openai = provider_auth_capability(Provider::OpenAi, ProviderAuthMethod::ApiKey);
    assert!(openai.supported);
    assert_eq!(openai.reason, "supported");

    let anthropic = provider_auth_capability(Provider::Anthropic, ProviderAuthMethod::OauthToken);
    assert!(anthropic.supported);
    assert_eq!(anthropic.reason, "supported");

    let google = provider_auth_capability(Provider::Google, ProviderAuthMethod::OauthToken);
    assert!(google.supported);
    assert_eq!(google.reason, "supported");
}

#[test]
fn regression_build_provider_client_anthropic_oauth_mode_requires_backend_when_disabled() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let mut cli = test_cli();
    cli.anthropic_auth_mode = CliProviderAuthMode::OauthToken;
    cli.anthropic_claude_backend = false;
    cli.anthropic_api_key = None;
    cli.api_key = None;

    let snapshot = snapshot_env_vars(&["ANTHROPIC_API_KEY", "TAU_API_KEY"]);
    std::env::remove_var("ANTHROPIC_API_KEY");
    std::env::remove_var("TAU_API_KEY");

    match build_provider_client(&cli, Provider::Anthropic) {
        Ok(_) => panic!("oauth mode without backend should fail"),
        Err(error) => {
            assert!(error.to_string().contains("requires Claude Code backend"));
        }
    }

    restore_env_vars(snapshot);
}

#[test]
fn regression_build_provider_client_google_oauth_mode_requires_backend_when_disabled() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let mut cli = test_cli();
    cli.google_auth_mode = CliProviderAuthMode::OauthToken;
    cli.google_gemini_backend = false;
    cli.google_api_key = None;
    cli.api_key = None;

    let snapshot = snapshot_env_vars(&["GEMINI_API_KEY", "GOOGLE_API_KEY", "TAU_API_KEY"]);
    std::env::remove_var("GEMINI_API_KEY");
    std::env::remove_var("GOOGLE_API_KEY");
    std::env::remove_var("TAU_API_KEY");

    match build_provider_client(&cli, Provider::Google) {
        Ok(_) => panic!("oauth mode without backend should fail"),
        Err(error) => {
            assert!(error.to_string().contains("requires Gemini CLI backend"));
        }
    }

    restore_env_vars(snapshot);
}

#[test]
fn unit_build_provider_client_openai_oauth_mode_falls_back_to_api_key_when_oauth_unavailable() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let temp = tempdir().expect("tempdir");

    let mut cli = test_cli();
    cli.openai_auth_mode = CliProviderAuthMode::OauthToken;
    cli.openai_codex_backend = false;
    cli.openai_api_key = Some("openai-fallback-key".to_string());
    cli.api_key = None;
    cli.credential_store = temp.path().join("missing-openai-oauth-store.json");
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::None;

    let snapshot = snapshot_env_vars(&[
        "TAU_AUTH_ACCESS_TOKEN",
        "TAU_AUTH_EXPIRES_UNIX",
        "OPENAI_ACCESS_TOKEN",
        "OPENAI_AUTH_EXPIRES_UNIX",
    ]);
    std::env::remove_var("TAU_AUTH_ACCESS_TOKEN");
    std::env::remove_var("TAU_AUTH_EXPIRES_UNIX");
    std::env::remove_var("OPENAI_ACCESS_TOKEN");
    std::env::remove_var("OPENAI_AUTH_EXPIRES_UNIX");

    let client =
        build_provider_client(&cli, Provider::OpenAi).expect("build openai api-key fallback");
    let ptr = Arc::as_ptr(&client);
    assert!(!ptr.is_null());

    restore_env_vars(snapshot);
}

#[test]
fn functional_build_provider_client_anthropic_oauth_mode_falls_back_to_api_key_when_backend_unavailable(
) {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let mut cli = test_cli();
    cli.anthropic_auth_mode = CliProviderAuthMode::OauthToken;
    cli.anthropic_claude_backend = false;
    cli.anthropic_api_key = Some("anthropic-fallback-key".to_string());
    cli.api_key = None;

    let snapshot = snapshot_env_vars(&["ANTHROPIC_API_KEY", "TAU_API_KEY"]);
    std::env::remove_var("ANTHROPIC_API_KEY");
    std::env::remove_var("TAU_API_KEY");

    let client =
        build_provider_client(&cli, Provider::Anthropic).expect("build anthropic api-key fallback");
    let ptr = Arc::as_ptr(&client);
    assert!(!ptr.is_null());

    restore_env_vars(snapshot);
}

#[test]
fn regression_build_provider_client_anthropic_oauth_mode_strict_blocks_api_key_fallback() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let mut cli = test_cli();
    cli.anthropic_auth_mode = CliProviderAuthMode::OauthToken;
    cli.provider_subscription_strict = true;
    cli.anthropic_claude_backend = false;
    cli.anthropic_api_key = Some("anthropic-fallback-key".to_string());
    cli.api_key = None;

    let snapshot = snapshot_env_vars(&["ANTHROPIC_API_KEY", "TAU_API_KEY"]);
    std::env::remove_var("ANTHROPIC_API_KEY");
    std::env::remove_var("TAU_API_KEY");

    let error = match build_provider_client(&cli, Provider::Anthropic) {
        Ok(_) => panic!("strict mode should block api-key fallback"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("requires Claude Code backend"));

    restore_env_vars(snapshot);
}

#[test]
fn integration_build_provider_client_google_adc_mode_falls_back_to_api_key_when_backend_unavailable(
) {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let mut cli = test_cli();
    cli.google_auth_mode = CliProviderAuthMode::Adc;
    cli.google_gemini_backend = false;
    cli.google_api_key = Some("google-fallback-key".to_string());
    cli.api_key = None;

    let snapshot = snapshot_env_vars(&["GEMINI_API_KEY", "GOOGLE_API_KEY", "TAU_API_KEY"]);
    std::env::remove_var("GEMINI_API_KEY");
    std::env::remove_var("GOOGLE_API_KEY");
    std::env::remove_var("TAU_API_KEY");

    let client =
        build_provider_client(&cli, Provider::Google).expect("build google api-key fallback");
    let ptr = Arc::as_ptr(&client);
    assert!(!ptr.is_null());

    restore_env_vars(snapshot);
}

#[test]
fn regression_build_provider_client_google_adc_mode_strict_blocks_api_key_fallback() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let mut cli = test_cli();
    cli.google_auth_mode = CliProviderAuthMode::Adc;
    cli.provider_subscription_strict = true;
    cli.google_gemini_backend = false;
    cli.google_api_key = Some("google-fallback-key".to_string());
    cli.api_key = None;

    let snapshot = snapshot_env_vars(&["GEMINI_API_KEY", "GOOGLE_API_KEY", "TAU_API_KEY"]);
    std::env::remove_var("GEMINI_API_KEY");
    std::env::remove_var("GOOGLE_API_KEY");
    std::env::remove_var("TAU_API_KEY");

    let error = match build_provider_client(&cli, Provider::Google) {
        Ok(_) => panic!("strict mode should block api-key fallback"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("requires Gemini CLI backend"));

    restore_env_vars(snapshot);
}

#[test]
fn regression_build_provider_client_anthropic_oauth_mode_without_backend_or_api_key_still_errors() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let mut cli = test_cli();
    cli.anthropic_auth_mode = CliProviderAuthMode::OauthToken;
    cli.anthropic_claude_backend = false;
    cli.anthropic_api_key = None;
    cli.api_key = None;

    let snapshot = snapshot_env_vars(&["ANTHROPIC_API_KEY", "TAU_API_KEY"]);
    std::env::remove_var("ANTHROPIC_API_KEY");
    std::env::remove_var("TAU_API_KEY");

    match build_provider_client(&cli, Provider::Anthropic) {
        Ok(_) => panic!("missing fallback credential should fail"),
        Err(error) => {
            assert!(error.to_string().contains("requires Claude Code backend"));
        }
    }

    restore_env_vars(snapshot);
}

#[cfg(unix)]
#[tokio::test]
async fn integration_build_provider_client_uses_claude_backend_for_anthropic_oauth_mode() {
    let temp = tempdir().expect("tempdir");
    let script = write_mock_claude_script(
        temp.path(),
        r#"
if [ "$1" != "-p" ]; then
  echo "missing -p" >&2
  exit 8
fi
printf '{"type":"result","subtype":"success","is_error":false,"result":"claude backend response"}'
"#,
    );

    let mut cli = test_cli();
    cli.anthropic_auth_mode = CliProviderAuthMode::OauthToken;
    cli.anthropic_claude_backend = true;
    cli.anthropic_claude_cli = script.display().to_string();
    cli.anthropic_claude_timeout_ms = 5_000;
    cli.anthropic_api_key = None;

    let client =
        build_provider_client(&cli, Provider::Anthropic).expect("build claude backend client");
    let response = client
        .complete(test_chat_request())
        .await
        .expect("claude backend completion");
    assert_eq!(response.message.text_content(), "claude backend response");
}

#[cfg(unix)]
#[tokio::test]
async fn integration_build_provider_client_uses_gemini_backend_for_google_oauth_mode() {
    let temp = tempdir().expect("tempdir");
    let script = write_mock_gemini_script(
        temp.path(),
        r#"
if [ "$1" != "-p" ]; then
  echo "missing -p" >&2
  exit 8
fi
printf '{"response":"gemini backend response"}'
"#,
    );

    let mut cli = test_cli();
    cli.google_auth_mode = CliProviderAuthMode::OauthToken;
    cli.google_gemini_backend = true;
    cli.google_gemini_cli = script.display().to_string();
    cli.google_gemini_timeout_ms = 5_000;
    cli.google_api_key = None;

    let client =
        build_provider_client(&cli, Provider::Google).expect("build gemini backend client");
    let response = client
        .complete(test_chat_request())
        .await
        .expect("gemini backend completion");
    assert_eq!(response.message.text_content(), "gemini backend response");
}

#[test]
fn integration_build_provider_client_preserves_api_key_mode_behavior() {
    let mut cli = test_cli();
    cli.openai_api_key = Some("test-openai-key".to_string());

    let client = build_provider_client(&cli, Provider::OpenAi).expect("build client");
    let ptr = Arc::as_ptr(&client);
    assert!(!ptr.is_null());
}

#[cfg(unix)]
#[test]
fn unit_build_provider_client_openai_api_key_mode_falls_back_to_codex_backend_when_key_missing() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let temp = tempdir().expect("tempdir");
    let script = write_mock_codex_script(
        temp.path(),
        r#"
out=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output-last-message) out="$2"; shift 2;;
    *) shift;;
  esac
done
cat >/dev/null
printf "codex api-key fallback response" > "$out"
"#,
    );

    let mut cli = test_cli();
    cli.openai_auth_mode = CliProviderAuthMode::ApiKey;
    cli.openai_codex_backend = true;
    cli.openai_codex_cli = script.display().to_string();
    cli.openai_codex_timeout_ms = 5_000;
    cli.api_key = None;
    cli.openai_api_key = None;

    let snapshot = snapshot_env_vars(&[
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
        "GROQ_API_KEY",
        "XAI_API_KEY",
        "MISTRAL_API_KEY",
        "AZURE_OPENAI_API_KEY",
        "TAU_API_KEY",
    ]);
    std::env::remove_var("OPENAI_API_KEY");
    std::env::remove_var("OPENROUTER_API_KEY");
    std::env::remove_var("GROQ_API_KEY");
    std::env::remove_var("XAI_API_KEY");
    std::env::remove_var("MISTRAL_API_KEY");
    std::env::remove_var("AZURE_OPENAI_API_KEY");
    std::env::remove_var("TAU_API_KEY");

    let client =
        build_provider_client(&cli, Provider::OpenAi).expect("build codex fallback client");
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let response = runtime
        .block_on(client.complete(test_chat_request()))
        .expect("codex fallback completion");
    assert_eq!(
        response.message.text_content(),
        "codex api-key fallback response"
    );

    restore_env_vars(snapshot);
}

#[cfg(unix)]
#[test]
fn functional_build_provider_client_anthropic_api_key_mode_falls_back_to_claude_backend_when_key_missing(
) {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let temp = tempdir().expect("tempdir");
    let script = write_mock_claude_script(
        temp.path(),
        r#"
if [ "$1" != "-p" ]; then
  echo "missing -p" >&2
  exit 8
fi
printf '{"type":"result","subtype":"success","is_error":false,"result":"claude api-key fallback response"}'
"#,
    );

    let mut cli = test_cli();
    cli.anthropic_auth_mode = CliProviderAuthMode::ApiKey;
    cli.anthropic_claude_backend = true;
    cli.anthropic_claude_cli = script.display().to_string();
    cli.anthropic_claude_timeout_ms = 5_000;
    cli.api_key = None;
    cli.anthropic_api_key = None;

    let snapshot = snapshot_env_vars(&["ANTHROPIC_API_KEY", "TAU_API_KEY"]);
    std::env::remove_var("ANTHROPIC_API_KEY");
    std::env::remove_var("TAU_API_KEY");

    let client =
        build_provider_client(&cli, Provider::Anthropic).expect("build claude fallback client");
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let response = runtime
        .block_on(client.complete(test_chat_request()))
        .expect("claude fallback completion");
    assert_eq!(
        response.message.text_content(),
        "claude api-key fallback response"
    );

    restore_env_vars(snapshot);
}

#[cfg(unix)]
#[test]
fn integration_build_provider_client_google_api_key_mode_falls_back_to_gemini_backend_when_key_missing(
) {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let temp = tempdir().expect("tempdir");
    let script = write_mock_gemini_script(
        temp.path(),
        r#"
if [ "$1" != "-p" ]; then
  echo "missing -p" >&2
  exit 8
fi
printf '{"response":"gemini api-key fallback response"}'
"#,
    );

    let mut cli = test_cli();
    cli.google_auth_mode = CliProviderAuthMode::ApiKey;
    cli.google_gemini_backend = true;
    cli.google_gemini_cli = script.display().to_string();
    cli.google_gemini_timeout_ms = 5_000;
    cli.api_key = None;
    cli.google_api_key = None;

    let snapshot = snapshot_env_vars(&["GEMINI_API_KEY", "GOOGLE_API_KEY", "TAU_API_KEY"]);
    std::env::remove_var("GEMINI_API_KEY");
    std::env::remove_var("GOOGLE_API_KEY");
    std::env::remove_var("TAU_API_KEY");

    let client =
        build_provider_client(&cli, Provider::Google).expect("build gemini fallback client");
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let response = runtime
        .block_on(client.complete(test_chat_request()))
        .expect("gemini fallback completion");
    assert_eq!(
        response.message.text_content(),
        "gemini api-key fallback response"
    );

    restore_env_vars(snapshot);
}

#[test]
fn regression_build_provider_client_openai_api_key_mode_without_backend_keeps_missing_key_error() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let mut cli = test_cli();
    cli.openai_auth_mode = CliProviderAuthMode::ApiKey;
    cli.openai_codex_backend = false;
    cli.api_key = None;
    cli.openai_api_key = None;

    let snapshot = snapshot_env_vars(&[
        "OPENAI_API_KEY",
        "OPENROUTER_API_KEY",
        "GROQ_API_KEY",
        "XAI_API_KEY",
        "MISTRAL_API_KEY",
        "AZURE_OPENAI_API_KEY",
        "TAU_API_KEY",
    ]);
    std::env::remove_var("OPENAI_API_KEY");
    std::env::remove_var("OPENROUTER_API_KEY");
    std::env::remove_var("GROQ_API_KEY");
    std::env::remove_var("XAI_API_KEY");
    std::env::remove_var("MISTRAL_API_KEY");
    std::env::remove_var("AZURE_OPENAI_API_KEY");
    std::env::remove_var("TAU_API_KEY");

    match build_provider_client(&cli, Provider::OpenAi) {
        Ok(_) => panic!("missing key without backend should fail"),
        Err(error) => {
            assert!(error
                .to_string()
                .contains("missing OpenAI-compatible API key"));
        }
    }

    restore_env_vars(snapshot);
}

#[test]
fn unit_encrypt_and_decrypt_credential_store_secret_roundtrip_keyed() {
    let secret = "secret-token-123";
    let encoded = encrypt_credential_store_secret(
        secret,
        CredentialStoreEncryptionMode::Keyed,
        Some("very-strong-key"),
    )
    .expect("encode credential");
    assert!(encoded.starts_with("enc:v1:"));
    assert!(!encoded.contains(secret));

    let decoded = decrypt_credential_store_secret(
        &encoded,
        CredentialStoreEncryptionMode::Keyed,
        Some("very-strong-key"),
    )
    .expect("decode credential");
    assert_eq!(decoded, secret);
}

#[test]
fn regression_decrypt_credential_store_secret_rejects_wrong_key() {
    let encoded = encrypt_credential_store_secret(
        "secret-token-xyz",
        CredentialStoreEncryptionMode::Keyed,
        Some("correct-key-123"),
    )
    .expect("encode credential");

    let error = decrypt_credential_store_secret(
        &encoded,
        CredentialStoreEncryptionMode::Keyed,
        Some("wrong-key-123"),
    )
    .expect_err("wrong key should fail");
    assert!(error.to_string().contains("integrity check failed"));
}

#[test]
fn functional_credential_store_roundtrip_preserves_provider_records() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("credentials.json");
    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::Keyed,
        Some("credential-key"),
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("openai-access".to_string()),
            refresh_token: Some("openai-refresh".to_string()),
            expires_unix: Some(12345),
            revoked: false,
        },
    );

    let loaded = load_credential_store(
        &store_path,
        CredentialStoreEncryptionMode::None,
        Some("credential-key"),
    )
    .expect("load credential store");
    let entry = loaded
        .providers
        .get("openai")
        .expect("openai entry should exist");
    assert_eq!(entry.auth_method, ProviderAuthMethod::OauthToken);
    assert_eq!(entry.access_token.as_deref(), Some("openai-access"));
    assert_eq!(entry.refresh_token.as_deref(), Some("openai-refresh"));
    assert_eq!(entry.expires_unix, Some(12345));
    assert!(!entry.revoked);
}

#[test]
fn integration_credential_store_roundtrip_preserves_integration_records() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("integration-credentials.json");
    write_test_integration_credential(
        &store_path,
        CredentialStoreEncryptionMode::Keyed,
        Some("credential-key"),
        "github-token",
        IntegrationCredentialStoreRecord {
            secret: Some("ghp_top_secret".to_string()),
            revoked: false,
            updated_unix: Some(98765),
        },
    );

    let loaded = load_credential_store(
        &store_path,
        CredentialStoreEncryptionMode::None,
        Some("credential-key"),
    )
    .expect("load credential store");
    let entry = loaded
        .integrations
        .get("github-token")
        .expect("integration entry");
    assert_eq!(entry.secret.as_deref(), Some("ghp_top_secret"));
    assert!(!entry.revoked);
    assert_eq!(entry.updated_unix, Some(98765));
}

#[test]
fn regression_load_credential_store_allows_legacy_provider_only_payload() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("legacy-credentials.json");
    std::fs::write(
        &store_path,
        r#"{
  "schema_version": 1,
  "encryption": "none",
  "providers": {
    "openai": {
      "auth_method": "oauth_token",
      "access_token": "legacy-access",
      "refresh_token": "legacy-refresh",
      "expires_unix": 42,
      "revoked": false
    }
  }
}
"#,
    )
    .expect("write legacy credential store");

    let loaded = load_credential_store(&store_path, CredentialStoreEncryptionMode::None, None)
        .expect("load legacy credential store");
    assert!(loaded.integrations.is_empty());
    assert_eq!(
        loaded
            .providers
            .get("openai")
            .and_then(|entry| entry.access_token.as_deref()),
        Some("legacy-access")
    );
}

#[test]
fn functional_resolve_store_backed_provider_credential_refreshes_expired_token() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("credentials.json");
    let now = current_unix_timestamp();

    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("stale-access".to_string()),
            refresh_token: Some("refresh-token".to_string()),
            expires_unix: Some(now.saturating_sub(30)),
            revoked: false,
        },
    );

    let mut cli = test_cli();
    cli.credential_store = store_path.clone();
    cli.openai_auth_mode = CliProviderAuthMode::OauthToken;
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::None;

    let resolved = resolve_store_backed_provider_credential(
        &cli,
        Provider::OpenAi,
        ProviderAuthMethod::OauthToken,
    )
    .expect("resolve refreshed credential");
    assert_eq!(resolved.method, ProviderAuthMethod::OauthToken);
    assert_eq!(resolved.source.as_deref(), Some("credential_store"));
    let access = resolved.secret.expect("access token");
    assert!(access.starts_with("openai_access_"));
    assert_ne!(access, "stale-access");

    let persisted = load_credential_store(&store_path, CredentialStoreEncryptionMode::None, None)
        .expect("reload store");
    let entry = persisted.providers.get("openai").expect("openai entry");
    assert_eq!(entry.access_token.as_deref(), Some(access.as_str()));
    assert!(entry.expires_unix.unwrap_or(0) > now);
}

#[test]
fn functional_refresh_provider_access_token_generates_deterministic_shape() {
    let refreshed = refresh_provider_access_token(Provider::OpenAi, "refresh-token", 1700)
        .expect("refresh token");
    assert!(refreshed.access_token.starts_with("openai_access_"));
    assert!(refreshed
        .refresh_token
        .as_deref()
        .unwrap_or_default()
        .starts_with("openai_refresh_"));
    assert_eq!(refreshed.expires_unix, Some(1700 + 3600));
}

#[test]
fn regression_resolve_store_backed_provider_credential_marks_revoked_refresh_token() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("credentials.json");
    let now = current_unix_timestamp();

    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("stale-access".to_string()),
            refresh_token: Some("revoked-refresh-token".to_string()),
            expires_unix: Some(now.saturating_sub(5)),
            revoked: false,
        },
    );

    let mut cli = test_cli();
    cli.credential_store = store_path.clone();
    cli.openai_auth_mode = CliProviderAuthMode::OauthToken;
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::None;

    let error = resolve_store_backed_provider_credential(
        &cli,
        Provider::OpenAi,
        ProviderAuthMethod::OauthToken,
    )
    .expect_err("revoked refresh should require re-auth");
    assert!(error.to_string().contains("requires re-authentication"));
    assert!(error.to_string().contains("revoked"));

    let persisted = load_credential_store(&store_path, CredentialStoreEncryptionMode::None, None)
        .expect("reload store");
    let entry = persisted.providers.get("openai").expect("openai entry");
    assert!(entry.revoked);
}

#[test]
fn regression_resolve_store_backed_provider_credential_hides_corrupted_payload_values() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("credentials.json");
    let leaked_value = "leaky-secret-token";
    let payload = format!(
            "{{\"schema_version\":1,\"encryption\":\"keyed\",\"providers\":{{\"openai\":{{\"auth_method\":\"oauth_token\",\"access_token\":\"enc:v1:not-base64-{leaked_value}\",\"refresh_token\":null,\"expires_unix\":null,\"revoked\":false}}}}}}"
        );
    std::fs::write(&store_path, payload).expect("write corrupted store");

    let mut cli = test_cli();
    cli.credential_store = store_path;
    cli.credential_store_key = Some("valid-key-123".to_string());
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::Keyed;
    cli.openai_auth_mode = CliProviderAuthMode::OauthToken;

    let error = resolve_store_backed_provider_credential(
        &cli,
        Provider::OpenAi,
        ProviderAuthMethod::OauthToken,
    )
    .expect_err("corrupted store should fail");
    let message = error.to_string();
    assert!(
        message.contains("failed to load provider credential store")
            || message.contains("invalid or corrupted")
    );
    assert!(!error.to_string().contains(leaked_value));
}

#[test]
fn integration_build_provider_client_supports_openai_oauth_from_credential_store() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("credentials.json");
    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::OauthToken,
            access_token: Some("openai-oauth-access".to_string()),
            refresh_token: Some("refresh-token".to_string()),
            expires_unix: Some(current_unix_timestamp().saturating_add(900)),
            revoked: false,
        },
    );

    let mut cli = test_cli();
    cli.openai_auth_mode = CliProviderAuthMode::OauthToken;
    cli.credential_store = store_path;
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::None;

    let client = build_provider_client(&cli, Provider::OpenAi).expect("build oauth client");
    let ptr = Arc::as_ptr(&client);
    assert!(!ptr.is_null());
}

#[test]
fn integration_build_provider_client_supports_openai_session_token_from_credential_store() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("credentials.json");
    write_test_provider_credential(
        &store_path,
        CredentialStoreEncryptionMode::None,
        None,
        Provider::OpenAi,
        ProviderCredentialStoreRecord {
            auth_method: ProviderAuthMethod::SessionToken,
            access_token: Some("openai-session-access".to_string()),
            refresh_token: None,
            expires_unix: Some(current_unix_timestamp().saturating_add(900)),
            revoked: false,
        },
    );

    let mut cli = test_cli();
    cli.openai_auth_mode = CliProviderAuthMode::SessionToken;
    cli.credential_store = store_path;
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::None;

    let client = build_provider_client(&cli, Provider::OpenAi).expect("build session client");
    let ptr = Arc::as_ptr(&client);
    assert!(!ptr.is_null());
}

#[test]
fn unit_resolve_credential_store_encryption_mode_auto_uses_key_presence() {
    let mut cli = test_cli();
    cli.credential_store_encryption = CliCredentialStoreEncryptionMode::Auto;
    cli.credential_store_key = None;
    assert_eq!(
        resolve_credential_store_encryption_mode(&cli),
        CredentialStoreEncryptionMode::None
    );

    cli.credential_store_key = Some("configured-key".to_string());
    assert_eq!(
        resolve_credential_store_encryption_mode(&cli),
        CredentialStoreEncryptionMode::Keyed
    );
}
