use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use tau_agent_core::{AgentTool, ToolExecutionResult};
use tau_cli::Cli;

use crate::tool_policy_config::build_tool_policy;
use crate::tools::{BashTool, EditTool, ReadTool, ToolPolicy, WriteTool};

const MCP_JSONRPC_VERSION: &str = "2.0";
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const MCP_EXTERNAL_SERVER_SCHEMA_VERSION: u32 = 1;
const MCP_ERROR_PARSE: i64 = -32700;
const MCP_ERROR_INVALID_REQUEST: i64 = -32600;
const MCP_ERROR_METHOD_NOT_FOUND: i64 = -32601;
const MCP_ERROR_INVALID_PARAMS: i64 = -32602;
const MCP_TOOL_PREFIX_EXTERNAL: &str = "external.";
const MCP_TOOL_READ: &str = "tau.read";
const MCP_TOOL_WRITE: &str = "tau.write";
const MCP_TOOL_EDIT: &str = "tau.edit";
const MCP_TOOL_BASH: &str = "tau.bash";
const MCP_TOOL_CONTEXT_SESSION: &str = "tau.context.session";
const MCP_TOOL_CONTEXT_SKILLS: &str = "tau.context.skills";
const MCP_TOOL_CONTEXT_CHANNEL_STORE: &str = "tau.context.channel-store";
const MCP_CONTEXT_PROVIDER_SESSION: &str = "session";
const MCP_CONTEXT_PROVIDER_SKILLS: &str = "skills";
const MCP_CONTEXT_PROVIDER_CHANNEL_STORE: &str = "channel-store";
const MCP_EXTERNAL_INIT_REQUEST_ID: &str = "tau-ext-init";
const MCP_EXTERNAL_TOOLS_LIST_REQUEST_ID: &str = "tau-ext-tools-list";
const MCP_EXTERNAL_TOOLS_CALL_REQUEST_ID: &str = "tau-ext-tools-call";
const MCP_EXTERNAL_RESULT_TOOLS_FIELD: &str = "tools";
const MCP_CONTENT_TYPE_TEXT: &str = "text";
const RESERVED_MCP_TOOL_NAMES: &[&str] = &[
    MCP_TOOL_READ,
    MCP_TOOL_WRITE,
    MCP_TOOL_EDIT,
    MCP_TOOL_BASH,
    MCP_TOOL_CONTEXT_SESSION,
    MCP_TOOL_CONTEXT_SKILLS,
    MCP_TOOL_CONTEXT_CHANNEL_STORE,
];

fn default_mcp_context_providers() -> Vec<String> {
    vec![
        MCP_CONTEXT_PROVIDER_SESSION.to_string(),
        MCP_CONTEXT_PROVIDER_SKILLS.to_string(),
        MCP_CONTEXT_PROVIDER_CHANNEL_STORE.to_string(),
    ]
}

fn default_external_server_enabled() -> bool {
    true
}

#[derive(Debug, Clone)]
/// Public struct `McpServeReport` used across Tau components.
pub struct McpServeReport {
    pub processed_frames: usize,
    pub error_count: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct McpExternalServerConfigFile {
    schema_version: u32,
    #[serde(default)]
    servers: Vec<McpExternalServerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct McpExternalServerConfig {
    name: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    cwd: Option<PathBuf>,
    #[serde(default = "default_external_server_enabled")]
    enabled: bool,
}

#[derive(Debug, Clone)]
struct McpExternalDiscoveredTool {
    server_name: String,
    tool_name: String,
    description: String,
    input_schema: Value,
}

#[derive(Debug, Clone)]
struct McpServerState {
    tool_policy: ToolPolicy,
    session_path: PathBuf,
    skills_dir: PathBuf,
    channel_store_root: PathBuf,
    context_providers: BTreeSet<String>,
    external_servers: Vec<McpExternalServerConfig>,
    external_tools: Vec<McpExternalDiscoveredTool>,
}

#[derive(Debug, Clone)]
struct McpJsonRpcRequest {
    id: Value,
    method: String,
    params: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone)]
struct McpToolDescriptor {
    name: String,
    description: String,
    input_schema: Value,
}

#[derive(Debug, Clone)]
struct McpDispatchError {
    id: Value,
    code: i64,
    message: String,
}

impl McpDispatchError {
    fn new(id: Value, code: i64, message: impl Into<String>) -> Self {
        Self {
            id,
            code,
            message: message.into(),
        }
    }
}

pub fn resolve_mcp_context_providers(raw: &[String]) -> Result<Vec<String>> {
    if raw.is_empty() {
        return Ok(default_mcp_context_providers());
    }

    let mut resolved = Vec::new();
    for entry in raw {
        let normalized = entry.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            continue;
        }
        if !matches!(
            normalized.as_str(),
            MCP_CONTEXT_PROVIDER_SESSION
                | MCP_CONTEXT_PROVIDER_SKILLS
                | MCP_CONTEXT_PROVIDER_CHANNEL_STORE
        ) {
            bail!(
                "unsupported mcp context provider '{}'; supported values are session, skills, channel-store",
                entry
            );
        }
        if !resolved.iter().any(|existing| existing == &normalized) {
            resolved.push(normalized);
        }
    }

    if resolved.is_empty() {
        bail!("at least one valid --mcp-context-provider value is required");
    }
    Ok(resolved)
}

pub fn execute_mcp_server_command(cli: &Cli) -> Result<()> {
    if !cli.mcp_server {
        return Ok(());
    }

    let context_providers = resolve_mcp_context_providers(&cli.mcp_context_provider)?;
    let reserved_mcp_tool_names = reserved_builtin_mcp_tool_names();
    let external_servers = load_external_mcp_servers(cli.mcp_external_server_config.as_deref())?;
    let external_tools = discover_external_mcp_tools(&external_servers, &reserved_mcp_tool_names)?;
    let state = McpServerState {
        tool_policy: build_tool_policy(cli)?,
        session_path: cli.session.clone(),
        skills_dir: cli.skills_dir.clone(),
        channel_store_root: cli.channel_store_root.clone(),
        context_providers: context_providers.into_iter().collect(),
        external_servers,
        external_tools,
    };

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();
    let report = serve_mcp_jsonrpc_reader(&mut reader, &mut writer, &state)?;
    if report.error_count > 0 {
        bail!(
            "mcp server completed with {} error frame(s) after {} request(s)",
            report.error_count,
            report.processed_frames
        );
    }
    Ok(())
}

