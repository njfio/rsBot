//! Session store tests covering unit, functional, integration, and regression cases.
use std::{collections::HashSet, fs, path::PathBuf, sync::Arc, thread, time::Duration};

use tempfile::tempdir;

use super::{
    acquire_lock, CompactReport, RepairReport, SessionEntry, SessionImportMode,
    SessionMergeStrategy, SessionRecord, SessionStorageBackend, SessionStore,
    SessionValidationReport,
};

#[test]
fn appends_and_restores_lineage() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("session.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");
    let head = store
        .append_messages(
            head,
            &[
                tau_ai::Message::user("hello"),
                tau_ai::Message::assistant_text("hi"),
            ],
        )
        .expect("append");

    let lineage = store.lineage_messages(head).expect("lineage");
    assert_eq!(lineage.len(), 3);
    assert_eq!(lineage[0].text_content(), "sys");

    let reloaded = SessionStore::load(&path).expect("reload");
    assert_eq!(reloaded.entries().len(), 3);
}

#[test]
fn supports_branching_from_older_id() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("session.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");
    let head = store
        .append_messages(
            head,
            &[
                tau_ai::Message::user("q1"),
                tau_ai::Message::assistant_text("a1"),
                tau_ai::Message::user("q2"),
                tau_ai::Message::assistant_text("a2"),
            ],
        )
        .expect("append");

    let branch_from = Some(head.expect("head") - 2);
    let branch_head = store
        .append_messages(
            branch_from,
            &[
                tau_ai::Message::user("q2b"),
                tau_ai::Message::assistant_text("a2b"),
            ],
        )
        .expect("append");

    let lineage = store.lineage_messages(branch_head).expect("lineage");
    let texts = lineage
        .iter()
        .map(|message| message.text_content())
        .collect::<Vec<_>>();
    assert_eq!(texts, vec!["sys", "q1", "a1", "q2b", "a2b"]);
    assert_eq!(store.branch_tips().len(), 2);
}

#[test]
fn unit_merge_branches_append_replays_source_unique_entries_on_target_head() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("session-merge-append.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append")
        .expect("root");
    let main_tip = store
        .append_messages(
            Some(root),
            &[
                tau_ai::Message::user("main-u1"),
                tau_ai::Message::assistant_text("main-a1"),
            ],
        )
        .expect("append main")
        .expect("main tip");
    let branch_tip = store
        .append_messages(
            Some(root),
            &[
                tau_ai::Message::user("branch-u1"),
                tau_ai::Message::assistant_text("branch-a1"),
            ],
        )
        .expect("append branch")
        .expect("branch tip");

    let before = store.entries().len();
    let report = store
        .merge_branches(branch_tip, main_tip, SessionMergeStrategy::Append)
        .expect("append merge should succeed");

    assert_eq!(report.strategy, SessionMergeStrategy::Append);
    assert_eq!(report.common_ancestor, Some(root));
    assert_eq!(report.appended_entries, 2);
    assert!(report.merged_head > main_tip);
    assert_eq!(store.entries().len(), before + 2);
    let merged_lineage = store
        .lineage_messages(Some(report.merged_head))
        .expect("merged lineage should resolve")
        .into_iter()
        .map(|message| message.text_content())
        .collect::<Vec<_>>();
    assert_eq!(
        merged_lineage,
        vec!["sys", "main-u1", "main-a1", "branch-u1", "branch-a1"]
    );
}

#[test]
fn functional_merge_branches_squash_creates_single_summary_entry() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("session-merge-squash.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append")
        .expect("root");
    let target = store
        .append_messages(Some(root), &[tau_ai::Message::user("target-u1")])
        .expect("append target")
        .expect("target tip");
    let source = store
        .append_messages(
            Some(root),
            &[
                tau_ai::Message::user("source-u1"),
                tau_ai::Message::assistant_text("source-a1"),
            ],
        )
        .expect("append source")
        .expect("source tip");

    let report = store
        .merge_branches(source, target, SessionMergeStrategy::Squash)
        .expect("squash merge should succeed");

    assert_eq!(report.appended_entries, 1);
    let merged = store
        .lineage_messages(Some(report.merged_head))
        .expect("merged lineage should resolve");
    let merged_tail = merged.last().expect("tail message should exist");
    assert!(merged_tail.text_content().contains("squash merge:"));
    assert!(merged_tail.text_content().contains("- user: source-u1"));
}

