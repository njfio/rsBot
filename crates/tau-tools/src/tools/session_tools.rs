use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{json, Value};
use tau_agent_core::{AgentTool, ToolExecutionResult};
use tau_ai::{Message, ToolDefinition};
use tau_session::{
    compute_session_entry_depths, search_session_entries, session_message_preview,
    session_message_role, SessionStore,
};

use super::{
    canonicalize_best_effort, optional_u64, optional_usize, required_string,
    resolve_and_validate_path, validate_file_target, PathMode, ToolPolicy,
};

const SESSION_LIST_DEFAULT_LIMIT: usize = 64;
const SESSION_LIST_MAX_LIMIT: usize = 256;
const SESSION_HISTORY_DEFAULT_LIMIT: usize = 40;
const SESSION_HISTORY_MAX_LIMIT: usize = 200;
const SESSION_SEND_MAX_MESSAGE_CHARS: usize = 8_000;
const SESSION_SEARCH_TOOL_DEFAULT_LIMIT: usize = 50;
const SESSION_SEARCH_TOOL_MAX_LIMIT: usize = 200;
const SESSION_SEARCH_SCAN_DEFAULT_LIMIT: usize = 64;
const SESSION_STATS_SCAN_DEFAULT_LIMIT: usize = 64;
const SESSION_STATS_SCAN_MAX_LIMIT: usize = 256;
const SESSION_SCAN_MAX_DEPTH: usize = 8;
const SESSION_SCAN_MAX_DIRECTORIES: usize = 2_000;

#[derive(Debug, Clone, Serialize)]
struct SessionInventoryEntry {
    path: String,
    entries: usize,
    head_id: Option<u64>,
    newest_role: String,
    newest_preview: String,
}

#[derive(Debug, Clone, Serialize)]
struct SessionHistoryEntry {
    id: u64,
    parent_id: Option<u64>,
    role: String,
    preview: String,
}

#[derive(Debug, Clone, Serialize)]
struct SessionSearchToolMatch {
    path: String,
    id: u64,
    parent_id: Option<u64>,
    role: String,
    preview: String,
}

#[derive(Debug, Clone, Serialize)]
struct SessionStatsToolRow {
    path: String,
    entries: usize,
    branch_tips: usize,
    roots: usize,
    max_depth: usize,
    latest_head: Option<u64>,
    latest_depth: Option<usize>,
    role_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone)]
struct SessionStatsComputed {
    entries: usize,
    branch_tips: usize,
    roots: usize,
    max_depth: usize,
    latest_head: Option<u64>,
    latest_depth: Option<usize>,
    role_counts: BTreeMap<String, usize>,
}
/// Public struct `SessionsListTool` used across Tau components.
pub struct SessionsListTool {
    policy: Arc<ToolPolicy>,
}

impl SessionsListTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for SessionsListTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "sessions_list".to_string(),
            description: "List session stores discovered under allowed roots".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": format!(
                            "Maximum sessions to return (default {}, max {})",
                            SESSION_LIST_DEFAULT_LIMIT,
                            SESSION_LIST_MAX_LIMIT
                        )
                    }
                },
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let limit = match optional_usize(
            &arguments,
            "limit",
            SESSION_LIST_DEFAULT_LIMIT,
            SESSION_LIST_MAX_LIMIT,
        ) {
            Ok(limit) => limit,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        match collect_session_inventory(&self.policy, limit) {
            Ok((sessions, skipped_invalid)) => ToolExecutionResult::ok(json!({
                "limit": limit,
                "returned": sessions.len(),
                "skipped_invalid": skipped_invalid,
                "sessions": sessions,
            })),
            Err(error) => ToolExecutionResult::error(json!({ "error": error })),
        }
    }
}

/// Public struct `SessionsHistoryTool` used across Tau components.
pub struct SessionsHistoryTool {
    policy: Arc<ToolPolicy>,
}

