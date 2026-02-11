use super::*;
use crate::extension_manifest::{
    dispatch_extension_registered_command, ExtensionRegisteredCommandAction,
};
#[cfg(test)]
use crate::runtime_types::{
    ProfileAuthDefaults, ProfileMcpDefaults, ProfilePolicyDefaults, ProfileSessionDefaults,
};
use tau_session::{
    execute_session_diff_command, execute_session_search_command, execute_session_stats_command,
    parse_session_diff_args, parse_session_stats_args,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandAction {
    Continue,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParsedCommand<'a> {
    pub(crate) name: &'a str,
    pub(crate) args: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommandSpec {
    pub(crate) name: &'static str,
    pub(crate) usage: &'static str,
    pub(crate) description: &'static str,
    pub(crate) details: &'static str,
    pub(crate) example: &'static str,
}

pub(crate) const COMMAND_SPECS: &[CommandSpec] = &[
    CommandSpec {
        name: "/help",
        usage: "/help [command]",
        description: "Show command list or detailed command help",
        details: "Use '/help /command' (or '/help command') for command-specific guidance.",
        example: "/help /branch",
    },
    CommandSpec {
        name: "/session",
        usage: "/session",
        description: "Show session path, entry count, and active head id",
        details: "Read-only command; does not mutate session state.",
        example: "/session",
    },
    CommandSpec {
        name: "/session-search",
        usage: "/session-search <query> [--role <role>] [--limit <n>]",
        description: "Search session entries by role/text across all branches",
        details:
            "Case-insensitive search over role and message text. Optional --role scopes by message role and --limit controls displayed row count.",
        example: "/session-search retry budget --role user --limit 10",
    },
    CommandSpec {
        name: "/session-stats",
        usage: "/session-stats [--json]",
        description: "Summarize session size, branch tips, depth, and role counts",
        details:
            "Read-only session graph diagnostics including active/latest head indicators. Add --json for machine-readable output.",
        example: "/session-stats",
    },
    CommandSpec {
        name: "/session-diff",
        usage: "/session-diff [<left-id> <right-id>]",
        description: "Compare two session lineage heads",
        details:
            "Defaults to active vs latest head when ids are omitted. Includes shared and divergent lineage rows with deterministic previews.",
        example: "/session-diff 12 24",
    },
    CommandSpec {
        name: "/qa-loop",
        usage: QA_LOOP_USAGE,
        description: "Run staged quality checks with deterministic text/json reports",
        details:
            "Runs configured stages with timeout/retry controls, bounded stdout/stderr capture, and git changed-file summary.",
        example: "/qa-loop --json",
    },
    CommandSpec {
        name: "/doctor",
        usage: "/doctor [--json] [--online]",
        description: "Run deterministic runtime diagnostics",
        details:
            "Checks provider auth/session/skills/release posture. Add --json for machine-readable output and --online to include remote release checks.",
        example: "/doctor",
    },
    CommandSpec {
        name: "/release-channel",
        usage: RELEASE_CHANNEL_USAGE,
        description: "Show or persist release track selection",
        details:
            "Supports stable/beta/dev release tracks, update plan/apply with dry-run guardrails, and cache show/clear/refresh/prune operations in project-local .tau metadata.",
        example: "/release-channel set beta",
    },
    CommandSpec {
        name: "/session-graph-export",
        usage: "/session-graph-export <path>",
        description: "Export session graph as Mermaid or DOT file",
        details:
            "Uses .dot extension for Graphviz DOT; defaults to Mermaid for other destinations.",
        example: "/session-graph-export /tmp/session-graph.mmd",
    },
    CommandSpec {
        name: "/session-export",
        usage: "/session-export <path>",
        description: "Export active lineage snapshot to a JSONL file",
        details: "Writes only the active lineage entries, including schema metadata.",
        example: "/session-export /tmp/session-snapshot.jsonl",
    },
    CommandSpec {
        name: "/session-import",
        usage: "/session-import <path>",
        description: "Import a lineage snapshot into the current session",
        details:
            "Uses --session-import-mode (merge or replace). Merge remaps colliding ids; replace overwrites current entries.",
        example: "/session-import /tmp/session-snapshot.jsonl",
    },
    CommandSpec {
        name: "/policy",
        usage: "/policy",
        description: "Print the effective tool policy JSON",
        details: "Useful for debugging allowlists, limits, and sandbox settings.",
        example: "/policy",
    },
    CommandSpec {
        name: "/audit-summary",
        usage: "/audit-summary <path>",
        description: "Summarize tool and telemetry JSONL audit records",
        details:
            "Aggregates tool/provider counts, error rates, and p50/p95 durations from audit log files.",
        example: "/audit-summary .tau/audit/tool-events.jsonl",
    },
    CommandSpec {
        name: "/models-list",
        usage: MODELS_LIST_USAGE,
        description: "List/search/filter model catalog entries and capabilities",
        details:
            "Supports provider and capability filters (`--tools`, `--multimodal`, `--reasoning`) plus text query and result limits.",
        example: "/models-list gpt --provider openai --tools true --limit 20",
    },
    CommandSpec {
        name: "/model-show",
        usage: MODEL_SHOW_USAGE,
        description: "Show detailed capability metadata for one model catalog entry",
        details:
            "Accepts provider/model format and reports context window, capability flags, and configured cost metadata.",
        example: "/model-show openai/gpt-4o-mini",
    },
    CommandSpec {
        name: "/skills-search",
        usage: "/skills-search <query> [max_results]",
        description: "Search installed skills by name and content",
        details: "Ranks name matches first, then content-only matches.",
        example: "/skills-search checklist 10",
    },
    CommandSpec {
        name: "/skills-show",
        usage: "/skills-show <name>",
        description: "Display installed skill content and metadata",
        details: "Read-only command for inspecting a single skill by name.",
        example: "/skills-show checklist",
    },
    CommandSpec {
        name: "/skills-list",
        usage: "/skills-list",
        description: "List installed skills in the active skills directory",
        details: "Read-only command that prints deterministic skill inventory output.",
        example: "/skills-list",
    },
    CommandSpec {
        name: "/skills-lock-diff",
        usage: "/skills-lock-diff [lockfile_path] [--json]",
        description: "Inspect lockfile drift without enforcing sync",
        details: "Reports in-sync/drift/error status and supports structured JSON output.",
        example: "/skills-lock-diff .tau/skills/skills.lock.json --json",
    },
    CommandSpec {
        name: "/skills-prune",
        usage: "/skills-prune [lockfile_path] [--dry-run|--apply]",
        description: "Prune installed skills not tracked in lockfile",
        details:
            "Dry-run is default; use --apply to delete prune candidates after deterministic listing.",
        example: "/skills-prune .tau/skills/skills.lock.json --apply",
    },
    CommandSpec {
        name: "/skills-trust-list",
        usage: "/skills-trust-list [trust_root_file]",
        description: "List trust-root keys with revocation/expiry/rotation status",
        details:
            "Uses configured --skill-trust-root-file when no path argument is provided.",
        example: "/skills-trust-list .tau/skills/trust-roots.json",
    },
    CommandSpec {
        name: "/skills-trust-add",
        usage: "/skills-trust-add <id=base64_key> [trust_root_file]",
        description: "Add or update a trust-root key",
        details:
            "Mutates trust-root file atomically. Uses configured --skill-trust-root-file when path is omitted.",
        example: "/skills-trust-add root-v2=AbC... .tau/skills/trust-roots.json",
    },
    CommandSpec {
        name: "/skills-trust-revoke",
        usage: "/skills-trust-revoke <id> [trust_root_file]",
        description: "Revoke a trust-root key id",
        details:
            "Marks key as revoked in trust-root file. Uses configured --skill-trust-root-file when path is omitted.",
        example: "/skills-trust-revoke root-v1 .tau/skills/trust-roots.json",
    },
    CommandSpec {
        name: "/skills-trust-rotate",
        usage: "/skills-trust-rotate <old_id:new_id=base64_key> [trust_root_file]",
        description: "Rotate trust-root key id to a new key",
        details:
            "Revokes old id and adds/updates new id atomically. Uses configured --skill-trust-root-file when path is omitted.",
        example: "/skills-trust-rotate root-v1:root-v2=AbC... .tau/skills/trust-roots.json",
    },
    CommandSpec {
        name: "/skills-lock-write",
        usage: "/skills-lock-write [lockfile_path]",
        description: "Write/update skills lockfile from installed skills",
        details:
            "Uses <skills-dir>/skills.lock.json when path is omitted. Preserves existing source metadata when possible.",
        example: "/skills-lock-write .tau/skills/skills.lock.json",
    },
    CommandSpec {
        name: "/skills-sync",
        usage: "/skills-sync [lockfile_path]",
        description: "Validate installed skills against the lockfile",
        details:
            "Uses <skills-dir>/skills.lock.json when path is omitted. Prints drift diagnostics without exiting interactive mode.",
        example: "/skills-sync .tau/skills/skills.lock.json",
    },
    CommandSpec {
        name: "/skills-verify",
        usage: "/skills-verify [lockfile_path] [trust_root_file] [--json]",
        description: "Audit lockfile drift and trust/signature policy in one report",
        details:
            "Read-only compliance diagnostics across sync drift, signature metadata, and trust-root key status.",
        example: "/skills-verify .tau/skills/skills.lock.json .tau/skills/trust-roots.json --json",
    },
    CommandSpec {
        name: "/branches",
        usage: "/branches",
        description: "List branch tips in the current session graph",
        details: "Each row includes entry id, parent id, and a short message summary.",
        example: "/branches",
    },
    CommandSpec {
        name: "/macro",
        usage: "/macro <save|run|list|show|delete> ...",
        description: "Manage reusable command macros",
        details:
            "Persists macros in project-local config and supports dry-run validation, inspection, and deletion.",
        example: "/macro save quick-check /tmp/quick-check.commands",
    },
    CommandSpec {
        name: "/auth",
        usage: "/auth <login|reauth|status|logout|matrix> ...",
        description: "Manage provider authentication state and credential-store sessions",
        details:
            "Supports login/reauth/status/logout flows plus provider-mode matrix diagnostics with optional --json output.",
        example: "/auth reauth openai --mode oauth-token --launch --json",
    },
    CommandSpec {
        name: "/canvas",
        usage: CANVAS_USAGE,
        description: "Manage collaborative live-canvas state backed by Yrs CRDT persistence",
        details:
            "Supports create/update/show/export/import flows for node and edge state with deterministic markdown/json renderers and replay-safe event envelopes.",
        example: "/canvas update architecture node-upsert api \"API Service\" 120 64",
    },
    CommandSpec {
        name: "/rbac",
        usage: RBAC_USAGE,
        description: "Inspect RBAC principal resolution and authorization decisions",
        details:
            "Use whoami to resolve principal bindings and check to evaluate one action against active role policy.",
        example: "/rbac check command:/policy --json",
    },
    CommandSpec {
        name: "/approvals",
        usage: APPROVALS_USAGE,
        description: "Review and decide queued HITL approval requests",
        details:
            "Use list for queue visibility and approve/reject to unblock or deny pending requests.",
        example: "/approvals list --status pending",
    },
    CommandSpec {
        name: "/integration-auth",
        usage: "/integration-auth <set|status|rotate|revoke> ...",
        description: "Manage credential-store secrets for integrations (GitHub, Slack, webhooks)",
        details:
            "Supports set/status/rotate/revoke flows for integration secret ids with optional --json output.",
        example: "/integration-auth status github-token --json",
    },
    CommandSpec {
        name: "/pair",
        usage: "/pair <add|remove|status> ...",
        description: "Manage remote channel actor pairings and allowlist visibility",
        details:
            "Writes `.tau/security/pairings.json` atomically. Use `/pair status` to inspect effective pairings and allowlist rows.",
        example: "/pair add github:owner/repo alice --ttl-seconds 3600",
    },
    CommandSpec {
        name: "/unpair",
        usage: "/unpair <channel> <actor_id>",
        description: "Remove one actor pairing from a channel",
        details:
            "Alias for `/pair remove` with deterministic removal count and atomic persistence.",
        example: "/unpair github:owner/repo alice",
    },
    CommandSpec {
        name: "/profile",
        usage: "/profile <save|load|list|show|delete> ...",
        description: "Manage model, policy, and session default profiles",
        details:
            "Profiles are persisted in project-local config. Load reports diffs from current defaults; list/show/delete support lifecycle management.",
        example: "/profile save baseline",
    },
    CommandSpec {
        name: "/branch-alias",
        usage: "/branch-alias <set|list|use> ...",
        description: "Manage persistent branch aliases for quick navigation",
        details:
            "Aliases are stored in a sidecar JSON file next to the active session file.",
        example: "/branch-alias set hotfix 42",
    },
    CommandSpec {
        name: "/session-bookmark",
        usage: "/session-bookmark <set|list|use|delete> ...",
        description: "Manage persistent session bookmarks",
        details:
            "Bookmarks are stored in project-local metadata and can switch active head by name.",
        example: "/session-bookmark set investigation 42",
    },
    CommandSpec {
        name: "/branch",
        usage: "/branch <id>",
        description: "Switch active branch head to a specific entry id",
        details: "Reloads the agent message context to the selected lineage.",
        example: "/branch 12",
    },
    CommandSpec {
        name: "/resume",
        usage: "/resume",
        description: "Jump back to the latest session head",
        details: "Resets active branch to current head and reloads lineage messages.",
        example: "/resume",
    },
    CommandSpec {
        name: "/session-repair",
        usage: "/session-repair",
        description: "Repair malformed session graphs",
        details: "Removes duplicate ids, invalid parent references, and cyclic lineage entries.",
        example: "/session-repair",
    },
    CommandSpec {
        name: "/session-compact",
        usage: "/session-compact",
        description: "Compact session to active lineage",
        details: "Prunes inactive branches and retains only entries reachable from active head.",
        example: "/session-compact",
    },
    CommandSpec {
        name: "/quit",
        usage: "/quit",
        description: "Exit interactive mode",
        details: "Alias: /exit",
        example: "/quit",
    },
];

pub(crate) const COMMAND_NAMES: &[&str] = &[
    "/help",
    "/session",
    "/session-search",
    "/session-stats",
    "/session-diff",
    "/qa-loop",
    "/doctor",
    "/release-channel",
    "/session-graph-export",
    "/session-export",
    "/session-import",
    "/policy",
    "/audit-summary",
    "/models-list",
    "/model-show",
    "/skills-search",
    "/skills-show",
    "/skills-list",
    "/skills-lock-diff",
    "/skills-prune",
    "/skills-trust-list",
    "/skills-trust-add",
    "/skills-trust-revoke",
    "/skills-trust-rotate",
    "/skills-lock-write",
    "/skills-sync",
    "/skills-verify",
    "/branches",
    "/macro",
    "/auth",
    "/canvas",
    "/rbac",
    "/approvals",
    "/integration-auth",
    "/pair",
    "/unpair",
    "/profile",
    "/branch-alias",
    "/session-bookmark",
    "/branch",
    "/resume",
    "/session-repair",
    "/session-compact",
    "/quit",
    "/exit",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandFileEntry {
    pub(crate) line_number: usize,
    pub(crate) command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandFileReport {
    pub(crate) total: usize,
    pub(crate) executed: usize,
    pub(crate) succeeded: usize,
    pub(crate) failed: usize,
    pub(crate) halted_early: bool,
}

pub(crate) fn parse_command_file(path: &Path) -> Result<Vec<CommandFileEntry>> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read command file {}", path.display()))?;
    let mut entries = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        entries.push(CommandFileEntry {
            line_number: index + 1,
            command: trimmed.to_string(),
        });
    }
    Ok(entries)
}

pub(crate) fn execute_command_file(
    path: &Path,
    mode: CliCommandFileErrorMode,
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    command_context: CommandExecutionContext<'_>,
) -> Result<CommandFileReport> {
    let entries = parse_command_file(path)?;
    let mut report = CommandFileReport {
        total: entries.len(),
        executed: 0,
        succeeded: 0,
        failed: 0,
        halted_early: false,
    };

    for entry in entries {
        report.executed += 1;

        if !entry.command.starts_with('/') {
            report.failed += 1;
            println!(
                "command file error: path={} line={} command={} error=command must start with '/'",
                path.display(),
                entry.line_number,
                entry.command
            );
            if mode == CliCommandFileErrorMode::FailFast {
                report.halted_early = true;
                break;
            }
            continue;
        }

        match handle_command_with_session_import_mode(
            &entry.command,
            agent,
            session_runtime,
            command_context.tool_policy_json,
            command_context.session_import_mode,
            command_context.profile_defaults,
            command_context.skills_command_config,
            command_context.auth_command_config,
            command_context.model_catalog,
            command_context.extension_commands,
        ) {
            Ok(CommandAction::Continue) => {
                report.succeeded += 1;
            }
            Ok(CommandAction::Exit) => {
                report.succeeded += 1;
                report.halted_early = true;
                println!(
                    "command file notice: path={} line={} command={} action=exit",
                    path.display(),
                    entry.line_number,
                    entry.command
                );
                break;
            }
            Err(error) => {
                report.failed += 1;
                println!(
                    "command file error: path={} line={} command={} error={error}",
                    path.display(),
                    entry.line_number,
                    entry.command
                );
                if mode == CliCommandFileErrorMode::FailFast {
                    report.halted_early = true;
                    break;
                }
            }
        }
    }

    println!(
        "command file summary: path={} mode={} total={} executed={} succeeded={} failed={} halted_early={}",
        path.display(),
        command_file_error_mode_label(mode),
        report.total,
        report.executed,
        report.succeeded,
        report.failed,
        report.halted_early
    );

    if mode == CliCommandFileErrorMode::FailFast && report.failed > 0 {
        bail!(
            "command file execution failed: path={} failed={} mode={}",
            path.display(),
            report.failed,
            command_file_error_mode_label(mode)
        );
    }

    Ok(report)
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
            session_path: PathBuf::from(".tau/sessions/default.jsonl"),
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
            path: Some(".tau/sessions/default.jsonl".to_string()),
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
        tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &auth_command_config,
        &model_catalog,
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_command_with_session_import_mode(
    command: &str,
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    tool_policy_json: &serde_json::Value,
    session_import_mode: SessionImportMode,
    profile_defaults: &ProfileDefaults,
    skills_command_config: &SkillsSyncCommandConfig,
    auth_command_config: &AuthCommandConfig,
    model_catalog: &ModelCatalog,
    extension_commands: &[crate::extension_manifest::ExtensionRegisteredCommand],
) -> Result<CommandAction> {
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
        if !command_args.is_empty() {
            println!("usage: /session");
            return Ok(CommandAction::Continue);
        }
        match session_runtime.as_ref() {
            Some(runtime) => {
                println!(
                    "session: path={} entries={} active_head={}",
                    runtime.store.path().display(),
                    runtime.store.entries().len(),
                    runtime
                        .active_head
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "none".to_string())
                );
            }
            None => println!("session: disabled"),
        }
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-search" {
        let Some(runtime) = session_runtime.as_ref() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };
        if command_args.trim().is_empty() {
            println!("usage: /session-search <query> [--role <role>] [--limit <n>]");
            return Ok(CommandAction::Continue);
        }

        println!("{}", execute_session_search_command(runtime, command_args));
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-stats" {
        let Some(runtime) = session_runtime.as_ref() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };
        let format = match parse_session_stats_args(command_args) {
            Ok(format) => format,
            Err(_) => {
                println!("usage: /session-stats [--json]");
                return Ok(CommandAction::Continue);
            }
        };

        println!("{}", execute_session_stats_command(runtime, format));
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-diff" {
        let Some(runtime) = session_runtime.as_ref() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };
        let heads = match parse_session_diff_args(command_args) {
            Ok(heads) => heads,
            Err(_) => {
                println!("usage: /session-diff [<left-id> <right-id>]");
                return Ok(CommandAction::Continue);
            }
        };

        println!("{}", execute_session_diff_command(runtime, heads));
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
        if command_args.trim().is_empty() {
            println!("usage: /session-graph-export <path>");
            return Ok(CommandAction::Continue);
        }

        println!(
            "{}",
            execute_session_graph_export_command(runtime, command_args)
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-export" {
        let Some(runtime) = session_runtime.as_ref() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };
        if command_args.is_empty() {
            println!("usage: /session-export <path>");
            return Ok(CommandAction::Continue);
        }

        let destination = PathBuf::from(command_args);
        let exported = runtime
            .store
            .export_lineage(runtime.active_head, &destination)?;
        println!(
            "session export complete: path={} entries={} head={}",
            destination.display(),
            exported,
            runtime
                .active_head
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_string())
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-import" {
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };
        if command_args.is_empty() {
            println!("usage: /session-import <path>");
            return Ok(CommandAction::Continue);
        }

        let source = PathBuf::from(command_args);
        let report = runtime
            .store
            .import_snapshot(&source, session_import_mode)?;
        runtime.active_head = report.active_head;
        agent.replace_messages(session_lineage_messages(runtime)?);
        println!(
            "session import complete: path={} mode={} imported_entries={} remapped_entries={} remapped_ids={} replaced_entries={} total_entries={} head={}",
            source.display(),
            session_import_mode_label(session_import_mode),
            report.imported_entries,
            report.remapped_entries,
            format_remap_ids(&report.remapped_ids),
            report.replaced_entries,
            report.resulting_entries,
            runtime
                .active_head
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_string())
        );
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

        runtime.active_head = runtime.store.head_id();
        agent.replace_messages(session_lineage_messages(runtime)?);
        println!(
            "resumed at head {}",
            runtime
                .active_head
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_string())
        );
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

        let tips = runtime.store.branch_tips();
        if tips.is_empty() {
            println!("no branches");
        } else {
            for tip in tips {
                println!(
                    "id={} parent={} text={}",
                    tip.id,
                    tip.parent_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string()),
                    summarize_message(&tip.message)
                );
            }
        }

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
        if !command_args.is_empty() {
            println!("usage: /session-repair");
            return Ok(CommandAction::Continue);
        }
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        let report = runtime.store.repair()?;
        runtime.active_head = runtime
            .active_head
            .filter(|head| runtime.store.contains(*head))
            .or_else(|| runtime.store.head_id());
        agent.replace_messages(session_lineage_messages(runtime)?);

        println!(
            "repair complete: removed_duplicates={} duplicate_ids={} removed_invalid_parent={} invalid_parent_ids={} removed_cycles={} cycle_ids={}",
            report.removed_duplicates,
            format_id_list(&report.duplicate_ids),
            report.removed_invalid_parent,
            format_id_list(&report.invalid_parent_ids),
            report.removed_cycles,
            format_id_list(&report.cycle_ids)
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-compact" {
        if !command_args.is_empty() {
            println!("usage: /session-compact");
            return Ok(CommandAction::Continue);
        }
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        let report = runtime.store.compact_to_lineage(runtime.active_head)?;
        runtime.active_head = report
            .head_id
            .filter(|head| runtime.store.contains(*head))
            .or_else(|| runtime.store.head_id());
        agent.replace_messages(session_lineage_messages(runtime)?);

        println!(
            "compact complete: removed_entries={} retained_entries={} head={}",
            report.removed_entries,
            report.retained_entries,
            runtime
                .active_head
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_string())
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/branch" {
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };
        if command_args.is_empty() {
            println!("usage: /branch <id>");
            return Ok(CommandAction::Continue);
        }

        let target = command_args
            .parse::<u64>()
            .map_err(|_| anyhow!("invalid branch id '{}'; expected an integer", command_args))?;

        if !runtime.store.contains(target) {
            bail!("unknown session id {}", target);
        }

        runtime.active_head = Some(target);
        agent.replace_messages(session_lineage_messages(runtime)?);
        println!("switched to branch id {target}");
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

