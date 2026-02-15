//! Branch alias parsing and navigation command helpers for session runtime.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use tau_core::write_text_atomic;

use crate::{session_lineage_messages, SessionRuntime, SessionStore};

pub const BRANCH_ALIAS_SCHEMA_VERSION: u32 = 1;
pub const BRANCH_ALIAS_USAGE: &str = "usage: /branch-alias <set|list|use> ...";

#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `BranchAliasCommand` values.
pub enum BranchAliasCommand {
    List,
    Set { name: String, id: u64 },
    Use { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `BranchAliasFile` used across Tau components.
pub struct BranchAliasFile {
    pub schema_version: u32,
    pub aliases: BTreeMap<String, u64>,
}

pub fn branch_alias_path_for_session(session_path: &Path) -> PathBuf {
    session_path.with_extension("aliases.json")
}

pub fn validate_branch_alias_name(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        bail!("alias name must not be empty");
    };
    if !first.is_ascii_alphabetic() {
        bail!("alias name '{}' must start with an ASCII letter", name);
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_')) {
        bail!(
            "alias name '{}' must contain only ASCII letters, digits, '-' or '_'",
            name
        );
    }
    Ok(())
}

pub fn parse_branch_alias_command(command_args: &str) -> Result<BranchAliasCommand> {
    const USAGE_LIST: &str = "usage: /branch-alias list";
    const USAGE_SET: &str = "usage: /branch-alias set <name> <id>";
    const USAGE_USE: &str = "usage: /branch-alias use <name>";

    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("{BRANCH_ALIAS_USAGE}");
    }

    match tokens[0] {
        "list" => {
            if tokens.len() != 1 {
                bail!("{USAGE_LIST}");
            }
            Ok(BranchAliasCommand::List)
        }
        "set" => {
            if tokens.len() != 3 {
                bail!("{USAGE_SET}");
            }
            validate_branch_alias_name(tokens[1])?;
            let id = tokens[2]
                .parse::<u64>()
                .map_err(|_| anyhow!("invalid branch id '{}'; expected an integer", tokens[2]))?;
            Ok(BranchAliasCommand::Set {
                name: tokens[1].to_string(),
                id,
            })
        }
        "use" => {
            if tokens.len() != 2 {
                bail!("{USAGE_USE}");
            }
            validate_branch_alias_name(tokens[1])?;
            Ok(BranchAliasCommand::Use {
                name: tokens[1].to_string(),
            })
        }
        other => bail!("unknown subcommand '{}'; {BRANCH_ALIAS_USAGE}", other),
    }
}

pub fn load_branch_aliases(path: &Path) -> Result<BTreeMap<String, u64>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read alias file {}", path.display()))?;
    let parsed = serde_json::from_str::<BranchAliasFile>(&raw)
        .with_context(|| format!("failed to parse alias file {}", path.display()))?;
    if parsed.schema_version != BRANCH_ALIAS_SCHEMA_VERSION {
        bail!(
            "unsupported alias schema_version {} in {} (expected {})",
            parsed.schema_version,
            path.display(),
            BRANCH_ALIAS_SCHEMA_VERSION
        );
    }
    Ok(parsed.aliases)
}

pub fn save_branch_aliases(path: &Path, aliases: &BTreeMap<String, u64>) -> Result<()> {
    let payload = BranchAliasFile {
        schema_version: BRANCH_ALIAS_SCHEMA_VERSION,
        aliases: aliases.clone(),
    };
    let mut encoded =
        serde_json::to_string_pretty(&payload).context("failed to encode branch aliases")?;
    encoded.push('\n');
    write_text_atomic(path, &encoded)
}

pub const SESSION_NAVIGATION_SCHEMA_VERSION: u32 = 1;
const SESSION_NAVIGATION_MAX_STACK_DEPTH: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `SessionNavigationState` used across Tau components.
pub struct SessionNavigationState {
    pub schema_version: u32,
    pub current_head: Option<u64>,
    pub undo_stack: Vec<Option<u64>>,
    pub redo_stack: Vec<Option<u64>>,
}

