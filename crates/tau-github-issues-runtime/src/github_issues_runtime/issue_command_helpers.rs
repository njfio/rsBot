use std::path::{Path, PathBuf};

use tau_github_issues::issue_artifacts_command::{
    parse_issue_artifacts_command as parse_shared_issue_artifacts_command, ArtifactsIssueCommand,
};
use tau_github_issues::issue_auth_command::{
    parse_issue_auth_command as parse_shared_issue_auth_command, TauIssueAuthCommandKind,
};
use tau_github_issues::issue_chat_command::{
    parse_issue_chat_command as parse_shared_issue_chat_command, IssueChatCommand,
    IssueChatParseConfig,
};
use tau_github_issues::issue_command_parser::{
    parse_issue_command as parse_shared_issue_command, ParsedIssueCommand,
};
use tau_github_issues::issue_command_usage::{
    artifacts_command_usage as artifacts_shared_command_usage,
    chat_command_usage as chat_shared_command_usage,
    chat_search_command_usage as chat_search_shared_command_usage,
    chat_show_command_usage as chat_show_shared_command_usage,
    demo_index_command_usage as demo_index_shared_command_usage,
    doctor_command_usage as doctor_shared_command_usage,
    issue_auth_command_usage as issue_auth_shared_command_usage,
    tau_command_usage as tau_shared_command_usage,
};
use tau_github_issues::issue_core_command::{
    parse_issue_core_command as parse_shared_issue_core_command, IssueCoreCommand,
};
use tau_github_issues::issue_demo_index::parse_demo_index_run_command as parse_shared_demo_index_run_command;
use tau_github_issues::issue_demo_index_command::{
    parse_demo_index_issue_command as parse_shared_demo_index_issue_command, DemoIndexIssueCommand,
};
use tau_github_issues::issue_doctor_command::parse_issue_doctor_command as parse_shared_issue_doctor_command;
use tau_session::parse_session_search_args;

use super::{
    DemoIndexRunCommand, TauIssueCommand, CHAT_SEARCH_MAX_LIMIT, CHAT_SHOW_DEFAULT_LIMIT,
    CHAT_SHOW_MAX_LIMIT, DEMO_INDEX_DEFAULT_TIMEOUT_SECONDS, DEMO_INDEX_MAX_TIMEOUT_SECONDS,
    DEMO_INDEX_SCENARIOS,
};
use crate::auth_commands::{parse_auth_command, AuthCommand, AUTH_MATRIX_USAGE, AUTH_STATUS_USAGE};

pub(super) fn default_demo_index_repo_root() -> PathBuf {
    let manifest_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .to_path_buf();
    if manifest_root.join("scripts/demo/index.sh").exists() {
        return manifest_root;
    }
    if let Ok(current_dir) = std::env::current_dir() {
        if current_dir.join("scripts/demo/index.sh").exists() {
            return current_dir;
        }
    }
    manifest_root
}

pub(super) fn default_demo_index_binary_path() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/debug/tau-coding-agent")
            .to_path_buf()
    })
}

/// Parses a single issue comment body into a supported `/tau` command variant.
pub(super) fn parse_tau_issue_command(body: &str) -> Option<TauIssueCommand> {
    let usage = tau_shared_command_usage("/tau");
    let parsed = parse_shared_issue_command(
        body,
        "/tau",
        &usage,
        parse_shared_issue_core_command,
        |command, remainder| match command {
            "auth" => Some(Ok(parse_issue_auth_command(remainder))),
            "doctor" => Some(Ok(parse_doctor_issue_command(remainder))),
            "chat" => Some(Ok(parse_chat_command(remainder))),
            "artifacts" => Some(Ok(parse_artifacts_command(remainder))),
            "demo-index" => Some(Ok(parse_demo_index_command(remainder))),
            _ => None,
        },
    )?;

    let parsed = match parsed {
        ParsedIssueCommand::Core(core) => map_issue_core_command(core),
        ParsedIssueCommand::Special(command) => command,
        ParsedIssueCommand::Invalid { message } => TauIssueCommand::Invalid { message },
        ParsedIssueCommand::Unknown { command } => TauIssueCommand::Invalid {
            message: format!(
                "Unknown command `{}`.\n\n{}",
                command,
                tau_shared_command_usage("/tau")
            ),
        },
    };
    Some(parsed)
}

fn map_issue_core_command(command: IssueCoreCommand) -> TauIssueCommand {
    match command {
        IssueCoreCommand::Run { prompt } => TauIssueCommand::Run { prompt },
        IssueCoreCommand::Stop => TauIssueCommand::Stop,
        IssueCoreCommand::Status => TauIssueCommand::Status,
        IssueCoreCommand::Health => TauIssueCommand::Health,
        IssueCoreCommand::Compact => TauIssueCommand::Compact,
        IssueCoreCommand::Help => TauIssueCommand::Help,
        IssueCoreCommand::Canvas { args } => TauIssueCommand::Canvas { args },
        IssueCoreCommand::Summarize { focus } => TauIssueCommand::Summarize { focus },
    }
}

