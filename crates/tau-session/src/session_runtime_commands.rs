use anyhow::{anyhow, bail, Result};

use crate::{format_id_list, SessionImportMode, SessionRuntime};

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

#[cfg(test)]
mod tests {
    use super::{
        execute_branch_switch_command, execute_resume_command, execute_session_compact_command,
        execute_session_repair_command, session_import_mode_label,
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
