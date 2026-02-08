use super::*;

pub(crate) fn default_provider_auth_method() -> ProviderAuthMethod {
    ProviderAuthMethod::ApiKey
}

pub(crate) fn build_auth_command_config(cli: &Cli) -> AuthCommandConfig {
    AuthCommandConfig {
        credential_store: cli.credential_store.clone(),
        credential_store_key: cli.credential_store_key.clone(),
        credential_store_encryption: resolve_credential_store_encryption_mode(cli),
        api_key: cli.api_key.clone(),
        openai_api_key: cli.openai_api_key.clone(),
        anthropic_api_key: cli.anthropic_api_key.clone(),
        google_api_key: cli.google_api_key.clone(),
        openai_auth_mode: cli.openai_auth_mode.into(),
        anthropic_auth_mode: cli.anthropic_auth_mode.into(),
        google_auth_mode: cli.google_auth_mode.into(),
        openai_codex_backend: cli.openai_codex_backend,
        openai_codex_cli: cli.openai_codex_cli.clone(),
        anthropic_claude_backend: cli.anthropic_claude_backend,
        anthropic_claude_cli: cli.anthropic_claude_cli.clone(),
        google_gemini_backend: cli.google_gemini_backend,
        google_gemini_cli: cli.google_gemini_cli.clone(),
        google_gcloud_cli: cli.google_gcloud_cli.clone(),
    }
}

pub(crate) fn build_profile_defaults(cli: &Cli) -> ProfileDefaults {
    ProfileDefaults {
        model: cli.model.clone(),
        fallback_models: cli.fallback_model.clone(),
        session: ProfileSessionDefaults {
            enabled: !cli.no_session,
            path: if cli.no_session {
                None
            } else {
                Some(cli.session.display().to_string())
            },
            import_mode: format!("{:?}", cli.session_import_mode).to_lowercase(),
        },
        policy: ProfilePolicyDefaults {
            tool_policy_preset: format!("{:?}", cli.tool_policy_preset).to_lowercase(),
            bash_profile: format!("{:?}", cli.bash_profile).to_lowercase(),
            bash_dry_run: cli.bash_dry_run,
            os_sandbox_mode: format!("{:?}", cli.os_sandbox_mode).to_lowercase(),
            enforce_regular_files: cli.enforce_regular_files,
            bash_timeout_ms: cli.bash_timeout_ms,
            max_command_length: cli.max_command_length,
            max_tool_output_bytes: cli.max_tool_output_bytes,
            max_file_read_bytes: cli.max_file_read_bytes,
            max_file_write_bytes: cli.max_file_write_bytes,
            allow_command_newlines: cli.allow_command_newlines,
        },
        auth: ProfileAuthDefaults {
            openai: cli.openai_auth_mode.into(),
            anthropic: cli.anthropic_auth_mode.into(),
            google: cli.google_auth_mode.into(),
        },
    }
}
