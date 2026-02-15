use std::path::{Path, PathBuf};

use anyhow::Result;
use tau_access::approvals::{
    evaluate_approval_gate, execute_approvals_command, ApprovalAction, ApprovalGateResult,
};
use tau_access::pairing::{execute_pair_command, execute_unpair_command};
use tau_access::rbac::{
    authorize_command_for_principal, execute_rbac_command, resolve_local_principal, RbacDecision,
};
use tau_agent_core::Agent;
#[cfg(test)]
use tau_ai::Provider;
use tau_cli::{
    canonical_command_name, normalize_help_topic, parse_command, CliCommandFileErrorMode,
    CommandFileReport,
};
use tau_diagnostics::{
    execute_audit_summary_command, execute_doctor_cli_command, execute_policy_command,
};
#[cfg(test)]
use tau_diagnostics::{
    DoctorCommandConfig, DoctorMultiChannelReadinessConfig, DoctorProviderKeyStatus,
};
#[cfg(test)]
use tau_onboarding::startup_config::ProfileDefaults;
use tau_provider::{
    execute_integration_auth_command, parse_models_list_args, render_model_show,
    render_models_list, MODELS_LIST_USAGE, MODEL_SHOW_USAGE,
};
#[cfg(test)]
use tau_provider::{
    AuthCommandConfig, CredentialStoreEncryptionMode, ModelCatalog, ProviderAuthMethod,
};
#[cfg(test)]
use tau_session::SessionImportMode;
use tau_session::{
    execute_branch_alias_command, execute_branch_switch_command, execute_branches_command,
    execute_redo_command, execute_resume_command, execute_session_bookmark_command,
    execute_session_compact_command, execute_session_diff_runtime_command,
    execute_session_export_command, execute_session_graph_export_runtime_command,
    execute_session_import_command, execute_session_merge_command, execute_session_repair_command,
    execute_session_search_runtime_command, execute_session_stats_runtime_command,
    execute_session_status_command, execute_undo_command, session_lineage_messages, SessionRuntime,
};
#[cfg(test)]
use tau_skills::default_skills_lock_path;
use tau_skills::{
    execute_skills_list_command, execute_skills_lock_diff_command,
    execute_skills_lock_write_command, execute_skills_prune_command, execute_skills_search_command,
    execute_skills_show_command, execute_skills_sync_command, execute_skills_trust_add_command,
    execute_skills_trust_list_command, execute_skills_trust_revoke_command,
    execute_skills_trust_rotate_command, execute_skills_verify_command,
};
#[cfg(test)]
use tau_startup::SkillsSyncCommandConfig;
use tau_startup::{execute_command_file_with_handler, CommandFileAction};

use crate::auth_commands::execute_auth_command;
use crate::canvas::{
    execute_canvas_command, CanvasCommandConfig, CanvasEventOrigin, CanvasSessionLinkContext,
};
use crate::extension_manifest::{
    dispatch_extension_registered_command, ExtensionRegisteredCommandAction,
};
use crate::macro_profile_commands::{
    default_macro_config_path, default_profile_store_path, execute_macro_command,
    execute_profile_command,
};
use crate::qa_loop_commands::execute_qa_loop_cli_command;
use crate::release_channel_commands::{
    default_release_channel_path, execute_release_channel_command,
};
use crate::runtime_types::CommandExecutionContext;
#[cfg(test)]
use crate::runtime_types::{
    ProfileAuthDefaults, ProfileMcpDefaults, ProfilePolicyDefaults, ProfileSessionDefaults,
};
pub(crate) use tau_ops::COMMAND_NAMES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandAction {
    Continue,
    Exit,
}

pub(crate) fn execute_command_file(
    path: &Path,
    mode: CliCommandFileErrorMode,
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    command_context: CommandExecutionContext<'_>,
) -> Result<CommandFileReport> {
    execute_command_file_with_handler(path, mode, |command| {
        match handle_command_with_session_import_mode(
            command,
            agent,
            session_runtime,
            command_context,
        )? {
            CommandAction::Continue => Ok(CommandFileAction::Continue),
            CommandAction::Exit => Ok(CommandFileAction::Exit),
        }
    })
}