#[test]
fn integration_merge_branches_fast_forward_moves_head_without_writes() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("session-merge-fast-forward.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append")
        .expect("root");
    let first = store
        .append_messages(Some(root), &[tau_ai::Message::user("u1")])
        .expect("append")
        .expect("first");
    let source = store
        .append_messages(Some(first), &[tau_ai::Message::assistant_text("a1")])
        .expect("append")
        .expect("source");
    let before_ids = store
        .entries()
        .iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    let report = store
        .merge_branches(source, first, SessionMergeStrategy::FastForward)
        .expect("fast-forward should succeed");

    assert_eq!(report.appended_entries, 0);
    assert_eq!(report.merged_head, source);
    let after_ids = store
        .entries()
        .iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();
    assert_eq!(after_ids, before_ids);
}

#[test]
fn regression_merge_branches_fast_forward_rejects_diverged_branches() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("session-merge-fast-forward-error.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append")
        .expect("root");
    let left = store
        .append_messages(Some(root), &[tau_ai::Message::user("left")])
        .expect("append left")
        .expect("left");
    let right = store
        .append_messages(Some(root), &[tau_ai::Message::user("right")])
        .expect("append right")
        .expect("right");

    let error = store
        .merge_branches(right, left, SessionMergeStrategy::FastForward)
        .expect_err("fast-forward should fail");
    assert!(error.to_string().contains("cannot fast-forward target"));
}

#[test]
fn functional_export_lineage_writes_schema_valid_snapshot() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("session.jsonl");
    let export = temp.path().join("export.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");
    let head = store
        .append_messages(
            head,
            &[
                tau_ai::Message::user("q1"),
                tau_ai::Message::assistant_text("a1"),
            ],
        )
        .expect("append");

    let exported = store
        .export_lineage(head, &export)
        .expect("lineage export should succeed");
    assert_eq!(exported, 3);

    let snapshot = SessionStore::load(&export).expect("load export");
    assert_eq!(snapshot.entries().len(), 3);
    assert_eq!(snapshot.entries()[0].message.text_content(), "sys");
    assert_eq!(snapshot.entries()[1].message.text_content(), "q1");
    assert_eq!(snapshot.entries()[2].message.text_content(), "a1");
}

#[test]
fn unit_export_lineage_jsonl_includes_meta_and_entries() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("session.jsonl");
    let mut store = SessionStore::load(&path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");
    store
        .append_messages(head, &[tau_ai::Message::user("q1")])
        .expect("append");

    let snapshot = store
        .export_lineage_jsonl(store.head_id())
        .expect("export jsonl");
    let lines = snapshot.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3);
    let meta: SessionRecord = serde_json::from_str(lines[0]).expect("meta");
    assert!(matches!(meta, SessionRecord::Meta(_)));
    let entry: SessionRecord = serde_json::from_str(lines[1]).expect("entry");
    assert!(matches!(entry, SessionRecord::Entry(_)));
}

#[test]
fn unit_import_snapshot_merge_remaps_colliding_ids() {
    let temp = tempdir().expect("tempdir");
    let target = temp.path().join("target.jsonl");
    let source = temp.path().join("source.jsonl");

    let mut target_store = SessionStore::load(&target).expect("load target");
    let target_head = target_store
        .append_messages(None, &[tau_ai::Message::system("target-root")])
        .expect("append target root");
    target_store
        .append_messages(target_head, &[tau_ai::Message::user("target-user")])
        .expect("append target user");

    let mut source_store = SessionStore::load(&source).expect("load source");
    let source_head = source_store
        .append_messages(None, &[tau_ai::Message::system("import-root")])
        .expect("append source root");
    source_store
        .append_messages(
            source_head,
            &[tau_ai::Message::assistant_text("import-assistant")],
        )
        .expect("append source assistant");

    let report = target_store
        .import_snapshot(&source, SessionImportMode::Merge)
        .expect("merge import");

    assert_eq!(report.imported_entries, 2);
    assert_eq!(report.remapped_entries, 2);
    assert_eq!(report.remapped_ids, vec![(1, 3), (2, 4)]);
    assert_eq!(report.replaced_entries, 0);
    assert_eq!(report.resulting_entries, 4);
    assert_eq!(report.active_head, Some(4));

    let entries = target_store.entries();
    assert_eq!(entries[2].id, 3);
    assert_eq!(entries[2].parent_id, None);
    assert_eq!(entries[2].message.text_content(), "import-root");
    assert_eq!(entries[3].id, 4);
    assert_eq!(entries[3].parent_id, Some(3));
    assert_eq!(entries[3].message.text_content(), "import-assistant");
}

