use anyhow::Result;
use std::sync::Arc;

use tau_ai::{LlmClient, ModelRef};
use tau_cli::Cli;
use tau_cli::CliGatewayOpenResponsesAuthMode;
use tau_gateway::{
    GatewayOpenResponsesAuthMode, GatewayOpenResponsesServerConfig, GatewayToolRegistrarFn,
};
use tau_multi_channel::{
    MultiChannelLiveConnectorsConfig, MultiChannelMediaUnderstandingConfig,
    MultiChannelOutboundConfig, MultiChannelTelemetryConfig,
};
use tau_provider::{load_credential_store, resolve_credential_store_encryption_mode};
use tau_tools::tools::{register_builtin_tools, ToolPolicy};

pub fn map_gateway_openresponses_auth_mode(
    mode: CliGatewayOpenResponsesAuthMode,
) -> GatewayOpenResponsesAuthMode {
    match mode {
        CliGatewayOpenResponsesAuthMode::Token => GatewayOpenResponsesAuthMode::Token,
        CliGatewayOpenResponsesAuthMode::PasswordSession => {
            GatewayOpenResponsesAuthMode::PasswordSession
        }
        CliGatewayOpenResponsesAuthMode::LocalhostDev => GatewayOpenResponsesAuthMode::LocalhostDev,
    }
}

pub fn resolve_gateway_openresponses_auth(cli: &Cli) -> (Option<String>, Option<String>) {
    let auth_token = resolve_non_empty_cli_value(cli.gateway_openresponses_auth_token.as_deref());
    let auth_password =
        resolve_non_empty_cli_value(cli.gateway_openresponses_auth_password.as_deref());
    (auth_token, auth_password)
}

pub fn build_gateway_openresponses_server_config(
    cli: &Cli,
    client: Arc<dyn LlmClient>,
    model_ref: &ModelRef,
    system_prompt: &str,
    tool_policy: &ToolPolicy,
) -> GatewayOpenResponsesServerConfig {
    let (auth_token, auth_password) = resolve_gateway_openresponses_auth(cli);
    let policy = tool_policy.clone();
    GatewayOpenResponsesServerConfig {
        client,
        model: model_ref.model.clone(),
        system_prompt: system_prompt.to_string(),
        max_turns: cli.max_turns,
        tool_registrar: Arc::new(GatewayToolRegistrarFn::new(move |agent| {
            register_builtin_tools(agent, policy.clone());
        })),
        turn_timeout_ms: cli.turn_timeout_ms,
        session_lock_wait_ms: cli.session_lock_wait_ms,
        session_lock_stale_ms: cli.session_lock_stale_ms,
        state_dir: cli.gateway_state_dir.clone(),
        bind: cli.gateway_openresponses_bind.clone(),
        auth_mode: map_gateway_openresponses_auth_mode(cli.gateway_openresponses_auth_mode),
        auth_token,
        auth_password,
        session_ttl_seconds: cli.gateway_openresponses_session_ttl_seconds,
        rate_limit_window_seconds: cli.gateway_openresponses_rate_limit_window_seconds,
        rate_limit_max_requests: cli.gateway_openresponses_rate_limit_max_requests,
        max_input_chars: cli.gateway_openresponses_max_input_chars,
    }
}

pub async fn run_gateway_openresponses_server_if_requested(
    cli: &Cli,
    client: Arc<dyn LlmClient>,
    model_ref: &ModelRef,
    system_prompt: &str,
    tool_policy: &ToolPolicy,
) -> Result<bool> {
    if !cli.gateway_openresponses_server {
        return Ok(false);
    }
    let config = build_gateway_openresponses_server_config(
        cli,
        client,
        model_ref,
        system_prompt,
        tool_policy,
    );
    tau_gateway::run_gateway_openresponses_server(config).await?;
    Ok(true)
}

