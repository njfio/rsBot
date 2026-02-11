use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;

use tau_ai::{
    ChatRequest, ChatResponse, ChatUsage, ContentBlock, LlmClient, Message, MessageRole,
    StreamDeltaHandler, TauAiError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeCliConfig {
    pub executable: String,
    pub extra_args: Vec<String>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeCliClient {
    config: ClaudeCliConfig,
}

impl ClaudeCliClient {
    pub fn new(config: ClaudeCliConfig) -> Result<Self, TauAiError> {
        if config.executable.trim().is_empty() {
            return Err(TauAiError::InvalidResponse(
                "claude cli executable is empty".to_string(),
            ));
        }
        if config.timeout_ms == 0 {
            return Err(TauAiError::InvalidResponse(
                "claude cli timeout must be greater than 0ms".to_string(),
            ));
        }
        Ok(Self { config })
    }
}

async fn spawn_with_text_file_busy_retry(
    command: &mut Command,
    executable: &str,
) -> Result<tokio::process::Child, TauAiError> {
    const MAX_TEXT_FILE_BUSY_RETRIES: u32 = 5;
    const TEXT_FILE_BUSY_ERRNO: i32 = 26;
    for attempt in 0..=MAX_TEXT_FILE_BUSY_RETRIES {
        match command.spawn() {
            Ok(child) => return Ok(child),
            Err(error) => {
                if error.raw_os_error() == Some(TEXT_FILE_BUSY_ERRNO)
                    && attempt < MAX_TEXT_FILE_BUSY_RETRIES
                {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                    continue;
                }
                return Err(TauAiError::InvalidResponse(format!(
                    "failed to spawn claude cli '{executable}': {error}"
                )));
            }
        }
    }

    Err(TauAiError::InvalidResponse(format!(
        "failed to spawn claude cli '{executable}': unknown error"
    )))
}

#[async_trait]
impl LlmClient for ClaudeCliClient {
    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, TauAiError> {
        let prompt = render_claude_prompt(&request);
        let mut command = Command::new(&self.config.executable);
        command.kill_on_drop(true);
        command.arg("-p");
        command.arg(prompt);
        command.arg("--output-format");
        command.arg("json");
        command.arg("--model");
        command.arg(&request.model);
        command.args(&self.config.extra_args);
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        let child = spawn_with_text_file_busy_retry(&mut command, &self.config.executable).await?;

        let output = tokio::time::timeout(
            Duration::from_millis(self.config.timeout_ms),
            child.wait_with_output(),
        )
        .await
        .map_err(|_| {
            TauAiError::InvalidResponse(format!(
                "claude cli timed out after {}ms",
                self.config.timeout_ms
            ))
        })?
        .map_err(|error| {
            TauAiError::InvalidResponse(format!("claude cli process failed: {error}"))
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if !output.status.success() {
            let status = output
                .status
                .code()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "signal".to_string());
            let summary = summarize_process_failure(&stderr, &stdout);
            return Err(TauAiError::InvalidResponse(format!(
                "claude cli failed with status {status}: {summary}"
            )));
        }

        let message_text = extract_assistant_text(&stdout)?;
        if message_text.trim().is_empty() {
            return Err(TauAiError::InvalidResponse(
                "claude cli returned empty assistant output".to_string(),
            ));
        }

        Ok(ChatResponse {
            message: Message::assistant_text(message_text),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        })
    }

    async fn complete_with_stream(
        &self,
        request: ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<ChatResponse, TauAiError> {
        let response = self.complete(request).await?;
        if let Some(handler) = on_delta {
            let text = response.message.text_content();
            if !text.trim().is_empty() {
                handler(text);
            }
        }
        Ok(response)
    }
}

fn extract_assistant_text(stdout: &str) -> Result<String, TauAiError> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if let Some(error_message) = extract_error_message(&value) {
            return Err(TauAiError::InvalidResponse(format!(
                "claude cli returned an error payload: {error_message}"
            )));
        }
        if let Some(result) = extract_result_message(&value) {
            return Ok(result);
        }
    }
    Ok(trimmed.to_string())
}

fn extract_error_message(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            if map
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return map
                    .get("result")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|message| !message.is_empty())
                    .map(str::to_string)
                    .or_else(|| {
                        map.get("error")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|message| !message.is_empty())
                            .map(str::to_string)
                    })
                    .or_else(|| {
                        map.get("message")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|message| !message.is_empty())
                            .map(str::to_string)
                    })
                    .or(Some("claude cli reported an error".to_string()));
            }
            None
        }
        Value::Array(entries) => entries.iter().find_map(extract_error_message),
        _ => None,
    }
}

