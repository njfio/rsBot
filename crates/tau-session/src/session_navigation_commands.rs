use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use tau_core::write_text_atomic;

use crate::{session_lineage_messages, SessionRuntime};

pub const BRANCH_ALIAS_SCHEMA_VERSION: u32 = 1;
pub const BRANCH_ALIAS_USAGE: &str = "usage: /branch-alias <set|list|use> ...";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchAliasCommand {
    List,
    Set { name: String, id: u64 },
    Use { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
            runtime.active_head = Some(id);
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
pub enum SessionBookmarkCommand {
    List,
    Set { name: String, id: u64 },
    Use { name: String },
    Delete { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
            runtime.active_head = Some(id);
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