#[test]
fn integration_export_import_roundtrip_merge_produces_valid_session_graph() {
    let temp = tempdir().expect("tempdir");
    let source_path = temp.path().join("roundtrip-source.jsonl");
    let target_path = temp.path().join("roundtrip-target.jsonl");
    let export_path = temp.path().join("roundtrip-export.jsonl");

    let mut source_store = SessionStore::load(&source_path).expect("load source");
    let source_head = source_store
        .append_messages(None, &[tau_ai::Message::system("source-root")])
        .expect("append source root");
    let source_head = source_store
        .append_messages(
            source_head,
            &[
                tau_ai::Message::user("source-user"),
                tau_ai::Message::assistant_text("source-assistant"),
            ],
        )
        .expect("append source branch");
    source_store
        .export_lineage(source_head, &export_path)
        .expect("export source lineage");

    let mut target_store = SessionStore::load(&target_path).expect("load target");
    target_store
        .append_messages(None, &[tau_ai::Message::system("target-root")])
        .expect("append target");

    let report = target_store
        .import_snapshot(&export_path, SessionImportMode::Merge)
        .expect("import merge");
    assert_eq!(report.imported_entries, 3);
    assert_eq!(report.remapped_entries, 3);
    assert_eq!(report.remapped_ids, vec![(1, 2), (2, 3), (3, 4)]);
    assert_eq!(report.resulting_entries, 4);
    assert!(target_store.validation_report().is_valid());

    let lineage = target_store
        .lineage_messages(report.active_head)
        .expect("lineage from imported head");
    assert_eq!(
        lineage
            .iter()
            .map(|message| message.text_content())
            .collect::<Vec<_>>(),
        vec!["source-root", "source-user", "source-assistant"]
    );
}

#[test]
fn functional_export_import_roundtrip_replace_overwrites_target_graph() {
    let temp = tempdir().expect("tempdir");
    let source_path = temp.path().join("replace-roundtrip-source.jsonl");
    let target_path = temp.path().join("replace-roundtrip-target.jsonl");
    let export_path = temp.path().join("replace-roundtrip-export.jsonl");

    let mut source_store = SessionStore::load(&source_path).expect("load source");
    let source_head = source_store
        .append_messages(None, &[tau_ai::Message::system("source-root")])
        .expect("append source");
    source_store
        .append_messages(source_head, &[tau_ai::Message::user("source-user")])
        .expect("append source user");
    source_store
        .export_lineage(source_store.head_id(), &export_path)
        .expect("export source");

    let mut target_store = SessionStore::load(&target_path).expect("load target");
    let target_head = target_store
        .append_messages(None, &[tau_ai::Message::system("target-root")])
        .expect("append target");
    target_store
        .append_messages(
            target_head,
            &[tau_ai::Message::assistant_text("target-assistant")],
        )
        .expect("append target assistant");

    let report = target_store
        .import_snapshot(&export_path, SessionImportMode::Replace)
        .expect("replace import");
    assert_eq!(report.imported_entries, 2);
    assert_eq!(report.replaced_entries, 2);
    assert_eq!(report.remapped_entries, 0);
    assert!(report.remapped_ids.is_empty());
    assert_eq!(target_store.entries().len(), 2);
    assert_eq!(
        target_store.entries()[0].message.text_content(),
        "source-root"
    );
    assert_eq!(
        target_store.entries()[1].message.text_content(),
        "source-user"
    );
    assert!(target_store.validation_report().is_valid());
}

#[test]
fn functional_import_snapshot_replace_overwrites_entries() {
    let temp = tempdir().expect("tempdir");
    let target = temp.path().join("replace-target.jsonl");
    let source = temp.path().join("replace-source.jsonl");

    let mut target_store = SessionStore::load(&target).expect("load target");
    let head = target_store
        .append_messages(None, &[tau_ai::Message::system("target-root")])
        .expect("append target root");
    target_store
        .append_messages(head, &[tau_ai::Message::user("target-user")])
        .expect("append target user");

    let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":10,"parent_id":null,"message":tau_ai::Message::system("source-root")}).to_string(),
            serde_json::json!({"record_type":"entry","id":11,"parent_id":10,"message":tau_ai::Message::user("source-user")}).to_string(),
        ]
        .join("\n");
    fs::write(&source, format!("{raw}\n")).expect("write source");

    let report = target_store
        .import_snapshot(&source, SessionImportMode::Replace)
        .expect("replace import");
    assert_eq!(report.imported_entries, 2);
    assert_eq!(report.remapped_entries, 0);
    assert!(report.remapped_ids.is_empty());
    assert_eq!(report.replaced_entries, 2);
    assert_eq!(report.resulting_entries, 2);
    assert_eq!(report.active_head, Some(11));
    assert_eq!(target_store.entries().len(), 2);
    assert_eq!(target_store.entries()[0].id, 10);
    assert_eq!(target_store.entries()[1].id, 11);

    let next = target_store
        .append_messages(
            target_store.head_id(),
            &[tau_ai::Message::assistant_text("next")],
        )
        .expect("append after replace");
    assert_eq!(next, Some(12));
}

