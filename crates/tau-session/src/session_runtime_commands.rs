use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use tau_ai::Message;

use crate::{format_id_list, format_remap_ids, SessionImportMode, SessionRuntime};

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

pub fn session_import_mode_label(mode: SessionImportMode) -> &'static str {
    match mode {
        SessionImportMode::Merge => "merge",
        SessionImportMode::Replace => "replace",
    }
}

pub fn execute_resume_command(
    command_args: &str,
    runtime: &mut SessionRuntime,
) -> SessionRuntimeCommandOutcome {
    if !command_args.is_empty() {
        return SessionRuntimeCommandOutcome::new("usage: /resume".to_string(), false);
    }
    runtime.active_head = runtime.store.head_id();
    SessionRuntimeCommandOutcome::new(
        format!(
            "resumed at head {}",
            runtime
                .active_head
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_string())
        ),
        true,
    )
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
    runtime.active_head = runtime
        .active_head
        .filter(|head| runtime.store.contains(*head))
        .or_else(|| runtime.store.head_id());

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
    runtime.active_head = report
        .head_id
        .filter(|head| runtime.store.contains(*head))
        .or_else(|| runtime.store.head_id());

    Ok(SessionRuntimeCommandOutcome::new(
        format!(
            "compact complete: removed_entries={} retained_entries={} head={}",
            report.removed_entries,
            report.retained_entries,
            runtime
                .active_head
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_string())
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

    runtime.active_head = Some(target);
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
    runtime.active_head = report.active_head;
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
            runtime
                .active_head
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_string())
        ),
        true,
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
        execute_branch_switch_command, execute_branches_command, execute_resume_command,
        execute_session_compact_command, execute_session_export_command,
        execute_session_import_command, execute_session_repair_command,
        execute_session_status_command, session_import_mode_label,
    };
    use crate::{SessionImportMode, SessionRuntime, SessionStore};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tau_ai::Message;

    struct SessionRuntimeFixture {
        runtime: SessionRuntime,
        root: PathBuf,
    }

    impl SessionRuntimeFixture {
        fn empty() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos();
            let root = std::env::temp_dir()
                .join("tau-session-runtime-commands-tests")
                .join(format!("case-{unique}"));
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
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos();
            let root = std::env::temp_dir()
                .join("tau-session-runtime-commands-tests")
                .join(format!("case-{unique}"));
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
    fn functional_execute_resume_command_sets_head_and_requests_reload() {
        let mut fixture = SessionRuntimeFixture::seeded();
        fixture.runtime.active_head = None;

        let outcome = execute_resume_command("", &mut fixture.runtime);
        assert!(outcome.reload_active_head);
        assert_eq!(fixture.runtime.active_head, fixture.runtime.store.head_id());
        assert!(outcome.message.starts_with("resumed at head "));
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
}