fn load_external_mcp_servers(path: Option<&Path>) -> Result<Vec<McpExternalServerConfig>> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };

    let raw = std::fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read mcp external server config {}",
            path.display()
        )
    })?;
    let parsed = serde_json::from_str::<McpExternalServerConfigFile>(&raw).with_context(|| {
        format!(
            "failed to parse mcp external server config {}",
            path.display()
        )
    })?;
    if parsed.schema_version != MCP_EXTERNAL_SERVER_SCHEMA_VERSION {
        bail!(
            "unsupported mcp external server config schema_version {} in {} (expected {})",
            parsed.schema_version,
            path.display(),
            MCP_EXTERNAL_SERVER_SCHEMA_VERSION
        );
    }

    let mut servers = Vec::new();
    let mut seen_names = BTreeSet::new();
    for server in parsed.servers {
        if !server.enabled {
            continue;
        }
        let name = normalize_external_server_name(&server.name)?;
        if !seen_names.insert(name.clone()) {
            bail!(
                "duplicate external mcp server name '{}' in {}",
                name,
                path.display()
            );
        }
        let command = server.command.trim();
        if command.is_empty() {
            bail!(
                "external mcp server '{}' in {} is missing a command",
                name,
                path.display()
            );
        }
        servers.push(McpExternalServerConfig {
            name,
            command: command.to_string(),
            args: server.args,
            env: server.env,
            cwd: server.cwd,
            enabled: true,
        });
    }
    Ok(servers)
}

fn normalize_external_server_name(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("external mcp server name must be non-empty");
    }
    if !trimmed
        .chars()
        .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'))
    {
        bail!(
            "external mcp server name '{}' must contain only ASCII letters, digits, '-' or '_'",
            raw
        );
    }
    Ok(trimmed.to_ascii_lowercase())
}

fn discover_external_mcp_tools(
    servers: &[McpExternalServerConfig],
    reserved_tool_names: &BTreeSet<String>,
) -> Result<Vec<McpExternalDiscoveredTool>> {
    let mut tools = Vec::new();
    let mut seen_qualified_names = BTreeSet::new();
    for server in servers {
        let init = jsonrpc_request_frame(
            Value::String(MCP_EXTERNAL_INIT_REQUEST_ID.to_string()),
            "initialize",
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "tau-coding-agent",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        );
        let list = jsonrpc_request_frame(
            Value::String(MCP_EXTERNAL_TOOLS_LIST_REQUEST_ID.to_string()),
            "tools/list",
            json!({}),
        );
        let responses = call_external_mcp_server(server, &[init, list])?;
        let list_payload =
            external_response_result(&responses, MCP_EXTERNAL_TOOLS_LIST_REQUEST_ID, server)?;
        let list_tools = list_payload
            .get(MCP_EXTERNAL_RESULT_TOOLS_FIELD)
            .and_then(Value::as_array)
            .ok_or_else(|| {
                anyhow!(
                    "external mcp server '{}' returned invalid tools/list payload",
                    server.name
                )
            })?;
        for entry in list_tools {
            let object = entry.as_object().ok_or_else(|| {
                anyhow!(
                    "external mcp server '{}' returned non-object tool descriptor",
                    server.name
                )
            })?;
            let tool_name = object
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    anyhow!(
                        "external mcp server '{}' returned tool with invalid name",
                        server.name
                    )
                })?
                .to_string();
            if reserved_tool_names.contains(tool_name.as_str()) {
                bail!(
                    "external mcp server '{}' returned reserved built-in tool name '{}'",
                    server.name,
                    tool_name
                );
            }
            let qualified_name = format!("{MCP_TOOL_PREFIX_EXTERNAL}{}.{}", server.name, tool_name);
            if !seen_qualified_names.insert(qualified_name.clone()) {
                bail!(
                    "external mcp server '{}' returned duplicate tool registration '{}'",
                    server.name,
                    qualified_name
                );
            }
            let description = object
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("external mcp tool")
                .trim()
                .to_string();
            let input_schema = object.get("inputSchema").cloned().unwrap_or_else(
                || json!({"type":"object","properties":{},"additionalProperties":true}),
            );
            tools.push(McpExternalDiscoveredTool {
                server_name: server.name.clone(),
                tool_name,
                description,
                input_schema,
            });
        }
    }
    Ok(tools)
}

fn reserved_builtin_mcp_tool_names() -> BTreeSet<String> {
    RESERVED_MCP_TOOL_NAMES
        .iter()
        .map(|name| (*name).to_string())
        .collect()
}