impl SessionsHistoryTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for SessionsHistoryTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "sessions_history".to_string(),
            description: "Read bounded lineage/history from a session store".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to session JSONL file"
                    },
                    "head_id": {
                        "type": "integer",
                        "description": "Optional lineage head id. Defaults to current head."
                    },
                    "limit": {
                        "type": "integer",
                        "description": format!(
                            "Maximum lineage entries to return from the tail (default {}, max {})",
                            SESSION_HISTORY_DEFAULT_LIMIT,
                            SESSION_HISTORY_MAX_LIMIT
                        )
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let path = match required_string(&arguments, "path") {
            Ok(path) => path,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        let limit = match optional_usize(
            &arguments,
            "limit",
            SESSION_HISTORY_DEFAULT_LIMIT,
            SESSION_HISTORY_MAX_LIMIT,
        ) {
            Ok(limit) => limit,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        let head_id = match optional_u64(&arguments, "head_id") {
            Ok(head_id) => head_id,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Read) {
            Ok(path) => path,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": path,
                    "error": error,
                }))
            }
        };
        if let Err(error) =
            validate_file_target(&resolved, PathMode::Read, self.policy.enforce_regular_files)
        {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error,
            }));
        }

        let store = match SessionStore::load(&resolved) {
            Ok(store) => store,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": resolved.display().to_string(),
                    "error": format!("failed to load session: {error}"),
                }))
            }
        };

        let selected_head_id = head_id.or_else(|| store.head_id());
        let lineage = match store.lineage_entries(selected_head_id) {
            Ok(entries) => entries,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": resolved.display().to_string(),
                    "head_id": selected_head_id,
                    "error": format!("failed to resolve session lineage: {error}"),
                }))
            }
        };

        let start = lineage.len().saturating_sub(limit);
        let history_entries = lineage[start..]
            .iter()
            .map(|entry| SessionHistoryEntry {
                id: entry.id,
                parent_id: entry.parent_id,
                role: session_message_role(&entry.message),
                preview: session_message_preview(&entry.message),
            })
            .collect::<Vec<_>>();

        ToolExecutionResult::ok(json!({
            "path": resolved.display().to_string(),
            "head_id": selected_head_id,
            "lineage_entries": lineage.len(),
            "returned": history_entries.len(),
            "history": history_entries,
        }))
    }
}

/// Public struct `SessionsSearchTool` used across Tau components.
pub struct SessionsSearchTool {
    policy: Arc<ToolPolicy>,
}

