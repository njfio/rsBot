use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use reqwest::{blocking::Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tau_agent_core::{Agent, AgentTool, ToolExecutionResult};
use tau_ai::ToolDefinition;
use tau_cli::Cli;
use tau_provider::{
    load_credential_store, resolve_credential_store_encryption_mode, save_credential_store,
    CredentialStoreEncryptionMode, IntegrationCredentialStoreRecord,
};

const MCP_JSONRPC_VERSION: &str = "2.0";
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const MCP_CLIENT_CONFIG_SCHEMA_VERSION: u32 = 1;
const MCP_CLIENT_INIT_REQUEST_ID: &str = "tau-client-init";
const MCP_CLIENT_TOOLS_LIST_REQUEST_ID: &str = "tau-client-tools-list";
const MCP_CLIENT_TOOLS_CALL_REQUEST_ID: &str = "tau-client-tools-call";
const MCP_CLIENT_TOOL_PREFIX: &str = "mcp.";
const MCP_CLIENT_OAUTH_INTEGRATION_PREFIX: &str = "mcp.oauth.";
const DEFAULT_HTTP_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_OAUTH_REFRESH_SKEW_SECONDS: u64 = 60;
const OAUTH_CODE_CHALLENGE_METHOD_S256: &str = "S256";

fn default_server_enabled() -> bool {
    true
}

fn default_sse_probe() -> bool {
    true
}

fn default_http_timeout_ms() -> u64 {
    DEFAULT_HTTP_TIMEOUT_MS
}

fn default_oauth_refresh_skew_seconds() -> u64 {
    DEFAULT_OAUTH_REFRESH_SKEW_SECONDS
}

fn default_oauth_redirect_uri() -> String {
    "urn:ietf:wg:oauth:2.0:oob".to_string()
}

#[derive(Debug, Clone, Deserialize)]
struct McpClientConfigFile {
    schema_version: u32,
    #[serde(default)]
    servers: Vec<McpClientServerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct McpClientServerConfig {
    name: String,
    #[serde(default = "default_server_enabled")]
    enabled: bool,
    #[serde(flatten)]
    transport: McpClientTransportConfig,
    #[serde(default)]
    auth: Option<McpClientAuthConfig>,
    #[serde(default)]
    tool_prefix: Option<String>,
    #[serde(default = "default_sse_probe")]
    sse_probe: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum McpClientTransportConfig {
    Tagged(McpClientTaggedTransportConfig),
    LegacyStdio(McpClientStdioTransportConfig),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "transport", rename_all = "kebab-case")]
enum McpClientTaggedTransportConfig {
    Stdio(McpClientStdioTransportConfig),
    HttpSse(McpClientHttpSseTransportConfig),
}

#[derive(Debug, Clone, Deserialize)]
struct McpClientStdioTransportConfig {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
struct McpClientHttpSseTransportConfig {
    endpoint: String,
    #[serde(default)]
    sse_endpoint: Option<String>,
    #[serde(default = "default_http_timeout_ms")]
    timeout_ms: u64,
    #[serde(default)]
    headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum McpClientAuthConfig {
    #[serde(rename = "oauth_pkce")]
    OAuthPkce(McpClientOAuthPkceConfig),
}

#[derive(Debug, Clone, Deserialize)]
struct McpClientOAuthPkceConfig {
    authorization_url: String,
    token_url: String,
    client_id: String,
    #[serde(default = "default_oauth_redirect_uri")]
    redirect_uri: String,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    authorization_code: Option<String>,
    #[serde(default)]
    code_verifier: Option<String>,
    #[serde(default)]
    extra_authorization_params: BTreeMap<String, String>,
    #[serde(default)]
    extra_token_params: BTreeMap<String, String>,
    #[serde(default = "default_oauth_refresh_skew_seconds")]
    refresh_skew_seconds: u64,
}

#[derive(Debug, Clone)]
enum McpClientTransportRuntime {
    Stdio(McpClientStdioTransportConfig),
    HttpSse(McpClientHttpSseTransportConfig),
}

#[derive(Debug, Clone)]
struct McpClientServerRuntime {
    name: String,
    transport: McpClientTransportRuntime,
    auth: Option<McpClientAuthConfig>,
    tool_prefix: String,
    sse_probe: bool,
}

#[derive(Debug, Clone)]
struct McpClientDiscoveredTool {
    server: Arc<McpClientServerRuntime>,
    local_tool_name: String,
    remote_tool_name: String,
    description: String,
    input_schema: Value,
}

#[derive(Debug, Clone)]
struct McpClientDiscoveryOutcome {
    config_path: PathBuf,
    servers: Vec<Arc<McpClientServerRuntime>>,
    tools: Vec<McpClientDiscoveredTool>,
    diagnostics: Vec<McpClientDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
/// Diagnostic entry emitted during MCP client discovery and registration.
pub struct McpClientDiagnostic {
    pub server: String,
    pub phase: String,
    pub status: String,
    pub reason_code: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
/// Tool descriptor exposed by MCP client inspect reports.
pub struct McpClientInspectTool {
    pub server: String,
    pub local_tool_name: String,
    pub remote_tool_name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
/// Inspect report for MCP client discovery mode.
pub struct McpClientInspectReport {
    pub schema_version: u32,
    pub config_path: String,
    pub server_count: usize,
    pub discovered_tool_count: usize,
    pub tools: Vec<McpClientInspectTool>,
    pub diagnostics: Vec<McpClientDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
/// Summary report returned after registering MCP client tools on an agent.
pub struct McpClientRegistrationReport {
    pub config_path: String,
    pub server_count: usize,
    pub discovered_tool_count: usize,
    pub registered_tool_count: usize,
    pub diagnostics: Vec<McpClientDiagnostic>,
}

#[derive(Debug, Clone)]
struct McpClientRuntimeContext {
    credential_store: PathBuf,
    credential_store_key: Option<String>,
    credential_store_encryption: CredentialStoreEncryptionMode,
}

impl McpClientRuntimeContext {
    fn from_cli(cli: &Cli) -> Self {
        Self {
            credential_store: cli.credential_store.clone(),
            credential_store_key: cli.credential_store_key.clone(),
            credential_store_encryption: resolve_credential_store_encryption_mode(cli),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpOauthTokenRecord {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    expires_unix: Option<u64>,
    #[serde(default)]
    updated_unix: Option<u64>,
}

#[derive(Debug, Clone)]
struct McpClientProxyTool {
    definition: ToolDefinition,
    remote_tool_name: String,
    server: Arc<McpClientServerRuntime>,
    context: McpClientRuntimeContext,
}

#[async_trait]
impl AgentTool for McpClientProxyTool {
    fn definition(&self) -> ToolDefinition {
        self.definition.clone()
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let remote_tool_name = self.remote_tool_name.clone();
        let server = self.server.clone();
        let context = self.context.clone();
        let call_server = server.clone();
        let call_remote_tool_name = remote_tool_name.clone();
        let join = tokio::task::spawn_blocking(move || {
            execute_remote_tool_call(&call_server, &context, &call_remote_tool_name, arguments)
        })
        .await;

        match join {
            Ok(Ok(result)) => normalize_remote_tool_execution_result(result),
            Ok(Err(error)) => ToolExecutionResult::error(json!({
                "reason_code": classify_mcp_client_error(&error),
                "message": error.to_string(),
                "server": server.name,
                "tool_name": remote_tool_name,
            })),
            Err(error) => ToolExecutionResult::error(json!({
                "reason_code": "mcp_client_runtime_join_failed",
                "message": error.to_string(),
                "server": server.name,
                "tool_name": remote_tool_name,
            })),
        }
    }
}

pub fn execute_mcp_client_inspect_command(cli: &Cli) -> Result<()> {
    if !cli.mcp_client_inspect {
        return Ok(());
    }

    let report = build_mcp_client_inspect_report(cli)?;
    if cli.mcp_client_inspect_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render mcp client inspect json")?
        );
    } else {
        println!("{}", render_mcp_client_inspect_report(&report));
    }
    Ok(())
}

pub fn register_mcp_client_tools(
    agent: &mut Agent,
    cli: &Cli,
) -> Result<McpClientRegistrationReport> {
    if !cli.mcp_client {
        return Ok(McpClientRegistrationReport {
            config_path: String::new(),
            server_count: 0,
            discovered_tool_count: 0,
            registered_tool_count: 0,
            diagnostics: vec![McpClientDiagnostic {
                server: "global".to_string(),
                phase: "registration".to_string(),
                status: "info".to_string(),
                reason_code: "mcp_client_disabled".to_string(),
                detail: "mcp client mode is disabled".to_string(),
            }],
        });
    }

    let context = McpClientRuntimeContext::from_cli(cli);
    let outcome = discover_mcp_client_tools(cli, &context)?;
    let discovered_tool_count = outcome.tools.len();
    let mut diagnostics = outcome.diagnostics;
    let mut registered_tool_count = 0usize;
    for tool in outcome.tools {
        if agent.has_tool(&tool.local_tool_name) {
            diagnostics.push(McpClientDiagnostic {
                server: tool.server.name.clone(),
                phase: "registration".to_string(),
                status: "warn".to_string(),
                reason_code: "mcp_client_tool_name_conflict".to_string(),
                detail: format!(
                    "tool '{}' was skipped because a tool with the same name is already registered",
                    tool.local_tool_name
                ),
            });
            continue;
        }
        agent.register_tool(McpClientProxyTool {
            definition: ToolDefinition {
                name: tool.local_tool_name.clone(),
                description: tool.description.clone(),
                parameters: tool.input_schema.clone(),
            },
            remote_tool_name: tool.remote_tool_name.clone(),
            server: tool.server.clone(),
            context: context.clone(),
        });
        registered_tool_count = registered_tool_count.saturating_add(1);
        diagnostics.push(McpClientDiagnostic {
            server: tool.server.name.clone(),
            phase: "registration".to_string(),
            status: "ok".to_string(),
            reason_code: "mcp_client_tool_registered".to_string(),
            detail: format!(
                "registered '{}' mapped to '{}'",
                tool.local_tool_name, tool.remote_tool_name
            ),
        });
    }

    Ok(McpClientRegistrationReport {
        config_path: outcome.config_path.display().to_string(),
        server_count: outcome.servers.len(),
        discovered_tool_count,
        registered_tool_count,
        diagnostics,
    })
}

pub fn render_mcp_client_inspect_report(report: &McpClientInspectReport) -> String {
    let mut lines = vec![format!(
        "mcp client inspect: config={} servers={} tools={} diagnostics={}",
        report.config_path,
        report.server_count,
        report.discovered_tool_count,
        report.diagnostics.len()
    )];
    for tool in &report.tools {
        lines.push(format!(
            "mcp client tool: server={} local={} remote={} description={}",
            tool.server, tool.local_tool_name, tool.remote_tool_name, tool.description
        ));
    }
    for diagnostic in &report.diagnostics {
        lines.push(format!(
            "mcp client diagnostic: server={} phase={} status={} reason_code={} detail={}",
            diagnostic.server,
            diagnostic.phase,
            diagnostic.status,
            diagnostic.reason_code,
            diagnostic.detail
        ));
    }
    lines.join("\n")
}

fn build_mcp_client_inspect_report(cli: &Cli) -> Result<McpClientInspectReport> {
    let context = McpClientRuntimeContext::from_cli(cli);
    let outcome = discover_mcp_client_tools(cli, &context)?;
    let tools = outcome
        .tools
        .iter()
        .map(|tool| McpClientInspectTool {
            server: tool.server.name.clone(),
            local_tool_name: tool.local_tool_name.clone(),
            remote_tool_name: tool.remote_tool_name.clone(),
            description: tool.description.clone(),
        })
        .collect::<Vec<_>>();
    Ok(McpClientInspectReport {
        schema_version: MCP_CLIENT_CONFIG_SCHEMA_VERSION,
        config_path: outcome.config_path.display().to_string(),
        server_count: outcome.servers.len(),
        discovered_tool_count: tools.len(),
        tools,
        diagnostics: outcome.diagnostics,
    })
}

fn discover_mcp_client_tools(
    cli: &Cli,
    context: &McpClientRuntimeContext,
) -> Result<McpClientDiscoveryOutcome> {
    let config_path = cli.mcp_external_server_config.clone().ok_or_else(|| {
        anyhow!(
            "--mcp-client requires --mcp-external-server-config <path> (or TAU_MCP_EXTERNAL_SERVER_CONFIG)"
        )
    })?;
    let servers = load_mcp_client_servers(&config_path)?;
    let server_refs = servers.into_iter().map(Arc::new).collect::<Vec<_>>();
    let mut diagnostics = Vec::new();
    let mut tools = Vec::new();

    for server in &server_refs {
        match discover_tools_for_server(server, context) {
            Ok(server_tools) => {
                diagnostics.push(McpClientDiagnostic {
                    server: server.name.clone(),
                    phase: "discovery".to_string(),
                    status: "ok".to_string(),
                    reason_code: "mcp_client_server_discovered".to_string(),
                    detail: format!("discovered {} tool(s)", server_tools.len()),
                });
                tools.extend(server_tools);
            }
            Err(error) => diagnostics.push(McpClientDiagnostic {
                server: server.name.clone(),
                phase: "discovery".to_string(),
                status: "error".to_string(),
                reason_code: classify_mcp_client_error(&error).to_string(),
                detail: error.to_string(),
            }),
        }
    }

    Ok(McpClientDiscoveryOutcome {
        config_path,
        servers: server_refs,
        tools,
        diagnostics,
    })
}

fn load_mcp_client_servers(path: &Path) -> Result<Vec<McpClientServerRuntime>> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read mcp client config {}", path.display()))?;
    let parsed = serde_json::from_str::<McpClientConfigFile>(&raw)
        .with_context(|| format!("failed to parse mcp client config {}", path.display()))?;
    if parsed.schema_version != MCP_CLIENT_CONFIG_SCHEMA_VERSION {
        bail!(
            "unsupported mcp client config schema_version {} in {} (expected {})",
            parsed.schema_version,
            path.display(),
            MCP_CLIENT_CONFIG_SCHEMA_VERSION
        );
    }

    let mut seen_names = BTreeSet::new();
    let mut servers = Vec::new();
    for server in parsed.servers {
        if !server.enabled {
            continue;
        }
        let name = normalize_mcp_client_server_name(&server.name)?;
        if !seen_names.insert(name.clone()) {
            bail!(
                "duplicate mcp client server '{}' in {}",
                name,
                path.display()
            );
        }

        let transport = match server.transport {
            McpClientTransportConfig::LegacyStdio(transport) => {
                validate_stdio_transport(&name, &transport)?;
                McpClientTransportRuntime::Stdio(transport)
            }
            McpClientTransportConfig::Tagged(McpClientTaggedTransportConfig::Stdio(transport)) => {
                validate_stdio_transport(&name, &transport)?;
                McpClientTransportRuntime::Stdio(transport)
            }
            McpClientTransportConfig::Tagged(McpClientTaggedTransportConfig::HttpSse(
                transport,
            )) => {
                validate_http_sse_transport(&name, &transport)?;
                McpClientTransportRuntime::HttpSse(transport)
            }
        };
        let tool_prefix = normalize_tool_prefix(server.tool_prefix.as_deref(), &name)?;
        servers.push(McpClientServerRuntime {
            name,
            transport,
            auth: server.auth,
            tool_prefix,
            sse_probe: server.sse_probe,
        });
    }

    Ok(servers)
}

fn normalize_mcp_client_server_name(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("mcp client server name must be non-empty");
    }
    let mut normalized = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            normalized.push(ch.to_ascii_lowercase());
        } else {
            bail!(
                "mcp client server name '{}' must contain only ASCII letters, digits, '-' or '_'",
                trimmed
            );
        }
    }
    Ok(normalized)
}

fn normalize_tool_prefix(raw: Option<&str>, server_name: &str) -> Result<String> {
    let prefix = raw
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("{MCP_CLIENT_TOOL_PREFIX}{server_name}."));
    if !prefix
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        bail!(
            "mcp client tool prefix '{}' contains unsupported characters; use [a-zA-Z0-9._-]",
            prefix
        );
    }
    let resolved = if prefix.ends_with('.') {
        prefix
    } else {
        format!("{prefix}.")
    };
    Ok(resolved.to_ascii_lowercase())
}

fn normalize_tool_name_segment(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("mcp client remote tool name must be non-empty");
    }
    let mut normalized = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
            normalized.push(ch.to_ascii_lowercase());
        } else {
            normalized.push('_');
        }
    }
    let collapsed = normalized.trim_matches('_').to_string();
    if collapsed.is_empty() {
        bail!(
            "mcp client remote tool name '{}' has no usable characters after normalization",
            trimmed
        );
    }
    Ok(collapsed)
}