fn extract_result_message(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => map
            .get("result")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|result| !result.is_empty())
            .map(str::to_string),
        Value::Array(entries) => entries.iter().rev().find_map(extract_result_message),
        _ => None,
    }
}

fn summarize_process_failure(stderr: &str, stdout: &str) -> String {
    let stderr = stderr.trim();
    if !stderr.is_empty() {
        return truncate_for_log(stderr);
    }

    let stdout = stdout.trim();
    if !stdout.is_empty() {
        return truncate_for_log(stdout);
    }

    "no error output".to_string()
}

fn truncate_for_log(text: &str) -> String {
    const MAX_CHARS: usize = 240;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }
    text.chars().take(MAX_CHARS).collect::<String>() + "..."
}

fn render_claude_prompt(request: &ChatRequest) -> String {
    let mut lines = vec![
        "You are the Anthropic Claude Code-compatible Tau backend.".to_string(),
        "Respond with the assistant's next message for the conversation below.".to_string(),
        "Return plain assistant text only.".to_string(),
        "Conversation:".to_string(),
    ];

    for message in &request.messages {
        lines.push(format!("[{}]", role_label(message.role)));
        if let Some(tool_name) = &message.tool_name {
            lines.push(format!("tool_name={tool_name}"));
        }
        if let Some(tool_call_id) = &message.tool_call_id {
            lines.push(format!("tool_call_id={tool_call_id}"));
        }
        if message.is_error {
            lines.push("tool_error=true".to_string());
        }
        for block in &message.content {
            match block {
                ContentBlock::Text { text } => lines.push(text.clone()),
                ContentBlock::ToolCall {
                    id,
                    name,
                    arguments,
                } => lines.push(format!(
                    "{{\"tool_call\":{{\"id\":\"{id}\",\"name\":\"{name}\",\"arguments\":{arguments}}}}}"
                )),
            }
        }
    }

    if !request.tools.is_empty() {
        lines.push("Tau tools available in caller runtime (context only):".to_string());
        for tool in &request.tools {
            lines.push(format!("- {}: {}", tool.name, tool.description));
        }
    }

    lines.join("\n")
}

fn role_label(role: MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use tau_ai::ToolDefinition;
    use tempfile::tempdir;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn test_request() -> ChatRequest {
        ChatRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![
                Message::system("system message"),
                Message::user("hello"),
                Message::assistant_text("intermediate"),
            ],
            tools: vec![ToolDefinition {
                name: "read".to_string(),
                description: "Read a file".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"}
                    },
                    "required": ["path"]
                }),
            }],
            max_tokens: None,
            temperature: None,
        }
    }

    #[cfg(unix)]
    fn write_script(dir: &Path, body: &str) -> PathBuf {
        let script = dir.join("mock-claude.sh");
        let content = format!("#!/bin/sh\nset -eu\n{body}\n");
        std::fs::write(&script, content).expect("write script");
        let mut perms = std::fs::metadata(&script)
            .expect("script metadata")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).expect("chmod script");
        script
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn integration_claude_cli_client_reads_json_result_field() {
        let dir = tempdir().expect("tempdir");
        let script = write_script(
            dir.path(),
            r#"
if [ "$1" != "-p" ]; then
  echo "expected -p argument" >&2
  exit 11
fi
shift 2
fmt=""
model=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output-format) shift; fmt="$1";;
    --model) shift; model="$1";;
  esac
  shift
done
if [ "$fmt" != "json" ]; then
  echo "expected json output format" >&2
  exit 12
fi
if [ "$model" != "claude-sonnet-4-20250514" ]; then
  echo "expected model argument" >&2
  exit 13