#[test]
fn integration_import_snapshot_replace_with_empty_source_clears_session() {
    let temp = tempdir().expect("tempdir");
    let target = temp.path().join("replace-empty-target.jsonl");
    let source = temp.path().join("replace-empty-source.jsonl");

    let mut target_store = SessionStore::load(&target).expect("load target");
    target_store
        .append_messages(None, &[tau_ai::Message::system("target-root")])
        .expect("append target");
    fs::write(
        &source,
        format!(
            "{}\n",
            serde_json::json!({"record_type":"meta","schema_version":1})
        ),
    )
    .expect("write empty snapshot");

    let report = target_store
        .import_snapshot(&source, SessionImportMode::Replace)
        .expect("replace import");
    assert_eq!(report.imported_entries, 0);
    assert_eq!(report.remapped_entries, 0);
    assert!(report.remapped_ids.is_empty());
    assert_eq!(report.replaced_entries, 1);
    assert_eq!(report.resulting_entries, 0);
    assert_eq!(report.active_head, None);
    assert!(target_store.entries().is_empty());
}

#[test]
fn regression_import_snapshot_rejects_invalid_source_and_preserves_target() {
    let temp = tempdir().expect("tempdir");
    let target = temp.path().join("invalid-target.jsonl");
    let source = temp.path().join("invalid-source.jsonl");

    let mut target_store = SessionStore::load(&target).expect("load target");
    target_store
        .append_messages(None, &[tau_ai::Message::system("target-root")])
        .expect("append target");

    let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":2,"message":tau_ai::Message::system("cycle-a")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":1,"message":tau_ai::Message::user("cycle-b")}).to_string(),
        ]
        .join("\n");
    fs::write(&source, format!("{raw}\n")).expect("write invalid source");

    let error = target_store
        .import_snapshot(&source, SessionImportMode::Merge)
        .expect_err("invalid import should fail");
    assert!(error
        .to_string()
        .contains("import session validation failed"));
    assert_eq!(target_store.entries().len(), 1);
    assert_eq!(
        target_store.entries()[0].message.text_content(),
        "target-root"
    );
}

#[test]
fn unit_validation_report_detects_duplicates_invalid_parents_and_cycles() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("validate-invalid.jsonl");

    let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":null,"message":tau_ai::Message::system("root")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":99,"message":tau_ai::Message::user("dangling")}).to_string(),
            serde_json::json!({"record_type":"entry","id":3,"parent_id":4,"message":tau_ai::Message::user("cycle-a")}).to_string(),
            serde_json::json!({"record_type":"entry","id":4,"parent_id":3,"message":tau_ai::Message::user("cycle-b")}).to_string(),
            serde_json::json!({"record_type":"entry","id":6,"parent_id":1,"message":tau_ai::Message::user("duplicate-a")}).to_string(),
            serde_json::json!({"record_type":"entry","id":6,"parent_id":1,"message":tau_ai::Message::user("duplicate-b")}).to_string(),
        ]
        .join("\n");
    fs::write(&path, format!("{raw}\n")).expect("write invalid session");

    let store = SessionStore::load(&path).expect("load");
    let report = store.validation_report();
    assert_eq!(
        report,
        SessionValidationReport {
            entries: 6,
            duplicates: 1,
            invalid_parent: 1,
            cycles: 2,
        }
    );
    assert!(!report.is_valid());
}

#[test]
fn regression_validation_report_for_valid_session_is_clean() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("validate-valid.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");
    store
        .append_messages(
            head,
            &[
                tau_ai::Message::user("q1"),
                tau_ai::Message::assistant_text("a1"),
            ],
        )
        .expect("append");

    let report = store.validation_report();
    assert_eq!(
        report,
        SessionValidationReport {
            entries: 3,
            duplicates: 0,
            invalid_parent: 0,
            cycles: 0,
        }
    );
    assert!(report.is_valid());
}

#[test]
fn append_rejects_unknown_parent_id() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("session.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let error = store
        .append_messages(Some(42), &[tau_ai::Message::user("hello")])
        .expect_err("must fail for unknown parent");

    assert!(error
        .to_string()
        .contains("parent id 42 does not exist in session"));
}

