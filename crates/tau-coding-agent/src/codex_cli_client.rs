use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use tau_ai::{
    ChatRequest, ChatResponse, ChatUsage, ContentBlock, LlmClient, Message, MessageRole,
    StreamDeltaHandler, TauAiError,
};

const DEFAULT_EXEC_ARGS: &[&str] = &[
    "exec",
    "--full-auto",
    "--skip-git-repo-check",
    "--color",
    "never",
];

static OUTPUT_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodexCliConfig {
    pub(crate) executable: String,
    pub(crate) extra_args: Vec<String>,
    pub(crate) timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodexCliClient {
    config: CodexCliConfig,
}

impl CodexCliClient {
    pub(crate) fn new(config: CodexCliConfig) -> Result<Self, TauAiError> {
        if config.executable.trim().is_empty() {
            return Err(TauAiError::InvalidResponse(
                "codex cli executable is empty".to_string(),
            ));
        }
        if config.timeout_ms == 0 {
            return Err(TauAiError::InvalidResponse(
                "codex cli timeout must be greater than 0ms".to_string(),
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
                    "failed to spawn codex cli '{executable}': {error}"
                )));
            }
        }
    }

    Err(TauAiError::InvalidResponse(format!(
        "failed to spawn codex cli '{executable}': unknown error"
    )))
}

#[async_trait]
impl LlmClient for CodexCliClient {
    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, TauAiError> {
        let output_file = build_output_file_path();
        let mut command = Command::new(&self.config.executable);
        command.kill_on_drop(true);
        command.args(DEFAULT_EXEC_ARGS);
        command.arg("--model");
        command.arg(&request.model);
        command.args(&self.config.extra_args);
        command.arg("--output-last-message");
        command.arg(&output_file);
        command.arg("-");
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let prompt = render_codex_exec_prompt(&request);
        let mut child =
            spawn_with_text_file_busy_retry(&mut command, &self.config.executable).await?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(prompt.as_bytes()).await.map_err(|error| {
                TauAiError::InvalidResponse(format!("failed to write prompt to codex cli: {error}"))
            })?;
        }

