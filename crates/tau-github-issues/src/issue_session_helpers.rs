use std::path::Path;

use anyhow::{Context, Result};
use tau_session::SessionStore;

/// Compact one issue session store to active lineage while honoring lock policy.
pub fn compact_issue_session(
    session_path: &Path,
    lock_wait_ms: u64,
    lock_stale_ms: u64,
) -> Result<tau_session::CompactReport> {
    if let Some(parent) = session_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let mut store = SessionStore::load(session_path)?;
    store.set_lock_policy(lock_wait_ms.max(1), lock_stale_ms);
    store.compact_to_lineage(store.head_id())
}

/// Ensure issue session store exists and is initialized with system prompt.
pub fn ensure_issue_session_initialized(
    session_path: &Path,
    system_prompt: &str,
    lock_wait_ms: u64,
    lock_stale_ms: u64,
) -> Result<(usize, usize, Option<u64>)> {
    if let Some(parent) = session_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let mut store = SessionStore::load(session_path)?;
    store.set_lock_policy(lock_wait_ms.max(1), lock_stale_ms);
    let before_entries = store.entries().len();
    let head = store.ensure_initialized(system_prompt)?;
    let after_entries = store.entries().len();
    Ok((before_entries, after_entries, head))
}

/// Remove issue session and lock files when present and report what was removed.
pub fn reset_issue_session_files(session_path: &Path) -> Result<(bool, bool)> {
    let mut removed_session = false;
    if session_path.exists() {
        std::fs::remove_file(session_path)
            .with_context(|| format!("failed to remove {}", session_path.display()))?;
        removed_session = true;
    }
    let lock_path = session_path.with_extension("lock");
    let mut removed_lock = false;
    if lock_path.exists() {
        std::fs::remove_file(&lock_path)
            .with_context(|| format!("failed to remove {}", lock_path.display()))?;
        removed_lock = true;
    }
    Ok((removed_session, removed_lock))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tau_ai::Message;
    use tau_session::SessionStore;

    use super::{
        compact_issue_session, ensure_issue_session_initialized, reset_issue_session_files,
    };

    fn unique_test_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "tau-github-issues-issue-session-helpers-{label}-{nanos}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create test dir");
        dir
    }

    #[test]
    fn unit_reset_issue_session_files_removes_session_and_lock_files() {
        let dir = unique_test_dir("unit-reset-removes");
        let session_path = dir.join("issue.md");
        let lock_path = session_path.with_extension("lock");
        std::fs::write(&session_path, "session").expect("write session file");
        std::fs::write(&lock_path, "lock").expect("write lock file");

        let (removed_session, removed_lock) =
            reset_issue_session_files(&session_path).expect("reset session files");

        assert!(removed_session);
        assert!(removed_lock);
        assert!(!session_path.exists());
        assert!(!lock_path.exists());
    }

    #[test]
    fn functional_ensure_issue_session_initialized_creates_and_initializes_store() {
        let dir = unique_test_dir("functional-ensure");
        let session_path = dir.join("nested").join("issue.md");

        let (before, after, head) =
            ensure_issue_session_initialized(&session_path, "system prompt", 0, 30_000)
                .expect("ensure session initialized");

        assert_eq!(before, 0);
        assert_eq!(after, 1);
        assert_eq!(head, Some(1));
        assert!(session_path.exists());
    }

    #[test]
    fn integration_compact_issue_session_returns_report_and_keeps_lineage() {
        let dir = unique_test_dir("integration-compact");
        let session_path = dir.join("issue.md");

        let mut store = SessionStore::load(&session_path).expect("load session store");
        let head = store
            .ensure_initialized("system prompt")
            .expect("initialize store");
        store
            .append_messages(
                head,
                &[Message::user("hello"), Message::assistant_text("world")],
            )
            .expect("append messages");

        let report = compact_issue_session(&session_path, 1, 30_000).expect("compact session");

        assert!(report.retained_entries >= 1);
        assert!(report.head_id.is_some());
    }

    #[test]
    fn regression_reset_issue_session_files_is_noop_for_missing_files() {
        let dir = unique_test_dir("regression-reset-noop");
        let session_path = dir.join("missing.md");

        let (removed_session, removed_lock) =
            reset_issue_session_files(&session_path).expect("reset missing files");

        assert!(!removed_session);
        assert!(!removed_lock);
    }
}