impl SessionsSearchTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for SessionsSearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "sessions_search".to_string(),
            description: "Search message content across session stores under allowed roots"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Case-insensitive search query"
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional path to a specific session JSONL file"
                    },
                    "role": {
                        "type": "string",
                        "description": "Optional role filter",
                        "enum": ["system", "user", "assistant", "tool"]
                    },
                    "limit": {
                        "type": "integer",
                        "description": format!(
                            "Maximum matches to return (default {}, max {})",
                            SESSION_SEARCH_TOOL_DEFAULT_LIMIT,
                            SESSION_SEARCH_TOOL_MAX_LIMIT
                        )
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let query = match required_string(&arguments, "query") {
            Ok(query) => query,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        if query.trim().is_empty() {
            return ToolExecutionResult::error(json!({
                "error": "query must not be empty",
            }));
        }
        let role_filter = match optional_session_search_role(&arguments, "role") {
            Ok(role_filter) => role_filter,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        let limit = match optional_usize(
            &arguments,
            "limit",
            SESSION_SEARCH_TOOL_DEFAULT_LIMIT,
            SESSION_SEARCH_TOOL_MAX_LIMIT,
        ) {
            Ok(limit) => limit,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        let requested_path = arguments
            .get("path")
            .and_then(Value::as_str)
            .map(|value| value.to_string());

        if let Some(path) = requested_path {
            let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Read) {
                Ok(path) => path,
                Err(error) => {
                    return ToolExecutionResult::error(json!({
                        "path": path,
                        "error": error,
                    }))
                }
            };
            if let Err(error) =
                validate_file_target(&resolved, PathMode::Read, self.policy.enforce_regular_files)
            {
                return ToolExecutionResult::error(json!({
                    "path": resolved.display().to_string(),
                    "error": error,
                }));
            }

            let store = match SessionStore::load(&resolved) {
                Ok(store) => store,
                Err(error) => {
                    return ToolExecutionResult::error(json!({
                        "path": resolved.display().to_string(),
                        "error": format!("failed to load session: {error}"),
                    }))
                }
            };
            let entries_scanned = store.entries().len();
            let (matches, total_matches) =
                search_session_entries(store.entries(), &query, role_filter.as_deref(), limit);
            let results = matches
                .into_iter()
                .map(|item| SessionSearchToolMatch {
                    path: resolved.display().to_string(),
                    id: item.id,
                    parent_id: item.parent_id,
                    role: item.role,
                    preview: item.preview,
                })
                .collect::<Vec<_>>();

            return ToolExecutionResult::ok(json!({
                "query": query,
                "role": role_filter.clone().unwrap_or_else(|| "any".to_string()),
                "path": resolved.display().to_string(),
                "limit": limit,
                "sessions_scanned": 1,
                "entries_scanned": entries_scanned,
                "matches": total_matches,
                "returned": results.len(),
                "skipped_invalid": 0,
                "results": results,
            }));
        }

        let session_paths =
            match discover_session_paths(&self.policy, SESSION_SEARCH_SCAN_DEFAULT_LIMIT) {
                Ok(session_paths) => session_paths,
                Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
            };

        let mut results = Vec::new();
        let mut sessions_scanned = 0usize;
        let mut entries_scanned = 0usize;
        let mut skipped_invalid = 0usize;
        let mut total_matches = 0usize;

        for path in session_paths {
            let store = match SessionStore::load(&path) {
                Ok(store) => store,
                Err(_) => {
                    skipped_invalid += 1;
                    continue;
                }
            };

            sessions_scanned += 1;
            entries_scanned += store.entries().len();
            let (session_matches, session_total_matches) =
                search_session_entries(store.entries(), &query, role_filter.as_deref(), limit);
            total_matches += session_total_matches;
            for item in session_matches {
                if results.len() >= limit {
                    break;
                }
                results.push(SessionSearchToolMatch {
                    path: path.display().to_string(),
                    id: item.id,
                    parent_id: item.parent_id,
                    role: item.role,
                    preview: item.preview,
                });
            }
        }

        ToolExecutionResult::ok(json!({
            "query": query,
            "role": role_filter.unwrap_or_else(|| "any".to_string()),
            "limit": limit,
            "sessions_scanned": sessions_scanned,
            "entries_scanned": entries_scanned,
            "matches": total_matches,
            "returned": results.len(),
            "skipped_invalid": skipped_invalid,
            "results": results,
        }))
    }
}

fn compute_store_stats(store: &SessionStore) -> Result<SessionStatsComputed, String> {
    let entries = store.entries();
    let depths = compute_session_entry_depths(entries)
        .map_err(|error| format!("failed to compute session entry depths: {error}"))?;

    let mut role_counts = BTreeMap::new();
    for entry in entries {
        let role = session_message_role(&entry.message);
        *role_counts.entry(role).or_insert(0) += 1;
    }

    let latest_head = store.head_id();
    let latest_depth = latest_head.and_then(|id| depths.get(&id).copied());

    Ok(SessionStatsComputed {
        entries: entries.len(),
        branch_tips: store.branch_tips().len(),
        roots: entries
            .iter()
            .filter(|entry| entry.parent_id.is_none())
            .count(),
        max_depth: depths.values().copied().max().unwrap_or(0),
        latest_head,
        latest_depth,
        role_counts,
    })
}

/// Public struct `SessionsStatsTool` used across Tau components.
pub struct SessionsStatsTool {
    policy: Arc<ToolPolicy>,
}

impl SessionsStatsTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for SessionsStatsTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "sessions_stats".to_string(),
            description: "Compute session depth/head/role metrics for one or many session stores"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Optional path to a specific session JSONL file"
                    },
                    "limit": {
                        "type": "integer",
                        "description": format!(
                            "Maximum session files to scan in aggregate mode (default {}, max {})",
                            SESSION_STATS_SCAN_DEFAULT_LIMIT,
                            SESSION_STATS_SCAN_MAX_LIMIT
                        )
                    }
                },
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let limit = match optional_usize(
            &arguments,
            "limit",
            SESSION_STATS_SCAN_DEFAULT_LIMIT,
            SESSION_STATS_SCAN_MAX_LIMIT,
        ) {
            Ok(limit) => limit,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        let requested_path = arguments
            .get("path")
            .and_then(Value::as_str)
            .map(|value| value.to_string());

        if let Some(path) = requested_path {
            let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Read) {
                Ok(path) => path,
                Err(error) => {
                    return ToolExecutionResult::error(json!({
                        "path": path,
                        "error": error,
                    }))
                }
            };
            if let Err(error) =
                validate_file_target(&resolved, PathMode::Read, self.policy.enforce_regular_files)
            {
                return ToolExecutionResult::error(json!({
                    "path": resolved.display().to_string(),
                    "error": error,
                }));
            }

            let store = match SessionStore::load(&resolved) {
                Ok(store) => store,
                Err(error) => {
                    return ToolExecutionResult::error(json!({
                        "path": resolved.display().to_string(),
                        "error": format!("failed to load session: {error}"),
                    }))
                }
            };
            let stats = match compute_store_stats(&store) {
                Ok(stats) => stats,
                Err(error) => {
                    return ToolExecutionResult::error(json!({
                        "path": resolved.display().to_string(),
                        "error": error,
                    }))
                }
            };

            return ToolExecutionResult::ok(json!({
                "mode": "single",
                "path": resolved.display().to_string(),
                "limit": limit,
                "sessions_scanned": 1,
                "skipped_invalid": 0,
                "entries": stats.entries,
                "branch_tips": stats.branch_tips,
                "roots": stats.roots,
                "max_depth": stats.max_depth,
                "latest_head": stats.latest_head,
                "latest_depth": stats.latest_depth,
                "role_counts": stats.role_counts,
            }));
        }

        let session_paths = match discover_session_paths(&self.policy, limit) {
            Ok(paths) => paths,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        let mut sessions = Vec::new();
        let mut skipped_invalid = 0usize;
        let mut total_entries = 0usize;
        let mut total_branch_tips = 0usize;
        let mut total_roots = 0usize;
        let mut total_max_depth = 0usize;
        let mut total_role_counts = BTreeMap::new();

        for path in session_paths {
            let store = match SessionStore::load(&path) {
                Ok(store) => store,
                Err(_) => {
                    skipped_invalid += 1;
                    continue;
                }
            };

            let stats = match compute_store_stats(&store) {
                Ok(stats) => stats,
                Err(_) => {
                    skipped_invalid += 1;
                    continue;
                }
            };

            total_entries += stats.entries;
            total_branch_tips += stats.branch_tips;
            total_roots += stats.roots;
            total_max_depth = total_max_depth.max(stats.max_depth);
            for (role, count) in &stats.role_counts {
                *total_role_counts.entry(role.clone()).or_insert(0) += count;
            }
            sessions.push(SessionStatsToolRow {
                path: path.display().to_string(),
                entries: stats.entries,
                branch_tips: stats.branch_tips,
                roots: stats.roots,
                max_depth: stats.max_depth,
                latest_head: stats.latest_head,
                latest_depth: stats.latest_depth,
                role_counts: stats.role_counts,
            });
        }
        sessions.sort_by(|left, right| left.path.cmp(&right.path));

        ToolExecutionResult::ok(json!({
            "mode": "aggregate",
            "limit": limit,
            "sessions_scanned": sessions.len(),
            "skipped_invalid": skipped_invalid,
            "entries": total_entries,
            "branch_tips": total_branch_tips,
            "roots": total_roots,
            "max_depth": total_max_depth,
            "role_counts": total_role_counts,
            "sessions": sessions,
        }))
    }
}

