use tau_cli::Cli;
use tau_multi_channel::{
    MultiChannelMediaUnderstandingConfig, MultiChannelOutboundConfig, MultiChannelTelemetryConfig,
};
use tau_provider::{load_credential_store, resolve_credential_store_encryption_mode};

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
        build_multi_channel_media_config, build_multi_channel_outbound_config,
        build_multi_channel_telemetry_config, resolve_multi_channel_outbound_secret,
    };
    use clap::Parser;
    use std::collections::BTreeMap;
    use std::path::Path;
    use tau_cli::Cli;
    use tau_provider::{
        load_credential_store, save_credential_store, CredentialStoreData,
        CredentialStoreEncryptionMode, IntegrationCredentialStoreRecord,
    };
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
}