pub fn build_multi_channel_live_connectors_config(cli: &Cli) -> MultiChannelLiveConnectorsConfig {
    MultiChannelLiveConnectorsConfig {
        state_path: cli.multi_channel_live_connectors_state_path.clone(),
        ingress_dir: cli.multi_channel_live_ingress_dir.clone(),
        processed_event_cap: cli.multi_channel_processed_event_cap.max(1),
        retry_max_attempts: cli.multi_channel_retry_max_attempts.max(1),
        retry_base_delay_ms: cli.multi_channel_retry_base_delay_ms,
        poll_once: cli.multi_channel_live_connectors_poll_once,
        webhook_bind: cli.multi_channel_live_webhook_bind.clone(),
        telegram_mode: cli.multi_channel_telegram_ingress_mode.into(),
        telegram_api_base: cli.multi_channel_telegram_api_base.trim().to_string(),
        telegram_bot_token: resolve_multi_channel_outbound_secret(
            cli,
            cli.multi_channel_telegram_bot_token.as_deref(),
            "telegram-bot-token",
        ),
        telegram_webhook_secret: resolve_non_empty_cli_value(
            cli.multi_channel_telegram_webhook_secret.as_deref(),
        ),
        discord_mode: cli.multi_channel_discord_ingress_mode.into(),
        discord_api_base: cli.multi_channel_discord_api_base.trim().to_string(),
        discord_bot_token: resolve_multi_channel_outbound_secret(
            cli,
            cli.multi_channel_discord_bot_token.as_deref(),
            "discord-bot-token",
        ),
        discord_ingress_channel_ids: cli
            .multi_channel_discord_ingress_channel_ids
            .iter()
            .map(|value| value.trim().to_string())
            .collect(),
        whatsapp_mode: cli.multi_channel_whatsapp_ingress_mode.into(),
        whatsapp_webhook_verify_token: resolve_non_empty_cli_value(
            cli.multi_channel_whatsapp_webhook_verify_token.as_deref(),
        ),
        whatsapp_webhook_app_secret: resolve_non_empty_cli_value(
            cli.multi_channel_whatsapp_webhook_app_secret.as_deref(),
        ),
    }
}

pub async fn run_multi_channel_live_connectors_if_requested(cli: &Cli) -> Result<bool> {
    if !cli.multi_channel_live_connectors_runner {
        return Ok(false);
    }
    let config = build_multi_channel_live_connectors_config(cli);
    tau_multi_channel::run_multi_channel_live_connectors_runner(config).await?;
    Ok(true)
}

