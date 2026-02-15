use anyhow::{anyhow, bail, Context, Result};
use tau_cli::Cli;
use tau_provider::{
    load_credential_store, resolve_credential_store_encryption_mode, resolve_non_empty_cli_value,
};

pub fn execute_multi_channel_send_command(cli: &Cli) -> Result<()> {
    let transport: tau_multi_channel::MultiChannelTransport = cli
        .multi_channel_send
        .ok_or_else(|| anyhow!("--multi-channel-send is required"))?
        .into();
    let target = cli
        .multi_channel_send_target
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("--multi-channel-send-target is required"))?
        .to_string();

    let text = tau_multi_channel::resolve_multi_channel_send_text(
        cli.multi_channel_send_text.as_deref(),
        cli.multi_channel_send_text_file.as_deref(),
    )?;

    let (credential_store, credential_store_unreadable) =
        load_multi_channel_credential_snapshot(cli);

    let config = tau_multi_channel::MultiChannelSendCommandConfig {
        transport,
        target,
        text,
        state_dir: cli.multi_channel_state_dir.clone(),
        outbound_mode: cli.multi_channel_outbound_mode.into(),
        outbound_max_chars: cli.multi_channel_outbound_max_chars.max(1),
        outbound_http_timeout_ms: cli.multi_channel_outbound_http_timeout_ms.max(1),
        outbound_ssrf_protection_enabled: cli.multi_channel_outbound_ssrf_protection,
        outbound_ssrf_allow_http: cli.multi_channel_outbound_ssrf_allow_http,
        outbound_ssrf_allow_private_network: cli.multi_channel_outbound_ssrf_allow_private_network,
        outbound_max_redirects: cli.multi_channel_outbound_max_redirects,
        telegram_api_base: cli.multi_channel_telegram_api_base.trim().to_string(),
        discord_api_base: cli.multi_channel_discord_api_base.trim().to_string(),
        whatsapp_api_base: cli.multi_channel_whatsapp_api_base.trim().to_string(),
        credential_store,
        credential_store_unreadable,
        telegram_bot_token: resolve_non_empty_cli_value(
            cli.multi_channel_telegram_bot_token.as_deref(),
        ),
        discord_bot_token: resolve_non_empty_cli_value(
            cli.multi_channel_discord_bot_token.as_deref(),
        ),
        whatsapp_access_token: resolve_non_empty_cli_value(
            cli.multi_channel_whatsapp_access_token.as_deref(),
        ),
        whatsapp_phone_number_id: resolve_non_empty_cli_value(
            cli.multi_channel_whatsapp_phone_number_id.as_deref(),
        ),
    };

    let report = tau_multi_channel::execute_multi_channel_send_action(&config)?;
    if cli.multi_channel_send_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render multi-channel send json")?
        );
    } else {
        println!(
            "{}",
            tau_multi_channel::render_multi_channel_send_report(&report)
        );
    }
    Ok(())
}

pub fn execute_multi_channel_channel_lifecycle_command(cli: &Cli) -> Result<()> {
    let (action, transport, json_output) = if let Some(transport) = cli.multi_channel_channel_status
    {
        (
            tau_multi_channel::MultiChannelLifecycleAction::Status,
            transport,
            cli.multi_channel_channel_status_json,
        )
    } else if let Some(transport) = cli.multi_channel_channel_login {
        (
            tau_multi_channel::MultiChannelLifecycleAction::Login,
            transport,
            cli.multi_channel_channel_login_json,
        )
    } else if let Some(transport) = cli.multi_channel_channel_logout {
        (
            tau_multi_channel::MultiChannelLifecycleAction::Logout,
            transport,
            cli.multi_channel_channel_logout_json,
        )
    } else if let Some(transport) = cli.multi_channel_channel_probe {
        (
            tau_multi_channel::MultiChannelLifecycleAction::Probe,
            transport,
            cli.multi_channel_channel_probe_json,
        )
    } else {
        bail!("no multi-channel lifecycle action requested");
    };

    let (credential_store, credential_store_unreadable) =
        load_multi_channel_credential_snapshot(cli);
    let config = tau_multi_channel::MultiChannelLifecycleCommandConfig {
        state_dir: cli.multi_channel_state_dir.clone(),
        ingress_dir: cli.multi_channel_live_ingress_dir.clone(),
        telegram_api_base: cli.multi_channel_telegram_api_base.trim().to_string(),
        discord_api_base: cli.multi_channel_discord_api_base.trim().to_string(),
        whatsapp_api_base: cli.multi_channel_whatsapp_api_base.trim().to_string(),
        credential_store,
        credential_store_unreadable,
        telegram_bot_token: resolve_non_empty_cli_value(
            cli.multi_channel_telegram_bot_token.as_deref(),
        ),
        discord_bot_token: resolve_non_empty_cli_value(
            cli.multi_channel_discord_bot_token.as_deref(),
        ),
        whatsapp_access_token: resolve_non_empty_cli_value(
            cli.multi_channel_whatsapp_access_token.as_deref(),
        ),
        whatsapp_phone_number_id: resolve_non_empty_cli_value(
            cli.multi_channel_whatsapp_phone_number_id.as_deref(),
        ),
        probe_online: cli.multi_channel_channel_probe_online,
        probe_online_timeout_ms: tau_multi_channel::default_probe_timeout_ms(),
        probe_online_max_attempts: tau_multi_channel::default_probe_max_attempts(),
        probe_online_retry_delay_ms: tau_multi_channel::default_probe_retry_delay_ms(),
    };

    let report = tau_multi_channel::execute_multi_channel_lifecycle_action(
        &config,
        action,
        transport.into(),
    )?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render multi-channel lifecycle json")?
        );
    } else {
        println!(
            "{}",
            tau_multi_channel::render_multi_channel_lifecycle_report(&report)
        );
    }
    Ok(())
}

fn load_multi_channel_credential_snapshot(
    cli: &Cli,
) -> (
    Option<tau_multi_channel::MultiChannelCredentialStoreSnapshot>,
    bool,
) {
    let store = load_credential_store(
        &cli.credential_store,
        resolve_credential_store_encryption_mode(cli),
        cli.credential_store_key.as_deref(),
    );
    match store {
        Ok(store) => {
            let integrations = store
                .integrations
                .into_iter()
                .map(|(id, record)| {
                    (
                        id,
                        tau_multi_channel::MultiChannelCredentialRecord {
                            secret: record.secret,
                            revoked: record.revoked,
                        },
                    )
                })
                .collect();
            (
                Some(tau_multi_channel::MultiChannelCredentialStoreSnapshot { integrations }),
                false,
            )
        }
        Err(_) => (None, true),
    }
}