/// Public struct `SessionsSendTool` used across Tau components.
pub struct SessionsSendTool {
    policy: Arc<ToolPolicy>,
}

impl SessionsSendTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for SessionsSendTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "sessions_send".to_string(),
            description:
                "Append a user handoff message into a target session store under allowed roots"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to target session JSONL file"
                    },
                    "message": {
                        "type": "string",
                        "description": format!(
                            "User handoff message (max {} characters)",
                            SESSION_SEND_MAX_MESSAGE_CHARS
                        )
                    },
                    "parent_id": {
                        "type": "integer",
                        "description": "Optional parent entry id. Defaults to current head."
                    }
                },
                "required": ["path", "message"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let path = match required_string(&arguments, "path") {
            Ok(path) => path,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        let message = match required_string(&arguments, "message") {
            Ok(message) => message,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        let parent_id = match optional_u64(&arguments, "parent_id") {
            Ok(parent_id) => parent_id,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        if message.trim().is_empty() {
            return ToolExecutionResult::error(json!({
                "path": path,
                "error": "message must not be empty",
            }));
        }
        if message.chars().count() > SESSION_SEND_MAX_MESSAGE_CHARS {
            return ToolExecutionResult::error(json!({
                "path": path,
                "error": format!(
                    "message exceeds max length of {} characters",
                    SESSION_SEND_MAX_MESSAGE_CHARS
                ),
            }));
        }

        let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Write) {
            Ok(path) => path,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": path,
                    "error": error,
                }))
            }
        };
        if let Err(error) = validate_file_target(
            &resolved,
            PathMode::Write,
            self.policy.enforce_regular_files,
        ) {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error,
            }));
        }

        let mut store = match SessionStore::load(&resolved) {
            Ok(store) => store,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": resolved.display().to_string(),
                    "error": format!("failed to load session: {error}"),
                }))
            }
        };

        let before_entries = store.entries().len();
        let previous_head_id = store.head_id();
        let selected_parent_id = parent_id.or(previous_head_id);
        let handoff_message = Message::user(message.clone());
        let new_head_id = match store.append_messages(selected_parent_id, &[handoff_message]) {
            Ok(head_id) => head_id,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": resolved.display().to_string(),
                    "parent_id": selected_parent_id,
                    "error": format!("failed to append handoff message: {error}"),
                }))
            }
        };
        let after_entries = store.entries().len();

        ToolExecutionResult::ok(json!({
            "path": resolved.display().to_string(),
            "parent_id": selected_parent_id,
            "previous_head_id": previous_head_id,
            "new_head_id": new_head_id,
            "before_entries": before_entries,
            "after_entries": after_entries,
            "appended_entries": after_entries.saturating_sub(before_entries),
            "message_preview": session_message_preview(&Message::user(message)),
        }))
    }
}
fn collect_session_inventory(
    policy: &ToolPolicy,
    limit: usize,
) -> Result<(Vec<SessionInventoryEntry>, usize), String> {
    let session_paths = discover_session_paths(policy, limit)?;
    let mut sessions = Vec::with_capacity(session_paths.len());
    let mut skipped_invalid = 0usize;

    for path in session_paths {
        if sessions.len() >= limit {
            break;
        }
        match SessionStore::load(&path) {
            Ok(store) => {
                let newest = store.entries().last();
                sessions.push(SessionInventoryEntry {
                    path: path.display().to_string(),
                    entries: store.entries().len(),
                    head_id: store.head_id(),
                    newest_role: newest
                        .map(|entry| session_message_role(&entry.message))
                        .unwrap_or_else(|| "none".to_string()),
                    newest_preview: newest
                        .map(|entry| session_message_preview(&entry.message))
                        .unwrap_or_else(|| "(empty session)".to_string()),
                });
            }
            Err(_) => {
                skipped_invalid += 1;
            }
        }
    }

    sessions.sort_by(|left, right| left.path.cmp(&right.path));
    Ok((sessions, skipped_invalid))
}

