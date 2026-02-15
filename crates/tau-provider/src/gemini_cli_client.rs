//! Gemini CLI-backed `LlmClient` implementation.
//!
//! This adapter runs Gemini CLI commands, converts outputs to Tau chat
//! responses, and emits structured failures for timeout/parse/subprocess errors.
//! It provides auth-mode parity with other provider CLI adapters.

use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;

use tau_ai::{
    ChatRequest, ChatResponse, ChatUsage, ContentBlock, LlmClient, MediaSource, Message,
    MessageRole, StreamDeltaHandler, TauAiError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `GeminiCliConfig` used across Tau components.
pub struct GeminiCliConfig {
    pub executable: String,
    pub extra_args: Vec<String>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `GeminiCliClient` used across Tau components.
pub struct GeminiCliClient {
    config: GeminiCliConfig,
}

impl GeminiCliClient {
    pub fn new(config: GeminiCliConfig) -> Result<Self, TauAiError> {
        if config.executable.trim().is_empty() {
            return Err(TauAiError::InvalidResponse(
                "gemini cli executable is empty".to_string(),
            ));
        }
        if config.timeout_ms == 0 {
            return Err(TauAiError::InvalidResponse(
                "gemini cli timeout must be greater than 0ms".to_string(),
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
                    "failed to spawn gemini cli '{executable}': {error}"
                )));
            }
        }
    }

    Err(TauAiError::InvalidResponse(format!(
        "failed to spawn gemini cli '{executable}': unknown error"
    )))
}