fi
printf '{"type":"result","subtype":"success","is_error":false,"result":"claude mock reply"}'
"#,
        );
        let client = ClaudeCliClient::new(ClaudeCliConfig {
            executable: script.display().to_string(),
            extra_args: vec![],
            timeout_ms: 6_000,
        })
        .expect("build client");

        let response = client.complete(test_request()).await.expect("completion");
        assert_eq!(response.message.text_content(), "claude mock reply");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn functional_claude_cli_client_falls_back_to_plain_stdout() {
        let dir = tempdir().expect("tempdir");
        let script = write_script(dir.path(), r#"printf "plain claude stdout""#);
        let client = ClaudeCliClient::new(ClaudeCliConfig {
            executable: script.display().to_string(),
            extra_args: vec![],
            timeout_ms: 30_000,
        })
        .expect("build client");

        let response = client.complete(test_request()).await.expect("completion");
        assert_eq!(response.message.text_content(), "plain claude stdout");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn regression_claude_cli_client_reports_non_zero_exit() {
        let dir = tempdir().expect("tempdir");
        let script = write_script(
            dir.path(),
            r#"
echo "claude auth failed" >&2
exit 42
"#,
        );
        let client = ClaudeCliClient::new(ClaudeCliConfig {
            executable: script.display().to_string(),
            extra_args: vec![],
            timeout_ms: 30_000,
        })
        .expect("build client");

        let error = client
            .complete(test_request())
            .await
            .expect_err("expected failure");
        assert!(error.to_string().contains("status 42"));
        assert!(error.to_string().contains("claude auth failed"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn integration_claude_cli_client_stream_callback_receives_text() {
        let dir = tempdir().expect("tempdir");
        let script = write_script(
            dir.path(),
            r#"printf '{"type":"result","subtype":"success","is_error":false,"result":"stream payload"}'"#,
        );
        let client = ClaudeCliClient::new(ClaudeCliConfig {
            executable: script.display().to_string(),
            extra_args: vec![],
            timeout_ms: 30_000,
        })
        .expect("build client");

        let chunks = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&chunks);
        let stream_sink: StreamDeltaHandler = Arc::new(move |delta: String| {
            sink.lock().expect("delta lock").push(delta);
        });
        let response = client
            .complete_with_stream(test_request(), Some(stream_sink))
            .await
            .expect("stream completion");
        assert_eq!(response.message.text_content(), "stream payload");

        let captured = chunks.lock().expect("chunks lock");
        assert_eq!(captured.as_slice(), ["stream payload"]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn regression_claude_cli_client_reports_timeout() {
        let dir = tempdir().expect("tempdir");
        let script = write_script(
            dir.path(),
            r#"
sleep 1
printf '{"type":"result","subtype":"success","is_error":false,"result":"late"}'
"#,
        );
        let client = ClaudeCliClient::new(ClaudeCliConfig {
            executable: script.display().to_string(),
            extra_args: vec![],
            timeout_ms: 50,
        })
        .expect("build client");

        let error = client
            .complete(test_request())
            .await
            .expect_err("timeout should fail");
        let message = error.to_string();
        let is_timeout_shape = message.contains("timed out")
            || message.contains("timeout")
            || message.contains("elapsed");
        assert!(
            is_timeout_shape,
            "expected timeout-like error, got: {message}"
        );
    }

    #[test]
    fn unit_render_claude_prompt_includes_roles_and_tools() {
        let prompt = render_claude_prompt(&test_request());
        assert!(prompt.contains("Anthropic Claude Code-compatible Tau backend"));
        assert!(prompt.contains("[system]"));
        assert!(prompt.contains("[user]"));
        assert!(prompt.contains("[assistant]"));
        assert!(prompt.contains("Tau tools available in caller runtime"));
        assert!(prompt.contains("- read: Read a file"));
    }

    #[test]
    fn unit_extract_assistant_text_prefers_result_field() {
        let text =
            extract_assistant_text("{\"type\":\"result\",\"is_error\":false,\"result\":\"hello\"}")
                .expect("extract");
        assert_eq!(text, "hello");
    }

    #[test]
    fn regression_extract_assistant_text_reports_error_payload() {
        let error =
            extract_assistant_text("{\"type\":\"result\",\"is_error\":true,\"result\":\"denied\"}")
                .expect_err("error payload should fail");
        assert!(error.to_string().contains("denied"));
    }
}