#[test]
fn detects_cycles_in_session_lineage() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("session.jsonl");

    let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":2,"message":tau_ai::Message::system("sys")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":1,"message":tau_ai::Message::user("hello")}).to_string(),
        ]
        .join("\n");
    fs::write(&path, format!("{raw}\n")).expect("write session file");

    let store = SessionStore::load(&path).expect("load");
    let error = store
        .lineage_messages(Some(1))
        .expect_err("lineage should fail for cycle");
    assert!(error.to_string().contains("detected a cycle"));
}

#[test]
fn writes_schema_meta_record() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("session.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");

    let raw = fs::read_to_string(&path).expect("session must exist");
    let first_line = raw.lines().next().expect("first line");
    assert!(first_line.contains("\"record_type\":\"meta\""));
    assert!(first_line.contains("\"schema_version\":1"));
}

#[test]
fn loads_legacy_session_entry_lines_without_meta() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("legacy.jsonl");

    let legacy = SessionEntry {
        id: 1,
        parent_id: None,
        message: tau_ai::Message::system("legacy"),
    };
    fs::write(
        &path,
        format!("{}\n", serde_json::to_string(&legacy).expect("serialize")),
    )
    .expect("write legacy");

    let store = SessionStore::load(&path).expect("load legacy");
    assert_eq!(store.entries().len(), 1);
    assert_eq!(store.entries()[0].message.text_content(), "legacy");
}

#[test]
fn repair_removes_invalid_and_cycle_entries() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("repair.jsonl");

    let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":null,"message":tau_ai::Message::system("sys")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":99,"message":tau_ai::Message::user("dangling")}).to_string(),
            serde_json::json!({"record_type":"entry","id":3,"parent_id":4,"message":tau_ai::Message::user("cycle-a")}).to_string(),
            serde_json::json!({"record_type":"entry","id":4,"parent_id":3,"message":tau_ai::Message::user("cycle-b")}).to_string(),
        ]
        .join("\n");
    fs::write(&path, format!("{raw}\n")).expect("write malformed session");

    let mut store = SessionStore::load(&path).expect("load");
    let report = store.repair().expect("repair should succeed");

    assert_eq!(
        report,
        RepairReport {
            removed_duplicates: 0,
            duplicate_ids: Vec::new(),
            removed_invalid_parent: 1,
            invalid_parent_ids: vec![2],
            removed_cycles: 2,
            cycle_ids: vec![3, 4],
        }
    );
    assert_eq!(store.entries().len(), 1);
    assert_eq!(store.entries()[0].id, 1);
}

#[test]
fn functional_compact_to_lineage_prunes_other_branches() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("compact.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");
    let head = store
        .append_messages(
            head,
            &[
                tau_ai::Message::user("q1"),
                tau_ai::Message::assistant_text("a1"),
                tau_ai::Message::user("q2"),
                tau_ai::Message::assistant_text("a2"),
            ],
        )
        .expect("append")
        .expect("head");

    store
        .append_messages(
            Some(head - 2),
            &[
                tau_ai::Message::user("q2b"),
                tau_ai::Message::assistant_text("a2b"),
            ],
        )
        .expect("append branch");

    let report = store.compact_to_lineage(Some(head)).expect("compact");
    assert_eq!(
        report,
        CompactReport {
            removed_entries: 2,
            retained_entries: 5,
            head_id: Some(head),
        }
    );
    assert_eq!(store.entries().len(), 5);
    assert_eq!(store.branch_tips().len(), 1);
    assert_eq!(store.branch_tips()[0].id, head);

    let reloaded = SessionStore::load(&path).expect("reload");
    assert_eq!(reloaded.entries().len(), 5);
    assert!(!reloaded.contains(head + 1));
    assert!(!reloaded.contains(head + 2));
}

#[test]
fn integration_compact_then_append_preserves_next_id_progression() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("compact-append.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append")
        .expect("head");
    store
        .append_messages(Some(head), &[tau_ai::Message::user("main")])
        .expect("append");
    store
        .append_messages(Some(head), &[tau_ai::Message::user("branch")])
        .expect("append");

    store
        .compact_to_lineage(Some(head + 1))
        .expect("compact should succeed");
    let next_head = store
        .append_messages(
            store.head_id(),
            &[tau_ai::Message::assistant_text("after-compact")],
        )
        .expect("append after compact")
        .expect("next head");

    assert_eq!(next_head, 3);
}