impl Default for SessionNavigationState {
    fn default() -> Self {
        Self {
            schema_version: SESSION_NAVIGATION_SCHEMA_VERSION,
            current_head: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Public struct `SessionNavigationTransition` used across Tau components.
pub struct SessionNavigationTransition {
    pub previous_head: Option<u64>,
    pub active_head: Option<u64>,
    pub undo_depth: usize,
    pub redo_depth: usize,
    pub skipped_invalid_targets: usize,
    pub changed: bool,
}

pub fn session_navigation_path_for_session(session_path: &Path) -> PathBuf {
    session_path.with_extension("navigation.json")
}

pub fn load_session_navigation_state(path: &Path) -> Result<SessionNavigationState> {
    if !path.exists() {
        return Ok(SessionNavigationState::default());
    }

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read navigation file {}", path.display()))?;
    let parsed = serde_json::from_str::<SessionNavigationState>(&raw)
        .with_context(|| format!("failed to parse navigation file {}", path.display()))?;
    if parsed.schema_version != SESSION_NAVIGATION_SCHEMA_VERSION {
        bail!(
            "unsupported navigation schema_version {} in {} (expected {})",
            parsed.schema_version,
            path.display(),
            SESSION_NAVIGATION_SCHEMA_VERSION
        );
    }
    Ok(parsed)
}

pub fn save_session_navigation_state(path: &Path, state: &SessionNavigationState) -> Result<()> {
    let mut state_to_save = state.clone();
    state_to_save.schema_version = SESSION_NAVIGATION_SCHEMA_VERSION;
    let mut encoded = serde_json::to_string_pretty(&state_to_save)
        .context("failed to encode session navigation state")?;
    encoded.push('\n');
    write_text_atomic(path, &encoded)
}

fn trim_navigation_stack(stack: &mut Vec<Option<u64>>) {
    if stack.len() <= SESSION_NAVIGATION_MAX_STACK_DEPTH {
        return;
    }
    let remove = stack.len() - SESSION_NAVIGATION_MAX_STACK_DEPTH;
    stack.drain(0..remove);
}

fn prune_navigation_stack(store: &SessionStore, stack: &mut Vec<Option<u64>>) -> usize {
    let initial = stack.len();
    stack.retain(|candidate| match candidate {
        Some(id) => store.contains(*id),
        None => true,
    });
    initial.saturating_sub(stack.len())
}

fn normalize_runtime_head(store: &SessionStore, head: Option<u64>) -> Option<u64> {
    match head {
        Some(id) if store.contains(id) => Some(id),
        Some(_) => None,
        None => None,
    }
}

fn prune_navigation_state(store: &SessionStore, state: &mut SessionNavigationState) -> usize {
    let mut skipped = 0usize;
    if let Some(id) = state.current_head {
        if !store.contains(id) {
            state.current_head = None;
            skipped += 1;
        }
    }
    skipped += prune_navigation_stack(store, &mut state.undo_stack);
    skipped += prune_navigation_stack(store, &mut state.redo_stack);
    skipped
}

pub fn resolve_session_navigation_head(store: &SessionStore) -> Result<Option<u64>> {
    let state_path = session_navigation_path_for_session(store.path());
    let mut state = load_session_navigation_state(&state_path)?;
    let original_state = state.clone();
    let skipped_invalid_targets = prune_navigation_state(store, &mut state);
    if state.current_head.is_none() {
        state.current_head = store.head_id();
    }
    if skipped_invalid_targets > 0 || state != original_state {
        save_session_navigation_state(&state_path, &state)?;
    }
    Ok(state.current_head)
}

pub fn navigate_session_head(
    runtime: &mut SessionRuntime,
    target_head: Option<u64>,
) -> Result<SessionNavigationTransition> {
    if let Some(target) = target_head {
        if !runtime.store.contains(target) {
            bail!("unknown session id {}", target);
        }
    }

    let state_path = session_navigation_path_for_session(runtime.store.path());
    let mut state = load_session_navigation_state(&state_path)?;
    let previous_head = normalize_runtime_head(&runtime.store, runtime.active_head);
    let original_state = state.clone();
    let skipped_invalid_targets = prune_navigation_state(&runtime.store, &mut state);
    state.current_head = previous_head;

    let changed = previous_head != target_head;
    if changed {
        state.undo_stack.push(previous_head);
        trim_navigation_stack(&mut state.undo_stack);
        state.redo_stack.clear();
        state.current_head = target_head;
    }

    runtime.active_head = state.current_head;
    if skipped_invalid_targets > 0 || state != original_state {
        save_session_navigation_state(&state_path, &state)?;
    }

    Ok(SessionNavigationTransition {
        previous_head,
        active_head: runtime.active_head,
        undo_depth: state.undo_stack.len(),
        redo_depth: state.redo_stack.len(),
        skipped_invalid_targets,
        changed,
    })
}

pub fn undo_session_head(runtime: &mut SessionRuntime) -> Result<SessionNavigationTransition> {
    let state_path = session_navigation_path_for_session(runtime.store.path());
    let mut state = load_session_navigation_state(&state_path)?;
    let previous_head = normalize_runtime_head(&runtime.store, runtime.active_head);
    let original_state = state.clone();
    let mut skipped_invalid_targets = prune_navigation_state(&runtime.store, &mut state);
    state.current_head = previous_head;

    let mut target_head = None;
    while let Some(candidate) = state.undo_stack.pop() {
        if candidate
            .map(|id| runtime.store.contains(id))
            .unwrap_or(true)
        {
            target_head = Some(candidate);
            break;
        }
        skipped_invalid_targets += 1;
    }

    let changed = if let Some(target_head) = target_head {
        state.redo_stack.push(previous_head);
        trim_navigation_stack(&mut state.redo_stack);
        state.current_head = target_head;
        true
    } else {
        false
    };

    runtime.active_head = state.current_head;
    if skipped_invalid_targets > 0 || state != original_state {
        save_session_navigation_state(&state_path, &state)?;
    }

    Ok(SessionNavigationTransition {
        previous_head,
        active_head: runtime.active_head,
        undo_depth: state.undo_stack.len(),
        redo_depth: state.redo_stack.len(),
        skipped_invalid_targets,
        changed,
    })
}

pub fn redo_session_head(runtime: &mut SessionRuntime) -> Result<SessionNavigationTransition> {
    let state_path = session_navigation_path_for_session(runtime.store.path());
    let mut state = load_session_navigation_state(&state_path)?;
    let previous_head = normalize_runtime_head(&runtime.store, runtime.active_head);
    let original_state = state.clone();
    let mut skipped_invalid_targets = prune_navigation_state(&runtime.store, &mut state);
    state.current_head = previous_head;

    let mut target_head = None;
    while let Some(candidate) = state.redo_stack.pop() {
        if candidate
            .map(|id| runtime.store.contains(id))
            .unwrap_or(true)
        {
            target_head = Some(candidate);
            break;
        }
        skipped_invalid_targets += 1;
    }

    let changed = if let Some(target_head) = target_head {
        state.undo_stack.push(previous_head);
        trim_navigation_stack(&mut state.undo_stack);
        state.current_head = target_head;
        true
    } else {
        false
    };

    runtime.active_head = state.current_head;
    if skipped_invalid_targets > 0 || state != original_state {
        save_session_navigation_state(&state_path, &state)?;
    }

    Ok(SessionNavigationTransition {
        previous_head,
        active_head: runtime.active_head,
        undo_depth: state.undo_stack.len(),
        redo_depth: state.redo_stack.len(),
        skipped_invalid_targets,
        changed,
    })
}

fn render_branch_alias_list(
    path: &Path,
    aliases: &BTreeMap<String, u64>,
    runtime: &SessionRuntime,
) -> String {
    let mut lines = vec![format!(
        "branch alias list: path={} count={}",
        path.display(),
        aliases.len()
    )];
    if aliases.is_empty() {
        lines.push("aliases: none".to_string());
        return lines.join("\n");
    }
    for (name, id) in aliases {
        let status = if runtime.store.contains(*id) {
            "ok"
        } else {
            "stale"
        };
        lines.push(format!("alias: name={} id={} status={}", name, id, status));
    }
    lines.join("\n")
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `SessionNavigationOutcome` used across Tau components.
pub struct SessionNavigationOutcome {
    pub message: String,
    pub reload_active_head: bool,
}

impl SessionNavigationOutcome {
    fn new(message: String, reload_active_head: bool) -> Self {
        Self {
            message,
            reload_active_head,
        }
    }
}

pub fn execute_branch_alias_command(
    command_args: &str,
    runtime: &mut SessionRuntime,
) -> SessionNavigationOutcome {
    let alias_path = branch_alias_path_for_session(runtime.store.path());
    let command = match parse_branch_alias_command(command_args) {
        Ok(command) => command,
        Err(error) => {
            return SessionNavigationOutcome::new(
                format!(
                    "branch alias error: path={} error={error}",
                    alias_path.display()
                ),
                false,
            );
        }
    };

    let mut aliases = match load_branch_aliases(&alias_path) {
        Ok(aliases) => aliases,
        Err(error) => {
            return SessionNavigationOutcome::new(
                format!(
                    "branch alias error: path={} error={error}",
                    alias_path.display()
                ),
                false,
            );
        }
    };

    match command {
        BranchAliasCommand::List => SessionNavigationOutcome::new(
            render_branch_alias_list(&alias_path, &aliases, runtime),
            false,
        ),
        BranchAliasCommand::Set { name, id } => {
            if !runtime.store.contains(id) {
                return SessionNavigationOutcome::new(
                    format!(
                        "branch alias error: path={} name={} error=unknown session id {}",
                        alias_path.display(),
                        name,
                        id
                    ),
                    false,
                );
            }
            aliases.insert(name.clone(), id);
            match save_branch_aliases(&alias_path, &aliases) {
                Ok(()) => SessionNavigationOutcome::new(
                    format!(
                        "branch alias set: path={} name={} id={}",
                        alias_path.display(),
                        name,
                        id
                    ),
                    false,
                ),
                Err(error) => SessionNavigationOutcome::new(
                    format!(
                        "branch alias error: path={} name={} error={error}",
                        alias_path.display(),
                        name
                    ),
                    false,
                ),
            }
        }
        BranchAliasCommand::Use { name } => {
            let Some(id) = aliases.get(&name).copied() else {
                return SessionNavigationOutcome::new(
                    format!(
                        "branch alias error: path={} name={} error=unknown alias '{}'",
                        alias_path.display(),
                        name,
                        name
                    ),
                    false,
                );
            };
            if !runtime.store.contains(id) {
                return SessionNavigationOutcome::new(
                    format!(
                        "branch alias error: path={} name={} error=alias points to unknown session id {}",
                        alias_path.display(),
                        name,
                        id
                    ),
                    false,
                );
            }
            if let Err(error) = navigate_session_head(runtime, Some(id)) {
                return SessionNavigationOutcome::new(
                    format!(
                        "branch alias error: path={} name={} error={error}",
                        alias_path.display(),
                        name
                    ),
                    false,
                );
            }
            match session_lineage_messages(runtime) {
                Ok(_) => SessionNavigationOutcome::new(
                    format!(
                        "branch alias use: path={} name={} id={}",
                        alias_path.display(),
                        name,
                        id
                    ),
                    true,
                ),
                Err(error) => SessionNavigationOutcome::new(
                    format!(
                        "branch alias error: path={} name={} error={error}",
                        alias_path.display(),
                        name
                    ),
                    false,
                ),
            }
        }
    }
}

pub const SESSION_BOOKMARK_SCHEMA_VERSION: u32 = 1;
pub const SESSION_BOOKMARK_USAGE: &str = "usage: /session-bookmark <set|list|use|delete> ...";

#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `SessionBookmarkCommand` values.
pub enum SessionBookmarkCommand {
    List,
    Set { name: String, id: u64 },
    Use { name: String },
    Delete { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `SessionBookmarkFile` used across Tau components.
pub struct SessionBookmarkFile {
    pub schema_version: u32,
    pub bookmarks: BTreeMap<String, u64>,
}

pub fn session_bookmark_path_for_session(session_path: &Path) -> PathBuf {
    session_path.with_extension("bookmarks.json")
}

pub fn parse_session_bookmark_command(command_args: &str) -> Result<SessionBookmarkCommand> {
    const USAGE_LIST: &str = "usage: /session-bookmark list";
    const USAGE_SET: &str = "usage: /session-bookmark set <name> <id>";
    const USAGE_USE: &str = "usage: /session-bookmark use <name>";
    const USAGE_DELETE: &str = "usage: /session-bookmark delete <name>";

    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("{SESSION_BOOKMARK_USAGE}");
    }

    match tokens[0] {
        "list" => {
            if tokens.len() != 1 {
                bail!("{USAGE_LIST}");
            }
            Ok(SessionBookmarkCommand::List)
        }
        "set" => {
            if tokens.len() != 3 {
                bail!("{USAGE_SET}");
            }
            validate_branch_alias_name(tokens[1])?;
            let id = tokens[2]
                .parse::<u64>()
                .map_err(|_| anyhow!("invalid bookmark id '{}'; expected an integer", tokens[2]))?;
            Ok(SessionBookmarkCommand::Set {
                name: tokens[1].to_string(),
                id,
            })
        }
        "use" => {
            if tokens.len() != 2 {
                bail!("{USAGE_USE}");
            }
            validate_branch_alias_name(tokens[1])?;
            Ok(SessionBookmarkCommand::Use {
                name: tokens[1].to_string(),
            })
        }
        "delete" => {
            if tokens.len() != 2 {
                bail!("{USAGE_DELETE}");
            }
            validate_branch_alias_name(tokens[1])?;
            Ok(SessionBookmarkCommand::Delete {
                name: tokens[1].to_string(),
            })
        }
        other => bail!("unknown subcommand '{}'; {SESSION_BOOKMARK_USAGE}", other),
    }
}

pub fn load_session_bookmarks(path: &Path) -> Result<BTreeMap<String, u64>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read session bookmark file {}", path.display()))?;
    let parsed = serde_json::from_str::<SessionBookmarkFile>(&raw)
        .with_context(|| format!("failed to parse session bookmark file {}", path.display()))?;
    if parsed.schema_version != SESSION_BOOKMARK_SCHEMA_VERSION {
        bail!(
            "unsupported session bookmark schema_version {} in {} (expected {})",
            parsed.schema_version,
            path.display(),
            SESSION_BOOKMARK_SCHEMA_VERSION
        );
    }
    Ok(parsed.bookmarks)
}

pub fn save_session_bookmarks(path: &Path, bookmarks: &BTreeMap<String, u64>) -> Result<()> {
    let payload = SessionBookmarkFile {
        schema_version: SESSION_BOOKMARK_SCHEMA_VERSION,
        bookmarks: bookmarks.clone(),
    };
    let mut encoded =
        serde_json::to_string_pretty(&payload).context("failed to encode session bookmarks")?;
    encoded.push('\n');
    write_text_atomic(path, &encoded)
}

fn render_session_bookmark_list(
    path: &Path,
    bookmarks: &BTreeMap<String, u64>,
    runtime: &SessionRuntime,
) -> String {
    let mut lines = vec![format!(
        "session bookmark list: path={} count={}",
        path.display(),
        bookmarks.len()
    )];
    if bookmarks.is_empty() {
        lines.push("bookmarks: none".to_string());
        return lines.join("\n");
    }
    for (name, id) in bookmarks {
        let status = if runtime.store.contains(*id) {
            "ok"
        } else {
            "stale"
        };
        lines.push(format!(
            "bookmark: name={} id={} status={}",
            name, id, status
        ));
    }
    lines.join("\n")
}

pub fn execute_session_bookmark_command(
    command_args: &str,
    runtime: &mut SessionRuntime,
) -> SessionNavigationOutcome {
    let bookmark_path = session_bookmark_path_for_session(runtime.store.path());
    let command = match parse_session_bookmark_command(command_args) {
        Ok(command) => command,
        Err(error) => {
            return SessionNavigationOutcome::new(
                format!(
                    "session bookmark error: path={} error={error}",
                    bookmark_path.display()
                ),
                false,
            );
        }
    };

    let mut bookmarks = match load_session_bookmarks(&bookmark_path) {
        Ok(bookmarks) => bookmarks,
        Err(error) => {
            return SessionNavigationOutcome::new(
                format!(
                    "session bookmark error: path={} error={error}",
                    bookmark_path.display()
                ),
                false,
            );
        }
    };

    match command {
        SessionBookmarkCommand::List => SessionNavigationOutcome::new(
            render_session_bookmark_list(&bookmark_path, &bookmarks, runtime),
            false,
        ),
        SessionBookmarkCommand::Set { name, id } => {
            if !runtime.store.contains(id) {
                return SessionNavigationOutcome::new(
                    format!(
                        "session bookmark error: path={} name={} error=unknown session id {}",
                        bookmark_path.display(),
                        name,
                        id
                    ),
                    false,
                );
            }
            bookmarks.insert(name.clone(), id);
            match save_session_bookmarks(&bookmark_path, &bookmarks) {
                Ok(()) => SessionNavigationOutcome::new(
                    format!(
                        "session bookmark set: path={} name={} id={}",
                        bookmark_path.display(),
                        name,
                        id
                    ),
                    false,
                ),
                Err(error) => SessionNavigationOutcome::new(
                    format!(
                        "session bookmark error: path={} name={} error={error}",
                        bookmark_path.display(),
                        name
                    ),
                    false,
                ),
            }
        }
        SessionBookmarkCommand::Use { name } => {
            let Some(id) = bookmarks.get(&name).copied() else {
                return SessionNavigationOutcome::new(
                    format!(
                        "session bookmark error: path={} name={} error=unknown bookmark '{}'",
                        bookmark_path.display(),
                        name,
                        name
                    ),
                    false,
                );
            };
            if !runtime.store.contains(id) {
                return SessionNavigationOutcome::new(
                    format!(
                        "session bookmark error: path={} name={} error=bookmark points to unknown session id {}",
                        bookmark_path.display(),
                        name,
                        id
                    ),
                    false,
                );
            }
            if let Err(error) = navigate_session_head(runtime, Some(id)) {
                return SessionNavigationOutcome::new(
                    format!(
                        "session bookmark error: path={} name={} error={error}",
                        bookmark_path.display(),
                        name
                    ),
                    false,
                );
            }
            match session_lineage_messages(runtime) {
                Ok(_) => SessionNavigationOutcome::new(
                    format!(
                        "session bookmark use: path={} name={} id={}",
                        bookmark_path.display(),
                        name,
                        id
                    ),
                    true,
                ),
                Err(error) => SessionNavigationOutcome::new(
                    format!(
                        "session bookmark error: path={} name={} error={error}",
                        bookmark_path.display(),
                        name
                    ),
                    false,
                ),
            }
        }
        SessionBookmarkCommand::Delete { name } => {
            if bookmarks.remove(&name).is_none() {
                return SessionNavigationOutcome::new(
                    format!(
                        "session bookmark error: path={} name={} error=unknown bookmark '{}'",
                        bookmark_path.display(),
                        name,
                        name
                    ),
                    false,
                );
            }
            match save_session_bookmarks(&bookmark_path, &bookmarks) {
                Ok(()) => SessionNavigationOutcome::new(
                    format!(
                        "session bookmark delete: path={} name={} status=deleted remaining={}",
                        bookmark_path.display(),
                        name,
                        bookmarks.len()
                    ),
                    false,
                ),
                Err(error) => SessionNavigationOutcome::new(
                    format!(
                        "session bookmark error: path={} name={} error={error}",
                        bookmark_path.display(),
                        name
                    ),
                    false,
                ),
            }
        }
    }
}
