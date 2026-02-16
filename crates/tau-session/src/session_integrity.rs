//! Session lineage/integrity helpers (validation, cycle detection, import remap).
use super::*;

/// Detect whether lineage traversal from start id forms a cycle.
pub(super) fn has_cycle(start_id: u64, entries: &HashMap<u64, SessionEntry>) -> bool {
    let mut visited = HashSet::new();
    let mut current = Some(start_id);

    while let Some(id) = current {
        if !visited.insert(id) {
            return true;
        }

        current = entries.get(&id).and_then(|entry| entry.parent_id);
    }

    false
}

/// Collect lineage ids reachable from head, failing on unknown ids or cycles.
pub(super) fn collect_lineage_ids(entries: &[SessionEntry], head_id: u64) -> Result<HashSet<u64>> {
    let id_to_entry = entries
        .iter()
        .cloned()
        .map(|entry| (entry.id, entry))
        .collect::<HashMap<_, _>>();
    if !id_to_entry.contains_key(&head_id) {
        bail!("unknown session id {head_id}");
    }

    let mut lineage_ids = HashSet::new();
    let mut visited = HashSet::new();
    let mut current_id = head_id;
    loop {
        if !visited.insert(current_id) {
            bail!("detected a cycle while compacting session lineage at id {current_id}");
        }

        let entry = id_to_entry
            .get(&current_id)
            .ok_or_else(|| anyhow!("unknown session id {current_id}"))?;
        lineage_ids.insert(entry.id);

        match entry.parent_id {
            Some(parent_id) => {
                if !id_to_entry.contains_key(&parent_id) {
                    bail!("missing parent id {parent_id} while compacting");
                }
                current_id = parent_id;
            }
            None => break,
        }
    }

    Ok(lineage_ids)
}

pub(super) fn validation_report_for_entries(entries: &[SessionEntry]) -> SessionValidationReport {
    let mut report = SessionValidationReport {
        entries: entries.len(),
        ..SessionValidationReport::default()
    };

    let mut seen = HashSet::new();
    for entry in entries {
        if !seen.insert(entry.id) {
            report.duplicates += 1;
        }
    }

    let id_to_entry = entries
        .iter()
        .cloned()
        .map(|entry| (entry.id, entry))
        .collect::<HashMap<_, _>>();

    for entry in entries {
        if let Some(parent_id) = entry.parent_id {
            if !id_to_entry.contains_key(&parent_id) {
                report.invalid_parent += 1;
            }
        }
    }

    let mut cycle_ids = HashSet::new();
    for entry in entries {
        if has_cycle(entry.id, &id_to_entry) {
            cycle_ids.insert(entry.id);
        }
    }
    report.cycles = cycle_ids.len();

    report
}

pub(super) type MergeImportResult = (Vec<SessionEntry>, Vec<(u64, u64)>, Option<u64>);

/// Merge imported entries into existing session graph with id remapping on collisions.
pub(super) fn merge_entries_with_remap(
    existing_entries: &[SessionEntry],
    imported_entries: &[SessionEntry],
) -> Result<MergeImportResult> {
    let mut merged = existing_entries.to_vec();
    if imported_entries.is_empty() {
        let active_head = merged.last().map(|entry| entry.id);
        return Ok((merged, Vec::new(), active_head));
    }

    let mut used_ids = existing_entries
        .iter()
        .map(|entry| entry.id)
        .collect::<HashSet<_>>();
    let mut next_id = used_ids.iter().max().copied().unwrap_or(0) + 1;
    let mut remapped_ids = Vec::new();
    let mut id_map = HashMap::with_capacity(imported_entries.len());

    for entry in imported_entries {
        let mapped_id = if used_ids.contains(&entry.id) {
            let replacement = next_id;
            next_id += 1;
            remapped_ids.push((entry.id, replacement));
            replacement
        } else {
            entry.id
        };
        used_ids.insert(mapped_id);
        id_map.insert(entry.id, mapped_id);
    }

    for entry in imported_entries {
        let mapped_id = *id_map
            .get(&entry.id)
            .ok_or_else(|| anyhow!("missing remap id for {}", entry.id))?;
        let mapped_parent_id = entry
            .parent_id
            .map(|parent_id| {
                id_map
                    .get(&parent_id)
                    .copied()
                    .ok_or_else(|| anyhow!("missing remap parent id for {}", parent_id))
            })
            .transpose()?;
        merged.push(SessionEntry {
            id: mapped_id,
            parent_id: mapped_parent_id,
            message: entry.message.clone(),
        });
    }

    let active_head = imported_entries
        .last()
        .and_then(|entry| id_map.get(&entry.id).copied());

    Ok((merged, remapped_ids, active_head))
}