fn validate_stdio_transport(
    server_name: &str,
    transport: &McpClientStdioTransportConfig,
) -> Result<()> {
    if transport.command.trim().is_empty() {
        bail!(
            "mcp client server '{}' stdio transport requires a non-empty command",
            server_name
        );
    }
    Ok(())
}

fn validate_http_sse_transport(
    server_name: &str,
    transport: &McpClientHttpSseTransportConfig,
) -> Result<()> {
    if transport.endpoint.trim().is_empty() {
        bail!(
            "mcp client server '{}' http-sse transport requires endpoint",
            server_name
        );
    }
    if transport.timeout_ms == 0 {
        bail!(
            "mcp client server '{}' http-sse timeout must be greater than 0",
            server_name
        );
    }
    Ok(())
}

fn discover_tools_for_server(
    server: &Arc<McpClientServerRuntime>,
    context: &McpClientRuntimeContext,
) -> Result<Vec<McpClientDiscoveredTool>> {
    let responses = call_mcp_server(
        server,
        context,
        &[
            jsonrpc_request_frame(
                MCP_CLIENT_INIT_REQUEST_ID,
                "initialize",
                json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {"tools": {"listChanged": true}},
                    "clientInfo": {
                        "name": "tau-rs",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }),
            ),
            jsonrpc_request_frame(MCP_CLIENT_TOOLS_LIST_REQUEST_ID, "tools/list", json!({})),
        ],
        true,
    )?;
    let tools_result =
        jsonrpc_result_for_id(&responses, MCP_CLIENT_TOOLS_LIST_REQUEST_ID, &server.name)?;
    let tools_array = tools_result
        .get("tools")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            anyhow!(
                "mcp client server '{}' returned invalid tools/list payload",
                server.name
            )
        })?;

    let mut discovered = Vec::new();
    let mut seen_names = BTreeSet::new();
    for tool in tools_array {
        let object = tool.as_object().ok_or_else(|| {
            anyhow!(
                "mcp client server '{}' returned non-object tool descriptor",
                server.name
            )
        })?;
        let remote_tool_name = object
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                anyhow!(
                    "mcp client server '{}' returned tool descriptor missing name",
                    server.name
                )
            })?
            .to_string();
        let local_tool_name = format!(
            "{}{}",
            server.tool_prefix,
            normalize_tool_name_segment(&remote_tool_name)?
        );
        if !seen_names.insert(local_tool_name.clone()) {
            bail!(
                "mcp client server '{}' returned duplicate local tool '{}'",
                server.name,
                local_tool_name
            );
        }
        let description = object
            .get("description")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("external mcp client tool")
            .to_string();
        let input_schema = object
            .get("inputSchema")
            .cloned()
            .unwrap_or_else(|| json!({"type":"object","properties":{}}));
        discovered.push(McpClientDiscoveredTool {
            server: server.clone(),
            local_tool_name,
            remote_tool_name,
            description,
            input_schema,
        });
    }
    Ok(discovered)
}