fn call_external_mcp_server(
    server: &McpExternalServerConfig,
    requests: &[Value],
) -> Result<Vec<Value>> {
    let mut command = Command::new(&server.command);
    command.args(&server.args);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    if let Some(cwd) = &server.cwd {
        command.current_dir(cwd);
    }
    for (key, value) in &server.env {
        command.env(key, value);
    }

    let mut child = command.spawn().with_context(|| {
        format!(
            "failed to spawn external mcp server '{}' command '{}'",
            server.name, server.command
        )
    })?;
    let mut child_stdin = child.stdin.take().ok_or_else(|| {
        anyhow!(
            "failed to open stdin for external mcp server '{}'",
            server.name
        )
    })?;
    for request in requests {
        let line = serde_json::to_string(request).context("failed to encode external request")?;
        writeln!(child_stdin, "{line}").with_context(|| {
            format!(
                "failed to write request to external mcp server '{}'",
                server.name
            )
        })?;
    }
    drop(child_stdin);

    let mut responses = Vec::new();
    {
        let child_stdout = child.stdout.take().ok_or_else(|| {
            anyhow!(
                "failed to open stdout for external mcp server '{}'",
                server.name
            )
        })?;
        let mut reader = BufReader::new(child_stdout);
        let mut line = String::new();
        loop {
            line.clear();
            let bytes = reader.read_line(&mut line).with_context(|| {
                format!(
                    "failed to read response from external mcp server '{}'",
                    server.name
                )
            })?;
            if bytes == 0 {
                break;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let value = serde_json::from_str::<Value>(trimmed).with_context(|| {
                format!(
                    "external mcp server '{}' returned invalid JSON line '{}'",
                    server.name, trimmed
                )
            })?;
            responses.push(value);
        }
    }

    let status = child
        .wait()
        .with_context(|| format!("failed to wait for external mcp server '{}'", server.name))?;
    if !status.success() {
        let mut stderr = String::new();
        if let Some(mut handle) = child.stderr.take() {
            let _ = handle.read_to_string(&mut stderr);
        }
        bail!(
            "external mcp server '{}' exited with status {}{}",
            server.name,
            status,
            if stderr.trim().is_empty() {
                String::new()
            } else {
                format!(" stderr={}", stderr.trim())
            }
        );
    }

    Ok(responses)
}

fn external_response_result(
    responses: &[Value],
    request_id: &str,
    server: &McpExternalServerConfig,
) -> Result<Value> {
    for response in responses {
        let object = match response.as_object() {
            Some(object) => object,
            None => continue,
        };
        let Some(id) = object.get("id") else {
            continue;
        };
        let matches = id
            .as_str()
            .map(|value| value == request_id)
            .unwrap_or(false);
        if !matches {
            continue;
        }
        if let Some(error) = object.get("error") {
            bail!(
                "external mcp server '{}' returned error for request '{}': {}",
                server.name,
                request_id,
                error
            );
        }
        if let Some(result) = object.get("result") {
            return Ok(result.clone());
        }
        bail!(
            "external mcp server '{}' returned response without result for request '{}'",
            server.name,
            request_id
        );
    }

    bail!(
        "external mcp server '{}' returned no response for request '{}'",
        server.name,
        request_id
    )
}

fn serve_mcp_jsonrpc_reader<R, W>(
    reader: &mut R,
    writer: &mut W,
    state: &McpServerState,
) -> Result<McpServeReport>
where
    R: BufRead,
    W: Write,
{
    let mut processed_frames = 0usize;
    let mut error_count = 0usize;

    loop {
        let frame = match read_jsonrpc_content_length_frame(reader) {
            Ok(Some(value)) => value,
            Ok(None) => break,
            Err(error) => {
                let response = jsonrpc_error_frame(
                    Value::Null,
                    MCP_ERROR_PARSE,
                    format!("failed to read mcp frame: {error}"),
                );
                write_jsonrpc_content_length_frame(writer, &response)?;
                error_count = error_count.saturating_add(1);
                break;
            }
        };
        processed_frames = processed_frames.saturating_add(1);

        let response = match parse_jsonrpc_request(&frame) {
            Ok(request) => match dispatch_jsonrpc_request(&request, state) {
                Ok(result) => jsonrpc_result_frame(request.id, result),
                Err(error) => {
                    error_count = error_count.saturating_add(1);
                    jsonrpc_error_frame(error.id, error.code, error.message)
                }
            },
            Err(error) => {
                error_count = error_count.saturating_add(1);
                jsonrpc_error_frame(error.id, error.code, error.message)
            }
        };
        write_jsonrpc_content_length_frame(writer, &response)?;
    }

    Ok(McpServeReport {
        processed_frames,
        error_count,
    })
}

fn parse_jsonrpc_request(value: &Value) -> Result<McpJsonRpcRequest, McpDispatchError> {
    let Some(object) = value.as_object() else {
        return Err(McpDispatchError::new(
            Value::Null,
            MCP_ERROR_INVALID_REQUEST,
            "jsonrpc request must be an object",
        ));
    };
    let id = object.get("id").cloned().ok_or_else(|| {
        McpDispatchError::new(
            Value::Null,
            MCP_ERROR_INVALID_REQUEST,
            "jsonrpc request must include id",
        )
    })?;
    let jsonrpc = object
        .get("jsonrpc")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if jsonrpc != MCP_JSONRPC_VERSION {
        return Err(McpDispatchError::new(
            id,
            MCP_ERROR_INVALID_REQUEST,
            format!("jsonrpc must be '{}'", MCP_JSONRPC_VERSION),
        ));
    }
    let method = object
        .get("method")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            McpDispatchError::new(
                id.clone(),
                MCP_ERROR_INVALID_REQUEST,
                "jsonrpc request must include non-empty method",
            )
        })?;
    let params = match object.get("params") {
        Some(Value::Object(params)) => params.clone(),
        Some(_) => {
            return Err(McpDispatchError::new(
                id,
                MCP_ERROR_INVALID_PARAMS,
                "jsonrpc request params must be an object",
            ))
        }
        None => serde_json::Map::new(),
    };
    Ok(McpJsonRpcRequest {
        id,
        method: method.to_string(),
        params,
    })
}

fn dispatch_jsonrpc_request(
    request: &McpJsonRpcRequest,
    state: &McpServerState,
) -> Result<Value, McpDispatchError> {
    match request.method.as_str() {
        "initialize" => Ok(handle_initialize(state)),
        "tools/list" => Ok(handle_tools_list(state)),
        "tools/call" => handle_tools_call(state, &request.params).map_err(|error| {
            McpDispatchError::new(
                request.id.clone(),
                MCP_ERROR_INVALID_PARAMS,
                error.to_string(),
            )
        }),
        other => Err(McpDispatchError::new(
            request.id.clone(),
            MCP_ERROR_METHOD_NOT_FOUND,
            format!("unsupported method '{}'", other),
        )),
    }
}

