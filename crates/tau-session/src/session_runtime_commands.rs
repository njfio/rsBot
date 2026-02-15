//! Runtime-facing session command dispatcher and outcome formatting helpers.

use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use tau_ai::Message;

use crate::{
    execute_session_diff_command, execute_session_graph_export_command,
    execute_session_search_command, execute_session_stats_command, format_id_list,
    format_remap_ids, navigate_session_head, parse_session_diff_args, parse_session_stats_args,
    redo_session_head, undo_session_head, SessionImportMode, SessionMergeStrategy, SessionRuntime,
};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `SessionRuntimeCommandOutcome` used across Tau components.
pub struct SessionRuntimeCommandOutcome {
    pub message: String,
    pub reload_active_head: bool,
}

impl SessionRuntimeCommandOutcome {
    fn new(message: String, reload_active_head: bool) -> Self {
        Self {
            message,
            reload_active_head,
        }
    }
}

fn format_head(head: Option<u64>) -> String {
    head.map(|id| id.to_string())
        .unwrap_or_else(|| "none".to_string())
}

pub fn session_import_mode_label(mode: SessionImportMode) -> &'static str {
    match mode {
        SessionImportMode::Merge => "merge",
        SessionImportMode::Replace => "replace",
    }
}

pub fn session_merge_strategy_label(strategy: SessionMergeStrategy) -> &'static str {
    strategy.label()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SessionMergeCommandArgs {
    source_head: u64,
    target_head: Option<u64>,
    strategy: SessionMergeStrategy,
}

fn parse_session_merge_strategy(raw: &str) -> Result<SessionMergeStrategy> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "append" => Ok(SessionMergeStrategy::Append),
        "squash" => Ok(SessionMergeStrategy::Squash),
        "fast-forward" | "fast_forward" | "ff" => Ok(SessionMergeStrategy::FastForward),
        _ => bail!(
            "invalid merge strategy '{}'; expected append, squash, or fast-forward",
            raw
        ),
    }
}

fn parse_session_merge_command_args(command_args: &str) -> Result<SessionMergeCommandArgs> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("usage: /session-merge <source-id> [target-id] [--strategy <append|squash|fast-forward>]");
    }

    let source_head = tokens[0].parse::<u64>().map_err(|_| {
        anyhow!(
            "invalid source session id '{}'; expected an integer",
            tokens[0]
        )
    })?;
    let mut target_head = None;
    let mut strategy = SessionMergeStrategy::Append;

    let mut index = 1usize;
    if let Some(token) = tokens.get(index) {
        if !token.starts_with("--") {
            target_head = Some(token.parse::<u64>().map_err(|_| {
                anyhow!("invalid target session id '{}'; expected an integer", token)
            })?);
            index += 1;
        }
    }

    while index < tokens.len() {
        let token = tokens[index];
        if token == "--strategy" {
            let value = tokens
                .get(index + 1)
                .ok_or_else(|| anyhow!("missing value for --strategy"))?;
            strategy = parse_session_merge_strategy(value)?;
            index += 2;
            continue;
        }
        if let Some(value) = token.strip_prefix("--strategy=") {
            strategy = parse_session_merge_strategy(value)?;
            index += 1;
            continue;
        }
        bail!("unknown flag '{}'", token);
    }

    Ok(SessionMergeCommandArgs {
        source_head,
        target_head,
        strategy,
    })
}

pub fn execute_resume_command(
    command_args: &str,
    runtime: &mut SessionRuntime,
) -> SessionRuntimeCommandOutcome {
    if !command_args.is_empty() {
        return SessionRuntimeCommandOutcome::new("usage: /resume".to_string(), false);
    }
    let target_head = runtime.store.head_id();
    let message = match navigate_session_head(runtime, target_head) {
        Ok(transition) => format!(
            "resumed at head {} reason_code=session_resume undo_depth={} redo_depth={}",
            format_head(transition.active_head),
            transition.undo_depth,
            transition.redo_depth
        ),
        Err(error) => {
            runtime.active_head = target_head;
            format!(
                "resumed at head {} warning=failed_to_record_navigation error={error}",
                format_head(runtime.active_head)
            )
        }
    };
    SessionRuntimeCommandOutcome::new(message, true)
}