fn execute_remote_tool_call(
    server: &Arc<McpClientServerRuntime>,
    context: &McpClientRuntimeContext,
    remote_tool_name: &str,
    arguments: Value,
) -> Result<Value> {
    let responses = call_mcp_server(
        server,
        context,
        &[
            jsonrpc_request_frame(
                MCP_CLIENT_INIT_REQUEST_ID,
                "initialize",
                json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {"tools": {"listChanged": true}},
                    "clientInfo": {
                        "name": "tau-rs",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }),
            ),
            jsonrpc_request_frame(
                MCP_CLIENT_TOOLS_CALL_REQUEST_ID,
                "tools/call",
                json!({
                    "name": remote_tool_name,
                    "arguments": arguments,
                }),
            ),
        ],
        false,
    )?;
    jsonrpc_result_for_id(&responses, MCP_CLIENT_TOOLS_CALL_REQUEST_ID, &server.name)
}

fn call_mcp_server(
    server: &Arc<McpClientServerRuntime>,
    context: &McpClientRuntimeContext,
    requests: &[Value],
    probe_sse: bool,
) -> Result<Vec<Value>> {
    match &server.transport {
        McpClientTransportRuntime::Stdio(transport) => {
            call_stdio_mcp_server(server, transport, requests)
        }
        McpClientTransportRuntime::HttpSse(transport) => {
            let bearer_token = resolve_server_bearer_token(server, context)?;
            if probe_sse && server.sse_probe {
                probe_http_sse_endpoint(transport, bearer_token.as_deref())?;
            }
            call_http_mcp_server(transport, bearer_token.as_deref(), requests)
        }
    }
}

