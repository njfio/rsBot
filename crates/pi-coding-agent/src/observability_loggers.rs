use super::*;

#[derive(Clone)]
pub(crate) struct ToolAuditLogger {
    path: PathBuf,
    file: Arc<Mutex<std::fs::File>>,
    starts: Arc<Mutex<HashMap<String, Instant>>>,
}

impl ToolAuditLogger {
    pub(crate) fn open(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "failed to create tool audit log directory {}",
                        parent.display()
                    )
                })?;
            }
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open tool audit log {}", path.display()))?;
        Ok(Self {
            path,
            file: Arc::new(Mutex::new(file)),
            starts: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub(crate) fn log_event(&self, event: &AgentEvent) -> Result<()> {
        let payload = {
            let mut starts = self
                .starts
                .lock()
                .map_err(|_| anyhow!("tool audit state lock is poisoned"))?;
            tool_audit_event_json(event, &mut starts)
        };

        let Some(payload) = payload else {
            return Ok(());
        };
        let line = serde_json::to_string(&payload).context("failed to encode tool audit event")?;
        let mut file = self
            .file
            .lock()
            .map_err(|_| anyhow!("tool audit file lock is poisoned"))?;
        writeln!(file, "{line}")
            .with_context(|| format!("failed to write tool audit log {}", self.path.display()))?;
        file.flush()
            .with_context(|| format!("failed to flush tool audit log {}", self.path.display()))?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct PromptTelemetryState {
    next_prompt_id: u64,
    active: Option<PromptTelemetryRunState>,
}

#[derive(Debug)]
struct PromptTelemetryRunState {
    prompt_id: u64,
    started_unix_ms: u64,
    started: Instant,
    turn_count: u64,
    request_duration_ms_total: u64,
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    tool_calls: u64,
    tool_errors: u64,
    finish_reason: Option<String>,
}

#[derive(Clone)]
pub(crate) struct PromptTelemetryLogger {
    path: PathBuf,
    provider: String,
    model: String,
    file: Arc<Mutex<std::fs::File>>,
    state: Arc<Mutex<PromptTelemetryState>>,
}

impl PromptTelemetryLogger {
    pub(crate) fn open(path: PathBuf, provider: &str, model: &str) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "failed to create telemetry log directory {}",
                        parent.display()
                    )
                })?;
            }
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open telemetry log {}", path.display()))?;
        Ok(Self {
            path,
            provider: provider.to_string(),
            model: model.to_string(),
            file: Arc::new(Mutex::new(file)),
            state: Arc::new(Mutex::new(PromptTelemetryState::default())),
        })
    }

    fn build_record(
        &self,
        active: PromptTelemetryRunState,
        status: &'static str,
        success: bool,
    ) -> Value {
        serde_json::json!({
            "record_type": "prompt_telemetry_v1",
            "schema_version": 1,
            "timestamp_unix_ms": current_unix_timestamp_ms(),
            "prompt_id": active.prompt_id,
            "provider": self.provider,
            "model": self.model,
            "status": status,
            "success": success,
            "started_unix_ms": active.started_unix_ms,
            "duration_ms": active.started.elapsed().as_millis() as u64,
            "turn_count": active.turn_count,
            "request_duration_ms_total": active.request_duration_ms_total,
            "finish_reason": active.finish_reason,
            "token_usage": {
                "input_tokens": active.input_tokens,
                "output_tokens": active.output_tokens,
                "total_tokens": active.total_tokens,
            },
            "tool_calls": active.tool_calls,
            "tool_errors": active.tool_errors,
            "redaction_policy": {
                "prompt_content": "omitted",
                "tool_arguments": "omitted",
                "tool_results": "bytes_only",
            }
        })
    }

    pub(crate) fn log_event(&self, event: &AgentEvent) -> Result<()> {
        let mut records = Vec::new();
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow!("telemetry state lock is poisoned"))?;
            match event {
                AgentEvent::AgentStart => {
                    if let Some(active) = state.active.take() {
                        records.push(self.build_record(active, "interrupted", false));
                    }
                    state.next_prompt_id = state.next_prompt_id.saturating_add(1);
                    let prompt_id = state.next_prompt_id;
                    state.active = Some(PromptTelemetryRunState {
                        prompt_id,
                        started_unix_ms: current_unix_timestamp_ms(),
                        started: Instant::now(),
                        turn_count: 0,
                        request_duration_ms_total: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                        total_tokens: 0,
                        tool_calls: 0,
                        tool_errors: 0,
                        finish_reason: None,
                    });
                }
                AgentEvent::TurnEnd {
                    request_duration_ms,
                    usage,
                    finish_reason,
                    ..
                } => {
                    if let Some(active) = state.active.as_mut() {
                        active.turn_count = active.turn_count.saturating_add(1);
                        active.request_duration_ms_total = active
                            .request_duration_ms_total
                            .saturating_add(*request_duration_ms);
                        active.input_tokens =
                            active.input_tokens.saturating_add(usage.input_tokens);
                        active.output_tokens =
                            active.output_tokens.saturating_add(usage.output_tokens);
                        active.total_tokens =
                            active.total_tokens.saturating_add(usage.total_tokens);
                        active.finish_reason = finish_reason.clone();
                    }
                }
                AgentEvent::ToolExecutionEnd { result, .. } => {
                    if let Some(active) = state.active.as_mut() {
                        active.tool_calls = active.tool_calls.saturating_add(1);
                        if result.is_error {
                            active.tool_errors = active.tool_errors.saturating_add(1);
                        }
                    }
                }
                AgentEvent::AgentEnd { .. } => {
                    if let Some(active) = state.active.take() {
                        let success = active.tool_errors == 0;
                        let status = if success {
                            "completed"
                        } else {
                            "completed_with_tool_errors"
                        };
                        records.push(self.build_record(active, status, success));
                    }
                }
                _ => {}
            }
        }

        if records.is_empty() {
            return Ok(());
        }
        let mut file = self
            .file
            .lock()
            .map_err(|_| anyhow!("telemetry file lock is poisoned"))?;
        for record in records {
            let line =
                serde_json::to_string(&record).context("failed to encode telemetry event")?;
            writeln!(file, "{line}").with_context(|| {
                format!("failed to write telemetry log {}", self.path.display())
            })?;
        }
        file.flush()
            .with_context(|| format!("failed to flush telemetry log {}", self.path.display()))?;
        Ok(())
    }
}

pub(crate) fn tool_audit_event_json(
    event: &AgentEvent,
    starts: &mut HashMap<String, Instant>,
) -> Option<serde_json::Value> {
    match event {
        AgentEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            arguments,
        } => {
            starts.insert(tool_call_id.clone(), Instant::now());
            Some(serde_json::json!({
                "timestamp_unix_ms": current_unix_timestamp_ms(),
                "event": "tool_execution_start",
                "tool_call_id": tool_call_id,
                "tool_name": tool_name,
                "arguments_bytes": arguments.to_string().len(),
            }))
        }
        AgentEvent::ToolExecutionEnd {
            tool_call_id,
            tool_name,
            result,
        } => {
            let duration_ms = starts
                .remove(tool_call_id)
                .map(|started| started.elapsed().as_millis() as u64);
            Some(serde_json::json!({
                "timestamp_unix_ms": current_unix_timestamp_ms(),
                "event": "tool_execution_end",
                "tool_call_id": tool_call_id,
                "tool_name": tool_name,
                "duration_ms": duration_ms,
                "is_error": result.is_error,
                "result_bytes": result.as_text().len(),
            }))
        }
        _ => None,
    }
}