fn handle_initialize(state: &McpServerState) -> Value {
    let context_providers = state
        .context_providers
        .iter()
        .map(|value| Value::String(value.clone()))
        .collect::<Vec<_>>();
    json!({
        "protocolVersion": MCP_PROTOCOL_VERSION,
        "serverInfo": {
            "name": "tau-coding-agent",
            "version": env!("CARGO_PKG_VERSION")
        },
        "capabilities": {
            "tools": {
                "listChanged": false
            },
            "experimental": {
                "contextProviders": context_providers
            }
        }
    })
}

fn handle_tools_list(state: &McpServerState) -> Value {
    let mut tools = builtin_mcp_tools(state);
    tools.extend(
        state
            .external_tools
            .iter()
            .map(|tool| McpToolDescriptor {
                name: format!(
                    "{MCP_TOOL_PREFIX_EXTERNAL}{}.{}",
                    tool.server_name, tool.tool_name
                ),
                description: format!(
                    "{} (external server {})",
                    tool.description, tool.server_name
                ),
                input_schema: tool.input_schema.clone(),
            })
            .collect::<Vec<_>>(),
    );
    tools.sort_by(|left, right| left.name.cmp(&right.name));
    json!({
        "tools": tools
            .into_iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema
                })
            })
            .collect::<Vec<_>>()
    })
}

fn handle_tools_call(
    state: &McpServerState,
    params: &serde_json::Map<String, Value>,
) -> Result<Value> {
    let tool_name = params
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("tools/call requires non-empty field 'name'"))?;
    let arguments = match params.get("arguments") {
        Some(Value::Object(arguments)) => Value::Object(arguments.clone()),
        Some(_) => bail!("tools/call field 'arguments' must be an object when provided"),
        None => Value::Object(serde_json::Map::new()),
    };

    if let Some(qualified) = tool_name.strip_prefix(MCP_TOOL_PREFIX_EXTERNAL) {
        let mut parts = qualified.splitn(2, '.');
        let server_name = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("external tool name must include server name"))?;
        let external_tool_name = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("external tool name must include tool identifier"))?;
        let server = state
            .external_servers
            .iter()
            .find(|candidate| candidate.name == server_name)
            .ok_or_else(|| anyhow!("unknown external mcp server '{}'", server_name))?;
        let init = jsonrpc_request_frame(
            Value::String(MCP_EXTERNAL_INIT_REQUEST_ID.to_string()),
            "initialize",
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "tau-coding-agent",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        );
        let call = jsonrpc_request_frame(
            Value::String(MCP_EXTERNAL_TOOLS_CALL_REQUEST_ID.to_string()),
            "tools/call",
            json!({
                "name": external_tool_name,
                "arguments": arguments
            }),
        );
        let responses = call_external_mcp_server(server, &[init, call])?;
        let result =
            external_response_result(&responses, MCP_EXTERNAL_TOOLS_CALL_REQUEST_ID, server)?;
        let is_error = result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        return Ok(mcp_tool_call_result(result, is_error));
    }

    if matches!(
        tool_name,
        MCP_TOOL_CONTEXT_SESSION | MCP_TOOL_CONTEXT_SKILLS | MCP_TOOL_CONTEXT_CHANNEL_STORE
    ) {
        let context_payload = execute_context_provider_tool(state, tool_name)?;
        return Ok(mcp_tool_call_result(context_payload, false));
    }

    let execution = execute_builtin_tool_call(tool_name, arguments, &state.tool_policy)?;
    Ok(mcp_tool_call_result(execution.content, execution.is_error))
}

fn execute_context_provider_tool(state: &McpServerState, tool_name: &str) -> Result<Value> {
    match tool_name {
        MCP_TOOL_CONTEXT_SESSION => {
            if !state
                .context_providers
                .contains(MCP_CONTEXT_PROVIDER_SESSION)
            {
                bail!(
                    "context provider '{}' is disabled",
                    MCP_CONTEXT_PROVIDER_SESSION
                );
            }
            let exists = state.session_path.exists();
            let entries = if exists {
                std::fs::read_to_string(&state.session_path)
                    .map(|raw| raw.lines().filter(|line| !line.trim().is_empty()).count())
                    .unwrap_or(0)
            } else {
                0
            };
            Ok(json!({
                "provider": MCP_CONTEXT_PROVIDER_SESSION,
                "path": state.session_path.display().to_string(),
                "exists": exists,
                "entries": entries,
            }))
        }
        MCP_TOOL_CONTEXT_SKILLS => {
            if !state
                .context_providers
                .contains(MCP_CONTEXT_PROVIDER_SKILLS)
            {
                bail!(
                    "context provider '{}' is disabled",
                    MCP_CONTEXT_PROVIDER_SKILLS
                );
            }
            let skills = list_skill_files(&state.skills_dir, 128)?;
            Ok(json!({
                "provider": MCP_CONTEXT_PROVIDER_SKILLS,
                "path": state.skills_dir.display().to_string(),
                "count": skills.len(),
                "files": skills,
            }))
        }
        MCP_TOOL_CONTEXT_CHANNEL_STORE => {
            if !state
                .context_providers
                .contains(MCP_CONTEXT_PROVIDER_CHANNEL_STORE)
            {
                bail!(
                    "context provider '{}' is disabled",
                    MCP_CONTEXT_PROVIDER_CHANNEL_STORE
                );
            }
            let channels_root = state.channel_store_root.join("channels");
            let channel_count = if channels_root.is_dir() {
                std::fs::read_dir(&channels_root)
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| entry.path().is_dir())
                    .count()
            } else {
                0
            };
            Ok(json!({
                "provider": MCP_CONTEXT_PROVIDER_CHANNEL_STORE,
                "path": channels_root.display().to_string(),
                "channel_count": channel_count,
            }))
        }
        other => bail!("unknown context provider tool '{}'", other),
    }
}

