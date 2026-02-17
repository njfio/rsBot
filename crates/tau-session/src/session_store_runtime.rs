//! Core `SessionStore` runtime and merge/import implementation.

use super::*;
use tracing::debug;

impl SessionStore {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("failed to create session directory {}", parent.display())
                })?;
            }
        }
        let resolved_backend = resolve_session_backend(&path)?;
        let backend = resolved_backend.backend;
        let backend_reason_code = resolved_backend.reason_code;
        let _imported_legacy_entries = maybe_import_legacy_jsonl_into_sqlite(&path, backend)?;
        let entries = read_session_entries(&path, backend)?;
        let usage_summary = read_session_usage_summary(&path, backend)?;
        let next_id = entries.iter().map(|entry| entry.id).max().unwrap_or(0) + 1;
        debug!(
            target: "tau.session",
            path = %path.display(),
            backend = %backend.label(),
            usage_total_tokens = usage_summary.total_tokens,
            usage_estimated_cost_usd = usage_summary.estimated_cost_usd,
            "loaded session store"
        );

        Ok(Self {
            path,
            backend,
            backend_reason_code,
            entries,
            usage_summary,
            next_id,
            lock_wait_ms: DEFAULT_LOCK_WAIT_MS,
            lock_stale_ms: DEFAULT_LOCK_STALE_MS,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn storage_backend(&self) -> SessionStorageBackend {
        self.backend
    }

    pub fn storage_backend_reason_code(&self) -> &str {
        self.backend_reason_code.as_str()
    }

    pub fn entries(&self) -> &[SessionEntry] {
        &self.entries
    }

    pub fn usage_summary(&self) -> SessionUsageSummary {
        self.usage_summary
    }

    pub fn head_id(&self) -> Option<u64> {
        self.entries.last().map(|entry| entry.id)
    }

    pub fn contains(&self, id: u64) -> bool {
        self.entries.iter().any(|entry| entry.id == id)
    }

    pub fn set_lock_policy(&mut self, lock_wait_ms: u64, lock_stale_ms: u64) {
        self.lock_wait_ms = lock_wait_ms.max(1);
        self.lock_stale_ms = lock_stale_ms;
    }

    pub fn ensure_initialized(&mut self, system_prompt: &str) -> Result<Option<u64>> {
        if !self.entries.is_empty() {
            return Ok(self.head_id());
        }

        if system_prompt.trim().is_empty() {
            return Ok(None);
        }

        let system_message = Message::system(system_prompt);
        self.append_messages(None, &[system_message])
    }

    pub fn append_messages(
        &mut self,
        mut parent_id: Option<u64>,
        messages: &[Message],
    ) -> Result<Option<u64>> {
        if messages.is_empty() {
            return Ok(parent_id);
        }

        let lock_path = self.lock_path();
        let _lock = acquire_lock(
            &lock_path,
            Duration::from_millis(self.lock_wait_ms),
            Duration::from_millis(self.lock_stale_ms),
        )?;

        let mut entries = read_session_entries(&self.path, self.backend)?;
        let mut next_id = entries.iter().map(|entry| entry.id).max().unwrap_or(0) + 1;

        if let Some(parent) = parent_id {
            if !entries.iter().any(|entry| entry.id == parent) {
                bail!("parent id {parent} does not exist in session");
            }
        }

        for message in messages {
            let entry = SessionEntry {
                id: next_id,
                parent_id,
                message: message.clone(),
            };
            next_id += 1;
            parent_id = Some(entry.id);
            entries.push(entry);
        }

        write_session_entries_atomic(&self.path, &entries, self.backend)?;
        self.entries = entries;
        self.next_id = next_id;

        Ok(parent_id)
    }

    pub fn record_usage_delta(&mut self, delta: SessionUsageSummary) -> Result<()> {
        if delta.input_tokens == 0
            && delta.output_tokens == 0
            && delta.total_tokens == 0
            && delta.estimated_cost_usd.abs() <= f64::EPSILON
        {
            return Ok(());
        }

        let lock_path = self.lock_path();
        let _lock = acquire_lock(
            &lock_path,
            Duration::from_millis(self.lock_wait_ms),
            Duration::from_millis(self.lock_stale_ms),
        )?;

        let mut summary = read_session_usage_summary(&self.path, self.backend)?;
        summary.input_tokens = summary.input_tokens.saturating_add(delta.input_tokens);
        summary.output_tokens = summary.output_tokens.saturating_add(delta.output_tokens);
        summary.total_tokens = summary.total_tokens.saturating_add(delta.total_tokens);
        summary.estimated_cost_usd += delta.estimated_cost_usd.max(0.0);

        write_session_usage_summary_atomic(&self.path, &summary, self.backend)?;
        self.usage_summary = summary;
        debug!(
            target: "tau.session",
            path = %self.path.display(),
            delta_input_tokens = delta.input_tokens,
            delta_output_tokens = delta.output_tokens,
            delta_total_tokens = delta.total_tokens,
            delta_estimated_cost_usd = delta.estimated_cost_usd,
            cumulative_total_tokens = self.usage_summary.total_tokens,
            cumulative_estimated_cost_usd = self.usage_summary.estimated_cost_usd,
            "recorded session usage delta"
        );
        Ok(())
    }

    pub fn lineage_messages(&self, head_id: Option<u64>) -> Result<Vec<Message>> {
        Ok(self
            .lineage_entries(head_id)?
            .into_iter()
            .map(|entry| entry.message)
            .collect())
    }

    pub fn lineage_entries(&self, head_id: Option<u64>) -> Result<Vec<SessionEntry>> {
        let Some(mut current_id) = head_id else {
            return Ok(Vec::new());
        };

        let mut lineage = Vec::new();
        let mut visited = HashSet::new();

        loop {
            if !visited.insert(current_id) {
                bail!("detected a cycle while resolving session lineage at id {current_id}");
            }

            let entry = self
                .entries
                .iter()
                .find(|entry| entry.id == current_id)
                .ok_or_else(|| anyhow!("unknown session id {current_id}"))?;

            lineage.push(entry.clone());
            match entry.parent_id {
                Some(parent) => current_id = parent,
                None => break,
            }
        }
        lineage.reverse();
        Ok(lineage)
    }

    pub fn export_lineage(
        &self,
        head_id: Option<u64>,
        destination: impl AsRef<Path>,
    ) -> Result<usize> {
        let lineage = self.lineage_entries(head_id)?;
        let export_path = destination.as_ref();
        let export_backend = resolve_session_backend(export_path)?.backend;
        write_session_entries_atomic(export_path, &lineage, export_backend)?;
        Ok(lineage.len())
    }

    pub fn export_lineage_jsonl(&self, head_id: Option<u64>) -> Result<String> {
        let lineage = self.lineage_entries(head_id)?;
        let mut lines = Vec::with_capacity(lineage.len() + 1);
        let meta = SessionRecord::Meta(SessionMetaRecord {
            schema_version: SESSION_SCHEMA_VERSION,
        });
        lines.push(serde_json::to_string(&meta)?);
        for entry in lineage {
            lines.push(serde_json::to_string(&SessionRecord::Entry(entry))?);
        }
        Ok(lines.join("\n"))
    }

    pub fn import_snapshot(
        &mut self,
        source: impl AsRef<Path>,
        mode: SessionImportMode,
    ) -> Result<ImportReport> {
        let source_path = source.as_ref();
        let source_backend = resolve_session_backend(source_path)?.backend;
        let imported_entries = read_session_entries(source_path, source_backend)?;
        let source_report = validation_report_for_entries(&imported_entries);
        if !source_report.is_valid() {
            bail!(
                "import session validation failed: path={} entries={} duplicates={} invalid_parent={} cycles={}",
                source_path.display(),
                source_report.entries,
                source_report.duplicates,
                source_report.invalid_parent,
                source_report.cycles
            );
        }

        let lock_path = self.lock_path();
        let _lock = acquire_lock(
            &lock_path,
            Duration::from_millis(self.lock_wait_ms),
            Duration::from_millis(self.lock_stale_ms),
        )?;

        let existing_entries = read_session_entries(&self.path, self.backend)?;
        let report = match mode {
            SessionImportMode::Merge => {
                let (merged_entries, remapped_ids, active_head) =
                    merge_entries_with_remap(&existing_entries, &imported_entries)?;
                write_session_entries_atomic(&self.path, &merged_entries, self.backend)?;
                self.entries = merged_entries;
                ImportReport {
                    imported_entries: imported_entries.len(),
                    remapped_entries: remapped_ids.len(),
                    remapped_ids,
                    replaced_entries: 0,
                    resulting_entries: self.entries.len(),
                    active_head,
                }
            }
            SessionImportMode::Replace => {
                write_session_entries_atomic(&self.path, &imported_entries, self.backend)?;
                self.entries = imported_entries;
                ImportReport {
                    imported_entries: self.entries.len(),
                    remapped_entries: 0,
                    remapped_ids: Vec::new(),
                    replaced_entries: existing_entries.len(),
                    resulting_entries: self.entries.len(),
                    active_head: self.entries.last().map(|entry| entry.id),
                }
            }
        };

        self.next_id = self.entries.iter().map(|entry| entry.id).max().unwrap_or(0) + 1;
        Ok(report)
    }

    pub fn merge_branches(
        &mut self,
        source_head: u64,
        target_head: u64,
        strategy: SessionMergeStrategy,
    ) -> Result<BranchMergeReport> {
        let lock_path = self.lock_path();
        let _lock = acquire_lock(
            &lock_path,
            Duration::from_millis(self.lock_wait_ms),
            Duration::from_millis(self.lock_stale_ms),
        )?;

        let mut entries = read_session_entries(&self.path, self.backend)?;
        let entry_by_id = entries
            .iter()
            .cloned()
            .map(|entry| (entry.id, entry))
            .collect::<HashMap<_, _>>();

        if !entry_by_id.contains_key(&source_head) {
            bail!("unknown source session id {}", source_head);
        }
        if !entry_by_id.contains_key(&target_head) {
            bail!("unknown target session id {}", target_head);
        }
        if source_head == target_head {
            bail!("source and target session ids must differ");
        }

        let source_lineage = lineage_ids_root_to_head(&entry_by_id, source_head)?;
        let target_lineage = lineage_ids_root_to_head(&entry_by_id, target_head)?;

        let source_set = source_lineage.iter().copied().collect::<HashSet<_>>();
        let mut common_ancestor = None;
        for id in &target_lineage {
            if source_set.contains(id) {
                common_ancestor = Some(*id);
            }
        }

        let source_suffix_start = common_ancestor
            .and_then(|id| source_lineage.iter().position(|entry_id| *entry_id == id))
            .map(|index| index + 1)
            .unwrap_or(0);
        let source_unique_ids = source_lineage
            .iter()
            .skip(source_suffix_start)
            .copied()
            .collect::<Vec<_>>();

        let mut appended_entries = 0usize;
        let merged_head = match strategy {
            SessionMergeStrategy::FastForward => {
                if !source_lineage.contains(&target_head) {
                    bail!(
                        "cannot fast-forward target {} to source {} because target is not an ancestor",
                        target_head,
                        source_head
                    );
                }
                source_head
            }
            SessionMergeStrategy::Append => {
                let mut parent_id = Some(target_head);
                let mut next_id = entries.iter().map(|entry| entry.id).max().unwrap_or(0) + 1;
                for source_id in source_unique_ids {
                    let source_entry = entry_by_id
                        .get(&source_id)
                        .ok_or_else(|| anyhow!("missing source session id {}", source_id))?;
                    let new_entry = SessionEntry {
                        id: next_id,
                        parent_id,
                        message: source_entry.message.clone(),
                    };
                    parent_id = Some(new_entry.id);
                    entries.push(new_entry);
                    next_id += 1;
                    appended_entries += 1;
                }
                parent_id.unwrap_or(target_head)
            }
            SessionMergeStrategy::Squash => {
                let mut parent_id = Some(target_head);
                if source_unique_ids.is_empty() {
                    parent_id.unwrap_or(target_head)
                } else {
                    let next_id = entries.iter().map(|entry| entry.id).max().unwrap_or(0) + 1;
                    let summary = render_squash_merge_summary(
                        &entry_by_id,
                        source_head,
                        target_head,
                        &source_unique_ids,
                    )?;
                    let new_entry = SessionEntry {
                        id: next_id,
                        parent_id,
                        message: Message::assistant_text(summary),
                    };
                    parent_id = Some(new_entry.id);
                    entries.push(new_entry);
                    appended_entries = 1;
                    parent_id.unwrap_or(target_head)
                }
            }
        };

        if appended_entries > 0 {
            write_session_entries_atomic(&self.path, &entries, self.backend)?;
        }
        self.entries = entries;
        self.next_id = self.entries.iter().map(|entry| entry.id).max().unwrap_or(0) + 1;

        Ok(BranchMergeReport {
            source_head,
            target_head,
            strategy,
            common_ancestor,
            appended_entries,
            merged_head,
        })
    }

    pub fn validation_report(&self) -> SessionValidationReport {
        validation_report_for_entries(&self.entries)
    }

    pub fn branch_tips(&self) -> Vec<&SessionEntry> {
        let mut parent_ids = HashSet::new();
        for entry in &self.entries {
            if let Some(parent_id) = entry.parent_id {
                parent_ids.insert(parent_id);
            }
        }

        let mut tips = self
            .entries
            .iter()
            .filter(|entry| !parent_ids.contains(&entry.id))
            .collect::<Vec<_>>();
        tips.sort_by_key(|entry| entry.id);
        tips
    }

    pub fn repair(&mut self) -> Result<RepairReport> {
        let lock_path = self.lock_path();
        let _lock = acquire_lock(
            &lock_path,
            Duration::from_millis(self.lock_wait_ms),
            Duration::from_millis(self.lock_stale_ms),
        )?;

        let mut entries = read_session_entries(&self.path, self.backend)?;
        entries.sort_by_key(|entry| entry.id);

        let mut report = RepairReport::default();
        let mut unique = Vec::new();
        let mut seen = HashSet::new();
        for entry in entries {
            if seen.insert(entry.id) {
                unique.push(entry);
            } else {
                report.removed_duplicates += 1;
                report.duplicate_ids.push(entry.id);
            }
        }

        let mut id_to_entry: HashMap<u64, SessionEntry> = unique
            .iter()
            .cloned()
            .map(|entry| (entry.id, entry))
            .collect();

        // Remove entries with missing parents.
        loop {
            let before = id_to_entry.len();
            let invalid_ids = id_to_entry
                .values()
                .filter_map(|entry| match entry.parent_id {
                    Some(parent_id) if !id_to_entry.contains_key(&parent_id) => Some(entry.id),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let mut invalid_ids = invalid_ids;
            invalid_ids.sort_unstable();
            invalid_ids.dedup();

            if invalid_ids.is_empty() {
                break;
            }

            for id in invalid_ids {
                if id_to_entry.remove(&id).is_some() {
                    report.removed_invalid_parent += 1;
                    report.invalid_parent_ids.push(id);
                }
            }

            if id_to_entry.len() == before {
                break;
            }
        }

        // Remove cycles by walking each entry lineage.
        let cycle_ids = id_to_entry
            .keys()
            .copied()
            .filter(|id| has_cycle(*id, &id_to_entry))
            .collect::<Vec<_>>();
        let mut cycle_ids = cycle_ids;
        cycle_ids.sort_unstable();
        cycle_ids.dedup();
        for id in cycle_ids {
            if id_to_entry.remove(&id).is_some() {
                report.removed_cycles += 1;
                report.cycle_ids.push(id);
            }
        }

        let mut repaired = id_to_entry.into_values().collect::<Vec<_>>();
        repaired.sort_by_key(|entry| entry.id);

        write_session_entries_atomic(&self.path, &repaired, self.backend)?;
        self.entries = repaired;
        self.next_id = self.entries.iter().map(|entry| entry.id).max().unwrap_or(0) + 1;

        Ok(report)
    }

    pub fn compact_to_lineage(&mut self, preferred_head_id: Option<u64>) -> Result<CompactReport> {
        let lock_path = self.lock_path();
        let _lock = acquire_lock(
            &lock_path,
            Duration::from_millis(self.lock_wait_ms),
            Duration::from_millis(self.lock_stale_ms),
        )?;

        let entries = read_session_entries(&self.path, self.backend)?;
        if entries.is_empty() {
            self.entries = entries;
            self.next_id = 1;
            return Ok(CompactReport {
                removed_entries: 0,
                retained_entries: 0,
                head_id: None,
            });
        }

        let head_id = preferred_head_id.or_else(|| entries.last().map(|entry| entry.id));
        let Some(head_id) = head_id else {
            self.entries = entries;
            self.next_id = 1;
            return Ok(CompactReport {
                removed_entries: 0,
                retained_entries: 0,
                head_id: None,
            });
        };

        let lineage_ids = collect_lineage_ids(&entries, head_id)?;
        let compacted = entries
            .iter()
            .filter(|entry| lineage_ids.contains(&entry.id))
            .cloned()
            .collect::<Vec<_>>();

        let removed_entries = entries.len().saturating_sub(compacted.len());
        write_session_entries_atomic(&self.path, &compacted, self.backend)?;
        self.entries = compacted;
        self.next_id = self.entries.iter().map(|entry| entry.id).max().unwrap_or(0) + 1;

        Ok(CompactReport {
            removed_entries,
            retained_entries: self.entries.len(),
            head_id: Some(head_id),
        })
    }

    fn lock_path(&self) -> PathBuf {
        self.path.with_extension("lock")
    }
}

fn lineage_ids_root_to_head(
    entries: &HashMap<u64, SessionEntry>,
    head_id: u64,
) -> Result<Vec<u64>> {
    let mut lineage = Vec::new();
    let mut visited = HashSet::new();
    let mut current_id = head_id;

    loop {
        if !visited.insert(current_id) {
            bail!(
                "detected a cycle while resolving session lineage at id {}",
                current_id
            );
        }

        let entry = entries
            .get(&current_id)
            .ok_or_else(|| anyhow!("unknown session id {}", current_id))?;
        lineage.push(current_id);
        match entry.parent_id {
            Some(parent_id) => current_id = parent_id,
            None => break,
        }
    }

    lineage.reverse();
    Ok(lineage)
}

fn render_squash_merge_summary(
    entry_by_id: &HashMap<u64, SessionEntry>,
    source_head: u64,
    target_head: u64,
    source_unique_ids: &[u64],
) -> Result<String> {
    let mut lines = vec![format!(
        "squash merge: source={} target={} entries={}",
        source_head,
        target_head,
        source_unique_ids.len()
    )];

    for source_id in source_unique_ids.iter().take(6) {
        let entry = entry_by_id
            .get(source_id)
            .ok_or_else(|| anyhow!("missing source session id {}", source_id))?;
        let role = format!("{:?}", entry.message.role).to_ascii_lowercase();
        let mut preview = entry.message.text_content().replace('\n', " ");
        if preview.trim().is_empty() {
            preview = "(no text)".to_string();
        }
        if preview.chars().count() > 72 {
            preview = format!("{}...", preview.chars().take(72).collect::<String>());
        }
        lines.push(format!("- {}: {}", role, preview));
    }

    if source_unique_ids.len() > 6 {
        lines.push(format!(
            "- ... {} additional entries",
            source_unique_ids.len() - 6
        ));
    }

    Ok(lines.join("\n"))
}
