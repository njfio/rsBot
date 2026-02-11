use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{anyhow, bail, Context, Result};
use tau_ai::Message;

use crate::{SessionEntry, SessionRuntime};

pub const SESSION_SEARCH_DEFAULT_RESULTS: usize = 50;
const SESSION_SEARCH_MAX_RESULTS: usize = 200;
pub const SESSION_SEARCH_PREVIEW_CHARS: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSearchMatch {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub role: String,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSearchArgs {
    pub query: String,
    pub role: Option<String>,
    pub limit: usize,
}

fn parse_session_search_role(raw: &str) -> Result<String> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "system" | "user" | "assistant" | "tool" => Ok(normalized),
        _ => bail!(
            "invalid role '{}'; expected one of: system, user, assistant, tool",
            raw
        ),
    }
}

fn parse_session_search_limit(raw: &str) -> Result<usize> {
    let value = raw
        .trim()
        .parse::<usize>()
        .with_context(|| format!("invalid limit '{}'; expected an integer", raw))?;
    if value == 0 {
        bail!("limit must be greater than 0");
    }
    if value > SESSION_SEARCH_MAX_RESULTS {
        bail!(
            "limit {} exceeds maximum {}",
            value,
            SESSION_SEARCH_MAX_RESULTS
        );
    }
    Ok(value)
}

pub fn parse_session_search_args(command_args: &str) -> Result<SessionSearchArgs> {
    let mut query_parts = Vec::new();
    let mut role = None;
    let mut limit = SESSION_SEARCH_DEFAULT_RESULTS;
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();

    let mut index = 0usize;
    while index < tokens.len() {
        let token = tokens[index];
        if token == "--role" {
            let value = tokens
                .get(index + 1)
                .ok_or_else(|| anyhow!("missing value for --role"))?;
            role = Some(parse_session_search_role(value)?);
            index += 2;
            continue;
        }
        if let Some(value) = token.strip_prefix("--role=") {
            role = Some(parse_session_search_role(value)?);
            index += 1;
            continue;
        }
        if token == "--limit" {
            let value = tokens
                .get(index + 1)
                .ok_or_else(|| anyhow!("missing value for --limit"))?;
            limit = parse_session_search_limit(value)?;
            index += 2;
            continue;
        }
        if let Some(value) = token.strip_prefix("--limit=") {
            limit = parse_session_search_limit(value)?;
            index += 1;
            continue;
        }
        if token.starts_with("--") {
            bail!("unknown flag '{}'", token);
        }

        query_parts.push(token.to_string());
        index += 1;
    }

    let query = query_parts.join(" ");
    if query.trim().is_empty() {
        bail!("query is required");
    }

    Ok(SessionSearchArgs { query, role, limit })
}