fn list_skill_files(root: &Path, limit: usize) -> Result<Vec<String>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let entries = std::fs::read_dir(&path)
            .with_context(|| format!("failed to read skills directory {}", path.display()))?;
        for entry in entries {
            let entry = entry.with_context(|| format!("failed to read {}", path.display()))?;
            let entry_path = entry.path();
            if entry_path.is_dir() {
                stack.push(entry_path);
                continue;
            }
            if entry_path
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.eq_ignore_ascii_case("md"))
                .unwrap_or(false)
            {
                files.push(entry_path.display().to_string());
                if files.len() >= limit {
                    files.sort();
                    return Ok(files);
                }
            }
        }
    }

    files.sort();
    Ok(files)
}

fn execute_builtin_tool_call(
    tool_name: &str,
    arguments: Value,
    policy: &ToolPolicy,
) -> Result<ToolExecutionResult> {
    let policy = Arc::new(policy.clone());
    match tool_name {
        MCP_TOOL_READ => Ok(block_on_tool_future(
            ReadTool::new(policy).execute(arguments),
        )),
        MCP_TOOL_WRITE => Ok(block_on_tool_future(
            WriteTool::new(policy).execute(arguments),
        )),
        MCP_TOOL_EDIT => Ok(block_on_tool_future(
            EditTool::new(policy).execute(arguments),
        )),
        MCP_TOOL_BASH => Ok(block_on_tool_future(
            BashTool::new(policy).execute(arguments),
        )),
        other => bail!("unknown mcp tool '{}'", other),
    }
}

fn block_on_tool_future<F>(future: F) -> ToolExecutionResult
where
    F: Future<Output = ToolExecutionResult>,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(future)),
        Err(_) => {
            match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime.block_on(future),
                Err(error) => ToolExecutionResult::error(json!({
                    "error": format!("failed to create temporary tokio runtime for mcp tool execution: {error}")
                })),
            }
        }
    }
}

fn builtin_mcp_tools(state: &McpServerState) -> Vec<McpToolDescriptor> {
    let policy = Arc::new(state.tool_policy.clone());
    let mut tools = vec![
        agent_tool_descriptor(MCP_TOOL_READ, &ReadTool::new(policy.clone())),
        agent_tool_descriptor(MCP_TOOL_WRITE, &WriteTool::new(policy.clone())),
        agent_tool_descriptor(MCP_TOOL_EDIT, &EditTool::new(policy.clone())),
        agent_tool_descriptor(MCP_TOOL_BASH, &BashTool::new(policy)),
    ];

    if state
        .context_providers
        .contains(MCP_CONTEXT_PROVIDER_SESSION)
    {
        tools.push(McpToolDescriptor {
            name: MCP_TOOL_CONTEXT_SESSION.to_string(),
            description: "Summarize configured Tau session context".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        });
    }
    if state
        .context_providers
        .contains(MCP_CONTEXT_PROVIDER_SKILLS)
    {
        tools.push(McpToolDescriptor {
            name: MCP_TOOL_CONTEXT_SKILLS.to_string(),
            description: "List discovered skills context files".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        });
    }
    if state
        .context_providers
        .contains(MCP_CONTEXT_PROVIDER_CHANNEL_STORE)
    {
        tools.push(McpToolDescriptor {
            name: MCP_TOOL_CONTEXT_CHANNEL_STORE.to_string(),
            description: "Summarize channel-store context state".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        });
    }

    tools
}

fn agent_tool_descriptor<T: AgentTool>(name: &str, tool: &T) -> McpToolDescriptor {
    let definition = tool.definition();
    McpToolDescriptor {
        name: name.to_string(),
        description: definition.description,
        input_schema: definition.parameters,
    }
}

fn mcp_tool_call_result(content: Value, is_error: bool) -> Value {
    let text = serde_json::to_string_pretty(&content)
        .unwrap_or_else(|_| "{\"error\":\"failed to serialize tool result\"}".to_string());
    json!({
        "content": [{
            "type": MCP_CONTENT_TYPE_TEXT,
            "text": text
        }],
        "isError": is_error,
        "structuredContent": content,
    })
}

fn read_jsonrpc_content_length_frame<R>(reader: &mut R) -> Result<Option<Value>>
where
    R: BufRead,
{
    let mut content_length: Option<usize> = None;
    let mut saw_header = false;
    loop {
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .context("failed to read mcp frame header line")?;
        if bytes == 0 {
            if saw_header {
                bail!("unexpected eof while reading mcp frame headers");
            }
            return Ok(None);
        }
        saw_header = true;
        if line == "\n" || line == "\r\n" {
            break;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        let (name, value) = trimmed.split_once(':').ok_or_else(|| {
            anyhow!(
                "invalid mcp header '{}': expected 'Name: value' format",
                trimmed
            )
        })?;
        if name.trim().eq_ignore_ascii_case("content-length") {
            let parsed = value
                .trim()
                .parse::<usize>()
                .context("invalid Content-Length header value")?;
            content_length = Some(parsed);
        }
    }

    let content_length =
        content_length.ok_or_else(|| anyhow!("mcp frame is missing Content-Length header"))?;
    let mut body = vec![0_u8; content_length];
    reader
        .read_exact(&mut body)
        .context("failed to read mcp frame body bytes")?;
    let value = serde_json::from_slice::<Value>(&body).context("failed to parse mcp JSON frame")?;
    Ok(Some(value))
}

fn write_jsonrpc_content_length_frame<W>(writer: &mut W, value: &Value) -> Result<()>
where
    W: Write,
{
    let encoded = serde_json::to_vec(value).context("failed to encode mcp jsonrpc response")?;
    write!(writer, "Content-Length: {}\r\n\r\n", encoded.len())
        .context("failed to write mcp frame header")?;
    writer
        .write_all(&encoded)
        .context("failed to write mcp frame body")?;
    writer.flush().context("failed to flush mcp frame output")?;
    Ok(())
}

fn jsonrpc_request_frame(id: Value, method: &str, params: Value) -> Value {
    json!({
        "jsonrpc": MCP_JSONRPC_VERSION,
        "id": id,
        "method": method,
        "params": params,
    })
}

fn jsonrpc_result_frame(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": MCP_JSONRPC_VERSION,
        "id": id,
        "result": result,
    })
}

