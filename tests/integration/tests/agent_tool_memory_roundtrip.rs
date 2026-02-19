use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use serde_json::{json, Value};
use tau_agent_core::{Agent, AgentTool};
use tau_ai::{
    ChatRequest, ChatResponse, ChatUsage, ContentBlock, LlmClient, Message, MessageRole, TauAiError,
};
use tau_tools::tools::{MemorySearchTool, MemoryWriteTool, ToolPolicy};
use tokio::sync::Mutex as AsyncMutex;

static WORKSPACE_COUNTER: AtomicU64 = AtomicU64::new(1);

struct ScriptedClient {
    responses: AsyncMutex<VecDeque<ChatResponse>>,
    requests: AsyncMutex<Vec<ChatRequest>>,
}

impl ScriptedClient {
    fn new(responses: Vec<ChatResponse>) -> Self {
        Self {
            responses: AsyncMutex::new(VecDeque::from(responses)),
            requests: AsyncMutex::new(Vec::new()),
        }
    }

    async fn request_count(&self) -> usize {
        self.requests.lock().await.len()
    }
}

#[async_trait]
impl LlmClient for ScriptedClient {
    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, TauAiError> {
        self.requests.lock().await.push(request);
        let mut responses = self.responses.lock().await;
        responses
            .pop_front()
            .ok_or_else(|| TauAiError::InvalidResponse("scripted response queue exhausted".into()))
    }
}

struct IsolatedWorkspace {
    root: PathBuf,
}

impl IsolatedWorkspace {
    fn new(label: &str) -> Self {
        let tick = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        let count = WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "tau-2608-{label}-{}-{tick}-{count}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("must create isolated workspace root");
        Self { root }
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for IsolatedWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn integration_policy(root: &Path) -> Arc<ToolPolicy> {
    let mut policy = ToolPolicy::new(vec![root.to_path_buf()]);
    policy.memory_state_dir = root.join(".tau").join("memory");
    Arc::new(policy)
}

fn scripted_tool_call(id: &str, name: &str, arguments: Value) -> ChatResponse {
    ChatResponse {
        message: Message::assistant_blocks(vec![ContentBlock::ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments,
        }]),
        finish_reason: Some("tool_calls".to_string()),
        usage: ChatUsage::default(),
    }
}

fn scripted_assistant_text(text: &str) -> ChatResponse {
    ChatResponse {
        message: Message::assistant_text(text),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    }
}

fn latest_tool_payload(agent: &Agent, tool_name: &str) -> Value {
    let message = agent
        .messages()
        .iter()
        .rev()
        .find(|message| {
            message.role == MessageRole::Tool
                && !message.is_error
                && message.tool_name.as_deref() == Some(tool_name)
        })
        .unwrap_or_else(|| panic!("missing successful tool payload for {tool_name}"));
    serde_json::from_str(message.text_content().trim())
        .unwrap_or_else(|error| panic!("tool payload should be valid json: {error}"))
}

fn latest_assistant_text(messages: &[Message]) -> String {
    messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::Assistant)
        .map(Message::text_content)
        .unwrap_or_else(|| panic!("missing assistant message in prompt output"))
}

#[tokio::test]
async fn integration_spec_2608_c01_workspace_runs_integration_package() {
    let workspace = IsolatedWorkspace::new("c01-workspace-runs");
    let policy = integration_policy(workspace.root());
    let client = Arc::new(ScriptedClient::new(vec![scripted_assistant_text("ready")]));
    let mut agent = Agent::new(client.clone(), tau_agent_core::AgentConfig::default());
    agent.register_tool(MemoryWriteTool::new(policy.clone()));
    agent.register_tool(MemorySearchTool::new(policy));

    let response = agent
        .prompt("bootstrap smoke")
        .await
        .expect("prompt succeeds");

    assert_eq!(latest_assistant_text(&response), "ready");
    assert_eq!(client.request_count().await, 1);
}