#[test]
fn regression_compact_to_lineage_errors_for_unknown_head() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("compact-unknown-head.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");
    let error = store
        .compact_to_lineage(Some(999))
        .expect_err("unknown head should fail");
    assert!(error.to_string().contains("unknown session id 999"));
}

#[test]
fn regression_compact_to_lineage_fails_on_cycle() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("compact-cycle.jsonl");

    let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":2,"message":tau_ai::Message::system("sys")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":1,"message":tau_ai::Message::user("hello")}).to_string(),
        ]
        .join("\n");
    fs::write(&path, format!("{raw}\n")).expect("write cycle session");

    let mut store = SessionStore::load(&path).expect("load");
    let error = store
        .compact_to_lineage(Some(1))
        .expect_err("cycle should fail");
    assert!(error.to_string().contains("detected a cycle"));
}

#[test]
fn unit_compact_empty_store_is_noop() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("compact-empty.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let report = store.compact_to_lineage(None).expect("compact");
    assert_eq!(
        report,
        CompactReport {
            removed_entries: 0,
            retained_entries: 0,
            head_id: None,
        }
    );
    assert!(store.entries().is_empty());
}

#[test]
fn concurrent_appends_are_serialized_with_locking() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("concurrent.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("init");

    let path1 = path.clone();
    let path2 = path.clone();

    let worker = |path: PathBuf, label: &'static str| {
        thread::spawn(move || {
            let mut store = SessionStore::load(&path).expect("load worker");
            let head = store.head_id();
            store
                .append_messages(head, &[tau_ai::Message::user(label)])
                .expect("append worker");
        })
    };

    let t1 = worker(path1, "a");
    let t2 = worker(path2, "b");
    t1.join().expect("thread 1");
    t2.join().expect("thread 2");

    let store = SessionStore::load(&path).expect("reload");
    assert_eq!(store.entries().len(), 3);
}

#[test]
fn stress_parallel_appends_high_volume_remain_consistent() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("stress-concurrent.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("init");

    let workers = 8;
    let appends_per_worker = 25;
    let mut handles = Vec::new();
    for worker_index in 0..workers {
        let path = path.clone();
        handles.push(thread::spawn(move || {
            for append_index in 0..appends_per_worker {
                let mut retries = 0usize;
                loop {
                    let mut store = SessionStore::load(&path).expect("load worker");
                    let head = store.head_id();
                    match store.append_messages(
                        head,
                        &[tau_ai::Message::user(format!(
                            "worker-{worker_index}-append-{append_index}"
                        ))],
                    ) {
                        Ok(_) => break,
                        Err(error) if error.to_string().contains("timed out acquiring lock") => {
                            retries += 1;
                            if retries >= 8 {
                                panic!("append worker retries exhausted: {error}");
                            }
                            thread::sleep(Duration::from_millis(30));
                        }
                        Err(error) => panic!("append worker: {error}"),
                    }
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("worker join");
    }

    let reloaded = SessionStore::load(&path).expect("reload");
    assert_eq!(reloaded.entries().len(), 1 + workers * appends_per_worker);
    let unique_ids = reloaded
        .entries()
        .iter()
        .map(|entry| entry.id)
        .collect::<HashSet<_>>();
    assert_eq!(unique_ids.len(), reloaded.entries().len());
}

#[test]
fn repair_command_is_idempotent_for_valid_session() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("idempotent.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");
    store
        .append_messages(head, &[tau_ai::Message::user("hello")])
        .expect("append");

    let report = store.repair().expect("repair");
    assert_eq!(report, RepairReport::default());
    assert_eq!(store.entries().len(), 2);
}

#[test]
fn lock_file_is_cleaned_up_after_append() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("lock-cleanup.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");

    let lock = path.with_extension("lock");
    assert!(!lock.exists());
}

#[test]
fn session_file_remains_parseable_after_multiple_writes() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("atomic.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    for i in 0..10 {
        let head = store.head_id();
        store
            .append_messages(head, &[tau_ai::Message::user(format!("msg-{i}"))])
            .expect("append");
    }

    let reloaded = SessionStore::load(&path).expect("reload");
    assert_eq!(reloaded.entries().len(), 10);
}

#[test]
fn regression_lineage_unknown_id_still_errors() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("unknown-id.jsonl");
    let store = SessionStore::load(&path).expect("load");

    let error = store
        .lineage_messages(Some(999))
        .expect_err("unknown id should fail");
    assert!(error.to_string().contains("unknown session id 999"));
}

#[test]
fn functional_branch_tips_returns_all_leaf_nodes() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("tips.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append")
        .expect("head");

    let branch_a = store
        .append_messages(Some(head), &[tau_ai::Message::user("a")])
        .expect("append")
        .expect("branch a");
    let branch_b = store
        .append_messages(Some(head), &[tau_ai::Message::user("b")])
        .expect("append")
        .expect("branch b");

    let tips = store.branch_tips();
    let ids = tips.iter().map(|entry| entry.id).collect::<Vec<_>>();
    assert!(ids.contains(&branch_a));
    assert!(ids.contains(&branch_b));
}