pub fn resolve_multi_channel_outbound_secret(
    cli: &Cli,
    direct_secret: Option<&str>,
    integration_id: &str,
) -> Option<String> {
    if let Some(secret) = resolve_non_empty_cli_value(direct_secret) {
        return Some(secret);
    }
    let store = load_credential_store(
        &cli.credential_store,
        resolve_credential_store_encryption_mode(cli),
        cli.credential_store_key.as_deref(),
    )
    .ok()?;
    let entry = store.integrations.get(integration_id)?;
    if entry.revoked {
        return None;
    }
    entry
        .secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn build_multi_channel_outbound_config(cli: &Cli) -> MultiChannelOutboundConfig {
    MultiChannelOutboundConfig {
        mode: cli.multi_channel_outbound_mode.into(),
        max_chars: cli.multi_channel_outbound_max_chars.max(1),
        http_timeout_ms: cli.multi_channel_outbound_http_timeout_ms.max(1),
        telegram_api_base: cli.multi_channel_telegram_api_base.trim().to_string(),
        discord_api_base: cli.multi_channel_discord_api_base.trim().to_string(),
        whatsapp_api_base: cli.multi_channel_whatsapp_api_base.trim().to_string(),
        telegram_bot_token: resolve_multi_channel_outbound_secret(
            cli,
            cli.multi_channel_telegram_bot_token.as_deref(),
            "telegram-bot-token",
        ),
        discord_bot_token: resolve_multi_channel_outbound_secret(
            cli,
            cli.multi_channel_discord_bot_token.as_deref(),
            "discord-bot-token",
        ),
        whatsapp_access_token: resolve_multi_channel_outbound_secret(
            cli,
            cli.multi_channel_whatsapp_access_token.as_deref(),
            "whatsapp-access-token",
        ),
        whatsapp_phone_number_id: resolve_multi_channel_outbound_secret(
            cli,
            cli.multi_channel_whatsapp_phone_number_id.as_deref(),
            "whatsapp-phone-number-id",
        ),
    }
}

pub fn build_multi_channel_telemetry_config(cli: &Cli) -> MultiChannelTelemetryConfig {
    MultiChannelTelemetryConfig {
        typing_presence_enabled: cli.multi_channel_telemetry_typing_presence,
        usage_summary_enabled: cli.multi_channel_telemetry_usage_summary,
        include_identifiers: cli.multi_channel_telemetry_include_identifiers,
        typing_presence_min_response_chars: cli.multi_channel_telemetry_min_response_chars.max(1),
    }
}

pub fn build_multi_channel_media_config(cli: &Cli) -> MultiChannelMediaUnderstandingConfig {
    MultiChannelMediaUnderstandingConfig {
        enabled: cli.multi_channel_media_understanding,
        max_attachments_per_event: cli.multi_channel_media_max_attachments.max(1),
        max_summary_chars: cli.multi_channel_media_max_summary_chars.max(16),
    }
}

fn resolve_non_empty_cli_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::{
        build_gateway_openresponses_server_config, build_multi_channel_live_connectors_config,
        build_multi_channel_media_config, build_multi_channel_outbound_config,
        build_multi_channel_telemetry_config, map_gateway_openresponses_auth_mode,
        resolve_gateway_openresponses_auth, resolve_multi_channel_outbound_secret,
    };
    use async_trait::async_trait;
    use clap::Parser;
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::sync::Arc;
    use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, ModelRef, TauAiError};
    use tau_cli::{Cli, CliGatewayOpenResponsesAuthMode};
    use tau_gateway::GatewayOpenResponsesAuthMode;
    use tau_multi_channel::MultiChannelLiveConnectorMode;
    use tau_provider::{
        load_credential_store, save_credential_store, CredentialStoreData,
        CredentialStoreEncryptionMode, IntegrationCredentialStoreRecord,
    };
    use tau_tools::tools::ToolPolicy;
    use tempfile::tempdir;

    fn parse_cli_with_stack() -> Cli {
        std::thread::Builder::new()
            .name("tau-cli-parse".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(|| Cli::parse_from(["tau-rs"]))
            .expect("spawn cli parse thread")
            .join()
            .expect("join cli parse thread")
    }

    struct NoopClient;

    #[async_trait]
    impl LlmClient for NoopClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            Ok(ChatResponse {
                message: Message::assistant_text("ok"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            })
        }
    }

    fn write_integration_secret(
        path: &Path,
        integration_id: &str,
        secret: Option<&str>,
        revoked: bool,
    ) {
        let mut store = load_credential_store(path, CredentialStoreEncryptionMode::None, None)
            .unwrap_or(CredentialStoreData {
                encryption: CredentialStoreEncryptionMode::None,
                providers: BTreeMap::new(),
                integrations: BTreeMap::new(),
            });
        store.integrations.insert(
            integration_id.to_string(),
            IntegrationCredentialStoreRecord {
                secret: secret.map(str::to_string),
                revoked,
                updated_unix: Some(100),
            },
        );
        save_credential_store(path, &store, None).expect("save credential store");
    }

    #[test]
    fn unit_resolve_multi_channel_outbound_secret_prefers_direct_secret() {
        let cli = parse_cli_with_stack();
        let resolved =
            resolve_multi_channel_outbound_secret(&cli, Some("  direct-secret  "), "unused");
        assert_eq!(resolved.as_deref(), Some("direct-secret"));
    }

    #[test]
    fn unit_map_gateway_openresponses_auth_mode_matches_cli_variants() {
        assert_eq!(
            map_gateway_openresponses_auth_mode(CliGatewayOpenResponsesAuthMode::Token),
            GatewayOpenResponsesAuthMode::Token
        );
        assert_eq!(
            map_gateway_openresponses_auth_mode(CliGatewayOpenResponsesAuthMode::PasswordSession),
            GatewayOpenResponsesAuthMode::PasswordSession
        );
        assert_eq!(
            map_gateway_openresponses_auth_mode(CliGatewayOpenResponsesAuthMode::LocalhostDev),
            GatewayOpenResponsesAuthMode::LocalhostDev
        );
    }

    #[test]
    fn functional_resolve_gateway_openresponses_auth_trims_non_empty_values() {
        let mut cli = parse_cli_with_stack();
        cli.gateway_openresponses_auth_token = Some(" token-value ".to_string());
        cli.gateway_openresponses_auth_password = Some(" password-value ".to_string());

        let (token, password) = resolve_gateway_openresponses_auth(&cli);
        assert_eq!(token.as_deref(), Some("token-value"));
        assert_eq!(password.as_deref(), Some("password-value"));
    }

    #[test]
    fn regression_resolve_gateway_openresponses_auth_ignores_empty_values() {
        let mut cli = parse_cli_with_stack();
        cli.gateway_openresponses_auth_token = Some("   ".to_string());
        cli.gateway_openresponses_auth_password = Some(String::new());

        let (token, password) = resolve_gateway_openresponses_auth(&cli);
        assert!(token.is_none());
        assert!(password.is_none());
    }

    #[test]
    fn integration_build_gateway_openresponses_server_config_preserves_runtime_fields() {
        let mut cli = parse_cli_with_stack();
        cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::PasswordSession;
        cli.gateway_openresponses_auth_password = Some("  secret-pass  ".to_string());
        cli.gateway_openresponses_auth_token = Some("  secret-token  ".to_string());
        cli.gateway_openresponses_bind = "127.0.0.1:9090".to_string();
        cli.max_turns = 7;
        cli.turn_timeout_ms = 20_000;
        cli.gateway_openresponses_session_ttl_seconds = 1_800;
        cli.gateway_openresponses_rate_limit_window_seconds = 120;
        cli.gateway_openresponses_rate_limit_max_requests = 40;
        cli.gateway_openresponses_max_input_chars = 24_000;

        let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
        let client: Arc<dyn LlmClient> = Arc::new(NoopClient);
        let tool_policy = ToolPolicy::new(vec![]);
        let config = build_gateway_openresponses_server_config(
            &cli,
            client.clone(),
            &model_ref,
            "system prompt",
            &tool_policy,
        );

        assert_eq!(config.model, "gpt-4o-mini");
        assert_eq!(config.system_prompt, "system prompt");
        assert_eq!(config.max_turns, 7);
        assert_eq!(config.turn_timeout_ms, 20_000);
        assert_eq!(config.bind, "127.0.0.1:9090");
        assert_eq!(
            config.auth_mode,
            GatewayOpenResponsesAuthMode::PasswordSession
        );
        assert_eq!(config.auth_token.as_deref(), Some("secret-token"));
        assert_eq!(config.auth_password.as_deref(), Some("secret-pass"));
        assert_eq!(config.session_ttl_seconds, 1_800);
        assert_eq!(config.rate_limit_window_seconds, 120);
        assert_eq!(config.rate_limit_max_requests, 40);
        assert_eq!(config.max_input_chars, 24_000);
    }

    #[tokio::test]
    async fn unit_run_gateway_openresponses_server_if_requested_returns_false_when_disabled() {
        let cli = parse_cli_with_stack();
        let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
        let client: Arc<dyn LlmClient> = Arc::new(NoopClient);
        let tool_policy = ToolPolicy::new(vec![]);

        let handled = super::run_gateway_openresponses_server_if_requested(
            &cli,
            client,
            &model_ref,
            "system prompt",
            &tool_policy,
        )
        .await
        .expect("gateway helper");

        assert!(!handled);
    }

    #[tokio::test]
    async fn unit_run_multi_channel_live_connectors_if_requested_returns_false_when_disabled() {
        let cli = parse_cli_with_stack();

        let handled = super::run_multi_channel_live_connectors_if_requested(&cli)
            .await
            .expect("connectors helper");

        assert!(!handled);
    }

    #[test]
    fn integration_build_multi_channel_live_connectors_config_preserves_runtime_fields() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.credential_store = temp.path().join("credentials.json");
        cli.multi_channel_live_connectors_state_path = temp.path().join("connectors-state.json");
        cli.multi_channel_live_ingress_dir = temp.path().join("ingress");
        cli.multi_channel_processed_event_cap = 0;
        cli.multi_channel_retry_max_attempts = 0;
        cli.multi_channel_retry_base_delay_ms = 42;
        cli.multi_channel_live_connectors_poll_once = true;
        cli.multi_channel_live_webhook_bind = "127.0.0.1:9999".to_string();
        cli.multi_channel_telegram_ingress_mode =
            tau_cli::CliMultiChannelLiveConnectorMode::Polling;
        cli.multi_channel_discord_ingress_mode = tau_cli::CliMultiChannelLiveConnectorMode::Webhook;
        cli.multi_channel_whatsapp_ingress_mode =
            tau_cli::CliMultiChannelLiveConnectorMode::Webhook;
        cli.multi_channel_telegram_api_base = " https://telegram.example ".to_string();
        cli.multi_channel_discord_api_base = " https://discord.example ".to_string();
        cli.multi_channel_telegram_bot_token = Some(" telegram-direct ".to_string());
        cli.multi_channel_discord_ingress_channel_ids =
            vec![" 111 ".to_string(), "222".to_string()];
        cli.multi_channel_telegram_webhook_secret = Some(" tg-secret ".to_string());
        cli.multi_channel_whatsapp_webhook_verify_token = Some(" wa-verify-secret ".to_string());
        cli.multi_channel_whatsapp_webhook_app_secret = Some(" wa-app-secret ".to_string());
        write_integration_secret(
            &cli.credential_store,
            "discord-bot-token",
            Some("discord-store"),
            false,
        );

        let config = build_multi_channel_live_connectors_config(&cli);
        assert_eq!(
            config.state_path,
            cli.multi_channel_live_connectors_state_path
        );
        assert_eq!(config.ingress_dir, cli.multi_channel_live_ingress_dir);
        assert_eq!(config.processed_event_cap, 1);
        assert_eq!(config.retry_max_attempts, 1);
        assert_eq!(config.retry_base_delay_ms, 42);
        assert!(config.poll_once);
        assert_eq!(config.webhook_bind, "127.0.0.1:9999");
        assert_eq!(config.telegram_mode, MultiChannelLiveConnectorMode::Polling);
        assert_eq!(config.discord_mode, MultiChannelLiveConnectorMode::Webhook);
        assert_eq!(config.whatsapp_mode, MultiChannelLiveConnectorMode::Webhook);
        assert_eq!(config.telegram_api_base, "https://telegram.example");
        assert_eq!(config.discord_api_base, "https://discord.example");
        assert_eq!(
            config.telegram_bot_token.as_deref(),
            Some("telegram-direct")
        );
        assert_eq!(config.discord_bot_token.as_deref(), Some("discord-store"));
        assert_eq!(config.discord_ingress_channel_ids, vec!["111", "222"]);
        assert_eq!(config.telegram_webhook_secret.as_deref(), Some("tg-secret"));
        assert_eq!(
            config.whatsapp_webhook_verify_token.as_deref(),
            Some("wa-verify-secret")
        );
        assert_eq!(
            config.whatsapp_webhook_app_secret.as_deref(),
            Some("wa-app-secret")
        );
    }

    #[test]
    fn functional_resolve_multi_channel_outbound_secret_reads_active_store_entry() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.credential_store = temp.path().join("credentials.json");
        write_integration_secret(
            &cli.credential_store,
            "telegram-bot-token",
            Some("telegram-secret"),
            false,
        );

        let resolved = resolve_multi_channel_outbound_secret(&cli, None, "telegram-bot-token");
        assert_eq!(resolved.as_deref(), Some("telegram-secret"));
    }

    #[test]
    fn functional_build_multi_channel_outbound_config_resolves_direct_and_store_values() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.credential_store = temp.path().join("credentials.json");
        cli.multi_channel_telegram_bot_token = Some(" telegram-direct ".to_string());
        cli.multi_channel_telegram_api_base = " https://telegram.example ".to_string();
        cli.multi_channel_discord_api_base = " https://discord.example ".to_string();
        cli.multi_channel_whatsapp_api_base = " https://whatsapp.example ".to_string();
        cli.multi_channel_outbound_max_chars = 0;
        cli.multi_channel_outbound_http_timeout_ms = 0;
        write_integration_secret(
            &cli.credential_store,
            "discord-bot-token",
            Some("discord-store"),
            false,
        );
        write_integration_secret(
            &cli.credential_store,
            "whatsapp-access-token",
            Some("whatsapp-store"),
            false,
        );
        write_integration_secret(
            &cli.credential_store,
            "whatsapp-phone-number-id",
            Some("phone-store"),
            false,
        );

        let config = build_multi_channel_outbound_config(&cli);
        assert_eq!(
            config.telegram_bot_token.as_deref(),
            Some("telegram-direct")
        );
        assert_eq!(config.discord_bot_token.as_deref(), Some("discord-store"));
        assert_eq!(
            config.whatsapp_access_token.as_deref(),
            Some("whatsapp-store")
        );
        assert_eq!(
            config.whatsapp_phone_number_id.as_deref(),
            Some("phone-store")
        );
        assert_eq!(config.telegram_api_base, "https://telegram.example");
        assert_eq!(config.discord_api_base, "https://discord.example");
        assert_eq!(config.whatsapp_api_base, "https://whatsapp.example");
        assert_eq!(config.max_chars, 1);
        assert_eq!(config.http_timeout_ms, 1);
    }

    #[test]
    fn functional_build_multi_channel_live_connectors_config_resolves_store_fallbacks() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.credential_store = temp.path().join("credentials.json");
        cli.multi_channel_telegram_bot_token = None;
        cli.multi_channel_discord_bot_token = None;
        write_integration_secret(
            &cli.credential_store,
            "telegram-bot-token",
            Some("telegram-store"),
            false,
        );
        write_integration_secret(
            &cli.credential_store,
            "discord-bot-token",
            Some("discord-store"),
            false,
        );

        let config = build_multi_channel_live_connectors_config(&cli);
        assert_eq!(config.telegram_bot_token.as_deref(), Some("telegram-store"));
        assert_eq!(config.discord_bot_token.as_deref(), Some("discord-store"));
    }

    #[test]
    fn regression_resolve_multi_channel_outbound_secret_returns_none_for_revoked_entry() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.credential_store = temp.path().join("credentials.json");
        write_integration_secret(
            &cli.credential_store,
            "discord-bot-token",
            Some("discord-secret"),
            true,
        );

        let resolved = resolve_multi_channel_outbound_secret(&cli, None, "discord-bot-token");
        assert!(resolved.is_none());
    }

    #[test]
    fn regression_build_multi_channel_telemetry_and_media_config_enforce_minimums() {
        let mut cli = parse_cli_with_stack();
        cli.multi_channel_telemetry_min_response_chars = 0;
        cli.multi_channel_media_max_attachments = 0;
        cli.multi_channel_media_max_summary_chars = 0;

        let telemetry = build_multi_channel_telemetry_config(&cli);
        let media = build_multi_channel_media_config(&cli);

        assert_eq!(telemetry.typing_presence_min_response_chars, 1);
        assert_eq!(media.max_attachments_per_event, 1);
        assert_eq!(media.max_summary_chars, 16);
    }

    #[test]
    fn regression_build_multi_channel_live_connectors_config_ignores_revoked_store_secret() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.credential_store = temp.path().join("credentials.json");
        cli.multi_channel_discord_bot_token = None;
        write_integration_secret(
            &cli.credential_store,
            "discord-bot-token",
            Some("discord-secret"),
            true,
        );

        let config = build_multi_channel_live_connectors_config(&cli);
        assert!(config.discord_bot_token.is_none());
    }
}