pub fn execute_undo_command(
    command_args: &str,
    runtime: &mut SessionRuntime,
) -> Result<SessionRuntimeCommandOutcome> {
    if !command_args.is_empty() {
        return Ok(SessionRuntimeCommandOutcome::new(
            "usage: /undo".to_string(),
            false,
        ));
    }

    let transition = undo_session_head(runtime)?;
    if !transition.changed {
        return Ok(SessionRuntimeCommandOutcome::new(
            format!(
                "undo unavailable: active_head={} reason_code=session_undo_empty_stack undo_depth={} redo_depth={} skipped_invalid_targets={}",
                format_head(transition.active_head),
                transition.undo_depth,
                transition.redo_depth,
                transition.skipped_invalid_targets
            ),
            false,
        ));
    }

    Ok(SessionRuntimeCommandOutcome::new(
        format!(
            "undo complete: from={} to={} reason_code=session_undo_applied undo_depth={} redo_depth={} skipped_invalid_targets={}",
            format_head(transition.previous_head),
            format_head(transition.active_head),
            transition.undo_depth,
            transition.redo_depth,
            transition.skipped_invalid_targets
        ),
        true,
    ))
}

pub fn execute_redo_command(
    command_args: &str,
    runtime: &mut SessionRuntime,
) -> Result<SessionRuntimeCommandOutcome> {
    if !command_args.is_empty() {
        return Ok(SessionRuntimeCommandOutcome::new(
            "usage: /redo".to_string(),
            false,
        ));
    }

    let transition = redo_session_head(runtime)?;
    if !transition.changed {
        return Ok(SessionRuntimeCommandOutcome::new(
            format!(
                "redo unavailable: active_head={} reason_code=session_redo_empty_stack undo_depth={} redo_depth={} skipped_invalid_targets={}",
                format_head(transition.active_head),
                transition.undo_depth,
                transition.redo_depth,
                transition.skipped_invalid_targets
            ),
            false,
        ));
    }

    Ok(SessionRuntimeCommandOutcome::new(
        format!(
            "redo complete: from={} to={} reason_code=session_redo_applied undo_depth={} redo_depth={} skipped_invalid_targets={}",
            format_head(transition.previous_head),
            format_head(transition.active_head),
            transition.undo_depth,
            transition.redo_depth,
            transition.skipped_invalid_targets
        ),
        true,
    ))
}

pub fn execute_session_repair_command(
    command_args: &str,
    runtime: &mut SessionRuntime,
) -> Result<SessionRuntimeCommandOutcome> {
    if !command_args.is_empty() {
        return Ok(SessionRuntimeCommandOutcome::new(
            "usage: /session-repair".to_string(),
            false,
        ));
    }

    let report = runtime.store.repair()?;
    let target_head = runtime
        .active_head
        .filter(|head| runtime.store.contains(*head))
        .or_else(|| runtime.store.head_id());
    navigate_session_head(runtime, target_head)?;

    Ok(SessionRuntimeCommandOutcome::new(
        format!(
            "repair complete: removed_duplicates={} duplicate_ids={} removed_invalid_parent={} invalid_parent_ids={} removed_cycles={} cycle_ids={}",
            report.removed_duplicates,
            format_id_list(&report.duplicate_ids),
            report.removed_invalid_parent,
            format_id_list(&report.invalid_parent_ids),
            report.removed_cycles,
            format_id_list(&report.cycle_ids)
        ),
        true,
    ))
}

pub fn execute_session_compact_command(
    command_args: &str,
    runtime: &mut SessionRuntime,
) -> Result<SessionRuntimeCommandOutcome> {
    if !command_args.is_empty() {
        return Ok(SessionRuntimeCommandOutcome::new(
            "usage: /session-compact".to_string(),
            false,
        ));
    }

    let report = runtime.store.compact_to_lineage(runtime.active_head)?;
    let target_head = report
        .head_id
        .filter(|head| runtime.store.contains(*head))
        .or_else(|| runtime.store.head_id());
    navigate_session_head(runtime, target_head)?;

    Ok(SessionRuntimeCommandOutcome::new(
        format!(
            "compact complete: removed_entries={} retained_entries={} head={}",
            report.removed_entries,
            report.retained_entries,
            format_head(runtime.active_head)
        ),
        true,
    ))
}

pub fn execute_branch_switch_command(
    command_args: &str,
    runtime: &mut SessionRuntime,
) -> Result<SessionRuntimeCommandOutcome> {
    if command_args.is_empty() {
        return Ok(SessionRuntimeCommandOutcome::new(
            "usage: /branch <id>".to_string(),
            false,
        ));
    }

    let target = command_args
        .parse::<u64>()
        .map_err(|_| anyhow!("invalid branch id '{}'; expected an integer", command_args))?;

    if !runtime.store.contains(target) {
        bail!("unknown session id {}", target);
    }

    navigate_session_head(runtime, Some(target))?;
    Ok(SessionRuntimeCommandOutcome::new(
        format!("switched to branch id {target}"),
        true,
    ))
}