#[cfg(test)]
pub(crate) fn handle_command(
    command: &str,
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    tool_policy_json: &serde_json::Value,
) -> Result<CommandAction> {
    let skills_dir = PathBuf::from(".tau/skills");
    let skills_lock_path = default_skills_lock_path(&skills_dir);
    let skills_command_config = SkillsSyncCommandConfig {
        skills_dir,
        default_lock_path: skills_lock_path,
        default_trust_root_path: None,
        doctor_config: DoctorCommandConfig {
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
            release_channel_path: PathBuf::from(".tau/release-channel.json"),
            release_lookup_cache_path: PathBuf::from(".tau/release-lookup-cache.json"),
            release_lookup_cache_ttl_ms: 900_000,
            browser_automation_playwright_cli: "playwright-cli".to_string(),
            session_enabled: true,
            session_path: PathBuf::from(".tau/sessions/default.sqlite"),
            skills_dir: PathBuf::from(".tau/skills"),
            skills_lock_path: PathBuf::from(".tau/skills/skills.lock.json"),
            trust_root_path: None,
            multi_channel_live_readiness: DoctorMultiChannelReadinessConfig::default(),
        },
    };
    let profile_defaults = ProfileDefaults {
        model: "openai/gpt-4o-mini".to_string(),
        fallback_models: Vec::new(),
        session: ProfileSessionDefaults {
            enabled: true,
            path: Some(".tau/sessions/default.sqlite".to_string()),
            import_mode: "merge".to_string(),
        },
        policy: ProfilePolicyDefaults {
            tool_policy_preset: "balanced".to_string(),
            bash_profile: "balanced".to_string(),
            bash_dry_run: false,
            os_sandbox_mode: "off".to_string(),
            enforce_regular_files: true,
            bash_timeout_ms: 500,
            max_command_length: 4096,
            max_tool_output_bytes: 1024,
            max_file_read_bytes: 2048,
            max_file_write_bytes: 2048,
            allow_command_newlines: true,
        },
        mcp: ProfileMcpDefaults::default(),
        auth: ProfileAuthDefaults::default(),
    };
    let auth_command_config = AuthCommandConfig {
        credential_store: PathBuf::from(".tau/credentials.json"),
        credential_store_key: None,
        credential_store_encryption: CredentialStoreEncryptionMode::None,
        api_key: None,
        openai_api_key: None,
        anthropic_api_key: None,
        google_api_key: None,
        openai_auth_mode: ProviderAuthMethod::ApiKey,
        anthropic_auth_mode: ProviderAuthMethod::ApiKey,
        google_auth_mode: ProviderAuthMethod::ApiKey,
        provider_subscription_strict: false,
        openai_codex_backend: true,
        openai_codex_cli: "codex".to_string(),
        anthropic_claude_backend: true,
        anthropic_claude_cli: "claude".to_string(),
        google_gemini_backend: true,
        google_gemini_cli: "gemini".to_string(),
        google_gcloud_cli: "gcloud".to_string(),
    };
    let model_catalog = ModelCatalog::built_in();
    handle_command_with_session_import_mode(
        command,
        agent,
        session_runtime,
        CommandExecutionContext {
            tool_policy_json,
            session_import_mode: SessionImportMode::Merge,
            profile_defaults: &profile_defaults,
            skills_command_config: &skills_command_config,
            auth_command_config: &auth_command_config,
            model_catalog: &model_catalog,
            extension_commands: &[],
        },
    )
}