fn jsonrpc_error_frame(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": MCP_JSONRPC_VERSION,
        "id": id,
        "error": {
            "code": code,
            "message": message.into(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        execute_context_provider_tool, jsonrpc_request_frame, normalize_external_server_name,
        resolve_mcp_context_providers, serve_mcp_jsonrpc_reader, McpExternalServerConfig,
        McpServerState, MCP_CONTEXT_PROVIDER_CHANNEL_STORE, MCP_CONTEXT_PROVIDER_SESSION,
        MCP_ERROR_INVALID_REQUEST, MCP_ERROR_METHOD_NOT_FOUND, MCP_JSONRPC_VERSION,
    };
    use crate::tools::ToolPolicy;
    use serde::Deserialize;
    use serde_json::Value;
    use std::collections::BTreeMap;
    use std::io::{BufRead, Read};
    use std::path::Path;
    use tempfile::tempdir;

    fn encode_frames(frames: &[Value]) -> Vec<u8> {
        let mut encoded = Vec::new();
        for frame in frames {
            let payload = serde_json::to_vec(frame).expect("encode frame");
            encoded
                .extend_from_slice(format!("Content-Length: {}\r\n\r\n", payload.len()).as_bytes());
            encoded.extend_from_slice(&payload);
        }
        encoded
    }

    fn decode_frames(raw: &[u8]) -> Vec<Value> {
        let mut frames = Vec::new();
        let mut cursor = std::io::Cursor::new(raw);
        let mut reader = std::io::BufReader::new(&mut cursor);
        loop {
            let mut header = String::new();
            let bytes = reader.read_line(&mut header).expect("header");
            if bytes == 0 {
                break;
            }
            if header.trim().is_empty() {
                continue;
            }
            let length = header
                .split_once(':')
                .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                .expect("content length");
            let mut separator = String::new();
            reader.read_line(&mut separator).expect("separator");
            let mut body = vec![0_u8; length];
            reader.read_exact(&mut body).expect("body");
            let frame = serde_json::from_slice::<Value>(&body).expect("json frame");
            frames.push(frame);
        }
        frames
    }

    fn test_state() -> McpServerState {
        let temp = tempdir().expect("tempdir");
        let tau_root = temp.path().join(".tau");
        std::fs::create_dir_all(tau_root.join("skills")).expect("create skills");
        std::fs::create_dir_all(tau_root.join("sessions")).expect("create sessions");
        std::fs::create_dir_all(tau_root.join("channel-store/channels"))
            .expect("create channel store");
        std::fs::write(tau_root.join("sessions/default.jsonl"), "{}\n").expect("write session");
        McpServerState {
            tool_policy: ToolPolicy::new(vec![temp.path().to_path_buf()]),
            session_path: tau_root.join("sessions/default.jsonl"),
            skills_dir: tau_root.join("skills"),
            channel_store_root: tau_root.join("channel-store"),
            context_providers: resolve_mcp_context_providers(&[])
                .expect("providers")
                .into_iter()
                .collect(),
            external_servers: Vec::new(),
            external_tools: Vec::new(),
        }
    }

    #[derive(Debug, Clone, Deserialize)]
    struct McpProtocolFixture {
        schema_version: u32,
        name: String,
        requests: Vec<Value>,
        expected_response_ids: Vec<String>,
        expected_methods: Vec<String>,
    }

    fn load_mcp_protocol_fixture(name: &str) -> McpProtocolFixture {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("mcp-protocol")
            .join(name);
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        serde_json::from_str::<McpProtocolFixture>(&raw)
            .unwrap_or_else(|error| panic!("invalid fixture {}: {error}", path.display()))
    }

    #[test]
    fn unit_resolve_mcp_context_providers_defaults_and_validation() {
        let defaults = resolve_mcp_context_providers(&[]).expect("default providers");
        assert_eq!(defaults, vec!["session", "skills", "channel-store"]);

        let selected = resolve_mcp_context_providers(&[
            "skills".to_string(),
            "session".to_string(),
            "skills".to_string(),
        ])
        .expect("selected providers");
        assert_eq!(selected, vec!["skills", "session"]);

        let error = resolve_mcp_context_providers(&["bad-provider".to_string()])
            .expect_err("invalid provider should fail");
        assert!(error
            .to_string()
            .contains("unsupported mcp context provider"));
    }

    #[test]
    fn unit_normalize_external_server_name_rejects_invalid_tokens() {
        assert_eq!(
            normalize_external_server_name("Server_01").expect("name"),
            "server_01"
        );
        let error =
            normalize_external_server_name("bad name").expect_err("spaces should be rejected");
        assert!(error
            .to_string()
            .contains("must contain only ASCII letters"));
    }

    #[test]
    fn functional_mcp_server_initialize_and_tools_list_roundtrip() {
        let state = test_state();
        let request_frames = vec![
            jsonrpc_request_frame(
                Value::String("req-init".to_string()),
                "initialize",
                serde_json::json!({}),
            ),
            jsonrpc_request_frame(
                Value::String("req-tools".to_string()),
                "tools/list",
                serde_json::json!({}),
            ),
        ];
        let raw = encode_frames(&request_frames);
        let mut reader = std::io::BufReader::new(std::io::Cursor::new(raw));
        let mut writer = Vec::new();
        let report = serve_mcp_jsonrpc_reader(&mut reader, &mut writer, &state)
            .expect("serve should succeed");
        assert_eq!(report.processed_frames, 2);
        assert_eq!(report.error_count, 0);

        let responses = decode_frames(&writer);
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0]["jsonrpc"], MCP_JSONRPC_VERSION);
        assert_eq!(responses[0]["id"], "req-init");
        assert_eq!(responses[0]["result"]["protocolVersion"], "2024-11-05");
        let tools = responses[1]["result"]["tools"]
            .as_array()
            .expect("tools array");
        assert!(tools.iter().any(|tool| tool["name"] == "tau.read"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "tau.context.session"));
    }

    #[test]
    fn integration_mcp_protocol_fixture_initialize_tools_list_roundtrip() {
        let fixture = load_mcp_protocol_fixture("initialize-tools-list.json");
        assert_eq!(fixture.schema_version, 1);
        assert_eq!(fixture.name, "initialize-tools-list");
        assert_eq!(
            fixture.expected_methods,
            vec!["initialize".to_string(), "tools/list".to_string()]
        );

        let state = test_state();
        let raw = encode_frames(&fixture.requests);
        let mut reader = std::io::BufReader::new(std::io::Cursor::new(raw));
        let mut writer = Vec::new();
        let report = serve_mcp_jsonrpc_reader(&mut reader, &mut writer, &state)
            .expect("serve should succeed");
        assert_eq!(report.processed_frames, fixture.requests.len());
        assert_eq!(report.error_count, 0);

        let responses = decode_frames(&writer);
        let ids = responses
            .iter()
            .map(|response| {
                response["id"]
                    .as_str()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| response["id"].to_string())
            })
            .collect::<Vec<_>>();
        assert_eq!(ids, fixture.expected_response_ids);
    }

    #[test]
    fn integration_tools_call_context_provider_returns_structured_payload() {
        let mut state = test_state();
        state.context_providers = [MCP_CONTEXT_PROVIDER_SESSION.to_string()]
            .into_iter()
            .collect();
        let result = execute_context_provider_tool(&state, "tau.context.session")
            .expect("context provider call should succeed");
        assert_eq!(result["provider"], MCP_CONTEXT_PROVIDER_SESSION);
        assert!(result["exists"].is_boolean());
        assert!(result["entries"].is_number());
    }

    #[test]
    fn integration_tools_call_write_denies_protected_paths() {
        let temp = tempdir().expect("tempdir");
        let tau_root = temp.path().join(".tau");
        std::fs::create_dir_all(tau_root.join("skills")).expect("create skills");
        std::fs::create_dir_all(tau_root.join("sessions")).expect("create sessions");
        std::fs::create_dir_all(tau_root.join("channel-store/channels"))
            .expect("create channel store");
        std::fs::write(tau_root.join("sessions/default.jsonl"), "{}\n").expect("write session");
        let state = McpServerState {
            tool_policy: ToolPolicy::new(vec![temp.path().to_path_buf()]),
            session_path: tau_root.join("sessions/default.jsonl"),
            skills_dir: tau_root.join("skills"),
            channel_store_root: tau_root.join("channel-store"),
            context_providers: resolve_mcp_context_providers(&[])
                .expect("providers")
                .into_iter()
                .collect(),
            external_servers: Vec::new(),
            external_tools: Vec::new(),
        };
        let request_frames = vec![jsonrpc_request_frame(
            Value::String("req-write".to_string()),
            "tools/call",
            serde_json::json!({
                "name": "tau.write",
                "arguments": {
                    "path": temp.path().join("AGENTS.md").display().to_string(),
                    "content": "blocked"
                }
            }),
        )];
        let raw = encode_frames(&request_frames);
        let mut reader = std::io::BufReader::new(std::io::Cursor::new(raw));
        let mut writer = Vec::new();
        let report = serve_mcp_jsonrpc_reader(&mut reader, &mut writer, &state)
            .expect("serve should succeed");
        assert_eq!(report.processed_frames, 1);
        assert_eq!(report.error_count, 0);

        let responses = decode_frames(&writer);
        assert_eq!(responses[0]["id"], "req-write");
        assert_eq!(responses[0]["result"]["isError"], true);
        assert_eq!(
            responses[0]["result"]["structuredContent"]["policy_rule"],
            "protected_path"
        );
        assert_eq!(
            responses[0]["result"]["structuredContent"]["reason_code"],
            "protected_path_denied"
        );
    }

    #[test]
    fn regression_invalid_request_and_unknown_method_return_jsonrpc_errors() {
        let state = test_state();
        let request_frames = vec![
            serde_json::json!({
                "jsonrpc": MCP_JSONRPC_VERSION,
                "method": "initialize",
                "params": {}
            }),
            jsonrpc_request_frame(
                Value::String("req-unknown".to_string()),
                "method/unknown",
                serde_json::json!({}),
            ),
        ];
        let raw = encode_frames(&request_frames);
        let mut reader = std::io::BufReader::new(std::io::Cursor::new(raw));
        let mut writer = Vec::new();
        let report = serve_mcp_jsonrpc_reader(&mut reader, &mut writer, &state)
            .expect("serve should return report");
        assert_eq!(report.processed_frames, 2);
        assert_eq!(report.error_count, 2);

        let responses = decode_frames(&writer);
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0]["error"]["code"], MCP_ERROR_INVALID_REQUEST);
        assert_eq!(responses[1]["error"]["code"], MCP_ERROR_METHOD_NOT_FOUND);
    }

    #[test]
    fn regression_context_provider_guard_rejects_disabled_provider() {
        let mut state = test_state();
        state.context_providers = [MCP_CONTEXT_PROVIDER_CHANNEL_STORE.to_string()]
            .into_iter()
            .collect();
        let error = execute_context_provider_tool(&state, "tau.context.session")
            .expect_err("disabled provider should fail");
        assert!(error.to_string().contains("is disabled"));
    }

    #[test]
    fn integration_external_discovery_and_call_via_line_jsonrpc_server() {
        let temp = tempdir().expect("tempdir");
        let script = temp.path().join("mock-external-mcp.sh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
set -eu
while IFS= read -r line; do
  if [ -z "$line" ]; then
    continue
  fi
  method=$(printf '%s' "$line" | sed -n 's/.*"method":"\([^"]*\)".*/\1/p')
  id=$(printf '%s' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  if [ "$method" = "initialize" ]; then
    printf '{"jsonrpc":"2.0","id":"%s","result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{"listChanged":false}}}}\n' "$id"
    continue
  fi
  if [ "$method" = "tools/list" ]; then
    printf '{"jsonrpc":"2.0","id":"%s","result":{"tools":[{"name":"echo","description":"echo tool","inputSchema":{"type":"object","properties":{"value":{"type":"string"}},"required":["value"]}}]}}\n' "$id"
    continue
  fi
  if [ "$method" = "tools/call" ]; then
    printf '{"jsonrpc":"2.0","id":"%s","result":{"content":[{"type":"text","text":"external-ok"}],"isError":false,"structuredContent":{"ok":true}}}\n' "$id"
    continue
  fi
done
"#,
        )
        .expect("write script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script).expect("metadata").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script, perms).expect("chmod");
        }

        let config = McpExternalServerConfig {
            name: "mock".to_string(),
            command: script.display().to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            enabled: true,
        };
        let discovered = super::discover_external_mcp_tools(
            std::slice::from_ref(&config),
            &super::reserved_builtin_mcp_tool_names(),
        )
        .expect("discover external tool");
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].tool_name, "echo");

        let state = McpServerState {
            tool_policy: ToolPolicy::new(vec![temp.path().to_path_buf()]),
            session_path: temp.path().join(".tau/sessions/default.jsonl"),
            skills_dir: temp.path().join(".tau/skills"),
            channel_store_root: temp.path().join(".tau/channel-store"),
            context_providers: resolve_mcp_context_providers(&[])
                .expect("providers")
                .into_iter()
                .collect(),
            external_servers: vec![config],
            external_tools: discovered,
        };

        let request_frames = vec![jsonrpc_request_frame(
            Value::String("req-call".to_string()),
            "tools/call",
            serde_json::json!({
                "name": "external.mock.echo",
                "arguments": {"value":"hello"}
            }),
        )];
        let raw = encode_frames(&request_frames);
        let mut reader = std::io::BufReader::new(std::io::Cursor::new(raw));
        let mut writer = Vec::new();
        let report = serve_mcp_jsonrpc_reader(&mut reader, &mut writer, &state)
            .expect("serve should succeed");
        assert_eq!(report.processed_frames, 1);
        assert_eq!(report.error_count, 0);
        let responses = decode_frames(&writer);
        assert_eq!(responses[0]["id"], "req-call");
        assert_eq!(responses[0]["result"]["isError"], false);
        assert_eq!(
            responses[0]["result"]["structuredContent"]["isError"],
            false
        );
    }

    #[test]
    fn unit_reserved_builtin_mcp_tool_names_contains_catalog_entries() {
        let names = super::reserved_builtin_mcp_tool_names();
        assert!(names.contains(super::MCP_TOOL_READ));
        assert!(names.contains(super::MCP_TOOL_WRITE));
        assert!(names.contains(super::MCP_TOOL_EDIT));
        assert!(names.contains(super::MCP_TOOL_BASH));
        assert!(names.contains(super::MCP_TOOL_CONTEXT_SESSION));
        assert!(names.contains(super::MCP_TOOL_CONTEXT_SKILLS));
        assert!(names.contains(super::MCP_TOOL_CONTEXT_CHANNEL_STORE));
    }

    #[test]
    fn regression_external_discovery_rejects_reserved_builtin_name() {
        let temp = tempdir().expect("tempdir");
        let script = temp.path().join("mock-external-mcp-reserved.sh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
set -eu
while IFS= read -r line; do
  if [ -z "$line" ]; then
    continue
  fi
  method=$(printf '%s' "$line" | sed -n 's/.*"method":"\([^"]*\)".*/\1/p')
  id=$(printf '%s' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  if [ "$method" = "initialize" ]; then
    printf '{"jsonrpc":"2.0","id":"%s","result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{"listChanged":false}}}}\n' "$id"
    continue
  fi
  if [ "$method" = "tools/list" ]; then
    printf '{"jsonrpc":"2.0","id":"%s","result":{"tools":[{"name":"tau.read","description":"reserved","inputSchema":{"type":"object","properties":{}}}]}}\n' "$id"
    continue
  fi
done
"#,
        )
        .expect("write script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script).expect("metadata").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script, perms).expect("chmod");
        }

        let config = McpExternalServerConfig {
            name: "mock".to_string(),
            command: script.display().to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            enabled: true,
        };
        let error = super::discover_external_mcp_tools(
            std::slice::from_ref(&config),
            &super::reserved_builtin_mcp_tool_names(),
        )
        .expect_err("reserved tool names must be rejected");
        assert!(error
            .to_string()
            .contains("reserved built-in tool name 'tau.read'"));
    }

    #[test]
    fn regression_external_discovery_rejects_duplicate_qualified_names() {
        let temp = tempdir().expect("tempdir");
        let script = temp.path().join("mock-external-mcp-duplicate.sh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
set -eu
while IFS= read -r line; do
  if [ -z "$line" ]; then
    continue
  fi
  method=$(printf '%s' "$line" | sed -n 's/.*"method":"\([^"]*\)".*/\1/p')
  id=$(printf '%s' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  if [ "$method" = "initialize" ]; then
    printf '{"jsonrpc":"2.0","id":"%s","result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{"listChanged":false}}}}\n' "$id"
    continue
  fi
  if [ "$method" = "tools/list" ]; then
    printf '{"jsonrpc":"2.0","id":"%s","result":{"tools":[{"name":"echo","description":"first","inputSchema":{"type":"object","properties":{}}},{"name":"echo","description":"second","inputSchema":{"type":"object","properties":{}}}]}}\n' "$id"
    continue
  fi
done
"#,
        )
        .expect("write script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script).expect("metadata").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script, perms).expect("chmod");
        }

        let config = McpExternalServerConfig {
            name: "mock".to_string(),
            command: script.display().to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            enabled: true,
        };
        let error = super::discover_external_mcp_tools(
            std::slice::from_ref(&config),
            &super::reserved_builtin_mcp_tool_names(),
        )
        .expect_err("duplicate names must fail");
        assert!(error
            .to_string()
            .contains("duplicate tool registration 'external.mock.echo'"));
    }
}