fn normalize_preview_text(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn session_message_preview(message: &Message) -> String {
    let normalized = normalize_preview_text(&message.text_content());
    let preview = if normalized.is_empty() {
        "(no text)".to_string()
    } else {
        normalized
    };

    if preview.chars().count() <= SESSION_SEARCH_PREVIEW_CHARS {
        return preview;
    }
    let truncated = preview
        .chars()
        .take(SESSION_SEARCH_PREVIEW_CHARS)
        .collect::<String>();
    format!("{truncated}...")
}

pub fn session_message_role(message: &Message) -> String {
    format!("{:?}", message.role).to_lowercase()
}

pub fn search_session_entries(
    entries: &[SessionEntry],
    query: &str,
    role_filter: Option<&str>,
    max_results: usize,
) -> (Vec<SessionSearchMatch>, usize) {
    let normalized_query = query.to_lowercase();
    let mut ordered_entries = entries.iter().collect::<Vec<_>>();
    ordered_entries.sort_by_key(|entry| entry.id);

    let mut matches = Vec::new();
    let mut total_matches = 0usize;
    for entry in ordered_entries {
        let role = session_message_role(&entry.message);
        if let Some(role_filter) = role_filter {
            if role != role_filter {
                continue;
            }
        }
        let text = entry.message.text_content();
        let role_hit = role.contains(&normalized_query);
        let text_hit = text.to_lowercase().contains(&normalized_query);
        if !role_hit && !text_hit {
            continue;
        }

        total_matches += 1;
        if matches.len() >= max_results {
            continue;
        }
        matches.push(SessionSearchMatch {
            id: entry.id,
            parent_id: entry.parent_id,
            role,
            preview: session_message_preview(&entry.message),
        });
    }

    (matches, total_matches)
}

fn render_session_search(
    query: &str,
    role_filter: Option<&str>,
    entries_count: usize,
    matches: &[SessionSearchMatch],
    total_matches: usize,
    max_results: usize,
) -> String {
    let role = role_filter.unwrap_or("any");
    let mut lines = vec![format!(
        "session search: query=\"{}\" role={} entries={} matches={} shown={} limit={}",
        query,
        role,
        entries_count,
        total_matches,
        matches.len(),
        max_results
    )];
    if matches.is_empty() {
        lines.push("results: none".to_string());
        return lines.join("\n");
    }

    for item in matches {
        lines.push(format!(
            "result: id={} parent={} role={} preview={}",
            item.id,
            item.parent_id
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            item.role,
            item.preview
        ));
    }
    lines.join("\n")
}

pub fn execute_session_search_command(runtime: &SessionRuntime, command_args: &str) -> String {
    let args = match parse_session_search_args(command_args) {
        Ok(args) => args,
        Err(error) => return format!("session search error: error={error}"),
    };

    let (matches, total_matches) = search_session_entries(
        runtime.store.entries(),
        &args.query,
        args.role.as_deref(),
        args.limit,
    );
    render_session_search(
        &args.query,
        args.role.as_deref(),
        runtime.store.entries().len(),
        &matches,
        total_matches,
        args.limit,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDiffEntry {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub role: String,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDiffReport {
    pub source: &'static str,
    pub left_id: u64,
    pub right_id: u64,
    pub shared_depth: usize,
    pub left_depth: usize,
    pub right_depth: usize,
    pub shared_entries: Vec<SessionDiffEntry>,
    pub left_only_entries: Vec<SessionDiffEntry>,
    pub right_only_entries: Vec<SessionDiffEntry>,
}

pub fn parse_session_diff_args(command_args: &str) -> Result<Option<(u64, u64)>> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Ok(None);
    }
    if tokens.len() != 2 {
        bail!("usage: /session-diff [<left-id> <right-id>]");
    }
    let left = tokens[0].parse::<u64>().with_context(|| {
        format!(
            "invalid left session id '{}'; expected an integer",
            tokens[0]
        )
    })?;
    let right = tokens[1].parse::<u64>().with_context(|| {
        format!(
            "invalid right session id '{}'; expected an integer",
            tokens[1]
        )
    })?;
    Ok(Some((left, right)))
}

fn session_diff_entry(entry: &SessionEntry) -> SessionDiffEntry {
    SessionDiffEntry {
        id: entry.id,
        parent_id: entry.parent_id,
        role: session_message_role(&entry.message),
        preview: session_message_preview(&entry.message),
    }
}

pub fn shared_lineage_prefix_depth(left: &[SessionEntry], right: &[SessionEntry]) -> usize {
    let mut depth = 0usize;
    for (left_entry, right_entry) in left.iter().zip(right.iter()) {
        if left_entry.id != right_entry.id {
            break;
        }
        depth += 1;
    }
    depth
}

fn resolve_session_diff_heads(
    runtime: &SessionRuntime,
    heads: Option<(u64, u64)>,
) -> Result<(u64, u64, &'static str)> {
    match heads {
        Some((left_id, right_id)) => {
            if !runtime.store.contains(left_id) {
                bail!("unknown left session id {left_id}");
            }
            if !runtime.store.contains(right_id) {
                bail!("unknown right session id {right_id}");
            }
            Ok((left_id, right_id, "explicit"))
        }
        None => {
            let left_id = runtime
                .active_head
                .ok_or_else(|| anyhow!("active head is not set"))?;
            if !runtime.store.contains(left_id) {
                bail!("active head {} does not exist in session", left_id);
            }
            let right_id = runtime
                .store
                .head_id()
                .ok_or_else(|| anyhow!("latest head is not set"))?;
            Ok((left_id, right_id, "default"))
        }
    }
}

fn compute_session_diff(
    runtime: &SessionRuntime,
    heads: Option<(u64, u64)>,
) -> Result<SessionDiffReport> {
    let (left_id, right_id, source) = resolve_session_diff_heads(runtime, heads)?;
    let left_lineage = runtime.store.lineage_entries(Some(left_id))?;
    let right_lineage = runtime.store.lineage_entries(Some(right_id))?;
    let shared_depth = shared_lineage_prefix_depth(&left_lineage, &right_lineage);

    Ok(SessionDiffReport {
        source,
        left_id,
        right_id,
        shared_depth,
        left_depth: left_lineage.len(),
        right_depth: right_lineage.len(),
        shared_entries: left_lineage
            .iter()
            .take(shared_depth)
            .map(session_diff_entry)
            .collect(),
        left_only_entries: left_lineage
            .iter()
            .skip(shared_depth)
            .map(session_diff_entry)
            .collect(),
        right_only_entries: right_lineage
            .iter()
            .skip(shared_depth)
            .map(session_diff_entry)
            .collect(),
    })
}

fn render_session_diff_entry(prefix: &str, entry: &SessionDiffEntry) -> String {
    format!(
        "{prefix}: id={} parent={} role={} preview={}",
        entry.id,
        entry
            .parent_id
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        entry.role,
        entry.preview
    )
}

pub fn render_session_diff(report: &SessionDiffReport) -> String {
    let mut lines = vec![
        format!(
            "session diff: source={} left={} right={}",
            report.source, report.left_id, report.right_id
        ),
        format!(
            "summary: shared_depth={} left_depth={} right_depth={} left_only={} right_only={}",
            report.shared_depth,
            report.left_depth,
            report.right_depth,
            report.left_only_entries.len(),
            report.right_only_entries.len()
        ),
    ];

    if report.shared_entries.is_empty() {
        lines.push("shared: none".to_string());
    } else {
        for entry in &report.shared_entries {
            lines.push(render_session_diff_entry("shared", entry));
        }
    }

    if report.left_only_entries.is_empty() {
        lines.push("left-only: none".to_string());
    } else {
        for entry in &report.left_only_entries {
            lines.push(render_session_diff_entry("left-only", entry));
        }
    }

    if report.right_only_entries.is_empty() {
        lines.push("right-only: none".to_string());
    } else {
        for entry in &report.right_only_entries {
            lines.push(render_session_diff_entry("right-only", entry));
        }
    }

    lines.join("\n")
}

pub fn execute_session_diff_command(runtime: &SessionRuntime, heads: Option<(u64, u64)>) -> String {
    match compute_session_diff(runtime, heads) {
        Ok(report) => render_session_diff(&report),
        Err(error) => format!("session diff error: {error}"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStats {
    pub entries: usize,
    pub branch_tips: usize,
    pub roots: usize,
    pub max_depth: usize,
    pub active_depth: Option<usize>,
    pub latest_depth: Option<usize>,
    pub active_head: Option<u64>,
    pub latest_head: Option<u64>,
    pub active_is_latest: bool,
    pub role_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatsOutputFormat {
    Text,
    Json,
}

pub fn parse_session_stats_args(command_args: &str) -> Result<SessionStatsOutputFormat> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Ok(SessionStatsOutputFormat::Text);
    }
    if tokens.len() == 1 && tokens[0] == "--json" {
        return Ok(SessionStatsOutputFormat::Json);
    }
    bail!("usage: /session-stats [--json]");
}

pub fn compute_session_entry_depths(entries: &[SessionEntry]) -> Result<HashMap<u64, usize>> {
    let mut parent_by_id = HashMap::new();
    for entry in entries {
        if parent_by_id.insert(entry.id, entry.parent_id).is_some() {
            bail!("duplicate session entry id {}", entry.id);
        }
    }

    fn depth_for(
        id: u64,
        parent_by_id: &HashMap<u64, Option<u64>>,
        memo: &mut HashMap<u64, usize>,
        visiting: &mut HashSet<u64>,
    ) -> Result<usize> {
        if let Some(depth) = memo.get(&id) {
            return Ok(*depth);
        }
        if !visiting.insert(id) {
            bail!("detected cycle while computing depth at session id {id}");
        }

        let Some(parent_id) = parent_by_id.get(&id) else {
            bail!("unknown session entry id {}", id);
        };
        let depth = match parent_id {
            None => 1,
            Some(parent_id) => {
                if !parent_by_id.contains_key(parent_id) {
                    bail!("missing parent id {} for session entry {}", parent_id, id);
                }
                depth_for(*parent_id, parent_by_id, memo, visiting)? + 1
            }
        };
        visiting.remove(&id);
        memo.insert(id, depth);
        Ok(depth)
    }

    let mut memo = HashMap::new();
    for id in parent_by_id.keys().copied() {
        let mut visiting = HashSet::new();
        let _ = depth_for(id, &parent_by_id, &mut memo, &mut visiting)?;
    }
    Ok(memo)
}

pub fn compute_session_stats(runtime: &SessionRuntime) -> Result<SessionStats> {
    let entries = runtime.store.entries();
    let depths = compute_session_entry_depths(entries)?;
    let mut role_counts = BTreeMap::new();
    for entry in entries {
        let role = session_message_role(&entry.message);
        *role_counts.entry(role).or_insert(0) += 1;
    }

    let latest_head = runtime.store.head_id();
    let latest_depth = latest_head.and_then(|id| depths.get(&id).copied());
    let active_depth = match runtime.active_head {
        Some(id) => Some(
            *depths
                .get(&id)
                .ok_or_else(|| anyhow!("active head {} does not exist in session", id))?,
        ),
        None => None,
    };

    Ok(SessionStats {
        entries: entries.len(),
        branch_tips: runtime.store.branch_tips().len(),
        roots: entries
            .iter()
            .filter(|entry| entry.parent_id.is_none())
            .count(),
        max_depth: depths.values().copied().max().unwrap_or(0),
        active_depth,
        latest_depth,
        active_head: runtime.active_head,
        latest_head,
        active_is_latest: runtime.active_head == latest_head,
        role_counts,
    })
}

pub fn render_session_stats(stats: &SessionStats) -> String {
    let mut lines = vec![format!(
        "session stats: entries={} branch_tips={} roots={} max_depth={}",
        stats.entries, stats.branch_tips, stats.roots, stats.max_depth
    )];
    lines.push(format!(
        "heads: active={} latest={} active_is_latest={}",
        stats
            .active_head
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        stats
            .latest_head
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        stats.active_is_latest
    ));
    lines.push(format!(
        "depth: active={} latest={}",
        stats
            .active_depth
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        stats
            .latest_depth
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string())
    ));

    if stats.role_counts.is_empty() {
        lines.push("roles: none".to_string());
    } else {
        for (role, count) in &stats.role_counts {
            lines.push(format!("role: {}={}", role, count));
        }
    }

    lines.join("\n")
}

pub fn render_session_stats_json(stats: &SessionStats) -> String {
    serde_json::json!({
        "entries": stats.entries,
        "branch_tips": stats.branch_tips,
        "roots": stats.roots,
        "max_depth": stats.max_depth,
        "active_depth": stats.active_depth,
        "latest_depth": stats.latest_depth,
        "active_head": stats.active_head,
        "latest_head": stats.latest_head,
        "active_is_latest": stats.active_is_latest,
        "role_counts": stats.role_counts,
    })
    .to_string()
}

pub fn execute_session_stats_command(
    runtime: &SessionRuntime,
    format: SessionStatsOutputFormat,
) -> String {
    match compute_session_stats(runtime) {
        Ok(stats) => match format {
            SessionStatsOutputFormat::Text => render_session_stats(&stats),
            SessionStatsOutputFormat::Json => render_session_stats_json(&stats),
        },
        Err(error) => match format {
            SessionStatsOutputFormat::Text => format!("session stats error: {error}"),
            SessionStatsOutputFormat::Json => serde_json::json!({
                "error": format!("session stats error: {error}")
            })
            .to_string(),
        },
    }
}
