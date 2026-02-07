use super::*;

pub(crate) fn validate_session_file(cli: &Cli) -> Result<()> {
    if cli.no_session {
        bail!("--session-validate cannot be used together with --no-session");
    }

    let store = SessionStore::load(&cli.session)?;
    let report = store.validation_report();
    println!(
        "session validation: path={} entries={} duplicates={} invalid_parent={} cycles={}",
        cli.session.display(),
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

pub(crate) fn initialize_session(
    agent: &mut Agent,
    cli: &Cli,
    system_prompt: &str,
) -> Result<SessionRuntime> {
    let mut store = SessionStore::load(&cli.session)?;
    store.set_lock_policy(cli.session_lock_wait_ms.max(1), cli.session_lock_stale_ms);

    let mut active_head = store.ensure_initialized(system_prompt)?;
    if let Some(branch_id) = cli.branch_from {
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
    if !lineage.is_empty() {
        agent.replace_messages(lineage);
    }

    Ok(SessionRuntime { store, active_head })
}

pub(crate) fn format_id_list(ids: &[u64]) -> String {
    if ids.is_empty() {
        return "none".to_string();
    }
    ids.iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

pub(crate) fn format_remap_ids(remapped: &[(u64, u64)]) -> String {
    if remapped.is_empty() {
        return "none".to_string();
    }
    remapped
        .iter()
        .map(|(from, to)| format!("{from}->{to}"))
        .collect::<Vec<_>>()
        .join(",")
}

pub(crate) fn reload_agent_from_active_head(
    agent: &mut Agent,
    runtime: &SessionRuntime,
) -> Result<()> {
    let lineage = runtime.store.lineage_messages(runtime.active_head)?;
    agent.replace_messages(lineage);
    Ok(())
}
