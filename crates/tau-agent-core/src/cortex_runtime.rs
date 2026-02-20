//! Cortex runtime bulletin generation primitives.
//!
//! This module provides a bounded cross-session memory scanner and a shared
//! bulletin snapshot (`ArcSwap<String>`) that callers can inject into new
//! session system prompts.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arc_swap::ArcSwap;
use tau_ai::{ChatRequest, LlmClient, Message, PromptCacheConfig};
use tau_memory::runtime::{FileMemoryStore, RuntimeMemoryRecord};

const DEFAULT_CORTEX_MAX_SESSIONS: usize = 64;
const DEFAULT_CORTEX_MAX_RECORDS_PER_SESSION: usize = 8;
const DEFAULT_CORTEX_MAX_RECORDS_TOTAL: usize = 24;
const DEFAULT_CORTEX_MAX_BULLETIN_CHARS: usize = 2_000;
const CORTEX_BULLETIN_HEADER: &str = "## Cortex Memory Bulletin";
const CORTEX_SUMMARY_SYSTEM_PROMPT: &str = "You are Tau Cortex. Summarize cross-session memory\
 in concise operator bullet points. Focus on trends, unresolved work, and immediate risks.\
 Return plain text only.";

/// Public struct `CortexConfig` used across Tau components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CortexConfig {
    pub memory_store_root: PathBuf,
    pub max_sessions: usize,
    pub max_records_per_session: usize,
    pub max_records_total: usize,
    pub max_bulletin_chars: usize,
}

impl CortexConfig {
    pub fn new(memory_store_root: impl Into<PathBuf>) -> Self {
        Self {
            memory_store_root: memory_store_root.into(),
            max_sessions: DEFAULT_CORTEX_MAX_SESSIONS,
            max_records_per_session: DEFAULT_CORTEX_MAX_RECORDS_PER_SESSION,
            max_records_total: DEFAULT_CORTEX_MAX_RECORDS_TOTAL,
            max_bulletin_chars: DEFAULT_CORTEX_MAX_BULLETIN_CHARS,
        }
    }
}

/// Public struct `CortexRefreshReport` used across Tau components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CortexRefreshReport {
    pub sessions_scanned: usize,
    pub records_scanned: usize,
    pub bulletin_chars: usize,
    pub reason_code: String,
    pub diagnostics: Vec<String>,
}