pub fn execute_session_status_command(
    command_args: &str,
    runtime: &SessionRuntime,
) -> SessionRuntimeCommandOutcome {
    if !command_args.is_empty() {
        return SessionRuntimeCommandOutcome::new("usage: /session".to_string(), false);
    }

    SessionRuntimeCommandOutcome::new(
        format!(
            "session: path={} entries={} active_head={}",
            runtime.store.path().display(),
            runtime.store.entries().len(),
            runtime
                .active_head
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_string())
        ),
        false,
    )
}

pub fn execute_session_export_command(
    command_args: &str,
    runtime: &SessionRuntime,
) -> Result<SessionRuntimeCommandOutcome> {
    if command_args.is_empty() {
        return Ok(SessionRuntimeCommandOutcome::new(
            "usage: /session-export <path>".to_string(),
            false,
        ));
    }

    let destination = PathBuf::from(command_args);
    let exported = runtime
        .store
        .export_lineage(runtime.active_head, &destination)?;
    Ok(SessionRuntimeCommandOutcome::new(
        format!(
            "session export complete: path={} entries={} head={}",
            destination.display(),
            exported,
            runtime
                .active_head
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_string())
        ),
        false,
    ))
}

pub fn execute_session_import_command(
    command_args: &str,
    runtime: &mut SessionRuntime,
    session_import_mode: SessionImportMode,
) -> Result<SessionRuntimeCommandOutcome> {
    if command_args.is_empty() {
        return Ok(SessionRuntimeCommandOutcome::new(
            "usage: /session-import <path>".to_string(),
            false,
        ));
    }

    let source = PathBuf::from(command_args);
    let report = runtime
        .store
        .import_snapshot(&source, session_import_mode)?;
    navigate_session_head(runtime, report.active_head)?;
    Ok(SessionRuntimeCommandOutcome::new(
        format!(
            "session import complete: path={} mode={} imported_entries={} remapped_entries={} remapped_ids={} replaced_entries={} total_entries={} head={}",
            source.display(),
            session_import_mode_label(session_import_mode),
            report.imported_entries,
            report.remapped_entries,
            format_remap_ids(&report.remapped_ids),
            report.replaced_entries,
            report.resulting_entries,
            format_head(runtime.active_head)
        ),
        true,
    ))
}

pub fn execute_session_merge_command(
    command_args: &str,
    runtime: &mut SessionRuntime,
) -> Result<SessionRuntimeCommandOutcome> {
    let parsed = match parse_session_merge_command_args(command_args) {
        Ok(parsed) => parsed,
        Err(_) => {
            return Ok(SessionRuntimeCommandOutcome::new(
                "usage: /session-merge <source-id> [target-id] [--strategy <append|squash|fast-forward>]".to_string(),
                false,
            ));
        }
    };

    let target_head = parsed.target_head.or(runtime.active_head).ok_or_else(|| {
        anyhow!("target head is not set; provide explicit target id or set active head")
    })?;
    let previous_head = runtime.active_head;
    let report = runtime
        .store
        .merge_branches(parsed.source_head, target_head, parsed.strategy)?;
    navigate_session_head(runtime, Some(report.merged_head))?;

    Ok(SessionRuntimeCommandOutcome::new(
        format!(
            "session merge complete: source={} target={} strategy={} common_ancestor={} appended_entries={} head={}",
            report.source_head,
            report.target_head,
            session_merge_strategy_label(report.strategy),
            report
                .common_ancestor
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_string()),
            report.appended_entries,
            report.merged_head
        ),
        previous_head != runtime.active_head || report.appended_entries > 0,
    ))
}