#[tokio::test]
async fn conformance_spec_2608_c02_agent_tool_memory_roundtrip() {
    let workspace = IsolatedWorkspace::new("c02-roundtrip");
    let policy = integration_policy(workspace.root());
    let client = Arc::new(ScriptedClient::new(vec![
        scripted_tool_call(
            "call-write",
            "memory_write",
            json!({
                "memory_id": "memory-2608-roundtrip",
                "summary": "Deployment window approved for 03:00 UTC rollout.",
                "workspace_id": "workspace-2608",
                "channel_id": "release",
                "actor_id": "integration-suite",
                "tags": ["release", "window"]
            }),
        ),
        scripted_tool_call(
            "call-search",
            "memory_search",
            json!({
                "query": "Deployment window approved for 03:00 UTC rollout.",
                "workspace_id": "workspace-2608",
                "channel_id": "release",
                "actor_id": "integration-suite",
                "limit": 5
            }),
        ),
        scripted_assistant_text("roundtrip complete"),
    ]));
    let mut agent = Agent::new(client.clone(), tau_agent_core::AgentConfig::default());
    agent.register_tool(MemoryWriteTool::new(policy.clone()));
    agent.register_tool(MemorySearchTool::new(policy));

    let response = agent
        .prompt("Store then retrieve the release window memory.")
        .await
        .expect("roundtrip prompt should succeed");

    assert_eq!(latest_assistant_text(&response), "roundtrip complete");
    assert_eq!(client.request_count().await, 3);

    let write_payload = latest_tool_payload(&agent, "memory_write");
    assert_eq!(write_payload["memory_id"], "memory-2608-roundtrip");
    assert_eq!(
        write_payload["summary"],
        "Deployment window approved for 03:00 UTC rollout."
    );

    let search_payload = latest_tool_payload(&agent, "memory_search");
    let returned = search_payload["returned"]
        .as_u64()
        .expect("search payload must include returned count");
    assert!(returned >= 1, "search should return at least one memory");

    let matches = search_payload["matches"]
        .as_array()
        .expect("search payload must include matches array");
    assert!(
        matches
            .iter()
            .any(|entry| entry["memory_id"] == "memory-2608-roundtrip"),
        "search matches must contain the just-written memory"
    );
}

#[tokio::test]
async fn regression_spec_2608_c03_harness_uses_isolated_memory_state() {
    let workspace_a = IsolatedWorkspace::new("c03-isolation-a");
    let workspace_b = IsolatedWorkspace::new("c03-isolation-b");
    let policy_a = integration_policy(workspace_a.root());
    let policy_b = integration_policy(workspace_b.root());

    let write_tool = MemoryWriteTool::new(policy_a);
    let write = write_tool
        .execute(json!({
            "memory_id": "memory-isolated-a",
            "summary": "Only stored in workspace A",
            "workspace_id": "workspace-a",
            "channel_id": "qa",
            "actor_id": "integration-suite"
        }))
        .await;
    assert!(!write.is_error, "{}", write.content);

    let search_tool = MemorySearchTool::new(policy_b);
    let search = search_tool
        .execute(json!({
            "query": "Only stored in workspace A",
            "workspace_id": "workspace-a",
            "channel_id": "qa",
            "limit": 5
        }))
        .await;
    assert!(!search.is_error, "{}", search.content);
    assert_eq!(search.content["returned"], 0);
}

#[tokio::test]
async fn functional_spec_2608_c04_pattern_is_composable_for_new_scenarios() {
    let workspace = IsolatedWorkspace::new("c04-composable");
    let policy = integration_policy(workspace.root());
    let client = Arc::new(ScriptedClient::new(vec![
        scripted_tool_call(
            "call-write-second",
            "memory_write",
            json!({
                "memory_id": "memory-2608-composable",
                "summary": "Follow-up scenario using the same harness pattern.",
                "workspace_id": "workspace-2608",
                "channel_id": "qa",
                "actor_id": "integration-suite"
            }),
        ),
        scripted_assistant_text("second scenario complete"),
    ]));
    let mut agent = Agent::new(client.clone(), tau_agent_core::AgentConfig::default());
    agent.register_tool(MemoryWriteTool::new(policy));

    let response = agent
        .prompt("Run a second scripted integration scenario.")
        .await
        .expect("second scenario must succeed");

    assert_eq!(latest_assistant_text(&response), "second scenario complete");
    assert_eq!(client.request_count().await, 2);
    let write_payload = latest_tool_payload(&agent, "memory_write");
    assert_eq!(write_payload["memory_id"], "memory-2608-composable");
}
