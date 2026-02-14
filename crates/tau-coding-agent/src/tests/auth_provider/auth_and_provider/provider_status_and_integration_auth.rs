//! Tests for auth status filter composition and integration auth status lifecycle flows.

use super::*;

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