pub fn execute_branches_command(
    command_args: &str,
    runtime: &SessionRuntime,
) -> SessionRuntimeCommandOutcome {
    if !command_args.is_empty() {
        return SessionRuntimeCommandOutcome::new("usage: /branches".to_string(), false);
    }

    let tips = runtime.store.branch_tips();
    if tips.is_empty() {
        return SessionRuntimeCommandOutcome::new("no branches".to_string(), false);
    }

    let lines = tips
        .iter()
        .map(|tip| {
            format!(
                "id={} parent={} text={}",
                tip.id,
                tip.parent_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                summarize_message(&tip.message)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    SessionRuntimeCommandOutcome::new(lines, false)
}

pub fn execute_session_search_runtime_command(
    command_args: &str,
    runtime: &SessionRuntime,
) -> SessionRuntimeCommandOutcome {
    if command_args.trim().is_empty() {
        return SessionRuntimeCommandOutcome::new(
            "usage: /session-search <query> [--role <role>] [--limit <n>]".to_string(),
            false,
        );
    }

    SessionRuntimeCommandOutcome::new(execute_session_search_command(runtime, command_args), false)
}

pub fn execute_session_stats_runtime_command(
    command_args: &str,
    runtime: &SessionRuntime,
) -> SessionRuntimeCommandOutcome {
    let format = match parse_session_stats_args(command_args) {
        Ok(format) => format,
        Err(_) => {
            return SessionRuntimeCommandOutcome::new(
                "usage: /session-stats [--json]".to_string(),
                false,
            );
        }
    };

    SessionRuntimeCommandOutcome::new(execute_session_stats_command(runtime, format), false)
}

pub fn execute_session_diff_runtime_command(
    command_args: &str,
    runtime: &SessionRuntime,
) -> SessionRuntimeCommandOutcome {
    let heads = match parse_session_diff_args(command_args) {
        Ok(heads) => heads,
        Err(_) => {
            return SessionRuntimeCommandOutcome::new(
                "usage: /session-diff [<left-id> <right-id>]".to_string(),
                false,
            );
        }
    };

    SessionRuntimeCommandOutcome::new(execute_session_diff_command(runtime, heads), false)
}

pub fn execute_session_graph_export_runtime_command(
    command_args: &str,
    runtime: &SessionRuntime,
) -> SessionRuntimeCommandOutcome {
    if command_args.trim().is_empty() {
        return SessionRuntimeCommandOutcome::new(
            "usage: /session-graph-export <path>".to_string(),
            false,
        );
    }

    SessionRuntimeCommandOutcome::new(
        execute_session_graph_export_command(runtime, command_args),
        false,
    )
}

fn summarize_message(message: &Message) -> String {
    let text = message.text_content().replace('\n', " ");
    if text.trim().is_empty() {
        return format!(
            "{:?} (tool_calls={})",
            message.role,
            message.tool_calls().len()
        );
    }

    let max = 60usize;
    if text.chars().count() <= max {
        text
    } else {
        format!("{}...", text.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        execute_branch_switch_command, execute_branches_command, execute_redo_command,
        execute_resume_command, execute_session_compact_command,
        execute_session_diff_runtime_command, execute_session_export_command,
        execute_session_graph_export_runtime_command, execute_session_import_command,
        execute_session_merge_command, execute_session_repair_command,
        execute_session_search_runtime_command, execute_session_stats_runtime_command,
        execute_session_status_command, execute_undo_command, parse_session_merge_command_args,
        session_import_mode_label, session_merge_strategy_label, SessionMergeCommandArgs,
    };
    use crate::{SessionImportMode, SessionMergeStrategy, SessionRuntime, SessionStore};
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };
    use tau_ai::Message;

    static FIXTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct SessionRuntimeFixture {
        runtime: SessionRuntime,
        root: PathBuf,
    }

    impl SessionRuntimeFixture {
        fn next_root() -> PathBuf {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos();
            let counter = FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed);
            std::env::temp_dir()
                .join("tau-session-runtime-commands-tests")
                .join(format!("case-{unique}-{counter}"))
        }

        fn empty() -> Self {
            let root = Self::next_root();
            fs::create_dir_all(&root).expect("create fixture root");
            let session_path = root.join("session.jsonl");
            let store = SessionStore::load(&session_path).expect("load session store");
            Self {
                runtime: SessionRuntime {
                    store,
                    active_head: None,
                },
                root,
            }
        }

        fn seeded() -> Self {
            let root = Self::next_root();
            fs::create_dir_all(&root).expect("create fixture root");
            let session_path = root.join("session.jsonl");
            let mut store = SessionStore::load(&session_path).expect("load session store");
            let mut active_head = store
                .ensure_initialized("system prompt")
                .expect("initialize session");
            active_head = store
                .append_messages(
                    active_head,
                    &[
                        Message::user("first user prompt"),
                        Message::assistant_text("first assistant response"),
                    ],
                )
                .expect("append base messages");
            Self {
                runtime: SessionRuntime { store, active_head },
                root,
            }
        }

        fn with_diverged_branches() -> Self {
            let mut fixture = Self::seeded();
            let root_id = fixture
                .runtime
                .store
                .entries()
                .first()
                .expect("system entry should exist")
                .id;
            fixture
                .runtime
                .store
                .append_messages(Some(root_id), &[Message::user("branch alpha")])
                .expect("append alpha branch");
            fixture
                .runtime
                .store
                .append_messages(Some(root_id), &[Message::user("branch beta")])
                .expect("append beta branch");
            fixture
        }
    }

    impl Drop for SessionRuntimeFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn unit_session_import_mode_label_matches_variants() {
        assert_eq!(session_import_mode_label(SessionImportMode::Merge), "merge");
        assert_eq!(
            session_import_mode_label(SessionImportMode::Replace),
            "replace"
        );
    }

    #[test]
    fn unit_session_merge_strategy_label_matches_variants() {
        assert_eq!(
            session_merge_strategy_label(SessionMergeStrategy::Append),
            "append"
        );
        assert_eq!(
            session_merge_strategy_label(SessionMergeStrategy::Squash),
            "squash"
        );
        assert_eq!(
            session_merge_strategy_label(SessionMergeStrategy::FastForward),
            "fast-forward"
        );
    }

    #[test]
    fn unit_parse_session_merge_command_args_supports_optional_target_and_strategy() {
        let parsed = parse_session_merge_command_args("42 24 --strategy squash")
            .expect("merge args should parse");
        assert_eq!(
            parsed,
            SessionMergeCommandArgs {
                source_head: 42,
                target_head: Some(24),
                strategy: SessionMergeStrategy::Squash,
            }
        );

        let with_equals =
            parse_session_merge_command_args("42 --strategy=ff").expect("merge args should parse");
        assert_eq!(
            with_equals,
            SessionMergeCommandArgs {
                source_head: 42,
                target_head: None,
                strategy: SessionMergeStrategy::FastForward,
            }
        );
    }

    #[test]
    fn functional_execute_resume_command_sets_head_and_requests_reload() {
        let mut fixture = SessionRuntimeFixture::seeded();
        fixture.runtime.active_head = None;

        let outcome = execute_resume_command("", &mut fixture.runtime);
        assert!(outcome.reload_active_head);
        assert_eq!(fixture.runtime.active_head, fixture.runtime.store.head_id());
        assert!(outcome.message.starts_with("resumed at head "));
        assert!(outcome.message.contains("reason_code=session_resume"));
    }

    #[test]
    fn functional_execute_undo_command_rewinds_previous_navigation_head() {
        let mut fixture = SessionRuntimeFixture::seeded();
        let latest_head = fixture
            .runtime
            .active_head
            .expect("latest head should exist");
        let branch_target = fixture
            .runtime
            .store
            .entries()
            .get(1)
            .expect("second entry should exist")
            .id;

        execute_branch_switch_command(&branch_target.to_string(), &mut fixture.runtime)
            .expect("branch switch should succeed");
        assert_eq!(fixture.runtime.active_head, Some(branch_target));

        let undo = execute_undo_command("", &mut fixture.runtime).expect("undo should succeed");
        assert!(undo.reload_active_head);
        assert_eq!(fixture.runtime.active_head, Some(latest_head));
        assert!(undo.message.contains("reason_code=session_undo_applied"));
    }

    #[test]
    fn integration_execute_redo_command_reapplies_undone_navigation_head() {
        let mut fixture = SessionRuntimeFixture::seeded();
        let branch_target = fixture
            .runtime
            .store
            .entries()
            .get(1)
            .expect("second entry should exist")
            .id;

        execute_branch_switch_command(&branch_target.to_string(), &mut fixture.runtime)
            .expect("branch switch should succeed");
        execute_undo_command("", &mut fixture.runtime).expect("undo should succeed");

        let redo = execute_redo_command("", &mut fixture.runtime).expect("redo should succeed");
        assert!(redo.reload_active_head);
        assert_eq!(fixture.runtime.active_head, Some(branch_target));
        assert!(redo.message.contains("reason_code=session_redo_applied"));
    }

    #[test]
    fn functional_execute_session_export_command_writes_active_lineage_snapshot() {
        let mut fixture = SessionRuntimeFixture::seeded();
        let root_id = fixture
            .runtime
            .store
            .entries()
            .first()
            .expect("system entry should exist")
            .id;
        fixture
            .runtime
            .store
            .append_messages(Some(root_id), &[Message::assistant_text("branch-only")])
            .expect("append branch-only message");

        let destination = fixture.root.join("snapshot.jsonl");
        let outcome = execute_session_export_command(
            destination.to_str().expect("utf8 destination path"),
            &fixture.runtime,
        )
        .expect("export command should succeed");
        assert!(!outcome.reload_active_head);
        assert!(outcome.message.starts_with("session export complete:"));

        let exported = SessionStore::load(&destination).expect("load exported snapshot");
        assert_eq!(exported.entries().len(), 3);
        assert_eq!(
            exported.entries()[0].message.text_content(),
            "system prompt"
        );
        assert_eq!(
            exported.entries()[1].message.text_content(),
            "first user prompt"
        );
        assert_eq!(
            exported.entries()[2].message.text_content(),
            "first assistant response"
        );
    }

    #[test]
    fn unit_execute_session_search_runtime_command_usage_for_empty_query() {
        let fixture = SessionRuntimeFixture::seeded();
        let outcome = execute_session_search_runtime_command("   ", &fixture.runtime);
        assert_eq!(
            outcome.message,
            "usage: /session-search <query> [--role <role>] [--limit <n>]"
        );
        assert!(!outcome.reload_active_head);
    }

    #[test]
    fn functional_execute_session_search_runtime_command_returns_matches() {
        let fixture = SessionRuntimeFixture::seeded();
        let outcome = execute_session_search_runtime_command("assistant", &fixture.runtime);
        assert!(!outcome.reload_active_head);
        assert!(outcome.message.contains("session search:"));
        assert!(outcome.message.contains("matches="));
    }

    #[test]
    fn integration_execute_session_compact_command_prunes_diverged_lineage() {
        let mut fixture = SessionRuntimeFixture::with_diverged_branches();
        let branch_tips = fixture.runtime.store.branch_tips();
        assert!(branch_tips.len() >= 2);
        let selected_head = branch_tips
            .iter()
            .map(|entry| entry.id)
            .min()
            .expect("branch tip should exist");
        fixture.runtime.active_head = Some(selected_head);
        let before_entries = fixture.runtime.store.entries().len();

        let outcome =
            execute_session_compact_command("", &mut fixture.runtime).expect("compact command");
        assert!(outcome.reload_active_head);
        assert!(outcome.message.starts_with("compact complete:"));
        assert_eq!(fixture.runtime.active_head, Some(selected_head));
        assert!(fixture.runtime.store.entries().len() < before_entries);
        assert_eq!(fixture.runtime.store.branch_tips().len(), 1);
    }

    #[test]
    fn integration_execute_session_import_command_updates_head_and_requests_reload() {
        let mut fixture = SessionRuntimeFixture::seeded();
        let source = fixture.root.join("import.jsonl");

        let mut source_store = SessionStore::load(&source).expect("load source snapshot");
        let source_head = source_store
            .append_messages(None, &[Message::system("import-root")])
            .expect("append import root");
        source_store
            .append_messages(source_head, &[Message::user("import-user")])
            .expect("append import user");

        let outcome = execute_session_import_command(
            source.to_str().expect("utf8 source path"),
            &mut fixture.runtime,
            SessionImportMode::Merge,
        )
        .expect("import should succeed");
        assert!(outcome.reload_active_head);
        assert!(outcome.message.contains("session import complete:"));
        assert!(outcome.message.contains("mode=merge"));
        assert!(outcome.message.contains("imported_entries=2"));
        assert_eq!(fixture.runtime.active_head, fixture.runtime.store.head_id());
        assert_eq!(
            fixture
                .runtime
                .store
                .lineage_messages(fixture.runtime.active_head)
                .expect("lineage should resolve")
                .last()
                .expect("imported head message should exist")
                .text_content(),
            "import-user"
        );
    }

    #[test]
    fn functional_execute_session_merge_command_appends_source_branch_to_target_head() {
        let mut fixture = SessionRuntimeFixture::with_diverged_branches();
        let mut tips = fixture
            .runtime
            .store
            .branch_tips()
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        tips.sort_unstable();
        let target = *tips.first().expect("target tip should exist");
        let source = *tips.last().expect("source tip should exist");
        fixture.runtime.active_head = Some(target);
        let before_entries = fixture.runtime.store.entries().len();

        let outcome = execute_session_merge_command(
            &format!("{source} {target} --strategy append"),
            &mut fixture.runtime,
        )
        .expect("merge command should succeed");

        assert!(outcome.reload_active_head);
        assert!(outcome.message.contains("strategy=append"));
        assert!(outcome.message.contains("appended_entries=1"));
        assert!(fixture.runtime.store.entries().len() > before_entries);
        assert_ne!(fixture.runtime.active_head, Some(target));
    }

    #[test]
    fn integration_execute_session_merge_command_fast_forward_moves_active_head_without_append() {
        let mut fixture = SessionRuntimeFixture::seeded();
        let target = fixture
            .runtime
            .store
            .entries()
            .first()
            .expect("system entry should exist")
            .id;
        let source = fixture
            .runtime
            .store
            .head_id()
            .expect("source head should exist");
        fixture.runtime.active_head = Some(target);
        let before_entries = fixture.runtime.store.entries().len();

        let outcome = execute_session_merge_command(
            &format!("{source} {target} --strategy fast-forward"),
            &mut fixture.runtime,
        )
        .expect("fast-forward merge should succeed");

        assert!(outcome.reload_active_head);
        assert!(outcome.message.contains("strategy=fast-forward"));
        assert!(outcome.message.contains("appended_entries=0"));
        assert_eq!(fixture.runtime.active_head, Some(source));
        assert_eq!(fixture.runtime.store.entries().len(), before_entries);
    }

    #[test]
    fn regression_execute_session_merge_command_usage_and_non_ancestor_fast_forward_error() {
        let mut fixture = SessionRuntimeFixture::with_diverged_branches();

        let usage = execute_session_merge_command("", &mut fixture.runtime)
            .expect("usage should be returned as non-error outcome");
        assert_eq!(
            usage.message,
            "usage: /session-merge <source-id> [target-id] [--strategy <append|squash|fast-forward>]"
        );
        assert!(!usage.reload_active_head);

        let mut tips = fixture
            .runtime
            .store
            .branch_tips()
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        tips.sort_unstable();
        let target = *tips.first().expect("target tip should exist");
        let source = *tips.last().expect("source tip should exist");

        let error = execute_session_merge_command(
            &format!("{source} {target} --strategy fast-forward"),
            &mut fixture.runtime,
        )
        .expect_err("fast-forward should fail for diverged branches");
        assert!(error.to_string().contains("cannot fast-forward target"));
    }

    #[test]
    fn integration_execute_branches_command_lists_branch_tips_and_summarizes_messages() {
        let mut fixture = SessionRuntimeFixture::with_diverged_branches();
        let root_id = fixture
            .runtime
            .store
            .entries()
            .first()
            .expect("system entry should exist")
            .id;
        fixture
            .runtime
            .store
            .append_messages(
                Some(root_id),
                &[Message::user(
                    "line one\nline two to ensure branch summaries normalize whitespace",
                )],
            )
            .expect("append newline message branch");

        let outcome = execute_branches_command("", &fixture.runtime);
        assert!(!outcome.reload_active_head);
        assert!(outcome.message.contains("id="));
        assert!(outcome.message.contains("parent="));
        assert!(outcome.message.contains("text="));
        assert!(outcome.message.contains("line one line two"));
    }

    #[test]
    fn integration_execute_session_stats_runtime_command_supports_text_and_json() {
        let fixture = SessionRuntimeFixture::with_diverged_branches();

        let text_outcome = execute_session_stats_runtime_command("", &fixture.runtime);
        assert!(!text_outcome.reload_active_head);
        assert!(text_outcome.message.contains("session stats:"));

        let json_outcome = execute_session_stats_runtime_command("--json", &fixture.runtime);
        assert!(!json_outcome.reload_active_head);
        let payload = serde_json::from_str::<serde_json::Value>(&json_outcome.message)
            .expect("stats json output should parse");
        assert!(payload["entries"].as_u64().unwrap_or(0) >= 3);
        assert!(payload["branch_tips"].as_u64().unwrap_or(0) >= 1);
    }

    #[test]
    fn integration_execute_session_diff_runtime_command_supports_explicit_heads() {
        let fixture = SessionRuntimeFixture::with_diverged_branches();
        let mut heads = fixture
            .runtime
            .store
            .branch_tips()
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        assert!(heads.len() >= 2);
        heads.sort_unstable();
        let command_args = format!("{} {}", heads[0], heads[1]);

        let outcome = execute_session_diff_runtime_command(&command_args, &fixture.runtime);
        assert!(!outcome.reload_active_head);
        assert!(outcome.message.contains("session diff:"));
        assert!(outcome.message.contains(&heads[0].to_string()));
        assert!(outcome.message.contains(&heads[1].to_string()));
    }

    #[test]
    fn regression_execute_session_repair_command_usage_returns_non_reloading_outcome() {
        let mut fixture = SessionRuntimeFixture::seeded();
        let original_head = fixture.runtime.active_head;

        let outcome = execute_session_repair_command("--invalid", &mut fixture.runtime)
            .expect("usage path should not error");
        assert!(!outcome.reload_active_head);
        assert_eq!(outcome.message, "usage: /session-repair");
        assert_eq!(fixture.runtime.active_head, original_head);
    }

    #[test]
    fn regression_execute_session_status_command_usage_and_empty_state() {
        let fixture = SessionRuntimeFixture::empty();
        let usage = execute_session_status_command("extra", &fixture.runtime);
        assert_eq!(usage.message, "usage: /session");
        assert!(!usage.reload_active_head);

        let status = execute_session_status_command("", &fixture.runtime);
        assert!(!status.reload_active_head);
        assert!(status.message.contains("session: path="));
        assert!(status.message.contains("entries=0"));
        assert!(status.message.contains("active_head=none"));
    }

    #[test]
    fn regression_execute_branches_command_usage_and_empty_session() {
        let fixture = SessionRuntimeFixture::empty();
        let usage = execute_branches_command("extra", &fixture.runtime);
        assert_eq!(usage.message, "usage: /branches");
        assert!(!usage.reload_active_head);

        let no_branches = execute_branches_command("", &fixture.runtime);
        assert_eq!(no_branches.message, "no branches");
        assert!(!no_branches.reload_active_head);
    }

    #[test]
    fn regression_execute_session_export_and_import_usage_paths_are_non_reloading() {
        let mut fixture = SessionRuntimeFixture::seeded();

        let export_usage =
            execute_session_export_command("", &fixture.runtime).expect("usage should succeed");
        assert_eq!(export_usage.message, "usage: /session-export <path>");
        assert!(!export_usage.reload_active_head);

        let import_usage =
            execute_session_import_command("", &mut fixture.runtime, SessionImportMode::Merge)
                .expect("usage should succeed");
        assert_eq!(import_usage.message, "usage: /session-import <path>");
        assert!(!import_usage.reload_active_head);
    }

    #[test]
    fn regression_execute_session_stats_and_diff_runtime_commands_invalid_args_use_usage_messages()
    {
        let fixture = SessionRuntimeFixture::seeded();

        let stats_usage = execute_session_stats_runtime_command("--unknown", &fixture.runtime);
        assert_eq!(stats_usage.message, "usage: /session-stats [--json]");
        assert!(!stats_usage.reload_active_head);

        let diff_usage = execute_session_diff_runtime_command("one two three", &fixture.runtime);
        assert_eq!(
            diff_usage.message,
            "usage: /session-diff [<left-id> <right-id>]"
        );
        assert!(!diff_usage.reload_active_head);
    }

    #[test]
    fn regression_execute_session_graph_export_runtime_command_usage_and_invalid_destination() {
        let fixture = SessionRuntimeFixture::seeded();

        let usage = execute_session_graph_export_runtime_command("", &fixture.runtime);
        assert_eq!(usage.message, "usage: /session-graph-export <path>");
        assert!(!usage.reload_active_head);

        let invalid_path = fixture.root.to_str().expect("utf8 root path");
        let error_outcome =
            execute_session_graph_export_runtime_command(invalid_path, &fixture.runtime);
        assert!(!error_outcome.reload_active_head);
        assert!(error_outcome
            .message
            .contains("session graph export error:"));
    }

    #[test]
    fn regression_execute_branch_switch_command_rejects_unknown_branch_id() {
        let mut fixture = SessionRuntimeFixture::seeded();
        let error = execute_branch_switch_command("999999", &mut fixture.runtime)
            .expect_err("unknown branch id should error");
        assert!(error.to_string().contains("unknown session id"));
    }

    #[test]
    fn regression_execute_branch_switch_command_usage_and_success_modes() {
        let mut fixture = SessionRuntimeFixture::seeded();

        let usage_outcome = execute_branch_switch_command("", &mut fixture.runtime)
            .expect("empty args should return usage message");
        assert!(!usage_outcome.reload_active_head);
        assert_eq!(usage_outcome.message, "usage: /branch <id>");

        let target = fixture
            .runtime
            .store
            .entries()
            .get(1)
            .expect("expected at least two entries")
            .id;
        let success_outcome =
            execute_branch_switch_command(&target.to_string(), &mut fixture.runtime)
                .expect("branch switch should succeed");
        assert!(success_outcome.reload_active_head);
        assert_eq!(fixture.runtime.active_head, Some(target));
        assert_eq!(
            success_outcome.message,
            format!("switched to branch id {target}")
        );
    }

    #[test]
    fn regression_execute_undo_redo_commands_cover_usage_empty_and_stale_histories() {
        let mut fixture = SessionRuntimeFixture::seeded();

        let undo_usage = execute_undo_command("extra", &mut fixture.runtime)
            .expect("usage path should return outcome");
        assert_eq!(undo_usage.message, "usage: /undo");
        assert!(!undo_usage.reload_active_head);

        let redo_usage = execute_redo_command("extra", &mut fixture.runtime)
            .expect("usage path should return outcome");
        assert_eq!(redo_usage.message, "usage: /redo");
        assert!(!redo_usage.reload_active_head);

        let undo_empty =
            execute_undo_command("", &mut fixture.runtime).expect("undo empty should succeed");
        assert!(!undo_empty.reload_active_head);
        assert!(undo_empty
            .message
            .contains("reason_code=session_undo_empty_stack"));

        let branch_target = fixture
            .runtime
            .store
            .entries()
            .first()
            .expect("root entry should exist")
            .id;
        execute_branch_switch_command(&branch_target.to_string(), &mut fixture.runtime)
            .expect("branch switch should succeed");
        execute_session_compact_command("", &mut fixture.runtime)
            .expect("compact should remove stale branch heads");

        let undo_stale =
            execute_undo_command("", &mut fixture.runtime).expect("undo stale should succeed");
        assert!(!undo_stale.reload_active_head);
        assert!(undo_stale
            .message
            .contains("reason_code=session_undo_empty_stack"));
        assert!(undo_stale.message.contains("skipped_invalid_targets="));

        let redo_empty =
            execute_redo_command("", &mut fixture.runtime).expect("redo empty should succeed");
        assert!(!redo_empty.reload_active_head);
        assert!(redo_empty
            .message
            .contains("reason_code=session_redo_empty_stack"));
    }
}