fn call_stdio_mcp_server(
    server: &Arc<McpClientServerRuntime>,
    transport: &McpClientStdioTransportConfig,
    requests: &[Value],
) -> Result<Vec<Value>> {
    let mut command = Command::new(&transport.command);
    command.args(&transport.args);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = transport.cwd.as_ref() {
        command.current_dir(cwd);
    }
    for (key, value) in &transport.env {
        command.env(key, value);
    }
    let mut child = command.spawn().with_context(|| {
        format!(
            "failed to spawn mcp client server '{}' command '{}'",
            server.name, transport.command
        )
    })?;

    {
        let stdin = child.stdin.as_mut().ok_or_else(|| {
            anyhow!(
                "failed to open stdin for mcp client server '{}'",
                server.name
            )
        })?;
        for request in requests {
            let line =
                serde_json::to_string(request).context("failed to encode mcp client request")?;
            stdin.write_all(line.as_bytes()).with_context(|| {
                format!("failed to write mcp client request to '{}'", server.name)
            })?;
            stdin.write_all(b"\n").with_context(|| {
                format!(
                    "failed to write mcp client request terminator to '{}'",
                    server.name
                )
            })?;
        }
    }

    let stdout = child.stdout.take().ok_or_else(|| {
        anyhow!(
            "failed to open stdout for mcp client server '{}'",
            server.name
        )
    })?;
    let mut reader = BufReader::new(stdout);
    let mut responses = Vec::with_capacity(requests.len());
    for _ in requests {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).with_context(|| {
            format!(
                "failed to read response from mcp client server '{}'",
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
                "mcp client server '{}' returned invalid JSON '{}'",
                server.name, trimmed
            )
        })?;
        responses.push(value);
    }
    let status = child
        .wait()
        .with_context(|| format!("failed to wait for mcp client server '{}'", server.name))?;
    if !status.success() {
        bail!(
            "mcp client server '{}' exited with status {}",
            server.name,
            status
        );
    }
    Ok(responses)
}

fn call_http_mcp_server(
    transport: &McpClientHttpSseTransportConfig,
    bearer_token: Option<&str>,
    requests: &[Value],
) -> Result<Vec<Value>> {
    let client = Client::builder()
        .timeout(Duration::from_millis(transport.timeout_ms))
        .build()
        .context("failed to build mcp client http transport")?;
    let mut responses = Vec::with_capacity(requests.len());

    for request in requests {
        let mut outbound = client.post(&transport.endpoint).json(request);
        for (key, value) in &transport.headers {
            outbound = outbound.header(key, value);
        }
        if let Some(token) = bearer_token {
            outbound = outbound.bearer_auth(token);
        }
        let response = outbound.send().context("mcp client http request failed")?;
        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .unwrap_or_else(|_| "<unreadable response body>".to_string());
            bail!(
                "mcp client http request failed with status {} body {}",
                status,
                body
            );
        }
        let payload = response
            .json::<Value>()
            .context("failed to decode mcp client http json response")?;
        responses.push(payload);
    }

    Ok(responses)
}

