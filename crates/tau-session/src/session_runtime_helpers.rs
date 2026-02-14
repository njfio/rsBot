//! Session initialization and validation helpers shared by runtime entrypoints.

use std::path::Path;

use anyhow::{bail, Result};
use tau_ai::Message;

use crate::{SessionRuntime, SessionStore};

#[derive(Debug)]
/// Public struct `SessionInitializationOutcome` used across Tau components.
pub struct SessionInitializationOutcome {
    pub runtime: SessionRuntime,
    pub lineage: Vec<Message>,
}

pub fn validate_session_file(session_path: &Path, no_session: bool) -> Result<()> {
    if no_session {
        bail!("--session-validate cannot be used together with --no-session");
    }

    let store = SessionStore::load(session_path)?;
    let report = store.validation_report();
    println!(
        "session validation: path={} entries={} duplicates={} invalid_parent={} cycles={}",
        session_path.display(),
        report.entries,
        report.duplicates,
        report.invalid_parent,
        report.cycles
    );
    if report.is_valid() {
        println!("session validation passed");
        Ok(())
    } else {
        bail!(
            "session validation failed: duplicates={} invalid_parent={} cycles={}",
            report.duplicates,
            report.invalid_parent,
            report.cycles
        );
    }
}

pub fn initialize_session(
    session_path: &Path,
    lock_wait_ms: u64,
    lock_stale_ms: u64,
    branch_from: Option<u64>,
    system_prompt: &str,
) -> Result<SessionInitializationOutcome> {
    let mut store = SessionStore::load(session_path)?;
    store.set_lock_policy(lock_wait_ms.max(1), lock_stale_ms);

    let mut active_head = store.ensure_initialized(system_prompt)?;
    if let Some(branch_id) = branch_from {
        if !store.contains(branch_id) {
            bail!(
                "session {} does not contain entry id {}",
                store.path().display(),
                branch_id
            );
        }
        active_head = Some(branch_id);
    }

    let lineage = store.lineage_messages(active_head)?;

    Ok(SessionInitializationOutcome {
        runtime: SessionRuntime { store, active_head },
        lineage,
    })
}

pub fn format_id_list(ids: &[u64]) -> String {
    if ids.is_empty() {
        return "none".to_string();
    }
    ids.iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

pub fn format_remap_ids(remapped: &[(u64, u64)]) -> String {
    if remapped.is_empty() {
        return "none".to_string();
    }
    remapped
        .iter()
        .map(|(from, to)| format!("{from}->{to}"))
        .collect::<Vec<_>>()
        .join(",")
}

pub fn session_lineage_messages(runtime: &SessionRuntime) -> Result<Vec<Message>> {
    runtime.store.lineage_messages(runtime.active_head)
}
