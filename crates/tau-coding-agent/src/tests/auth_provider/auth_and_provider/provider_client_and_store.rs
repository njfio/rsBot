//! Tests for provider-client fallback behavior and credential-store encryption/refresh flows.

use super::*;
use tau_provider::{DecryptedSecret, FileSecretStore, SecretStore};

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
fn unit_spec_c01_decrypted_secret_redacts_debug_and_display() {
    let secret = DecryptedSecret::new("top-secret-token").expect("construct secret wrapper");
    assert_eq!(secret.expose(), "top-secret-token");
    assert_eq!(format!("{secret}"), "[REDACTED]");
    assert_eq!(format!("{secret:?}"), "[REDACTED]");
}

#[test]
fn functional_spec_c02_file_secret_store_roundtrip_preserves_integration_secret() {
    let temp = tempdir().expect("tempdir");
    let store_path = temp.path().join("secrets.json");
    let secret_store = FileSecretStore::new(store_path.clone());

    secret_store
        .write_integration_secret(
            "discord-bot-token",
            "super-secret-token",
            CredentialStoreEncryptionMode::Keyed,
            None,
            Some(123),
        )
        .expect("write integration secret");

    let decrypted = secret_store
        .read_integration_secret(
            "discord-bot-token",
            CredentialStoreEncryptionMode::None,
            None,
        )
        .expect("read integration secret")
        .expect("stored secret should exist");
    assert_eq!(decrypted.expose(), "super-secret-token");
    assert_eq!(format!("{decrypted}"), "[REDACTED]");

    let raw = std::fs::read_to_string(&store_path).expect("read raw secret store");
    assert!(raw.contains("\"encryption\": \"keyed\""));
    assert!(!raw.contains("super-secret-token"));
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
        CredentialStoreEncryptionMode::Keyed
    );

    cli.credential_store_key = Some("configured-key".to_string());
    assert_eq!(
        resolve_credential_store_encryption_mode(&cli),
        CredentialStoreEncryptionMode::Keyed
    );
}