fn probe_http_sse_endpoint(
    transport: &McpClientHttpSseTransportConfig,
    bearer_token: Option<&str>,
) -> Result<()> {
    let Some(sse_endpoint) = transport.sse_endpoint.as_ref() else {
        return Ok(());
    };
    let client = Client::builder()
        .timeout(Duration::from_millis(transport.timeout_ms))
        .build()
        .context("failed to build mcp client sse probe client")?;
    let mut request = client
        .get(sse_endpoint)
        .header("accept", "text/event-stream");
    for (key, value) in &transport.headers {
        request = request.header(key, value);
    }
    if let Some(token) = bearer_token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .context("mcp client sse probe request failed")?;
    let status = response.status();
    if !status.is_success() {
        bail!("mcp client sse probe returned status {}", status);
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !content_type.contains("text/event-stream") {
        bail!(
            "mcp client sse probe expected content-type text/event-stream but got '{}'",
            content_type
        );
    }
    Ok(())
}

fn jsonrpc_request_frame(id: &str, method: &str, params: Value) -> Value {
    json!({
        "jsonrpc": MCP_JSONRPC_VERSION,
        "id": id,
        "method": method,
        "params": params,
    })
}

fn jsonrpc_result_for_id(
    responses: &[Value],
    request_id: &str,
    server_name: &str,
) -> Result<Value> {
    let response = responses
        .iter()
        .find(|response| response.get("id").and_then(Value::as_str) == Some(request_id))
        .ok_or_else(|| {
            anyhow!(
                "mcp client server '{}' did not return a response for request '{}'",
                server_name,
                request_id
            )
        })?;
    if let Some(error) = response.get("error") {
        let code = error
            .get("code")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown json-rpc error");
        bail!(
            "mcp client server '{}' returned json-rpc error code={} message={}",
            server_name,
            code,
            message
        );
    }
    response.get("result").cloned().ok_or_else(|| {
        anyhow!(
            "mcp client server '{}' returned no result object",
            server_name
        )
    })
}

fn normalize_remote_tool_execution_result(result: Value) -> ToolExecutionResult {
    let is_error = result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut payload = result
        .get("structuredContent")
        .cloned()
        .or_else(|| result.get("content").cloned())
        .unwrap_or(result);

    if is_error {
        let has_reason_code = payload
            .as_object()
            .and_then(|object| object.get("reason_code"))
            .is_some();
        if !has_reason_code {
            payload = json!({
                "reason_code": "mcp_client_remote_error",
                "remote_result": payload,
            });
        }
        ToolExecutionResult::error(payload)
    } else {
        ToolExecutionResult::ok(payload)
    }
}

fn resolve_server_bearer_token(
    server: &Arc<McpClientServerRuntime>,
    context: &McpClientRuntimeContext,
) -> Result<Option<String>> {
    let Some(auth) = server.auth.as_ref() else {
        return Ok(None);
    };
    match auth {
        McpClientAuthConfig::OAuthPkce(config) => {
            let key = oauth_store_key(&server.name);
            let now_unix = current_unix_timestamp();
            if let Some(stored) = load_oauth_token(context, &key)? {
                if oauth_token_still_valid(&stored, now_unix, config.refresh_skew_seconds) {
                    return Ok(Some(stored.access_token));
                }
                if let Some(refresh_token) = stored
                    .refresh_token
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    let mut refreshed = refresh_oauth_token(config, refresh_token)?;
                    if refreshed.refresh_token.is_none() {
                        refreshed.refresh_token = Some(refresh_token.to_string());
                    }
                    save_oauth_token(context, &key, &refreshed)?;
                    return Ok(Some(refreshed.access_token));
                }
            }

            let authorization_code = config
                .authorization_code
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    let generated_verifier = generate_pkce_code_verifier(&server.name);
                    let authorization_url =
                        build_pkce_authorization_url(config, &generated_verifier).unwrap_or_else(|_| {
                            "<unable to build authorization URL>".to_string()
                        });
                    anyhow!(
                        "mcp client oauth authorization code missing for server '{}'; reason_code=mcp_client_oauth_authorization_code_missing authorization_url={} code_verifier={}",
                        server.name,
                        authorization_url,
                        generated_verifier
                    )
                })?;
            let code_verifier = config
                .code_verifier
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    anyhow!(
                        "mcp client oauth code_verifier missing for server '{}'; reason_code=mcp_client_oauth_code_verifier_missing",
                        server.name
                    )
                })?;
            validate_code_verifier(code_verifier)?;
            let token =
                exchange_oauth_authorization_code(config, authorization_code, code_verifier)?;
            save_oauth_token(context, &key, &token)?;
            Ok(Some(token.access_token))
        }
    }
}

fn oauth_store_key(server_name: &str) -> String {
    format!("{MCP_CLIENT_OAUTH_INTEGRATION_PREFIX}{server_name}")
}

fn oauth_token_still_valid(token: &McpOauthTokenRecord, now_unix: u64, skew_seconds: u64) -> bool {
    if token.access_token.trim().is_empty() {
        return false;
    }
    match token.expires_unix {
        Some(expires_unix) => expires_unix > now_unix.saturating_add(skew_seconds),
        None => true,
    }
}

