use super::*;
use crate::channel_store::{ChannelLogEntry, ChannelStore};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::BufRead;
use std::io::Write;
#[cfg(test)]
use std::path::Component;
use std::path::{Path, PathBuf};
use yrs::updates::decoder::Decode;
use yrs::{Doc, Map, MapPrelim, MapRef, Out, ReadTxn, StateVector, Transact, Update};

const CANVAS_SCHEMA_VERSION: u32 = 1;
const CANVAS_EVENT_SCHEMA_VERSION: u32 = 1;
const CANVAS_SESSION_LINK_SCHEMA_VERSION: u32 = 1;
const CANVAS_ROOT_TYPE: &str = "canvas";
const CANVAS_NODES_KEY: &str = "nodes";
const CANVAS_EDGES_KEY: &str = "edges";

pub(crate) const CANVAS_USAGE: &str =
    "/canvas <create|update|show|export|import> <canvas_id> ... (run /help /canvas)";

fn canvas_event_schema_version() -> u32 {
    CANVAS_EVENT_SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CanvasEventOrigin {
    pub(crate) transport: String,
    pub(crate) channel: Option<String>,
    pub(crate) source_event_key: Option<String>,
    pub(crate) source_unix_ms: Option<u64>,
}

impl Default for CanvasEventOrigin {
    fn default() -> Self {
        Self {
            transport: "local".to_string(),
            channel: None,
            source_event_key: None,
            source_unix_ms: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CanvasSessionLinkContext {
    pub(crate) session_path: PathBuf,
    pub(crate) session_head_id: Option<u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct CanvasCommandConfig {
    pub(crate) canvas_root: PathBuf,
    pub(crate) channel_store_root: PathBuf,
    pub(crate) principal: String,
    pub(crate) origin: CanvasEventOrigin,
    pub(crate) session_link: Option<CanvasSessionLinkContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct CanvasNode {
    pub id: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct CanvasEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct CanvasSnapshot {
    pub schema_version: u32,
    pub canvas_id: String,
    pub nodes: Vec<CanvasNode>,
    pub edges: Vec<CanvasEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct CanvasStoreMeta {
    schema_version: u32,
    canvas_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct CanvasEventEntry {
    #[serde(default = "canvas_event_schema_version")]
    schema_version: u32,
    #[serde(default)]
    event_id: String,
    timestamp_unix_ms: u64,
    principal: String,
    action: String,
    details: Value,
    #[serde(default)]
    origin: CanvasEventOriginRecord,
    #[serde(default)]
    session_link: Option<CanvasSessionLinkRecord>,
}

#[derive(Debug, Clone, PartialEq)]
enum CanvasCommand {
    Create {
        canvas_id: String,
    },
    Update {
        canvas_id: String,
        op: CanvasUpdateOp,
    },
    Show {
        canvas_id: String,
        format: CanvasShowFormat,
    },
    Export {
        canvas_id: String,
        format: CanvasExportFormat,
        path: Option<PathBuf>,
    },
    Import {
        canvas_id: String,
        path: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum CanvasUpdateOp {
    NodeUpsert {
        node_id: String,
        label: String,
        x: f64,
        y: f64,
    },
    NodeRemove {
        node_id: String,
    },
    EdgeUpsert {
        edge_id: String,
        from: String,
        to: String,
        label: Option<String>,
    },
    EdgeRemove {
        edge_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CanvasShowFormat {
    Markdown,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CanvasExportFormat {
    Markdown,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CanvasEventOriginRecord {
    transport: String,
    channel: Option<String>,
    source_event_key: Option<String>,
    source_unix_ms: Option<u64>,
}

impl Default for CanvasEventOriginRecord {
    fn default() -> Self {
        Self {
            transport: "local".to_string(),
            channel: None,
            source_event_key: None,
            source_unix_ms: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CanvasSessionLinkRecord {
    session_path: String,
    session_head_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CanvasSessionLinkEntry {
    schema_version: u32,
    timestamp_unix_ms: u64,
    event_id: String,
    principal: String,
    canvas_id: String,
    session_path: String,
    session_head_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ReplayGuardDecision {
    Apply,
    Duplicate,
    OutOfOrder {
        source_stream: String,
        latest_event_id: String,
        latest_source_unix_ms: u64,
    },
}

impl CanvasExportFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Json => "json",
        }
    }
}

pub(crate) fn execute_canvas_command(command_args: &str, config: &CanvasCommandConfig) -> String {
    match parse_canvas_command(command_args) {
        Ok(command) => match run_canvas_command(command, config) {
            Ok(output) => output,
            Err(error) => format!("canvas error: {error}"),
        },
        Err(error) => format!("canvas usage error: {error}\nusage: {CANVAS_USAGE}"),
    }
}

fn parse_canvas_command(command_args: &str) -> Result<CanvasCommand> {
    let tokens = shell_words::split(command_args)
        .map_err(|error| anyhow!("invalid canvas arguments: {error}"))?;
    if tokens.is_empty() {
        bail!("missing canvas subcommand");
    }

    match tokens[0].as_str() {
        "create" => {
            if tokens.len() != 2 {
                bail!("usage: /canvas create <canvas_id>");
            }
            Ok(CanvasCommand::Create {
                canvas_id: tokens[1].clone(),
            })
        }
        "update" => parse_canvas_update_command(&tokens),
        "show" => parse_canvas_show_command(&tokens),
        "export" => parse_canvas_export_command(&tokens),
        "import" => parse_canvas_import_command(&tokens),
        other => bail!("unknown canvas subcommand '{other}'"),
    }
}

fn parse_canvas_update_command(tokens: &[String]) -> Result<CanvasCommand> {
    if tokens.len() < 4 {
        bail!("usage: /canvas update <canvas_id> <node-upsert|node-remove|edge-upsert|edge-remove> ...");
    }
    let canvas_id = tokens[1].clone();
    let operation = tokens[2].as_str();
    let op = match operation {
        "node-upsert" => {
            if tokens.len() < 7 {
                bail!("usage: /canvas update <canvas_id> node-upsert <node_id> <label> <x> <y>");
            }
            let node_id = tokens[3].clone();
            let x = tokens[tokens.len() - 2]
                .parse::<f64>()
                .map_err(|_| anyhow!("invalid x coordinate '{}'", tokens[tokens.len() - 2]))?;
            let y = tokens[tokens.len() - 1]
                .parse::<f64>()
                .map_err(|_| anyhow!("invalid y coordinate '{}'", tokens[tokens.len() - 1]))?;
            let label = tokens[4..tokens.len() - 2].join(" ").trim().to_string();
            if label.is_empty() {
                bail!("node label must be non-empty");
            }
            CanvasUpdateOp::NodeUpsert {
                node_id,
                label,
                x,
                y,
            }
        }
        "node-remove" => {
            if tokens.len() != 4 {
                bail!("usage: /canvas update <canvas_id> node-remove <node_id>");
            }
            CanvasUpdateOp::NodeRemove {
                node_id: tokens[3].clone(),
            }
        }
        "edge-upsert" => {
            if tokens.len() < 6 {
                bail!(
                    "usage: /canvas update <canvas_id> edge-upsert <edge_id> <from_node> <to_node> [label]"
                );
            }
            let label = if tokens.len() > 6 {
                let rendered = tokens[6..].join(" ").trim().to_string();
                if rendered.is_empty() {
                    None
                } else {
                    Some(rendered)
                }
            } else {
                None
            };
            CanvasUpdateOp::EdgeUpsert {
                edge_id: tokens[3].clone(),
                from: tokens[4].clone(),
                to: tokens[5].clone(),
                label,
            }
        }
        "edge-remove" => {
            if tokens.len() != 4 {
                bail!("usage: /canvas update <canvas_id> edge-remove <edge_id>");
            }
            CanvasUpdateOp::EdgeRemove {
                edge_id: tokens[3].clone(),
            }
        }
        other => bail!("unknown canvas update operation '{other}'"),
    };

    Ok(CanvasCommand::Update { canvas_id, op })
}

fn parse_canvas_show_command(tokens: &[String]) -> Result<CanvasCommand> {
    if tokens.len() < 2 || tokens.len() > 3 {
        bail!("usage: /canvas show <canvas_id> [--json]");
    }
    let format = if tokens.len() == 3 {
        if tokens[2] != "--json" {
            bail!("usage: /canvas show <canvas_id> [--json]");
        }
        CanvasShowFormat::Json
    } else {
        CanvasShowFormat::Markdown
    };

    Ok(CanvasCommand::Show {
        canvas_id: tokens[1].clone(),
        format,
    })
}

fn parse_canvas_export_command(tokens: &[String]) -> Result<CanvasCommand> {
    if tokens.len() < 3 || tokens.len() > 4 {
        bail!("usage: /canvas export <canvas_id> <markdown|json> [path]");
    }

    let format = match tokens[2].as_str() {
        "markdown" | "md" => CanvasExportFormat::Markdown,
        "json" => CanvasExportFormat::Json,
        other => bail!("unsupported export format '{other}', expected markdown|json"),
    };
    let path = if tokens.len() == 4 {
        Some(PathBuf::from(tokens[3].clone()))
    } else {
        None
    };

    Ok(CanvasCommand::Export {
        canvas_id: tokens[1].clone(),
        format,
        path,
    })
}

fn parse_canvas_import_command(tokens: &[String]) -> Result<CanvasCommand> {
    if tokens.len() != 3 {
        bail!("usage: /canvas import <canvas_id> <path>");
    }
    Ok(CanvasCommand::Import {
        canvas_id: tokens[1].clone(),
        path: PathBuf::from(tokens[2].clone()),
    })
}

fn run_canvas_command(command: CanvasCommand, config: &CanvasCommandConfig) -> Result<String> {
    match command {
        CanvasCommand::Create { canvas_id } => execute_canvas_create(config, &canvas_id),
        CanvasCommand::Update { canvas_id, op } => execute_canvas_update(config, &canvas_id, op),
        CanvasCommand::Show { canvas_id, format } => {
            execute_canvas_show(config, &canvas_id, format)
        }
        CanvasCommand::Export {
            canvas_id,
            format,
            path,
        } => execute_canvas_export(config, &canvas_id, format, path.as_deref()),
        CanvasCommand::Import { canvas_id, path } => {
            execute_canvas_import(config, &canvas_id, &path)
        }
    }
}

fn execute_canvas_create(config: &CanvasCommandConfig, canvas_id: &str) -> Result<String> {
    let store = CanvasStore::open(&config.canvas_root, canvas_id)?;
    let origin = canvas_event_origin_record(config);
    let event_id = canvas_event_id(config, canvas_id, "create");
    match evaluate_replay_guard(&store, &event_id, &origin)? {
        ReplayGuardDecision::Apply => {}
        decision => {
            return Ok(render_replay_guard_message(
                canvas_id, "create", &event_id, &decision,
            ))
        }
    }
    let doc = store.load_doc()?;
    initialize_canvas_document(&doc);
    store.save_doc(&doc)?;
    let snapshot = canvas_snapshot_from_doc(&doc, canvas_id)?;
    let event = CanvasEventEntry {
        schema_version: CANVAS_EVENT_SCHEMA_VERSION,
        event_id: event_id.clone(),
        timestamp_unix_ms: current_unix_timestamp_ms(),
        principal: config.principal.clone(),
        action: "create".to_string(),
        details: json!({
            "canvas_id": canvas_id,
            "nodes": snapshot.nodes.len(),
            "edges": snapshot.edges.len(),
        }),
        origin,
        session_link: canvas_session_link_record(config),
    };
    store.append_event(&event)?;
    store.append_session_link(canvas_id, &event)?;
    append_canvas_event_to_channel_store(config, canvas_id, &event)?;
    Ok(format!(
        "canvas create: id={} path={} nodes={} edges={} event_id={}",
        canvas_id,
        store.canvas_dir().display(),
        snapshot.nodes.len(),
        snapshot.edges.len(),
        event_id
    ))
}

fn execute_canvas_update(
    config: &CanvasCommandConfig,
    canvas_id: &str,
    op: CanvasUpdateOp,
) -> Result<String> {
    let store = CanvasStore::open(&config.canvas_root, canvas_id)?;
    let action = canvas_update_action_name(&op);
    let origin = canvas_event_origin_record(config);
    let event_id = canvas_event_id(config, canvas_id, action);
    match evaluate_replay_guard(&store, &event_id, &origin)? {
        ReplayGuardDecision::Apply => {}
        decision => {
            return Ok(render_replay_guard_message(
                canvas_id, action, &event_id, &decision,
            ))
        }
    }
    let doc = store.load_doc()?;
    let (action, details) = apply_canvas_update(&doc, canvas_id, op)?;
    let event = CanvasEventEntry {
        schema_version: CANVAS_EVENT_SCHEMA_VERSION,
        event_id: event_id.clone(),
        timestamp_unix_ms: current_unix_timestamp_ms(),
        principal: config.principal.clone(),
        action: action.to_string(),
        details,
        origin,
        session_link: canvas_session_link_record(config),
    };
    store.save_doc(&doc)?;
    store.append_event(&event)?;
    store.append_session_link(canvas_id, &event)?;
    append_canvas_event_to_channel_store(config, canvas_id, &event)?;
    let snapshot = canvas_snapshot_from_doc(&doc, canvas_id)?;
    Ok(format!(
        "canvas update: id={} action={} nodes={} edges={} event_id={}",
        canvas_id,
        event.action,
        snapshot.nodes.len(),
        snapshot.edges.len(),
        event_id
    ))
}

fn execute_canvas_show(
    config: &CanvasCommandConfig,
    canvas_id: &str,
    format: CanvasShowFormat,
) -> Result<String> {
    let store = CanvasStore::open(&config.canvas_root, canvas_id)?;
    let doc = store.load_doc()?;
    let snapshot = canvas_snapshot_from_doc(&doc, canvas_id)?;
    Ok(match format {
        CanvasShowFormat::Markdown => render_canvas_markdown(&snapshot),
        CanvasShowFormat::Json => render_canvas_json(&snapshot)?,
    })
}

fn execute_canvas_export(
    config: &CanvasCommandConfig,
    canvas_id: &str,
    format: CanvasExportFormat,
    destination: Option<&Path>,
) -> Result<String> {
    let store = CanvasStore::open(&config.canvas_root, canvas_id)?;
    let doc = store.load_doc()?;
    let snapshot = canvas_snapshot_from_doc(&doc, canvas_id)?;
    let rendered = match format {
        CanvasExportFormat::Markdown => render_canvas_markdown(&snapshot),
        CanvasExportFormat::Json => render_canvas_json(&snapshot)?,
    };

    let destination = destination
        .map(PathBuf::from)
        .unwrap_or_else(|| default_canvas_export_path(&store, format));
    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    write_text_atomic(&destination, &rendered)
        .with_context(|| format!("failed to write {}", destination.display()))?;

    Ok(format!(
        "canvas export: id={} format={} path={} bytes={}",
        canvas_id,
        format.extension(),
        destination.display(),
        rendered.len()
    ))
}

fn execute_canvas_import(
    config: &CanvasCommandConfig,
    canvas_id: &str,
    source: &Path,
) -> Result<String> {
    let store = CanvasStore::open(&config.canvas_root, canvas_id)?;
    let origin = canvas_event_origin_record(config);
    let event_id = canvas_event_id(config, canvas_id, "import");
    match evaluate_replay_guard(&store, &event_id, &origin)? {
        ReplayGuardDecision::Apply => {}
        decision => {
            return Ok(render_replay_guard_message(
                canvas_id, "import", &event_id, &decision,
            ))
        }
    }

    let raw = std::fs::read_to_string(source)
        .with_context(|| format!("failed to read import snapshot {}", source.display()))?;
    let snapshot = serde_json::from_str::<CanvasSnapshot>(&raw)
        .with_context(|| format!("failed to parse import snapshot {}", source.display()))?;
    let snapshot = validate_import_snapshot(snapshot, canvas_id)?;

    let doc = store.load_doc()?;
    replace_doc_with_snapshot(&doc, &snapshot)?;
    store.save_doc(&doc)?;

    let event = CanvasEventEntry {
        schema_version: CANVAS_EVENT_SCHEMA_VERSION,
        event_id: event_id.clone(),
        timestamp_unix_ms: current_unix_timestamp_ms(),
        principal: config.principal.clone(),
        action: "import".to_string(),
        details: json!({
            "canvas_id": canvas_id,
            "source_path": source.display().to_string(),
            "nodes": snapshot.nodes.len(),
            "edges": snapshot.edges.len(),
        }),
        origin,
        session_link: canvas_session_link_record(config),
    };
    store.append_event(&event)?;
    store.append_session_link(canvas_id, &event)?;
    append_canvas_event_to_channel_store(config, canvas_id, &event)?;

    Ok(format!(
        "canvas import: id={} source={} nodes={} edges={} event_id={}",
        canvas_id,
        source.display(),
        snapshot.nodes.len(),
        snapshot.edges.len(),
        event_id
    ))
}

fn default_canvas_export_path(store: &CanvasStore, format: CanvasExportFormat) -> PathBuf {
    let stem = sanitize_for_path(&store.canvas_id);
    store
        .exports_dir()
        .join(format!("{}-snapshot.{}", stem, format.extension()))
}

fn canvas_update_action_name(op: &CanvasUpdateOp) -> &'static str {
    match op {
        CanvasUpdateOp::NodeUpsert { .. } => "node-upsert",
        CanvasUpdateOp::NodeRemove { .. } => "node-remove",
        CanvasUpdateOp::EdgeUpsert { .. } => "edge-upsert",
        CanvasUpdateOp::EdgeRemove { .. } => "edge-remove",
    }
}

fn canvas_event_origin_record(config: &CanvasCommandConfig) -> CanvasEventOriginRecord {
    let mut transport = config.origin.transport.trim().to_string();
    if transport.is_empty() {
        transport = "local".to_string();
    }
    CanvasEventOriginRecord {
        transport,
        channel: config
            .origin
            .channel
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        source_event_key: config
            .origin
            .source_event_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        source_unix_ms: config.origin.source_unix_ms.filter(|value| *value > 0),
    }
}

fn canvas_session_link_record(config: &CanvasCommandConfig) -> Option<CanvasSessionLinkRecord> {
    config
        .session_link
        .as_ref()
        .map(|link| CanvasSessionLinkRecord {
            session_path: link.session_path.display().to_string(),
            session_head_id: link.session_head_id,
        })
}

fn canvas_event_id(config: &CanvasCommandConfig, canvas_id: &str, action: &str) -> String {
    let origin = canvas_event_origin_record(config);
    if let Some(source_event_key) = origin.source_event_key.as_deref() {
        return format!(
            "{}:{}:{}:{}:{}",
            origin.transport,
            origin.channel.as_deref().unwrap_or("default"),
            source_event_key,
            canvas_id,
            action
        );
    }

    let timestamp_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let seed = format!(
        "{}:{}:{}:{}:{}",
        timestamp_ns, config.principal, origin.transport, canvas_id, action
    );
    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    let digest = hasher.finalize();
    format!(
        "local:{}:{}:{}:{:02x}{:02x}{:02x}{:02x}",
        canvas_id, action, timestamp_ns, digest[0], digest[1], digest[2], digest[3]
    )
}

fn evaluate_replay_guard(
    store: &CanvasStore,
    event_id: &str,
    origin: &CanvasEventOriginRecord,
) -> Result<ReplayGuardDecision> {
    let events = store.load_events()?;
    if events.iter().any(|event| event.event_id == event_id) {
        return Ok(ReplayGuardDecision::Duplicate);
    }

    let Some(source_unix_ms) = origin.source_unix_ms else {
        return Ok(ReplayGuardDecision::Apply);
    };
    let source_stream = source_stream_id(origin);
    if source_stream == "local" {
        return Ok(ReplayGuardDecision::Apply);
    }

    let latest = events
        .iter()
        .filter(|event| source_stream_id(&event.origin) == source_stream)
        .filter_map(|event| {
            event
                .origin
                .source_unix_ms
                .map(|unix_ms| (unix_ms, event.event_id.clone()))
        })
        .max_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));

    if let Some((latest_source_unix_ms, latest_event_id)) = latest {
        if (source_unix_ms, event_id) <= (latest_source_unix_ms, latest_event_id.as_str()) {
            return Ok(ReplayGuardDecision::OutOfOrder {
                source_stream,
                latest_event_id,
                latest_source_unix_ms,
            });
        }
    }

    Ok(ReplayGuardDecision::Apply)
}

fn source_stream_id(origin: &CanvasEventOriginRecord) -> String {
    let transport = origin.transport.trim();
    if transport.is_empty() || transport == "local" {
        return "local".to_string();
    }
    format!(
        "{}:{}",
        transport,
        origin.channel.as_deref().unwrap_or("default")
    )
}

fn render_replay_guard_message(
    canvas_id: &str,
    action: &str,
    event_id: &str,
    decision: &ReplayGuardDecision,
) -> String {
    match decision {
        ReplayGuardDecision::Apply => unreachable!("render helper only used for replay skips"),
        ReplayGuardDecision::Duplicate => format!(
            "canvas replay: id={} action={} status=duplicate-skipped event_id={}",
            canvas_id, action, event_id
        ),
        ReplayGuardDecision::OutOfOrder {
            source_stream,
            latest_event_id,
            latest_source_unix_ms,
        } => format!(
            "canvas replay: id={} action={} status=out-of-order-skipped event_id={} source={} latest_event_id={} latest_source_unix_ms={}",
            canvas_id, action, event_id, source_stream, latest_event_id, latest_source_unix_ms
        ),
    }
}

fn validate_import_snapshot(
    snapshot: CanvasSnapshot,
    expected_canvas_id: &str,
) -> Result<CanvasSnapshot> {
    if snapshot.schema_version != CANVAS_SCHEMA_VERSION {
        bail!(
            "unsupported canvas snapshot schema: expected {}, found {}",
            CANVAS_SCHEMA_VERSION,
            snapshot.schema_version
        );
    }
    if snapshot.canvas_id != expected_canvas_id {
        bail!(
            "canvas snapshot id mismatch: expected '{}', found '{}'",
            expected_canvas_id,
            snapshot.canvas_id
        );
    }

    let mut node_ids = HashSet::new();
    for node in &snapshot.nodes {
        let node_id = node.id.trim();
        if node_id.is_empty() {
            bail!("canvas snapshot contains a node with empty id");
        }
        if !node_ids.insert(node_id.to_string()) {
            bail!("canvas snapshot has duplicate node id '{}'", node.id);
        }
    }

    let mut edge_ids = HashSet::new();
    for edge in &snapshot.edges {
        let edge_id = edge.id.trim();
        if edge_id.is_empty() {
            bail!("canvas snapshot contains an edge with empty id");
        }
        if !edge_ids.insert(edge_id.to_string()) {
            bail!("canvas snapshot has duplicate edge id '{}'", edge.id);
        }
        if !node_ids.contains(edge.from.as_str()) {
            bail!(
                "canvas snapshot edge '{}' references missing source node '{}'",
                edge.id,
                edge.from
            );
        }
        if !node_ids.contains(edge.to.as_str()) {
            bail!(
                "canvas snapshot edge '{}' references missing destination node '{}'",
                edge.id,
                edge.to
            );
        }
    }

    let mut normalized = snapshot;
    normalized
        .nodes
        .sort_by(|left, right| left.id.cmp(&right.id));
    normalized
        .edges
        .sort_by(|left, right| left.id.cmp(&right.id));
    Ok(normalized)
}

fn replace_doc_with_snapshot(doc: &Doc, snapshot: &CanvasSnapshot) -> Result<()> {
    initialize_canvas_document(doc);
    let root = doc.get_or_insert_map(CANVAS_ROOT_TYPE);
    let mut txn = doc.transact_mut();
    let nodes: MapRef = root.get_or_init(&mut txn, CANVAS_NODES_KEY);
    let edges: MapRef = root.get_or_init(&mut txn, CANVAS_EDGES_KEY);

    let node_keys = nodes
        .iter(&txn)
        .map(|(node_id, _)| node_id.to_string())
        .collect::<Vec<_>>();
    for node_id in node_keys {
        nodes.remove(&mut txn, &node_id);
    }

    let edge_keys = edges
        .iter(&txn)
        .map(|(edge_id, _)| edge_id.to_string())
        .collect::<Vec<_>>();
    for edge_id in edge_keys {
        edges.remove(&mut txn, &edge_id);
    }

    for node in &snapshot.nodes {
        let node_map = get_or_insert_child_map(&nodes, &mut txn, &node.id);
        node_map.insert(&mut txn, "label", node.label.clone());
        node_map.insert(&mut txn, "x", node.x);
        node_map.insert(&mut txn, "y", node.y);
    }

    for edge in &snapshot.edges {
        let edge_map = get_or_insert_child_map(&edges, &mut txn, &edge.id);
        edge_map.insert(&mut txn, "from", edge.from.clone());
        edge_map.insert(&mut txn, "to", edge.to.clone());
        match edge.label.as_deref() {
            Some(value) if !value.trim().is_empty() => {
                edge_map.insert(&mut txn, "label", value.to_string());
            }
            _ => {
                edge_map.remove(&mut txn, "label");
            }
        }
    }
    Ok(())
}

fn apply_canvas_update(
    doc: &Doc,
    canvas_id: &str,
    op: CanvasUpdateOp,
) -> Result<(&'static str, Value)> {
    initialize_canvas_document(doc);
    let root = doc.get_or_insert_map(CANVAS_ROOT_TYPE);
    let mut txn = doc.transact_mut();
    let nodes: MapRef = root.get_or_init(&mut txn, CANVAS_NODES_KEY);
    let edges: MapRef = root.get_or_init(&mut txn, CANVAS_EDGES_KEY);

    let (action, details) = match op {
        CanvasUpdateOp::NodeUpsert {
            node_id,
            label,
            x,
            y,
        } => {
            let node_map = get_or_insert_child_map(&nodes, &mut txn, &node_id);
            node_map.insert(&mut txn, "label", label.clone());
            node_map.insert(&mut txn, "x", x);
            node_map.insert(&mut txn, "y", y);
            (
                "node-upsert",
                json!({
                "canvas_id": canvas_id,
                "node_id": node_id,
                "label": label,
                "x": x,
                "y": y,
                }),
            )
        }
        CanvasUpdateOp::NodeRemove { node_id } => {
            let removed_node = nodes.remove(&mut txn, &node_id).is_some();
            let edge_ids_to_remove = collect_edge_ids_for_node(&edges, &txn, &node_id);
            for edge_id in &edge_ids_to_remove {
                edges.remove(&mut txn, edge_id);
            }
            (
                "node-remove",
                json!({
                "canvas_id": canvas_id,
                "node_id": node_id,
                "removed_node": removed_node,
                "removed_edges": edge_ids_to_remove,
                }),
            )
        }
        CanvasUpdateOp::EdgeUpsert {
            edge_id,
            from,
            to,
            label,
        } => {
            if !nodes.contains_key(&txn, &from) {
                bail!("edge source node '{}' does not exist", from);
            }
            if !nodes.contains_key(&txn, &to) {
                bail!("edge destination node '{}' does not exist", to);
            }
            let edge_map = get_or_insert_child_map(&edges, &mut txn, &edge_id);
            edge_map.insert(&mut txn, "from", from.clone());
            edge_map.insert(&mut txn, "to", to.clone());
            match label.as_deref() {
                Some(value) if !value.trim().is_empty() => {
                    edge_map.insert(&mut txn, "label", value.to_string());
                }
                _ => {
                    edge_map.remove(&mut txn, "label");
                }
            }
            (
                "edge-upsert",
                json!({
                "canvas_id": canvas_id,
                "edge_id": edge_id,
                "from": from,
                "to": to,
                "label": label,
                }),
            )
        }
        CanvasUpdateOp::EdgeRemove { edge_id } => {
            let removed = edges.remove(&mut txn, &edge_id).is_some();
            (
                "edge-remove",
                json!({
                "canvas_id": canvas_id,
                "edge_id": edge_id,
                "removed": removed,
                }),
            )
        }
    };

    Ok((action, details))
}

fn collect_edge_ids_for_node<T: ReadTxn>(edges: &MapRef, txn: &T, node_id: &str) -> Vec<String> {
    let mut edge_ids = Vec::new();
    for (edge_id, value) in edges.iter(txn) {
        let Out::YMap(edge_map) = value else {
            continue;
        };
        let from = map_get_optional::<String, _>(&edge_map, txn, "from")
            .unwrap_or(None)
            .unwrap_or_default();
        let to = map_get_optional::<String, _>(&edge_map, txn, "to")
            .unwrap_or(None)
            .unwrap_or_default();
        if from == node_id || to == node_id {
            edge_ids.push(edge_id.to_string());
        }
    }
    edge_ids.sort();
    edge_ids
}

fn get_or_insert_child_map(
    parent: &MapRef,
    txn: &mut yrs::TransactionMut<'_>,
    key: &str,
) -> MapRef {
    if let Some(Out::YMap(existing)) = parent.get(txn, key) {
        existing
    } else {
        parent.insert(txn, key.to_string(), MapPrelim::default())
    }
}

fn initialize_canvas_document(doc: &Doc) {
    let root = doc.get_or_insert_map(CANVAS_ROOT_TYPE);
    let mut txn = doc.transact_mut();
    let _nodes: MapRef = root.get_or_init(&mut txn, CANVAS_NODES_KEY);
    let _edges: MapRef = root.get_or_init(&mut txn, CANVAS_EDGES_KEY);
}

fn canvas_snapshot_from_doc(doc: &Doc, canvas_id: &str) -> Result<CanvasSnapshot> {
    initialize_canvas_document(doc);
    let root = doc.get_or_insert_map(CANVAS_ROOT_TYPE);
    let txn = doc.transact();
    let nodes = read_canvas_nodes(&root, &txn)?;
    let edges = read_canvas_edges(&root, &txn)?;
    Ok(CanvasSnapshot {
        schema_version: CANVAS_SCHEMA_VERSION,
        canvas_id: canvas_id.to_string(),
        nodes,
        edges,
    })
}

fn read_canvas_nodes<T: ReadTxn>(root: &MapRef, txn: &T) -> Result<Vec<CanvasNode>> {
    let Some(Out::YMap(nodes_map)) = root.get(txn, CANVAS_NODES_KEY) else {
        return Ok(Vec::new());
    };
    let mut nodes = Vec::new();
    for (node_id, value) in nodes_map.iter(txn) {
        let Out::YMap(node_map) = value else {
            bail!("invalid node entry '{}': expected map", node_id);
        };
        let id = node_id.to_string();
        let label =
            map_get_optional::<String, _>(&node_map, txn, "label")?.unwrap_or_else(|| id.clone());
        let x = map_get_optional::<f64, _>(&node_map, txn, "x")?.unwrap_or(0.0);
        let y = map_get_optional::<f64, _>(&node_map, txn, "y")?.unwrap_or(0.0);
        nodes.push(CanvasNode { id, label, x, y });
    }
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(nodes)
}

fn read_canvas_edges<T: ReadTxn>(root: &MapRef, txn: &T) -> Result<Vec<CanvasEdge>> {
    let Some(Out::YMap(edges_map)) = root.get(txn, CANVAS_EDGES_KEY) else {
        return Ok(Vec::new());
    };
    let mut edges = Vec::new();
    for (edge_id, value) in edges_map.iter(txn) {
        let Out::YMap(edge_map) = value else {
            bail!("invalid edge entry '{}': expected map", edge_id);
        };
        let id = edge_id.to_string();
        let from = map_get_optional::<String, _>(&edge_map, txn, "from")?
            .ok_or_else(|| anyhow!("edge '{}' missing required field 'from'", id))?;
        let to = map_get_optional::<String, _>(&edge_map, txn, "to")?
            .ok_or_else(|| anyhow!("edge '{}' missing required field 'to'", id))?;
        let label = map_get_optional::<String, _>(&edge_map, txn, "label")?;
        edges.push(CanvasEdge {
            id,
            from,
            to,
            label,
        });
    }
    edges.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(edges)
}

fn map_get_optional<V, T>(map: &MapRef, txn: &T, key: &str) -> Result<Option<V>>
where
    V: DeserializeOwned,
    T: ReadTxn,
{
    map.get_as::<_, Option<V>>(txn, key)
        .map_err(|error| anyhow!("failed to decode '{}': {error}", key))
}

fn render_canvas_json(snapshot: &CanvasSnapshot) -> Result<String> {
    let mut rendered =
        serde_json::to_string_pretty(snapshot).context("failed to encode canvas snapshot")?;
    rendered.push('\n');
    Ok(rendered)
}

fn render_canvas_markdown(snapshot: &CanvasSnapshot) -> String {
    let mut lines = vec![
        format!("# Canvas `{}`", snapshot.canvas_id),
        String::new(),
        format!("- Schema: `{}`", snapshot.schema_version),
        format!("- Nodes: `{}`", snapshot.nodes.len()),
        format!("- Edges: `{}`", snapshot.edges.len()),
        String::new(),
        "## Nodes".to_string(),
        String::new(),
        "| id | label | x | y |".to_string(),
        "| --- | --- | ---: | ---: |".to_string(),
    ];
    if snapshot.nodes.is_empty() {
        lines.push("| _none_ |  |  |  |".to_string());
    } else {
        for node in &snapshot.nodes {
            lines.push(format!(
                "| {} | {} | {} | {} |",
                node.id,
                node.label,
                format_float(node.x),
                format_float(node.y)
            ));
        }
    }

    lines.push(String::new());
    lines.push("## Edges".to_string());
    lines.push(String::new());
    lines.push("| id | from | to | label |".to_string());
    lines.push("| --- | --- | --- | --- |".to_string());
    if snapshot.edges.is_empty() {
        lines.push("| _none_ |  |  |  |".to_string());
    } else {
        for edge in &snapshot.edges {
            lines.push(format!(
                "| {} | {} | {} | {} |",
                edge.id,
                edge.from,
                edge.to,
                edge.label.clone().unwrap_or_default()
            ));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

fn format_float(value: f64) -> String {
    let mut rendered = format!("{value:.4}");
    while rendered.contains('.') && rendered.ends_with('0') {
        rendered.pop();
    }
    if rendered.ends_with('.') {
        rendered.push('0');
    }
    rendered
}

fn append_canvas_event_to_channel_store(
    config: &CanvasCommandConfig,
    canvas_id: &str,
    event: &CanvasEventEntry,
) -> Result<()> {
    let channel_id = format!("canvas-{}", sanitize_for_path(canvas_id));
    let store = ChannelStore::open(&config.channel_store_root, "local", &channel_id)?;
    let payload = serde_json::to_value(event).context("failed to encode canvas event payload")?;
    store.append_log_entry(&ChannelLogEntry {
        timestamp_unix_ms: event.timestamp_unix_ms,
        direction: "internal".to_string(),
        event_key: Some(format!("canvas:{}:{}", event.action, event.event_id)),
        source: "canvas".to_string(),
        payload,
    })
}

#[derive(Debug, Clone)]
struct CanvasStore {
    root_dir: PathBuf,
    canvas_id: String,
}

impl CanvasStore {
    fn open(root_dir: &Path, canvas_id: &str) -> Result<Self> {
        let canvas_id = canvas_id.trim();
        if canvas_id.is_empty() {
            bail!("canvas id must be non-empty");
        }
        let store = Self {
            root_dir: root_dir.to_path_buf(),
            canvas_id: canvas_id.to_string(),
        };
        store.ensure_layout()?;
        Ok(store)
    }

    fn canvas_dir(&self) -> PathBuf {
        self.root_dir.join(sanitize_for_path(&self.canvas_id))
    }

    fn schema_path(&self) -> PathBuf {
        self.canvas_dir().join("schema.json")
    }

    fn state_path(&self) -> PathBuf {
        self.canvas_dir().join("state.yrs.bin")
    }

    fn events_path(&self) -> PathBuf {
        self.canvas_dir().join("events.jsonl")
    }

    fn session_links_path(&self) -> PathBuf {
        self.canvas_dir().join("session-links.jsonl")
    }

    fn exports_dir(&self) -> PathBuf {
        self.canvas_dir().join("exports")
    }

    fn ensure_layout(&self) -> Result<()> {
        let canvas_dir = self.canvas_dir();
        std::fs::create_dir_all(&canvas_dir)
            .with_context(|| format!("failed to create {}", canvas_dir.display()))?;
        std::fs::create_dir_all(self.exports_dir())
            .with_context(|| format!("failed to create {}", self.exports_dir().display()))?;

        for path in [
            self.state_path(),
            self.events_path(),
            self.session_links_path(),
        ] {
            if !path.exists() {
                std::fs::write(&path, "")
                    .with_context(|| format!("failed to initialize {}", path.display()))?;
            }
        }

        let schema_path = self.schema_path();
        if schema_path.exists() {
            let raw = std::fs::read_to_string(&schema_path)
                .with_context(|| format!("failed to read {}", schema_path.display()))?;
            let schema = serde_json::from_str::<CanvasStoreMeta>(&raw)
                .with_context(|| format!("failed to parse {}", schema_path.display()))?;
            if schema.schema_version != CANVAS_SCHEMA_VERSION {
                bail!(
                    "unsupported canvas schema: expected {}, found {}",
                    CANVAS_SCHEMA_VERSION,
                    schema.schema_version
                );
            }
            if schema.canvas_id != self.canvas_id {
                bail!(
                    "canvas schema mismatch for {} (expected id '{}', found '{}')",
                    schema_path.display(),
                    self.canvas_id,
                    schema.canvas_id
                );
            }
            return Ok(());
        }

        let mut payload = serde_json::to_string_pretty(&CanvasStoreMeta {
            schema_version: CANVAS_SCHEMA_VERSION,
            canvas_id: self.canvas_id.clone(),
        })
        .context("failed to encode canvas schema")?;
        payload.push('\n');
        write_text_atomic(&schema_path, &payload)
            .with_context(|| format!("failed to write {}", schema_path.display()))
    }

    fn load_doc(&self) -> Result<Doc> {
        let doc = Doc::new();
        let state = std::fs::read(self.state_path())
            .with_context(|| format!("failed to read {}", self.state_path().display()))?;
        if !state.is_empty() {
            let update = Update::decode_v1(state.as_slice())
                .context("failed to decode canvas CRDT state")?;
            doc.transact_mut()
                .apply_update(update)
                .context("failed to apply canvas CRDT state")?;
        }
        initialize_canvas_document(&doc);
        Ok(doc)
    }

    fn save_doc(&self, doc: &Doc) -> Result<()> {
        let payload = doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default());
        write_bytes_atomic(&self.state_path(), payload.as_slice())
            .with_context(|| format!("failed to persist {}", self.state_path().display()))
    }

    fn append_event(&self, event: &CanvasEventEntry) -> Result<()> {
        append_jsonl_line(&self.events_path(), event)
    }

    fn load_events(&self) -> Result<Vec<CanvasEventEntry>> {
        read_jsonl_records(&self.events_path())
    }

    fn append_session_link(&self, canvas_id: &str, event: &CanvasEventEntry) -> Result<()> {
        let Some(link) = &event.session_link else {
            return Ok(());
        };
        let entry = CanvasSessionLinkEntry {
            schema_version: CANVAS_SESSION_LINK_SCHEMA_VERSION,
            timestamp_unix_ms: event.timestamp_unix_ms,
            event_id: event.event_id.clone(),
            principal: event.principal.clone(),
            canvas_id: canvas_id.to_string(),
            session_path: link.session_path.clone(),
            session_head_id: link.session_head_id,
        };
        append_jsonl_line(&self.session_links_path(), &entry)
    }

    #[cfg(test)]
    fn load_session_links(&self) -> Result<Vec<CanvasSessionLinkEntry>> {
        read_jsonl_records(&self.session_links_path())
    }
}

fn append_jsonl_line<T>(path: &Path, value: &T) -> Result<()>
where
    T: Serialize,
{
    let line = serde_json::to_string(value).context("failed to encode jsonl record")?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    writeln!(file, "{line}").with_context(|| format!("failed to append to {}", path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush {}", path.display()))?;
    Ok(())
}

fn read_jsonl_records<T>(path: &Path) -> Result<Vec<T>>
where
    T: for<'de> Deserialize<'de>,
{
    let file =
        std::fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = std::io::BufReader::new(file);
    let mut rows = Vec::new();
    for (index, line_result) in reader.lines().enumerate() {
        let line_no = index + 1;
        let line = line_result
            .with_context(|| format!("failed reading line {} from {}", line_no, path.display()))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        rows.push(serde_json::from_str::<T>(trimmed).with_context(|| {
            format!("failed parsing JSON line {} in {}", line_no, path.display())
        })?);
    }
    Ok(rows)
}

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("path '{}' has no parent", path.display()))?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create {}", parent.display()))?;

    let temp_path = parent.join(format!(
        ".{}.{}.tmp",
        sanitize_for_path(
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("canvas-state")
        ),
        current_unix_timestamp_ms()
    ));
    std::fs::write(&temp_path, bytes)
        .with_context(|| format!("failed to write {}", temp_path.display()))?;
    std::fs::rename(&temp_path, path).with_context(|| {
        format!(
            "failed to atomically replace '{}' with '{}'",
            path.display(),
            temp_path.display()
        )
    })?;
    Ok(())
}

fn sanitize_for_path(raw: &str) -> String {
    let sanitized = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "canvas".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
fn resolve_safe_canvas_path(canvas_dir: &Path, relative_path: &str) -> Result<PathBuf> {
    let candidate = Path::new(relative_path);
    if candidate.is_absolute() {
        bail!("canvas path must be relative");
    }
    if candidate
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        bail!("canvas path cannot contain parent directory traversal");
    }
    let joined = canvas_dir.join(candidate);
    let canonical_parent = canvas_dir
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", canvas_dir.display()))?;
    let normalized = joined
        .components()
        .fold(PathBuf::new(), |mut acc, component| {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    acc.pop();
                }
                _ => acc.push(component),
            }
            acc
        });
    if !normalized.starts_with(&canonical_parent) {
        bail!("canvas path escapes canvas directory");
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionStore;
    use tempfile::tempdir;

    fn test_config(root: &Path) -> CanvasCommandConfig {
        CanvasCommandConfig {
            canvas_root: root.join(".tau/canvas"),
            channel_store_root: root.join(".tau/channel-store"),
            principal: "local:test-user".to_string(),
            origin: CanvasEventOrigin::default(),
            session_link: None,
        }
    }

    #[test]
    fn unit_parse_canvas_command_supports_create_show_export_import_and_update_operations() {
        let create = parse_canvas_command("create architecture").expect("parse create");
        assert_eq!(
            create,
            CanvasCommand::Create {
                canvas_id: "architecture".to_string()
            }
        );

        let update = parse_canvas_command(
            "update architecture node-upsert api \"API Service\" 120.5 -42.25",
        )
        .expect("parse node upsert");
        assert_eq!(
            update,
            CanvasCommand::Update {
                canvas_id: "architecture".to_string(),
                op: CanvasUpdateOp::NodeUpsert {
                    node_id: "api".to_string(),
                    label: "API Service".to_string(),
                    x: 120.5,
                    y: -42.25,
                }
            }
        );

        let show = parse_canvas_command("show architecture --json").expect("parse show");
        assert_eq!(
            show,
            CanvasCommand::Show {
                canvas_id: "architecture".to_string(),
                format: CanvasShowFormat::Json
            }
        );

        let export =
            parse_canvas_command("export architecture markdown /tmp/canvas.md").expect("parse");
        assert_eq!(
            export,
            CanvasCommand::Export {
                canvas_id: "architecture".to_string(),
                format: CanvasExportFormat::Markdown,
                path: Some(PathBuf::from("/tmp/canvas.md")),
            }
        );

        let import = parse_canvas_command("import architecture /tmp/canvas.json").expect("parse");
        assert_eq!(
            import,
            CanvasCommand::Import {
                canvas_id: "architecture".to_string(),
                path: PathBuf::from("/tmp/canvas.json"),
            }
        );
    }

    #[test]
    fn regression_parse_canvas_command_rejects_invalid_forms() {
        let err = parse_canvas_command("").expect_err("empty args fail");
        assert!(err.to_string().contains("missing canvas subcommand"));

        let err = parse_canvas_command("show").expect_err("missing canvas id");
        assert!(err
            .to_string()
            .contains("usage: /canvas show <canvas_id> [--json]"));

        let err = parse_canvas_command("update a edge-upsert e from")
            .expect_err("missing edge args should fail");
        assert!(err.to_string().contains("usage: /canvas update"));

        let err = parse_canvas_command("export a yaml").expect_err("invalid format should fail");
        assert!(err.to_string().contains("unsupported export format 'yaml'"));
    }

    #[test]
    fn functional_execute_canvas_command_create_update_show_and_export_roundtrip() {
        let temp = tempdir().expect("tempdir");
        let config = test_config(temp.path());

        let create = execute_canvas_command("create architecture", &config);
        assert!(create.contains("canvas create: id=architecture"));

        let update_node = execute_canvas_command(
            "update architecture node-upsert api \"API Service\" 100 200",
            &config,
        );
        assert!(update_node.contains("canvas update: id=architecture action=node-upsert"));

        let update_node_2 = execute_canvas_command(
            "update architecture node-upsert db \"DB Service\" 320 180",
            &config,
        );
        assert!(update_node_2.contains("action=node-upsert"));

        let update_edge = execute_canvas_command(
            "update architecture edge-upsert e1 api db \"Primary path\"",
            &config,
        );
        assert!(update_edge.contains("action=edge-upsert"));

        let show_json = execute_canvas_command("show architecture --json", &config);
        let snapshot = serde_json::from_str::<CanvasSnapshot>(&show_json).expect("parse snapshot");
        assert_eq!(snapshot.schema_version, CANVAS_SCHEMA_VERSION);
        assert_eq!(snapshot.canvas_id, "architecture");
        assert_eq!(snapshot.nodes.len(), 2);
        assert_eq!(snapshot.edges.len(), 1);

        let export_path = temp.path().join("canvas-export.md");
        let export = execute_canvas_command(
            format!("export architecture markdown {}", export_path.display()).as_str(),
            &config,
        );
        assert!(export.contains("canvas export: id=architecture format=md"));
        let markdown = std::fs::read_to_string(&export_path).expect("read export");
        assert!(markdown.contains("# Canvas `architecture`"));
        assert!(markdown.contains("| e1 | api | db | Primary path |"));
    }

    #[test]
    fn functional_execute_canvas_import_validates_schema_and_preserves_deterministic_rendering() {
        let temp = tempdir().expect("tempdir");
        let config = test_config(temp.path());

        execute_canvas_command("create architecture", &config);
        execute_canvas_command(
            "update architecture node-upsert api \"API Service\" 10 20",
            &config,
        );
        execute_canvas_command(
            "update architecture node-upsert db \"DB Service\" 30 40",
            &config,
        );
        execute_canvas_command(
            "update architecture edge-upsert e1 api db \"flow\"",
            &config,
        );
        let exported_json = temp.path().join("architecture-export.json");
        execute_canvas_command(
            format!("export architecture json {}", exported_json.display()).as_str(),
            &config,
        );

        let imported = execute_canvas_command(
            format!("import architecture {}", exported_json.display()).as_str(),
            &config,
        );
        assert!(imported.contains("canvas import: id=architecture"));

        let snapshot_json = execute_canvas_command("show architecture --json", &config);
        let snapshot = serde_json::from_str::<CanvasSnapshot>(&snapshot_json).expect("snapshot");
        assert_eq!(snapshot.nodes.len(), 2);
        assert_eq!(snapshot.edges.len(), 1);
        let markdown_a = render_canvas_markdown(&snapshot);
        let markdown_b = render_canvas_markdown(&snapshot);
        assert_eq!(markdown_a, markdown_b);

        let invalid_path = temp.path().join("invalid-snapshot.json");
        let mut invalid_snapshot = snapshot.clone();
        invalid_snapshot.schema_version = CANVAS_SCHEMA_VERSION + 1;
        std::fs::write(
            &invalid_path,
            serde_json::to_string_pretty(&invalid_snapshot).expect("encode"),
        )
        .expect("write invalid");
        let invalid = execute_canvas_command(
            format!("import architecture {}", invalid_path.display()).as_str(),
            &config,
        );
        assert!(invalid.contains("unsupported canvas snapshot schema"));
    }

    #[test]
    fn integration_canvas_crdt_converges_under_concurrent_updates() {
        let doc_a = Doc::with_client_id(1);
        let doc_b = Doc::with_client_id(2);
        initialize_canvas_document(&doc_a);

        apply_canvas_update(
            &doc_a,
            "architecture",
            CanvasUpdateOp::NodeUpsert {
                node_id: "root".to_string(),
                label: "Root".to_string(),
                x: 0.0,
                y: 0.0,
            },
        )
        .expect("seed root on a");
        sync_doc(&doc_a, &doc_b);

        apply_canvas_update(
            &doc_a,
            "architecture",
            CanvasUpdateOp::NodeUpsert {
                node_id: "api".to_string(),
                label: "API".to_string(),
                x: 10.0,
                y: 20.0,
            },
        )
        .expect("update api on a");
        apply_canvas_update(
            &doc_b,
            "architecture",
            CanvasUpdateOp::NodeUpsert {
                node_id: "db".to_string(),
                label: "DB".to_string(),
                x: 30.0,
                y: 40.0,
            },
        )
        .expect("update db on b");

        apply_canvas_update(
            &doc_a,
            "architecture",
            CanvasUpdateOp::EdgeUpsert {
                edge_id: "edge-api".to_string(),
                from: "root".to_string(),
                to: "api".to_string(),
                label: Some("route-a".to_string()),
            },
        )
        .expect("edge on a");
        apply_canvas_update(
            &doc_b,
            "architecture",
            CanvasUpdateOp::EdgeUpsert {
                edge_id: "edge-db".to_string(),
                from: "root".to_string(),
                to: "db".to_string(),
                label: Some("route-b".to_string()),
            },
        )
        .expect("edge on b");

        sync_doc(&doc_a, &doc_b);
        sync_doc(&doc_b, &doc_a);

        let snapshot_a = canvas_snapshot_from_doc(&doc_a, "architecture").expect("snapshot a");
        let snapshot_b = canvas_snapshot_from_doc(&doc_b, "architecture").expect("snapshot b");
        assert_eq!(snapshot_a, snapshot_b);
        assert_eq!(snapshot_a.nodes.len(), 3);
        assert_eq!(snapshot_a.edges.len(), 2);
    }

    #[test]
    fn integration_canvas_channel_store_event_logs_roundtrip_without_corruption() {
        let temp = tempdir().expect("tempdir");
        let config = test_config(temp.path());

        execute_canvas_command("create architecture", &config);
        execute_canvas_command(
            "update architecture node-upsert api \"API Service\" 100 200",
            &config,
        );
        execute_canvas_command(
            "update architecture node-upsert db \"DB Service\" 300 150",
            &config,
        );
        execute_canvas_command(
            "update architecture edge-upsert e1 api db \"flow\"",
            &config,
        );

        let store = ChannelStore::open(&config.channel_store_root, "local", "canvas-architecture")
            .expect("open channel store");
        let logs = store.load_log_entries().expect("load logs");
        assert!(logs.len() >= 4);
        assert!(logs.iter().all(|entry| entry.source == "canvas"));
        assert!(logs.iter().any(|entry| entry
            .event_key
            .as_deref()
            .map(|value| value.starts_with("canvas:create:"))
            .unwrap_or(false)));
        let inspect = store.inspect().expect("inspect channel store");
        assert_eq!(inspect.invalid_log_lines, 0);

        let canvas_store = CanvasStore::open(&config.canvas_root, "architecture").expect("store");
        let events = canvas_store.load_events().expect("load events");
        assert!(events.len() >= 4);
        assert!(events
            .iter()
            .all(|event| event.principal == "local:test-user"));
        assert!(events
            .iter()
            .all(|event| event.schema_version == CANVAS_EVENT_SCHEMA_VERSION));
        assert!(events.iter().all(|event| !event.event_id.is_empty()));
    }

    #[test]
    fn regression_canvas_store_rejects_schema_mismatch() {
        let temp = tempdir().expect("tempdir");
        let store = CanvasStore::open(temp.path(), "architecture").expect("open store");

        let mut payload = serde_json::to_string_pretty(&CanvasStoreMeta {
            schema_version: CANVAS_SCHEMA_VERSION + 10,
            canvas_id: "architecture".to_string(),
        })
        .expect("encode schema");
        payload.push('\n');
        std::fs::write(store.schema_path(), payload).expect("write schema");

        let error = CanvasStore::open(temp.path(), "architecture").expect_err("schema mismatch");
        assert!(error.to_string().contains("unsupported canvas schema"));
    }

    #[test]
    fn regression_canvas_export_rendering_is_deterministic_with_unsorted_updates() {
        let temp = tempdir().expect("tempdir");
        let config = test_config(temp.path());
        execute_canvas_command("create architecture", &config);
        execute_canvas_command(
            "update architecture node-upsert zeta \"Zeta\" 20 20",
            &config,
        );
        execute_canvas_command(
            "update architecture node-upsert alpha \"Alpha\" 10 10",
            &config,
        );
        execute_canvas_command(
            "update architecture edge-upsert edge-z alpha zeta \"A->Z\"",
            &config,
        );

        let snapshot_json = execute_canvas_command("show architecture --json", &config);
        let snapshot = serde_json::from_str::<CanvasSnapshot>(&snapshot_json).expect("snapshot");
        assert_eq!(snapshot.nodes[0].id, "alpha");
        assert_eq!(snapshot.nodes[1].id, "zeta");

        let markdown_a = render_canvas_markdown(&snapshot);
        let markdown_b = render_canvas_markdown(&snapshot);
        assert_eq!(markdown_a, markdown_b);

        let json_a = render_canvas_json(&snapshot).expect("json a");
        let json_b = render_canvas_json(&snapshot).expect("json b");
        assert_eq!(json_a, json_b);
    }

    #[test]
    fn integration_canvas_replay_guard_is_idempotent_for_duplicate_and_out_of_order_remote_events()
    {
        let temp = tempdir().expect("tempdir");
        let mut config = test_config(temp.path());
        config.origin = CanvasEventOrigin {
            transport: "github".to_string(),
            channel: Some("issue-42".to_string()),
            source_event_key: Some("issue-comment-created:100".to_string()),
            source_unix_ms: Some(1_000),
        };

        let create = execute_canvas_command("create architecture", &config);
        assert!(create.contains("canvas create: id=architecture"));
        let duplicate = execute_canvas_command("create architecture", &config);
        assert!(duplicate.contains("status=duplicate-skipped"));

        config.origin = CanvasEventOrigin {
            transport: "github".to_string(),
            channel: Some("issue-42".to_string()),
            source_event_key: Some("issue-comment-created:300".to_string()),
            source_unix_ms: Some(3_000),
        };
        let latest = execute_canvas_command(
            "update architecture node-upsert api \"API Latest\" 30 30",
            &config,
        );
        assert!(latest.contains("action=node-upsert"));

        config.origin = CanvasEventOrigin {
            transport: "github".to_string(),
            channel: Some("issue-42".to_string()),
            source_event_key: Some("issue-comment-created:200".to_string()),
            source_unix_ms: Some(2_000),
        };
        let stale = execute_canvas_command(
            "update architecture node-upsert api \"API Stale\" 20 20",
            &config,
        );
        assert!(stale.contains("status=out-of-order-skipped"));

        let show = execute_canvas_command("show architecture --json", &config);
        let snapshot = serde_json::from_str::<CanvasSnapshot>(&show).expect("snapshot");
        assert_eq!(snapshot.nodes.len(), 1);
        assert_eq!(snapshot.nodes[0].label, "API Latest");

        let canvas_store = CanvasStore::open(&config.canvas_root, "architecture").expect("store");
        let events = canvas_store.load_events().expect("events");
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn integration_canvas_session_link_index_persists_session_head_association() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join(".tau/sessions/default.jsonl");
        let mut session_store = SessionStore::load(&session_path).expect("session load");
        let head = session_store
            .append_messages(None, &[tau_ai::Message::system("sys")])
            .expect("append head");

        let mut config = test_config(temp.path());
        config.session_link = Some(CanvasSessionLinkContext {
            session_path: session_path.clone(),
            session_head_id: head,
        });

        let create = execute_canvas_command("create architecture", &config);
        assert!(create.contains("canvas create: id=architecture"));

        let canvas_store = CanvasStore::open(&config.canvas_root, "architecture").expect("store");
        let links = canvas_store.load_session_links().expect("load links");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].schema_version, CANVAS_SESSION_LINK_SCHEMA_VERSION);
        assert_eq!(links[0].session_path, session_path.display().to_string());
        assert_eq!(links[0].session_head_id, head);
    }

    #[test]
    fn regression_resolve_safe_canvas_path_rejects_parent_traversal() {
        let temp = tempdir().expect("tempdir");
        let canvas_dir = temp.path().join("canvas");
        std::fs::create_dir_all(&canvas_dir).expect("canvas dir");
        let error = resolve_safe_canvas_path(&canvas_dir, "../escape.md")
            .expect_err("parent traversal should fail");
        assert!(error
            .to_string()
            .contains("canvas path cannot contain parent directory traversal"));
    }

    fn sync_doc(from: &Doc, to: &Doc) {
        let state_vector = to.transact().state_vector();
        let update = from.transact().encode_diff_v1(&state_vector);
        let decoded = Update::decode_v1(update.as_slice()).expect("decode update");
        to.transact_mut()
            .apply_update(decoded)
            .expect("apply update");
    }
}