pub(crate) fn handle_command_with_session_import_mode(
    command: &str,
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    command_context: CommandExecutionContext<'_>,
) -> Result<CommandAction> {
    let CommandExecutionContext {
        tool_policy_json,
        session_import_mode,
        profile_defaults,
        skills_command_config,
        auth_command_config,
        model_catalog,
        extension_commands,
    } = command_context;

    let skills_dir = skills_command_config.skills_dir.as_path();
    let default_skills_lock_path = skills_command_config.default_lock_path.as_path();
    let default_trust_root_path = skills_command_config.default_trust_root_path.as_deref();

    let Some(parsed) = parse_command(command) else {
        println!("invalid command input: {command}");
        return Ok(CommandAction::Continue);
    };
    let command_name = canonical_command_name(parsed.name);
    let command_args = parsed.args;

    if command_name == "/quit" {
        return Ok(CommandAction::Exit);
    }

    if command_name == "/help" {
        if command_args.is_empty() {
            println!("{}", render_help_overview());
        } else {
            let topic = normalize_help_topic(command_args);
            match render_command_help(&topic) {
                Some(help) => println!("{help}"),
                None => println!("{}", unknown_help_topic_message(&topic)),
            }
        }
        return Ok(CommandAction::Continue);
    }

    if command_name == "/canvas" {
        let session_link = session_runtime
            .as_ref()
            .map(|runtime| CanvasSessionLinkContext {
                session_path: runtime.store.path().to_path_buf(),
                session_head_id: runtime.active_head,
            });
        println!(
            "{}",
            execute_canvas_command(
                command_args,
                &CanvasCommandConfig {
                    canvas_root: PathBuf::from(".tau/canvas"),
                    channel_store_root: PathBuf::from(".tau/channel-store"),
                    principal: resolve_local_principal(),
                    origin: CanvasEventOrigin::default(),
                    session_link,
                }
            )
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/rbac" {
        println!("{}", execute_rbac_command(command_args));
        return Ok(CommandAction::Continue);
    }

    let rbac_principal = resolve_local_principal();
    match authorize_command_for_principal(&rbac_principal, command_name) {
        Ok(RbacDecision::Allow { .. }) => {}
        Ok(RbacDecision::Deny {
            reason_code,
            matched_role,
            matched_pattern,
        }) => {
            println!(
                "rbac gate: status=denied principal={} action=command:{} reason_code={} matched_role={} matched_pattern={}",
                rbac_principal,
                command_name,
                reason_code,
                matched_role.as_deref().unwrap_or("none"),
                matched_pattern.as_deref().unwrap_or("none")
            );
            println!(
                "rbac gate hint: run '/rbac check command:{} --principal {}' for diagnostics",
                command_name, rbac_principal
            );
            return Ok(CommandAction::Continue);
        }
        Err(error) => {
            println!(
                "rbac gate error: principal={} action=command:{} error={error}",
                rbac_principal, command_name
            );
            return Ok(CommandAction::Continue);
        }
    }

    if command_name == "/approvals" {
        println!("{}", execute_approvals_command(command_args));
        return Ok(CommandAction::Continue);
    }

    match evaluate_approval_gate(&ApprovalAction::Command {
        name: command_name.to_string(),
        args: command_args.to_string(),
    }) {
        Ok(ApprovalGateResult::Allowed) => {}
        Ok(ApprovalGateResult::Denied {
            request_id,
            rule_id,
            reason_code,
            message,
        }) => {
            println!(
                "approval gate: status=denied command={} request_id={} rule_id={} reason_code={} message={}",
                command_name, request_id, rule_id, reason_code, message
            );
            println!(
                "approval gate hint: run '/approvals list' then '/approvals approve {}' to continue",
                request_id
            );
            return Ok(CommandAction::Continue);
        }
        Err(error) => {
            println!(
                "approval gate error: command={} error={error}",
                command_name
            );
            return Ok(CommandAction::Continue);
        }
    }

    if command_name == "/session" {
        let Some(runtime) = session_runtime.as_ref() else {
            if command_args.is_empty() {
                println!("session: disabled");
            } else {
                println!("usage: /session");
            }
            return Ok(CommandAction::Continue);
        };

        let outcome = execute_session_status_command(command_args, runtime);
        println!("{}", outcome.message);
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-export" {
        let Some(runtime) = session_runtime.as_ref() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        let outcome = execute_session_export_command(command_args, runtime)?;
        println!("{}", outcome.message);
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-import" {
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        let outcome = execute_session_import_command(command_args, runtime, session_import_mode)?;
        if outcome.reload_active_head {
            agent.replace_messages(session_lineage_messages(runtime)?);
        }
        println!("{}", outcome.message);
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-merge" {
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        let outcome = execute_session_merge_command(command_args, runtime)?;
        if outcome.reload_active_head {
            agent.replace_messages(session_lineage_messages(runtime)?);
        }
        println!("{}", outcome.message);
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-search" {
        let Some(runtime) = session_runtime.as_ref() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };
        let outcome = execute_session_search_runtime_command(command_args, runtime);
        println!("{}", outcome.message);
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-stats" {
        let Some(runtime) = session_runtime.as_ref() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };
        let outcome = execute_session_stats_runtime_command(command_args, runtime);
        println!("{}", outcome.message);
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-diff" {
        let Some(runtime) = session_runtime.as_ref() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };
        let outcome = execute_session_diff_runtime_command(command_args, runtime);
        println!("{}", outcome.message);
        return Ok(CommandAction::Continue);
    }

    if command_name == "/qa-loop" {
        println!("{}", execute_qa_loop_cli_command(command_args));
        return Ok(CommandAction::Continue);
    }

    if command_name == "/doctor" {
        println!(
            "{}",
            execute_doctor_cli_command(&skills_command_config.doctor_config, command_args)
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-graph-export" {
        let Some(runtime) = session_runtime.as_ref() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };
        let outcome = execute_session_graph_export_runtime_command(command_args, runtime);
        println!("{}", outcome.message);
        return Ok(CommandAction::Continue);
    }

    if command_name == "/policy" {
        match execute_policy_command(command_args, tool_policy_json) {
            Ok(output) => println!("{output}"),
            Err(_) => println!("usage: /policy"),
        }
        return Ok(CommandAction::Continue);
    }

    if command_name == "/audit-summary" {
        println!("{}", execute_audit_summary_command(command_args));
        return Ok(CommandAction::Continue);
    }

    if command_name == "/models-list" {
        match parse_models_list_args(command_args) {
            Ok(args) => println!("{}", render_models_list(model_catalog, &args)),
            Err(error) => {
                println!("models list error: {error}");
                println!("usage: {MODELS_LIST_USAGE}");
            }
        }
        return Ok(CommandAction::Continue);
    }

    if command_name == "/model-show" {
        if command_args.is_empty() {
            println!("usage: {MODEL_SHOW_USAGE}");
            return Ok(CommandAction::Continue);
        }
        match render_model_show(model_catalog, command_args) {
            Ok(output) => println!("{output}"),
            Err(error) => {
                println!("model show error: {error}");
                println!("usage: {MODEL_SHOW_USAGE}");
            }
        }
        return Ok(CommandAction::Continue);
    }

    if command_name == "/skills-search" {
        if command_args.is_empty() {
            println!("usage: /skills-search <query> [max_results]");
            return Ok(CommandAction::Continue);
        }
        println!(
            "{}",
            execute_skills_search_command(skills_dir, command_args)
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/skills-show" {
        if command_args.is_empty() {
            println!("usage: /skills-show <name>");
            return Ok(CommandAction::Continue);
        }
        println!("{}", execute_skills_show_command(skills_dir, command_args));
        return Ok(CommandAction::Continue);
    }

    if command_name == "/skills-list" {
        if !command_args.is_empty() {
            println!("usage: /skills-list");
            return Ok(CommandAction::Continue);
        }
        println!("{}", execute_skills_list_command(skills_dir));
        return Ok(CommandAction::Continue);
    }

    if command_name == "/skills-lock-diff" {
        println!(
            "{}",
            execute_skills_lock_diff_command(skills_dir, default_skills_lock_path, command_args)
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/skills-prune" {
        println!(
            "{}",
            execute_skills_prune_command(skills_dir, default_skills_lock_path, command_args)
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/skills-trust-list" {
        println!(
            "{}",
            execute_skills_trust_list_command(default_trust_root_path, command_args)
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/skills-trust-add" {
        println!(
            "{}",
            execute_skills_trust_add_command(default_trust_root_path, command_args)
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/skills-trust-revoke" {
        println!(
            "{}",
            execute_skills_trust_revoke_command(default_trust_root_path, command_args)
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/skills-trust-rotate" {
        println!(
            "{}",
            execute_skills_trust_rotate_command(default_trust_root_path, command_args)
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/skills-lock-write" {
        println!(
            "{}",
            execute_skills_lock_write_command(skills_dir, default_skills_lock_path, command_args)
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/skills-sync" {
        println!(
            "{}",
            execute_skills_sync_command(skills_dir, default_skills_lock_path, command_args)
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/skills-verify" {
        println!(
            "{}",
            execute_skills_verify_command(
                skills_dir,
                default_skills_lock_path,
                default_trust_root_path,
                command_args
            )
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/resume" {
        if !command_args.is_empty() {
            println!("usage: /resume");
            return Ok(CommandAction::Continue);
        }
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        let outcome = execute_resume_command(command_args, runtime);
        if outcome.reload_active_head {
            agent.replace_messages(session_lineage_messages(runtime)?);
        }
        println!("{}", outcome.message);
        return Ok(CommandAction::Continue);
    }

    if command_name == "/undo" {
        if !command_args.is_empty() {
            println!("usage: /undo");
            return Ok(CommandAction::Continue);
        }
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        let outcome = execute_undo_command(command_args, runtime)?;
        if outcome.reload_active_head {
            agent.replace_messages(session_lineage_messages(runtime)?);
        }
        println!("{}", outcome.message);
        return Ok(CommandAction::Continue);
    }

    if command_name == "/redo" {
        if !command_args.is_empty() {
            println!("usage: /redo");
            return Ok(CommandAction::Continue);
        }
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        let outcome = execute_redo_command(command_args, runtime)?;
        if outcome.reload_active_head {
            agent.replace_messages(session_lineage_messages(runtime)?);
        }
        println!("{}", outcome.message);
        return Ok(CommandAction::Continue);
    }

    if command_name == "/branches" {
        if !command_args.is_empty() {
            println!("usage: /branches");
            return Ok(CommandAction::Continue);
        }
        let Some(runtime) = session_runtime.as_ref() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        let outcome = execute_branches_command(command_args, runtime);
        println!("{}", outcome.message);
        return Ok(CommandAction::Continue);
    }

    if command_name == "/macro" {
        let macro_path = match default_macro_config_path() {
            Ok(path) => path,
            Err(error) => {
                println!("macro error: path=unknown error={error}");
                return Ok(CommandAction::Continue);
            }
        };
        println!(
            "{}",
            execute_macro_command(
                command_args,
                &macro_path,
                agent,
                session_runtime,
                CommandExecutionContext {
                    tool_policy_json,
                    session_import_mode,
                    profile_defaults,
                    skills_command_config,
                    auth_command_config,
                    model_catalog,
                    extension_commands,
                }
            )
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/profile" {
        let profile_path = match default_profile_store_path() {
            Ok(path) => path,
            Err(error) => {
                println!("profile error: path=unknown error={error}");
                return Ok(CommandAction::Continue);
            }
        };
        println!(
            "{}",
            execute_profile_command(command_args, &profile_path, profile_defaults)
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/release-channel" {
        let release_channel_path = match default_release_channel_path() {
            Ok(path) => path,
            Err(error) => {
                println!("release channel error: path=unknown error={error}");
                return Ok(CommandAction::Continue);
            }
        };
        println!(
            "{}",
            execute_release_channel_command(command_args, &release_channel_path)
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/auth" {
        println!(
            "{}",
            execute_auth_command(auth_command_config, command_args)
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/integration-auth" {
        println!(
            "{}",
            execute_integration_auth_command(auth_command_config, command_args)
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/pair" {
        println!("{}", execute_pair_command(command_args, "local"));
        return Ok(CommandAction::Continue);
    }

    if command_name == "/unpair" {
        println!("{}", execute_unpair_command(command_args));
        return Ok(CommandAction::Continue);
    }

    if command_name == "/branch-alias" {
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        let outcome = execute_branch_alias_command(command_args, runtime);
        if outcome.reload_active_head {
            let lineage = session_lineage_messages(runtime)?;
            agent.replace_messages(lineage);
        }
        println!("{}", outcome.message);
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-bookmark" {
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        let outcome = execute_session_bookmark_command(command_args, runtime);
        if outcome.reload_active_head {
            let lineage = session_lineage_messages(runtime)?;
            agent.replace_messages(lineage);
        }
        println!("{}", outcome.message);
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-repair" {
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        let outcome = execute_session_repair_command(command_args, runtime)?;
        if outcome.reload_active_head {
            agent.replace_messages(session_lineage_messages(runtime)?);
        }
        println!("{}", outcome.message);
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-compact" {
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        let outcome = execute_session_compact_command(command_args, runtime)?;
        if outcome.reload_active_head {
            agent.replace_messages(session_lineage_messages(runtime)?);
        }
        println!("{}", outcome.message);
        return Ok(CommandAction::Continue);
    }

    if command_name == "/branch" {
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        let outcome = execute_branch_switch_command(command_args, runtime)?;
        if outcome.reload_active_head {
            agent.replace_messages(session_lineage_messages(runtime)?);
        }
        println!("{}", outcome.message);
        return Ok(CommandAction::Continue);
    }

    match dispatch_extension_registered_command(extension_commands, command_name, command_args) {
        Ok(Some(dispatch_result)) => {
            if let Some(output) = dispatch_result.output {
                println!("{output}");
            }
            return Ok(match dispatch_result.action {
                ExtensionRegisteredCommandAction::Continue => CommandAction::Continue,
                ExtensionRegisteredCommandAction::Exit => CommandAction::Exit,
            });
        }
        Ok(None) => {}
        Err(error) => {
            println!("extension command error: command={command_name} error={error}");
            return Ok(CommandAction::Continue);
        }
    }

    println!("{}", unknown_command_message(parsed.name));
    Ok(CommandAction::Continue)
}

pub(crate) fn render_help_overview() -> String {
    tau_ops::render_help_overview()
}

pub(crate) fn render_command_help(topic: &str) -> Option<String> {
    tau_ops::render_command_help(topic)
}

pub(crate) fn unknown_help_topic_message(topic: &str) -> String {
    tau_ops::unknown_help_topic_message(topic)
}

pub(crate) fn unknown_command_message(command: &str) -> String {
    tau_ops::unknown_command_message(command)
}