fn load_oauth_token(
    context: &McpClientRuntimeContext,
    key: &str,
) -> Result<Option<McpOauthTokenRecord>> {
    let store = load_credential_store(
        &context.credential_store,
        context.credential_store_encryption,
        context.credential_store_key.as_deref(),
    )
    .with_context(|| {
        format!(
            "failed to read credential store {}",
            context.credential_store.display()
        )
    })?;
    let Some(entry) = store.integrations.get(key) else {
        return Ok(None);
    };
    if entry.revoked {
        return Ok(None);
    }
    let Some(secret) = entry
        .secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let parsed = serde_json::from_str::<McpOauthTokenRecord>(secret).with_context(|| {
        format!(
            "failed to parse oauth token payload for integration '{}'",
            key
        )
    })?;
    Ok(Some(parsed))
}

fn save_oauth_token(
    context: &McpClientRuntimeContext,
    key: &str,
    token: &McpOauthTokenRecord,
) -> Result<()> {
    let mut store = load_credential_store(
        &context.credential_store,
        context.credential_store_encryption,
        context.credential_store_key.as_deref(),
    )?;
    let encoded = serde_json::to_string(token).context("failed to encode oauth token payload")?;
    store.integrations.insert(
        key.to_string(),
        IntegrationCredentialStoreRecord {
            secret: Some(encoded),
            revoked: false,
            updated_unix: Some(current_unix_timestamp()),
        },
    );
    save_credential_store(
        &context.credential_store,
        &store,
        context.credential_store_key.as_deref(),
    )
    .with_context(|| {
        format!(
            "failed to persist oauth token into credential store {}",
            context.credential_store.display()
        )
    })?;
    Ok(())
}

fn exchange_oauth_authorization_code(
    config: &McpClientOAuthPkceConfig,
    authorization_code: &str,
    code_verifier: &str,
) -> Result<McpOauthTokenRecord> {
    let mut form = BTreeMap::new();
    form.insert("grant_type".to_string(), "authorization_code".to_string());
    form.insert("client_id".to_string(), config.client_id.clone());
    form.insert("code".to_string(), authorization_code.to_string());
    form.insert("redirect_uri".to_string(), config.redirect_uri.clone());
    form.insert("code_verifier".to_string(), code_verifier.to_string());
    for (key, value) in &config.extra_token_params {
        form.insert(key.clone(), value.clone());
    }
    request_oauth_token(&config.token_url, &form)
}

fn refresh_oauth_token(
    config: &McpClientOAuthPkceConfig,
    refresh_token: &str,
) -> Result<McpOauthTokenRecord> {
    let mut form = BTreeMap::new();
    form.insert("grant_type".to_string(), "refresh_token".to_string());
    form.insert("client_id".to_string(), config.client_id.clone());
    form.insert("refresh_token".to_string(), refresh_token.to_string());
    if !config.scopes.is_empty() {
        form.insert("scope".to_string(), config.scopes.join(" "));
    }
    for (key, value) in &config.extra_token_params {
        form.insert(key.clone(), value.clone());
    }
    request_oauth_token(&config.token_url, &form)
}

fn request_oauth_token(
    token_url: &str,
    form: &BTreeMap<String, String>,
) -> Result<McpOauthTokenRecord> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to build oauth token http client")?;
    let response = client
        .post(token_url)
        .form(form)
        .send()
        .context("oauth token request failed")?;
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .unwrap_or_else(|_| "<unreadable response body>".to_string());
        bail!(
            "oauth token endpoint returned status {} body {}",
            status,
            body
        );
    }
    let payload = response
        .json::<Value>()
        .context("failed to decode oauth token response body")?;
    let access_token = payload
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("oauth token response missing access_token"))?
        .to_string();
    let expires_unix = payload
        .get("expires_in")
        .and_then(Value::as_u64)
        .map(|expires_in| current_unix_timestamp().saturating_add(expires_in));
    Ok(McpOauthTokenRecord {
        access_token,
        refresh_token: payload
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        token_type: payload
            .get("token_type")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        scope: payload
            .get("scope")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        expires_unix,
        updated_unix: Some(current_unix_timestamp()),
    })
}

fn validate_code_verifier(code_verifier: &str) -> Result<()> {
    let len = code_verifier.len();
    if !(43..=128).contains(&len) {
        bail!(
            "oauth code_verifier must be 43..=128 characters (found {})",
            len
        );
    }
    if !code_verifier
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_' | '~'))
    {
        bail!("oauth code_verifier contains unsupported characters");
    }
    Ok(())
}

fn generate_pkce_code_verifier(server_name: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let seed = format!(
        "{}:{}:{}:{}",
        server_name,
        std::process::id(),
        nanos,
        std::thread::current().name().unwrap_or("tau")
    );
    let digest = Sha256::digest(seed.as_bytes());
    let second = Sha256::digest(digest);
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(&digest);
    bytes.extend_from_slice(&second);
    URL_SAFE_NO_PAD.encode(bytes).chars().take(64).collect()
}

fn build_pkce_authorization_url(
    config: &McpClientOAuthPkceConfig,
    code_verifier: &str,
) -> Result<String> {
    validate_code_verifier(code_verifier)?;
    let mut url = Url::parse(&config.authorization_url).with_context(|| {
        format!(
            "failed to parse oauth authorization_url '{}'",
            config.authorization_url
        )
    })?;
    let challenge_bytes = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = URL_SAFE_NO_PAD.encode(challenge_bytes);
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("response_type", "code");
        query.append_pair("client_id", &config.client_id);
        query.append_pair("redirect_uri", &config.redirect_uri);
        query.append_pair("code_challenge", &code_challenge);
        query.append_pair("code_challenge_method", OAUTH_CODE_CHALLENGE_METHOD_S256);
        if !config.scopes.is_empty() {
            query.append_pair("scope", &config.scopes.join(" "));
        }
        for (key, value) in &config.extra_authorization_params {
            query.append_pair(key, value);
        }
    }
    Ok(url.to_string())
}

fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn classify_mcp_client_error(error: &anyhow::Error) -> &'static str {
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("authorization code missing") {
        return "mcp_client_oauth_authorization_code_missing";
    }
    if message.contains("code_verifier missing") {
        return "mcp_client_oauth_code_verifier_missing";
    }
    if message.contains("oauth token endpoint") {
        return "mcp_client_oauth_token_exchange_failed";
    }
    if message.contains("oauth token response") {
        return "mcp_client_oauth_invalid_token_response";
    }
    if message.contains("sse probe") {
        return "mcp_client_sse_probe_failed";
    }
    if message.contains("http request failed") {
        return "mcp_client_http_request_failed";
    }
    if message.contains("json-rpc error") {
        return "mcp_client_jsonrpc_error";
    }
    if message.contains("invalid tools/list payload")
        || message.contains("tool descriptor")
        || message.contains("duplicate local tool")
    {
        return "mcp_client_invalid_tool_catalog";
    }
    if message.contains("spawn mcp client server")
        || message.contains("open stdin")
        || message.contains("open stdout")
        || message.contains("exited with status")
    {
        return "mcp_client_stdio_transport_failed";
    }
    "mcp_client_runtime_error"
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use httpmock::{
        Method::{GET, POST},
        MockServer,
    };
    use tau_ai::{ChatRequest, ChatResponse, LlmClient, TauAiError};
    use tempfile::tempdir;

    struct NoopClient;

    #[async_trait]
    impl LlmClient for NoopClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            Err(TauAiError::InvalidResponse("not used in tests".to_string()))
        }
    }

    fn parse_cli_with_stack(args: &[&str]) -> Cli {
        let argv = args
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        std::thread::Builder::new()
            .name("tau-cli-parse".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || Cli::parse_from(argv))
            .expect("spawn cli parse thread")
            .join()
            .expect("join cli parse thread")
    }

    fn write_stdio_mock_script(path: &Path) {
        std::fs::write(
            path,
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
    printf '{"jsonrpc":"2.0","id":"%s","result":{"isError":false,"structuredContent":{"ok":true}}}\n' "$id"
    continue
  fi
done
"#,
        )
        .expect("write mock script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path).expect("metadata").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(path, perms).expect("chmod");
        }
    }

    fn write_client_config(path: &Path, payload: Value) {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&payload).expect("serialize config"),
        )
        .expect("write config");
    }

    #[test]
    fn unit_load_mcp_client_config_accepts_stdio_and_http_sse_servers() {
        let temp = tempdir().expect("tempdir");
        let config_path = temp.path().join("mcp-client.json");
        write_client_config(
            &config_path,
            json!({
                "schema_version": 1,
                "servers": [
                    {
                        "name": "local_stdio",
                        "command": "echo"
                    },
                    {
                        "name": "remote_http",
                        "transport": "http-sse",
                        "endpoint": "http://127.0.0.1:9999/rpc",
                        "sse_endpoint": "http://127.0.0.1:9999/sse"
                    }
                ]
            }),
        );

        let servers = load_mcp_client_servers(&config_path).expect("load config");
        assert_eq!(servers.len(), 2);
        assert!(matches!(
            servers[0].transport,
            McpClientTransportRuntime::Stdio(_)
        ));
        assert!(matches!(
            servers[1].transport,
            McpClientTransportRuntime::HttpSse(_)
        ));
    }

    #[tokio::test]
    async fn functional_mcp_client_stdio_discovery_and_proxy_call_roundtrip() {
        let temp = tempdir().expect("tempdir");
        let script_path = temp.path().join("mock-mcp-client.sh");
        write_stdio_mock_script(&script_path);
        let config_path = temp.path().join("mcp-client.json");
        write_client_config(
            &config_path,
            json!({
                "schema_version": 1,
                "servers": [
                    {
                        "name": "local_stdio",
                        "command": script_path.display().to_string()
                    }
                ]
            }),
        );

        let mut cli = parse_cli_with_stack(
            [
                "tau-rs",
                "--mcp-client",
                "--mcp-external-server-config",
                config_path.to_string_lossy().as_ref(),
            ]
            .as_slice(),
        );
        cli.credential_store = temp.path().join("credentials.json");

        let context = McpClientRuntimeContext::from_cli(&cli);
        let outcome = discover_mcp_client_tools(&cli, &context).expect("discover tools");
        assert_eq!(outcome.tools.len(), 1);
        assert_eq!(outcome.tools[0].local_tool_name, "mcp.local_stdio.echo");

        let tool = McpClientProxyTool {
            definition: ToolDefinition {
                name: outcome.tools[0].local_tool_name.clone(),
                description: outcome.tools[0].description.clone(),
                parameters: outcome.tools[0].input_schema.clone(),
            },
            remote_tool_name: outcome.tools[0].remote_tool_name.clone(),
            server: outcome.tools[0].server.clone(),
            context,
        };
        let result = tool.execute(json!({"value":"hello"})).await;
        assert!(!result.is_error);
        assert_eq!(result.content["ok"], true);
    }

    #[test]
    fn integration_mcp_client_http_sse_discovery_roundtrip() {
        let server = MockServer::start();
        let _sse_mock = server.mock(|when, then| {
            when.method(GET).path("/sse");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body("event: ready\ndata: {}\n\n");
        });
        let _init_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/rpc")
                .body_includes("\"method\":\"initialize\"");
            then.status(200).json_body(json!({
                "jsonrpc": "2.0",
                "id": "tau-client-init",
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {"tools": {"listChanged": false}}
                }
            }));
        });
        let _list_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/rpc")
                .body_includes("\"method\":\"tools/list\"");
            then.status(200).json_body(json!({
                "jsonrpc": "2.0",
                "id": "tau-client-tools-list",
                "result": {
                    "tools": [
                        {
                            "name": "status",
                            "description": "status tool",
                            "inputSchema": {"type":"object"}
                        }
                    ]
                }
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config_path = temp.path().join("mcp-client-http.json");
        write_client_config(
            &config_path,
            json!({
                "schema_version": 1,
                "servers": [
                    {
                        "name": "remote_http",
                        "transport": "http-sse",
                        "endpoint": server.url("/rpc"),
                        "sse_endpoint": server.url("/sse")
                    }
                ]
            }),
        );

        let mut cli = parse_cli_with_stack(
            [
                "tau-rs",
                "--mcp-client",
                "--mcp-external-server-config",
                config_path.to_string_lossy().as_ref(),
            ]
            .as_slice(),
        );
        cli.credential_store = temp.path().join("credentials.json");

        let context = McpClientRuntimeContext::from_cli(&cli);
        let outcome = discover_mcp_client_tools(&cli, &context).expect("discover tools");
        assert_eq!(outcome.tools.len(), 1);
        assert_eq!(outcome.tools[0].local_tool_name, "mcp.remote_http.status");
    }

    #[test]
    fn regression_mcp_client_oauth_pkce_requires_code_verifier_when_auth_code_is_configured() {
        let server = MockServer::start();
        let _init_mock = server.mock(|when, then| {
            when.method(POST).path("/rpc");
            then.status(200).json_body(json!({
                "jsonrpc": "2.0",
                "id": "tau-client-init",
                "result": {"protocolVersion": "2024-11-05", "capabilities": {"tools": {"listChanged": false}}}
            }));
        });
        let temp = tempdir().expect("tempdir");
        let config_path = temp.path().join("mcp-client-oauth.json");
        write_client_config(
            &config_path,
            json!({
                "schema_version": 1,
                "servers": [
                    {
                        "name": "oauth_http",
                        "transport": "http-sse",
                        "endpoint": server.url("/rpc"),
                        "auth": {
                            "type": "oauth_pkce",
                            "authorization_url": server.url("/authorize"),
                            "token_url": server.url("/token"),
                            "client_id": "tau-test",
                            "authorization_code": "auth-code-no-verifier"
                        }
                    }
                ]
            }),
        );

        let mut cli = parse_cli_with_stack(
            [
                "tau-rs",
                "--mcp-client",
                "--mcp-external-server-config",
                config_path.to_string_lossy().as_ref(),
            ]
            .as_slice(),
        );
        cli.credential_store = temp.path().join("credentials.json");

        let context = McpClientRuntimeContext::from_cli(&cli);
        let outcome = discover_mcp_client_tools(&cli, &context).expect("discovery outcome");
        assert!(outcome.tools.is_empty());
        assert!(outcome
            .diagnostics
            .iter()
            .any(|entry| entry.reason_code == "mcp_client_oauth_code_verifier_missing"));
    }

    #[test]
    fn integration_mcp_client_oauth_pkce_token_exchange_persists_credential_store() {
        let server = MockServer::start();
        let _token_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/token")
                .body_includes("grant_type=authorization_code")
                .body_includes("code=demo-code");
            then.status(200).json_body(json!({
                "access_token": "token-abc",
                "refresh_token": "refresh-xyz",
                "expires_in": 3600,
                "token_type": "Bearer"
            }));
        });
        let _sse_mock = server.mock(|when, then| {
            when.method(GET).path("/sse");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body("event: ready\ndata: {}\n\n");
        });
        let _init_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/rpc")
                .header("authorization", "Bearer token-abc")
                .body_includes("\"method\":\"initialize\"");
            then.status(200).json_body(json!({
                "jsonrpc": "2.0",
                "id": "tau-client-init",
                "result": {"protocolVersion": "2024-11-05", "capabilities": {"tools": {"listChanged": false}}}
            }));
        });
        let _list_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/rpc")
                .header("authorization", "Bearer token-abc")
                .body_includes("\"method\":\"tools/list\"");
            then.status(200).json_body(json!({
                "jsonrpc": "2.0",
                "id": "tau-client-tools-list",
                "result": {"tools":[{"name":"status","description":"status","inputSchema":{"type":"object"}}]}
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config_path = temp.path().join("mcp-client-oauth-success.json");
        write_client_config(
            &config_path,
            json!({
                "schema_version": 1,
                "servers": [
                    {
                        "name": "oauth_http",
                        "transport": "http-sse",
                        "endpoint": server.url("/rpc"),
                        "sse_endpoint": server.url("/sse"),
                        "auth": {
                            "type": "oauth_pkce",
                            "authorization_url": server.url("/authorize"),
                            "token_url": server.url("/token"),
                            "client_id": "tau-test",
                            "authorization_code": "demo-code",
                            "code_verifier": "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG"
                        }
                    }
                ]
            }),
        );

        let mut cli = parse_cli_with_stack(
            [
                "tau-rs",
                "--mcp-client",
                "--mcp-external-server-config",
                config_path.to_string_lossy().as_ref(),
            ]
            .as_slice(),
        );
        cli.credential_store = temp.path().join("credentials.json");

        let context = McpClientRuntimeContext::from_cli(&cli);
        let outcome = discover_mcp_client_tools(&cli, &context).expect("discover tools");
        assert_eq!(outcome.tools.len(), 1);

        let key = oauth_store_key("oauth_http");
        let store = load_credential_store(
            &cli.credential_store,
            context.credential_store_encryption,
            context.credential_store_key.as_deref(),
        )
        .expect("load credential store");
        assert!(store.integrations.contains_key(&key));
    }

    #[test]
    fn integration_mcp_client_register_tools_attaches_proxy_tools_to_agent() {
        let temp = tempdir().expect("tempdir");
        let script_path = temp.path().join("mock-mcp-client-register.sh");
        write_stdio_mock_script(&script_path);
        let config_path = temp.path().join("mcp-client-register.json");
        write_client_config(
            &config_path,
            json!({
                "schema_version": 1,
                "servers": [
                    {
                        "name": "local_stdio",
                        "command": script_path.display().to_string()
                    }
                ]
            }),
        );
        let mut cli = parse_cli_with_stack(
            [
                "tau-rs",
                "--mcp-client",
                "--mcp-external-server-config",
                config_path.to_string_lossy().as_ref(),
            ]
            .as_slice(),
        );
        cli.credential_store = temp.path().join("credentials.json");

        let mut agent = Agent::new(Arc::new(NoopClient), tau_agent_core::AgentConfig::default());
        let report = register_mcp_client_tools(&mut agent, &cli).expect("register tools");
        assert_eq!(report.registered_tool_count, 1);
        assert!(agent.has_tool("mcp.local_stdio.echo"));
    }
}