fn discover_session_paths(policy: &ToolPolicy, limit: usize) -> Result<Vec<PathBuf>, String> {
    let mut roots = if policy.allowed_roots.is_empty() {
        vec![std::env::current_dir().map_err(|error| format!("failed to resolve cwd: {error}"))?]
    } else {
        policy.allowed_roots.clone()
    };
    roots.sort_by_key(|left| left.display().to_string());

    let mut found = Vec::new();
    let mut seen = BTreeSet::new();
    for root in roots {
        if found.len() >= limit {
            break;
        }
        let tau_root = root.join(".tau");
        if !tau_root.exists() {
            continue;
        }

        let mut queue = VecDeque::from([(tau_root, 0usize)]);
        let mut visited_directories = 0usize;
        while let Some((directory, depth)) = queue.pop_front() {
            if found.len() >= limit || visited_directories >= SESSION_SCAN_MAX_DIRECTORIES {
                break;
            }
            visited_directories += 1;

            let entries = std::fs::read_dir(&directory).map_err(|error| {
                format!(
                    "failed to scan session directory '{}': {error}",
                    directory.display()
                )
            })?;
            let mut child_paths = entries
                .filter_map(|entry| entry.ok().map(|item| item.path()))
                .collect::<Vec<_>>();
            child_paths.sort();

            for path in child_paths {
                if found.len() >= limit {
                    break;
                }
                let metadata = match std::fs::symlink_metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(_) => continue,
                };
                if metadata.file_type().is_symlink() {
                    continue;
                }
                if metadata.is_dir() {
                    if depth < SESSION_SCAN_MAX_DEPTH {
                        queue.push_back((path, depth + 1));
                    }
                    continue;
                }
                if !metadata.is_file() || !is_session_candidate_path(&path) {
                    continue;
                }
                let canonical = canonicalize_best_effort(&path).map_err(|error| {
                    format!(
                        "failed to canonicalize session candidate '{}': {error}",
                        path.display()
                    )
                })?;
                let key = canonical.display().to_string();
                if seen.insert(key) {
                    found.push(canonical);
                }
            }
        }
    }

    found.sort();
    Ok(found)
}

pub(super) fn is_session_candidate_path(path: &Path) -> bool {
    if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
        return false;
    }

    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if file_name == "default.jsonl"
        || file_name == "session.jsonl"
        || file_name.starts_with("issue-")
    {
        return true;
    }

    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .map(|value| value.eq_ignore_ascii_case("sessions"))
            .unwrap_or(false)
    })
}
fn optional_session_search_role(arguments: &Value, key: &str) -> Result<Option<String>, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(None);
    };
    let raw = value
        .as_str()
        .ok_or_else(|| format!("optional argument '{key}' must be a string"))?;
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "system" | "user" | "assistant" | "tool" => Ok(Some(normalized)),
        _ => Err(format!(
            "optional argument '{key}' must be one of: system, user, assistant, tool"
        )),
    }
}