impl Default for CortexRefreshReport {
    fn default() -> Self {
        Self {
            sessions_scanned: 0,
            records_scanned: 0,
            bulletin_chars: 0,
            reason_code: "cortex_bulletin_not_refreshed".to_string(),
            diagnostics: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct CortexSessionRecord {
    session_key: String,
    record: RuntimeMemoryRecord,
}

/// Public struct `Cortex` used across Tau components.
#[derive(Debug, Clone)]
pub struct Cortex {
    config: CortexConfig,
    bulletin: Arc<ArcSwap<String>>,
}

impl Cortex {
    pub fn new(config: CortexConfig) -> Self {
        Self {
            config,
            bulletin: Arc::new(ArcSwap::from_pointee(String::new())),
        }
    }

    pub fn bulletin_handle(&self) -> Arc<ArcSwap<String>> {
        Arc::clone(&self.bulletin)
    }

    pub fn bulletin_snapshot(&self) -> String {
        self.bulletin.load_full().as_ref().clone()
    }

    pub fn set_bulletin_for_test(&self, bulletin: impl Into<String>) {
        self.bulletin.store(Arc::new(bulletin.into()));
    }

    pub fn compose_system_prompt(&self, base_system_prompt: &str) -> String {
        let bulletin = self.bulletin_snapshot();
        if bulletin.trim().is_empty() {
            return base_system_prompt.to_string();
        }
        if base_system_prompt.trim().is_empty() {
            return bulletin;
        }
        format!("{}\n\n{}", base_system_prompt.trim_end(), bulletin.trim())
    }

    pub async fn refresh_once(&self, client: &dyn LlmClient, model: &str) -> CortexRefreshReport {
        let mut report = CortexRefreshReport::default();
        let (records, sessions_scanned, diagnostics) = self.collect_cross_session_records();
        report.sessions_scanned = sessions_scanned;
        report.records_scanned = records.len();
        report.diagnostics = diagnostics;

        if records.is_empty() {
            report.reason_code = "cortex_bulletin_no_records".to_string();
            self.bulletin.store(Arc::new(String::new()));
            return report;
        }

        let llm_request = ChatRequest {
            model: model.trim().to_string(),
            messages: vec![
                Message::system(CORTEX_SUMMARY_SYSTEM_PROMPT),
                Message::user(render_llm_bulletin_input(records.as_slice())),
            ],
            tools: Vec::new(),
            tool_choice: None,
            json_mode: false,
            max_tokens: Some(256),
            temperature: Some(0.0),
            prompt_cache: PromptCacheConfig::default(),
        };

        let mut reason_code = "cortex_bulletin_llm_applied".to_string();
        let mut bulletin = match client.complete(llm_request).await {
            Ok(response) => {
                let text = collapse_whitespace(response.message.text_content().as_str());
                if text.is_empty() {
                    reason_code = "cortex_bulletin_llm_empty_fallback".to_string();
                    render_fallback_bulletin(records.as_slice())
                } else {
                    format!(
                        "{CORTEX_BULLETIN_HEADER}\n{}",
                        truncate_chars(text.as_str(), self.config.max_bulletin_chars)
                    )
                }
            }
            Err(error) => {
                reason_code = "cortex_bulletin_llm_error_fallback".to_string();
                report
                    .diagnostics
                    .push(format!("cortex_bulletin_llm_failed:{error}"));
                render_fallback_bulletin(records.as_slice())
            }
        };

        bulletin = truncate_chars(bulletin.as_str(), self.config.max_bulletin_chars);
        report.bulletin_chars = bulletin.chars().count();
        report.reason_code = reason_code;
        self.bulletin.store(Arc::new(bulletin));
        report
    }

    fn collect_cross_session_records(&self) -> (Vec<CortexSessionRecord>, usize, Vec<String>) {
        let mut diagnostics = Vec::new();
        let root = self.config.memory_store_root.as_path();
        let Some(mut session_paths) = list_session_directories(root, &mut diagnostics) else {
            return (Vec::new(), 0, diagnostics);
        };

        if session_paths.len() > self.config.max_sessions {
            diagnostics.push(format!(
                "cortex_memory_sessions_truncated: discovered={} max_sessions={}",
                session_paths.len(),
                self.config.max_sessions
            ));
            session_paths.truncate(self.config.max_sessions);
        }

        let mut sessions_scanned = 0usize;
        let mut records = Vec::new();
        for session_path in session_paths {
            let session_key = session_directory_name(session_path.as_path());
            sessions_scanned = sessions_scanned.saturating_add(1);
            let store = FileMemoryStore::new(&session_path);
            match store.list_latest_records(None, self.config.max_records_per_session) {
                Ok(session_records) => {
                    records.extend(
                        session_records
                            .into_iter()
                            .map(|record| CortexSessionRecord {
                                session_key: session_key.clone(),
                                record,
                            }),
                    );
                }
                Err(error) => diagnostics.push(format!(
                    "cortex_memory_scan_failed:{}:{error}",
                    session_path.display()
                )),
            }
        }

        records.sort_by(|left, right| {
            right
                .record
                .updated_unix_ms
                .cmp(&left.record.updated_unix_ms)
                .then_with(|| left.session_key.cmp(&right.session_key))
                .then_with(|| {
                    left.record
                        .entry
                        .memory_id
                        .cmp(&right.record.entry.memory_id)
                })
        });
        if records.len() > self.config.max_records_total {
            records.truncate(self.config.max_records_total);
        }
        (records, sessions_scanned, diagnostics)
    }
}

fn list_session_directories(root: &Path, diagnostics: &mut Vec<String>) -> Option<Vec<PathBuf>> {
    if !root.exists() {
        diagnostics.push(format!(
            "cortex_memory_store_root_missing:{}",
            root.display()
        ));
        return None;
    }
    if !root.is_dir() {
        diagnostics.push(format!(
            "cortex_memory_store_root_not_directory:{}",
            root.display()
        ));
        return None;
    }

    let mut paths = Vec::new();
    match std::fs::read_dir(root) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();
                        if path.is_dir() {
                            paths.push(path);
                        }
                    }
                    Err(error) => diagnostics.push(format!(
                        "cortex_memory_store_read_entry_failed:{}:{error}",
                        root.display()
                    )),
                }
            }
        }
        Err(error) => {
            diagnostics.push(format!(
                "cortex_memory_store_root_read_failed:{}:{error}",
                root.display()
            ));
            return None;
        }
    }

    paths.sort();
    Some(paths)
}

fn session_directory_name(path: &Path) -> String {
    path.file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("unknown-session")
        .to_string()
}

fn render_llm_bulletin_input(records: &[CortexSessionRecord]) -> String {
    let mut lines = vec![
        "Cross-session memory records (most recent first):".to_string(),
        "Format each output bullet as: topic | impact | next action.".to_string(),
    ];
    for record in records {
        let summary = collapse_whitespace(record.record.entry.summary.as_str());
        if summary.is_empty() {
            continue;
        }
        lines.push(format!(
            "- session={} type={} importance={:.2} summary={}",
            record.session_key,
            record.record.memory_type.as_str(),
            record.record.importance,
            summary
        ));
    }
    lines.join("\n")
}

fn render_fallback_bulletin(records: &[CortexSessionRecord]) -> String {
    let mut output = String::from(CORTEX_BULLETIN_HEADER);
    output.push_str("\nGenerated deterministic fallback summary from cross-session memory.");
    for record in records {
        let summary = collapse_whitespace(record.record.entry.summary.as_str());
        if summary.is_empty() {
            continue;
        }
        output.push_str(&format!(
            "\n- [{}] {} (type={}, importance={:.2})",
            record.session_key,
            summary,
            record.record.memory_type.as_str(),
            record.record.importance
        ));
    }
    output
}

fn collapse_whitespace(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(raw: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if raw.chars().count() <= max_chars {
        return raw.to_string();
    }
    raw.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::{collapse_whitespace, truncate_chars};

    #[test]
    fn unit_collapse_whitespace_normalizes_spacing() {
        assert_eq!(
            collapse_whitespace("  alpha   beta \n gamma\t"),
            "alpha beta gamma"
        );
    }

    #[test]
    fn unit_truncate_chars_enforces_character_limit() {
        assert_eq!(truncate_chars("abcdef", 4), "abcd");
        assert_eq!(truncate_chars("abc", 10), "abc");
        assert_eq!(truncate_chars("abc", 0), "");
    }
}