fn parse_demo_index_command(remainder: &str) -> TauIssueCommand {
    let usage = demo_index_shared_command_usage(
        "/tau",
        &DEMO_INDEX_SCENARIOS,
        DEMO_INDEX_DEFAULT_TIMEOUT_SECONDS,
        DEMO_INDEX_MAX_TIMEOUT_SECONDS,
    );
    match parse_shared_demo_index_issue_command(remainder, &usage, |raw| {
        let parsed = parse_shared_demo_index_run_command(
            raw,
            &DEMO_INDEX_SCENARIOS,
            DEMO_INDEX_DEFAULT_TIMEOUT_SECONDS,
            DEMO_INDEX_MAX_TIMEOUT_SECONDS,
            &usage,
        )?;
        Ok(DemoIndexRunCommand {
            scenarios: parsed.scenarios,
            timeout_seconds: parsed.timeout_seconds,
        })
    }) {
        Ok(DemoIndexIssueCommand::List) => TauIssueCommand::DemoIndexList,
        Ok(DemoIndexIssueCommand::Report) => TauIssueCommand::DemoIndexReport,
        Ok(DemoIndexIssueCommand::Run(command)) => TauIssueCommand::DemoIndexRun { command },
        Err(message) => TauIssueCommand::Invalid { message },
    }
}

fn parse_doctor_issue_command(remainder: &str) -> TauIssueCommand {
    let usage = doctor_shared_command_usage("/tau");
    match parse_shared_issue_doctor_command(remainder, &usage) {
        Ok(command) => TauIssueCommand::Doctor { command },
        Err(message) => TauIssueCommand::Invalid { message },
    }
}

fn parse_issue_auth_command(remainder: &str) -> TauIssueCommand {
    let usage = issue_auth_shared_command_usage("/tau", AUTH_STATUS_USAGE, AUTH_MATRIX_USAGE);
    match parse_shared_issue_auth_command(remainder, &usage, |args| {
        match parse_auth_command(args) {
            Ok(AuthCommand::Status { .. }) => Ok(Some(TauIssueAuthCommandKind::Status)),
            Ok(AuthCommand::Matrix { .. }) => Ok(Some(TauIssueAuthCommandKind::Matrix)),
            Ok(_) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }) {
        Ok(command) => TauIssueCommand::Auth { command },
        Err(message) => TauIssueCommand::Invalid { message },
    }
}

fn parse_chat_command(remainder: &str) -> TauIssueCommand {
    let usage = chat_shared_command_usage("/tau");
    let show_usage = chat_show_shared_command_usage("/tau");
    let search_usage = chat_search_shared_command_usage("/tau");
    match parse_shared_issue_chat_command(
        remainder,
        IssueChatParseConfig {
            show_default_limit: CHAT_SHOW_DEFAULT_LIMIT,
            show_max_limit: CHAT_SHOW_MAX_LIMIT,
            search_max_limit: CHAT_SEARCH_MAX_LIMIT,
            usage: &usage,
            show_usage: &show_usage,
            search_usage: &search_usage,
        },
        |raw| {
            parse_session_search_args(raw)
                .map(|args| (args.query, args.role, args.limit))
                .map_err(|error| error.to_string())
        },
    ) {
        Ok(IssueChatCommand::Start) => TauIssueCommand::ChatStart,
        Ok(IssueChatCommand::Resume) => TauIssueCommand::ChatResume,
        Ok(IssueChatCommand::Reset) => TauIssueCommand::ChatReset,
        Ok(IssueChatCommand::Export) => TauIssueCommand::ChatExport,
        Ok(IssueChatCommand::Status) => TauIssueCommand::ChatStatus,
        Ok(IssueChatCommand::Summary) => TauIssueCommand::ChatSummary,
        Ok(IssueChatCommand::Replay) => TauIssueCommand::ChatReplay,
        Ok(IssueChatCommand::Show { limit }) => TauIssueCommand::ChatShow { limit },
        Ok(IssueChatCommand::Search { query, role, limit }) => {
            TauIssueCommand::ChatSearch { query, role, limit }
        }
        Err(message) => TauIssueCommand::Invalid { message },
    }
}

fn parse_artifacts_command(remainder: &str) -> TauIssueCommand {
    let usage = artifacts_shared_command_usage("/tau");
    match parse_shared_issue_artifacts_command(remainder, &usage) {
        Ok(ArtifactsIssueCommand::List) => TauIssueCommand::Artifacts {
            purge: false,
            run_id: None,
        },
        Ok(ArtifactsIssueCommand::Purge) => TauIssueCommand::Artifacts {
            purge: true,
            run_id: None,
        },
        Ok(ArtifactsIssueCommand::Run { run_id }) => TauIssueCommand::Artifacts {
            purge: false,
            run_id: Some(run_id),
        },
        Ok(ArtifactsIssueCommand::Show { artifact_id }) => {
            TauIssueCommand::ArtifactShow { artifact_id }
        }
        Err(message) => TauIssueCommand::Invalid { message },
    }
}