pub(crate) fn parse_command(input: &str) -> Option<ParsedCommand<'_>> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or_default();
    let args = parts.next().map(str::trim).unwrap_or_default();
    Some(ParsedCommand { name, args })
}

pub(crate) fn canonical_command_name(name: &str) -> &str {
    if name == "/exit" {
        "/quit"
    } else {
        name
    }
}

pub(crate) fn session_import_mode_label(mode: SessionImportMode) -> &'static str {
    match mode {
        SessionImportMode::Merge => "merge",
        SessionImportMode::Replace => "replace",
    }
}

pub(crate) fn normalize_help_topic(topic: &str) -> String {
    let trimmed = topic.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

pub(crate) fn render_help_overview() -> String {
    let mut lines = vec!["commands:".to_string()];
    for spec in COMMAND_SPECS {
        lines.push(format!("  {:<22} {}", spec.usage, spec.description));
    }
    lines.push("tip: run /help <command> for details".to_string());
    lines.join("\n")
}

pub(crate) fn render_command_help(topic: &str) -> Option<String> {
    let normalized = normalize_help_topic(topic);
    let command_name = canonical_command_name(&normalized);
    let spec = COMMAND_SPECS
        .iter()
        .find(|entry| entry.name == command_name)?;
    Some(format!(
        "command: {}\nusage: {}\n{}\n{}\nexample: {}",
        spec.name, spec.usage, spec.description, spec.details, spec.example
    ))
}

pub(crate) fn unknown_help_topic_message(topic: &str) -> String {
    match suggest_command(topic) {
        Some(suggestion) => format!(
            "unknown help topic: {topic}\ndid you mean {suggestion}?\nrun /help for command list"
        ),
        None => format!("unknown help topic: {topic}\nrun /help for command list"),
    }
}

pub(crate) fn unknown_command_message(command: &str) -> String {
    match suggest_command(command) {
        Some(suggestion) => {
            format!("unknown command: {command}\ndid you mean {suggestion}?\nrun /help for command list")
        }
        None => format!("unknown command: {command}\nrun /help for command list"),
    }
}

fn suggest_command(command: &str) -> Option<&'static str> {
    let command = canonical_command_name(command);
    if command.is_empty() {
        return None;
    }

    if let Some(prefix_match) = COMMAND_NAMES
        .iter()
        .find(|candidate| candidate.starts_with(command))
    {
        return Some(prefix_match);
    }

    let mut best: Option<(&str, usize)> = None;
    for candidate in COMMAND_NAMES {
        let distance = levenshtein_distance(command, candidate);
        match best {
            Some((_, best_distance)) if distance >= best_distance => {}
            _ => best = Some((candidate, distance)),
        }
    }

    let (candidate, distance) = best?;
    let threshold = match command.len() {
        0..=4 => 1,
        5..=8 => 2,
        _ => 3,
    };
    if distance <= threshold {
        Some(candidate)
    } else {
        None
    }
}

fn levenshtein_distance(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    if a.is_empty() {
        return b.chars().count();
    }
    if b.is_empty() {
        return a.chars().count();
    }

    let b_chars = b.chars().collect::<Vec<_>>();
    let mut previous = (0..=b_chars.len()).collect::<Vec<_>>();
    let mut current = vec![0; b_chars.len() + 1];

    for (i, left) in a.chars().enumerate() {
        current[0] = i + 1;
        for (j, right) in b_chars.iter().enumerate() {
            let substitution_cost = if left == *right { 0 } else { 1 };
            let deletion = previous[j + 1] + 1;
            let insertion = current[j] + 1;
            let substitution = previous[j] + substitution_cost;
            current[j + 1] = deletion.min(insertion).min(substitution);
        }
        previous.clone_from_slice(&current);
    }

    previous[b_chars.len()]
}