#[test]
fn integration_load_after_external_rewrite_keeps_consistency() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("external.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");

    let external_raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":null,"message":tau_ai::Message::system("sys")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":1,"message":tau_ai::Message::user("external")}).to_string(),
        ]
        .join("\n");
    fs::write(&path, format!("{external_raw}\n")).expect("external write");

    let mut reloaded = SessionStore::load(&path).expect("reload");
    let head = reloaded.head_id();
    reloaded
        .append_messages(head, &[tau_ai::Message::assistant_text("local")])
        .expect("append");

    let final_store = SessionStore::load(&path).expect("final load");
    assert_eq!(final_store.entries().len(), 3);
}

#[test]
fn regression_repair_handles_duplicate_ids() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("dupe.jsonl");

    let duplicate_entry = SessionEntry {
        id: 1,
        parent_id: None,
        message: tau_ai::Message::system("sys"),
    };
    let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":null,"message":tau_ai::Message::system("sys")}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":null,"message":tau_ai::Message::user("dupe")}).to_string(),
        ]
        .join("\n");
    fs::write(&path, format!("{raw}\n")).expect("write dupes");

    let mut store = SessionStore::load(&path).expect("load");
    let report = store.repair().expect("repair");
    assert_eq!(report.removed_duplicates, 1);
    assert_eq!(report.duplicate_ids, vec![1]);
    assert_eq!(store.entries().len(), 1);
    assert_eq!(store.entries()[0].id, duplicate_entry.id);
}

#[test]
fn unit_append_messages_updates_internal_next_id() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("next-id.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");
    let head = store
        .append_messages(head, &[tau_ai::Message::user("u1")])
        .expect("append");
    store
        .append_messages(head, &[tau_ai::Message::assistant_text("a1")])
        .expect("append");

    assert_eq!(store.entries().last().map(|entry| entry.id), Some(3));
}

#[test]
fn regression_repair_retains_root_nodes() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("roots.jsonl");

    let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":null,"message":tau_ai::Message::system("root1")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":null,"message":tau_ai::Message::system("root2")}).to_string(),
        ]
        .join("\n");
    fs::write(&path, format!("{raw}\n")).expect("write roots");

    let mut store = SessionStore::load(&path).expect("load");
    let report = store.repair().expect("repair");
    assert_eq!(report, RepairReport::default());
    assert_eq!(store.entries().len(), 2);
}

#[test]
fn functional_lineage_for_none_head_returns_empty() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("none-head.jsonl");
    let store = SessionStore::load(&path).expect("load");

    let lineage = store.lineage_messages(None).expect("lineage");
    assert!(lineage.is_empty());
}

#[test]
fn integration_parallel_stores_append_consistent_ids() {
    let temp = tempdir().expect("tempdir");
    let path = Arc::new(temp.path().join("parallel-ids.jsonl"));

    let mut store = SessionStore::load(&*path).expect("load");
    store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");

    let handles = (0..4)
        .map(|i| {
            let path = path.clone();
            thread::spawn(move || {
                let mut store = SessionStore::load(&*path).expect("load thread");
                let head = store.head_id();
                store
                    .append_messages(head, &[tau_ai::Message::user(format!("u{i}"))])
                    .expect("append thread");
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.join().expect("thread join");
    }

    let store = SessionStore::load(&*path).expect("reload");
    let mut ids = store
        .entries()
        .iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), store.entries().len());
}

#[test]
fn regression_repair_report_counts_invalid_cycles() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("report-counts.jsonl");

    let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":2,"message":tau_ai::Message::system("a")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":1,"message":tau_ai::Message::system("b")}).to_string(),
        ]
        .join("\n");
    fs::write(&path, format!("{raw}\n")).expect("write cycles");

    let mut store = SessionStore::load(&path).expect("load");
    let report = store.repair().expect("repair");
    assert_eq!(report.removed_invalid_parent, 0);
    assert_eq!(report.removed_cycles, 2);
    assert_eq!(report.cycle_ids, vec![1, 2]);
}