        let output = tokio::time::timeout(
            Duration::from_millis(self.config.timeout_ms),
            child.wait_with_output(),
        )
        .await
        .map_err(|_| {
            TauAiError::InvalidResponse(format!(
                "codex cli timed out after {}ms",
                self.config.timeout_ms
            ))
        })?
        .map_err(|error| {
            TauAiError::InvalidResponse(format!("codex cli process failed: {error}"))
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
                "codex cli failed with status {status}: {summary}"
            )));
        }

        let message_text = read_assistant_text(&output_file, &stdout).await;
        if message_text.trim().is_empty() {
            return Err(TauAiError::InvalidResponse(
                "codex cli returned empty assistant output".to_string(),
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

fn build_output_file_path() -> PathBuf {
    let seq = OUTPUT_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_nanos();
    let mut path = std::env::temp_dir();
    path.push(format!(
        "tau-codex-last-message-{}-{now_nanos}-{seq}.txt",
        std::process::id()
    ));
    path
}

async fn read_assistant_text(output_file: &Path, stdout: &str) -> String {
    let from_file = tokio::fs::read_to_string(output_file)
        .await
        .unwrap_or_default();
    let _ = tokio::fs::remove_file(output_file).await;

    if !from_file.trim().is_empty() {
        return from_file.trim().to_string();
    }
    stdout.trim().to_string()
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

fn render_codex_exec_prompt(request: &ChatRequest) -> String {
    let mut lines = vec![
        "You are the OpenAI-compatible Tau backend.".to_string(),
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
    use std::sync::{Arc, Mutex};

    use tau_ai::ToolDefinition;
    use tempfile::tempdir;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn test_request() -> ChatRequest {
        ChatRequest {
            model: "openai/gpt-4o-mini".to_string(),
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
        let script = dir.join("mock-codex.sh");
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
    async fn integration_codex_cli_client_reads_output_last_message_file() {
        let dir = tempdir().expect("tempdir");
        let script = write_script(
            dir.path(),
            r#"
out=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output-last-message) out="$2"; shift 2;;
    *) shift;;
  esac
done
cat >/dev/null
printf "mock codex reply" > "$out"
"#,
        );
        let client = CodexCliClient::new(CodexCliConfig {
            executable: script.display().to_string(),
            extra_args: vec![],
            timeout_ms: 5_000,
        })
        .expect("client");

        let response = client.complete(test_request()).await.expect("complete");
        assert_eq!(response.message.text_content(), "mock codex reply");
        assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn functional_codex_cli_client_falls_back_to_stdout() {
        let dir = tempdir().expect("tempdir");
        let script = write_script(
            dir.path(),
            r#"
cat >/dev/null
printf "stdout fallback reply"
"#,
        );
        let client = CodexCliClient::new(CodexCliConfig {
            executable: script.display().to_string(),
            extra_args: vec![],
            timeout_ms: 5_000,
        })
        .expect("client");

        let response = client.complete(test_request()).await.expect("complete");
        assert_eq!(response.message.text_content(), "stdout fallback reply");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn regression_codex_cli_client_reports_non_zero_exit() {
        let dir = tempdir().expect("tempdir");
        let script = write_script(
            dir.path(),
            r#"
cat >/dev/null
echo "failed request" 1>&2
exit 17
"#,
        );
        let client = CodexCliClient::new(CodexCliConfig {
            executable: script.display().to_string(),
            extra_args: vec![],
            timeout_ms: 5_000,
        })
        .expect("client");

        let error = client
            .complete(test_request())
            .await
            .expect_err("must fail");
        assert!(error.to_string().contains("status 17"));
        assert!(error.to_string().contains("failed request"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn integration_codex_cli_client_stream_callback_receives_text() {
        let dir = tempdir().expect("tempdir");
        let script = write_script(
            dir.path(),
            r#"
out=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output-last-message) out="$2"; shift 2;;
    *) shift;;
  esac
done
cat >/dev/null
printf "streamed reply" > "$out"
"#,
        );
        let client = CodexCliClient::new(CodexCliConfig {
            executable: script.display().to_string(),
            extra_args: vec![],
            timeout_ms: 5_000,
        })
        .expect("client");
        let deltas = Arc::new(Mutex::new(Vec::new()));
        let sink_deltas = deltas.clone();
        let sink: StreamDeltaHandler = Arc::new(move |delta: String| {
            sink_deltas.lock().expect("delta lock").push(delta);
        });

        let response = client
            .complete_with_stream(test_request(), Some(sink))
            .await
            .expect("complete");
        assert_eq!(response.message.text_content(), "streamed reply");
        let deltas = deltas.lock().expect("delta lock");
        assert_eq!(deltas.as_slice(), ["streamed reply"]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn regression_codex_cli_client_reports_timeout() {
        let dir = tempdir().expect("tempdir");
        let script = write_script(
            dir.path(),
            r#"
cat >/dev/null
sleep 2
echo "too late"
"#,
        );
        let client = CodexCliClient::new(CodexCliConfig {
            executable: script.display().to_string(),
            extra_args: vec![],
            timeout_ms: 20,
        })
        .expect("client");

        let error = client
            .complete(test_request())
            .await
            .expect_err("must timeout");
        assert!(error.to_string().contains("timed out"));
    }

    #[test]
    fn unit_render_prompt_contains_context_sections() {
        let prompt = render_codex_exec_prompt(&test_request());
        assert!(prompt.contains("Conversation:"));
        assert!(prompt.contains("[system]"));
        assert!(prompt.contains("[user]"));
        assert!(prompt.contains("Tau tools available in caller runtime"));
        assert!(prompt.contains("- read: Read a file"));
    }
}