#[async_trait]
impl LlmClient for GeminiCliClient {
    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, TauAiError> {
        let prompt = render_gemini_prompt(&request);
        let mut command = Command::new(&self.config.executable);
        command.kill_on_drop(true);
        command.arg("-p");
        command.arg(prompt);
        command.arg("--output-format");
        command.arg("json");
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
                "gemini cli timed out after {}ms",
                self.config.timeout_ms
            ))
        })?
        .map_err(|error| {
            TauAiError::InvalidResponse(format!("gemini cli process failed: {error}"))
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
                "gemini cli failed with status {status}: {summary}"
            )));
        }

        let message_text = extract_assistant_text(&stdout)?;
        if message_text.trim().is_empty() {
            return Err(TauAiError::InvalidResponse(
                "gemini cli returned empty assistant output".to_string(),
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
    let parsed = serde_json::from_str::<Value>(trimmed);
    if let Ok(value) = parsed {
        if let Some(error_message) = value
            .get("error")
            .and_then(Value::as_object)
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|message| !message.is_empty())
        {
            return Err(TauAiError::InvalidResponse(format!(
                "gemini cli returned an error payload: {error_message}"
            )));
        }
        if let Some(response) = value
            .get("response")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|response| !response.is_empty())
        {
            return Ok(response.to_string());
        }
    }
    Ok(trimmed.to_string())
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

fn render_gemini_prompt(request: &ChatRequest) -> String {
    let mut lines = vec![
        "You are the Google Gemini-compatible Tau backend.".to_string(),
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
                ContentBlock::Image { source } => {
                    lines.push(format!("[tau-image:{}]", media_source_descriptor(source)))
                }
                ContentBlock::Audio { source } => {
                    lines.push(format!("[tau-audio:{}]", media_source_descriptor(source)))
                }
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

fn media_source_descriptor(source: &MediaSource) -> String {
    match source {
        MediaSource::Url { url } => format!("url:{url}"),
        MediaSource::Base64 { mime_type, data } => {
            format!("base64:{mime_type}:{}bytes", data.len())
        }
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
            model: "google/gemini-2.5-pro".to_string(),
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
            tool_choice: Some(tau_ai::ToolChoice::Auto),
            json_mode: false,
            max_tokens: None,
            temperature: None,
        }
    }

    #[cfg(unix)]
    fn write_script(dir: &Path, body: &str) -> PathBuf {
        let script = dir.join("mock-gemini.sh");
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
    async fn integration_gemini_cli_client_reads_json_response_field() {
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
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--output-format" ]; then
    shift
    fmt="$1"
  fi
  shift
done
if [ "$fmt" != "json" ]; then
  echo "expected json output format" >&2
  exit 12
fi
printf '{"response":"gemini mock reply"}'
"#,
        );
        let client = GeminiCliClient::new(GeminiCliConfig {
            executable: script.display().to_string(),
            extra_args: vec![],
            timeout_ms: 6_000,
        })
        .expect("build client");

        let response = client.complete(test_request()).await.expect("completion");
        assert_eq!(response.message.text_content(), "gemini mock reply");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn functional_gemini_cli_client_falls_back_to_plain_stdout() {
        let dir = tempdir().expect("tempdir");
        let script = write_script(dir.path(), r#"printf "plain gemini stdout""#);
        let client = GeminiCliClient::new(GeminiCliConfig {
            executable: script.display().to_string(),
            extra_args: vec![],
            timeout_ms: 30_000,
        })
        .expect("build client");

        let response = client.complete(test_request()).await.expect("completion");
        assert_eq!(response.message.text_content(), "plain gemini stdout");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn regression_gemini_cli_client_reports_non_zero_exit() {
        let dir = tempdir().expect("tempdir");
        let script = write_script(
            dir.path(),
            r#"
echo "fatal auth failure" >&2
exit 42
"#,
        );
        let client = GeminiCliClient::new(GeminiCliConfig {
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
        assert!(error.to_string().contains("fatal auth failure"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn integration_gemini_cli_client_stream_callback_receives_text() {
        let dir = tempdir().expect("tempdir");
        let script = write_script(dir.path(), r#"printf '{"response":"stream payload"}'"#);
        let client = GeminiCliClient::new(GeminiCliConfig {
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
    async fn regression_gemini_cli_client_reports_timeout() {
        let dir = tempdir().expect("tempdir");
        let script = write_script(
            dir.path(),
            r#"
sleep 1
printf '{"response":"late"}'
"#,
        );
        let client = GeminiCliClient::new(GeminiCliConfig {
            executable: script.display().to_string(),
            extra_args: vec![],
            timeout_ms: 50,
        })
        .expect("build client");

        let error = client
            .complete(test_request())
            .await
            .expect_err("timeout should fail");
        assert!(error.to_string().contains("timed out"));
    }

    #[test]
    fn unit_render_gemini_prompt_includes_roles_and_tools() {
        let prompt = render_gemini_prompt(&test_request());
        assert!(prompt.contains("Google Gemini-compatible Tau backend"));
        assert!(prompt.contains("[system]"));
        assert!(prompt.contains("[user]"));
        assert!(prompt.contains("[assistant]"));
        assert!(prompt.contains("Tau tools available in caller runtime"));
        assert!(prompt.contains("- read: Read a file"));
    }

    #[test]
    fn regression_render_gemini_prompt_includes_media_markers() {
        let mut request = test_request();
        request.messages.push(Message {
            role: MessageRole::User,
            content: vec![
                ContentBlock::image_url("https://example.com/cat.png"),
                ContentBlock::audio_base64("audio/wav", "YXVkaW8="),
            ],
            tool_call_id: None,
            tool_name: None,
            is_error: false,
        });

        let prompt = render_gemini_prompt(&request);
        assert!(prompt.contains("[tau-image:"));
        assert!(prompt.contains("[tau-audio:"));
    }

    #[test]
    fn unit_extract_assistant_text_prefers_response_field() {
        let text = extract_assistant_text("{\"response\":\"hello\"}").expect("extract");
        assert_eq!(text, "hello");
    }

    #[test]
    fn regression_extract_assistant_text_reports_error_payload() {
        let error = extract_assistant_text("{\"error\":{\"message\":\"denied\"}}")
            .expect_err("error payload should fail");
        assert!(error.to_string().contains("denied"));
    }
}