#[test]
fn regression_lock_timeout_when_lock_file_persists() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("timeout.jsonl");
    let lock_path = path.with_extension("lock");
    fs::write(&lock_path, "stale").expect("write lock");

    let mut store = SessionStore::load(&path).expect("load");
    store.set_lock_policy(150, 0);
    let error = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect_err("append should time out when lock persists");
    assert!(error.to_string().contains("timed out acquiring lock"));

    fs::remove_file(&lock_path).expect("cleanup lock");
}

#[test]
fn functional_stale_lock_file_is_reclaimed_after_threshold() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("stale-reclaim.jsonl");
    let lock_path = path.with_extension("lock");
    fs::write(&lock_path, "stale").expect("write lock");
    thread::sleep(Duration::from_millis(30));

    let mut store = SessionStore::load(&path).expect("load");
    store.set_lock_policy(1_000, 10);
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append should reclaim stale lock");

    assert_eq!(head, Some(1));
    assert!(!lock_path.exists());
}

#[test]
fn unit_acquire_lock_creates_missing_parent_directories() {
    let temp = tempdir().expect("tempdir");
    let lock_path = temp.path().join(".tau/sessions/default.lock");
    let parent = lock_path.parent().expect("lock parent");
    assert!(!parent.exists());

    let guard =
        acquire_lock(&lock_path, Duration::from_millis(200), Duration::ZERO).expect("acquire lock");
    assert!(parent.exists());

    drop(guard);
    assert!(!lock_path.exists());
}

#[test]
fn functional_append_messages_creates_missing_session_parent_directory() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join(".tau/sessions/default.sqlite");
    let parent = path.parent().expect("session parent");
    assert!(!parent.exists());

    let mut store = SessionStore::load(&path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");
    assert_eq!(head, Some(1));
    assert!(parent.exists());
    assert!(path.exists());
}

#[test]
fn regression_default_tau_session_initialization_succeeds_without_existing_dir() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join(".tau/sessions/default.sqlite");
    let parent = path.parent().expect("session parent");
    assert!(!parent.exists());

    let mut store = SessionStore::load(&path).expect("load");
    let head = store.ensure_initialized("system").expect("initialize");
    assert_eq!(head, Some(1));
    assert!(parent.exists());
    assert!(path.exists());
}

#[test]
fn regression_session_load_precreates_default_tau_parent_directory() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join(".tau/sessions/default.sqlite");
    let parent = path.parent().expect("session parent");
    assert!(!parent.exists());

    let store = SessionStore::load(&path).expect("load");
    assert!(store.entries().is_empty());
    assert!(parent.exists());
}

#[test]
fn functional_sqlite_backend_round_trip_preserves_lineage() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("session.sqlite");
    let mut store = SessionStore::load(&path).expect("load sqlite store");
    assert_eq!(store.storage_backend(), SessionStorageBackend::Sqlite);

    let head = store
        .append_messages(None, &[tau_ai::Message::system("sqlite-root")])
        .expect("append root");
    let head = store
        .append_messages(head, &[tau_ai::Message::assistant_text("sqlite-reply")])
        .expect("append reply");
    let lineage = store.lineage_messages(head).expect("lineage");
    assert_eq!(lineage.len(), 2);
    assert_eq!(lineage[0].text_content(), "sqlite-root");
    assert_eq!(lineage[1].text_content(), "sqlite-reply");

    let reloaded = SessionStore::load(&path).expect("reload sqlite store");
    assert_eq!(reloaded.storage_backend(), SessionStorageBackend::Sqlite);
    assert_eq!(reloaded.entries().len(), 2);
}

#[test]
fn integration_sqlite_backend_auto_imports_legacy_jsonl_snapshot() {
    let temp = tempdir().expect("tempdir");
    let legacy_path = temp.path().join("legacy.jsonl");
    let sqlite_path = temp.path().join("legacy.sqlite");

    let mut legacy_store = SessionStore::load(&legacy_path).expect("load legacy jsonl store");
    let head = legacy_store
        .append_messages(None, &[tau_ai::Message::system("legacy-root")])
        .expect("append root");
    legacy_store
        .append_messages(head, &[tau_ai::Message::assistant_text("legacy-reply")])
        .expect("append reply");

    let sqlite_store = SessionStore::load(&sqlite_path).expect("load sqlite store");
    assert_eq!(
        sqlite_store.storage_backend(),
        SessionStorageBackend::Sqlite
    );
    assert_eq!(sqlite_store.entries().len(), 2);
    assert_eq!(
        sqlite_store
            .lineage_messages(sqlite_store.head_id())
            .expect("lineage")[0]
            .text_content(),
        "legacy-root"
    );
}
