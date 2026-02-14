use std::{
    collections::HashMap,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use tau_agent_core::AgentEvent;

fn current_unix_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Clone)]
/// Public struct `ToolAuditLogger` used across Tau components.
pub struct ToolAuditLogger {
    path: PathBuf,
    file: Arc<Mutex<std::fs::File>>,
    starts: Arc<Mutex<HashMap<String, Instant>>>,
}

impl ToolAuditLogger {
    pub fn open(path: PathBuf) -> Result<Self> {
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

    pub fn log_event(&self, event: &AgentEvent) -> Result<()> {
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
    estimated_cost_usd: f64,
    cost_budget_usd: Option<f64>,
    budget_alerts: u64,
    tool_calls: u64,
    tool_errors: u64,
    secret_leak_detections: u64,
    secret_leak_pattern_class_counts: HashMap<String, u64>,
    finish_reason: Option<String>,
}

#[derive(Clone)]
/// Public struct `PromptTelemetryLogger` used across Tau components.
pub struct PromptTelemetryLogger {
    path: PathBuf,
    provider: String,
    model: String,
    file: Arc<Mutex<std::fs::File>>,
    state: Arc<Mutex<PromptTelemetryState>>,
}

impl PromptTelemetryLogger {
    pub fn open(path: PathBuf, provider: &str, model: &str) -> Result<Self> {
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
            "cost": {
                "estimated_usd": active.estimated_cost_usd,
                "budget_usd": active.cost_budget_usd,
                "budget_utilization": active.cost_budget_usd.map(|budget| {
                    if budget <= f64::EPSILON {
                        0.0
                    } else {
                        active.estimated_cost_usd / budget
                    }
                }),
                "budget_alerts": active.budget_alerts,
            },
            "tool_calls": active.tool_calls,
            "tool_errors": active.tool_errors,
            "secret_leak": {
                "detections_total": active.secret_leak_detections,
                "pattern_class_counts": active.secret_leak_pattern_class_counts,
            },
            "redaction_policy": {
                "prompt_content": "omitted",
                "tool_arguments": "omitted",
                "tool_results": "bytes_only",
            }
        })
    }

    pub fn log_event(&self, event: &AgentEvent) -> Result<()> {
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
                        estimated_cost_usd: 0.0,
                        cost_budget_usd: None,
                        budget_alerts: 0,
                        tool_calls: 0,
                        tool_errors: 0,
                        secret_leak_detections: 0,
                        secret_leak_pattern_class_counts: HashMap::new(),
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
                AgentEvent::CostUpdated {
                    cumulative_cost_usd,
                    budget_usd,
                    ..
                } => {
                    if let Some(active) = state.active.as_mut() {
                        active.estimated_cost_usd = *cumulative_cost_usd;
                        active.cost_budget_usd = *budget_usd;
                    }
                }
                AgentEvent::CostBudgetAlert { .. } => {
                    if let Some(active) = state.active.as_mut() {
                        active.budget_alerts = active.budget_alerts.saturating_add(1);
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
                AgentEvent::SafetyPolicyApplied { reason_codes, .. } => {
                    if let Some(active) = state.active.as_mut() {
                        for reason_code in reason_codes {
                            let Some(pattern_class) = secret_leak_pattern_class(reason_code) else {
                                continue;
                            };
                            active.secret_leak_detections =
                                active.secret_leak_detections.saturating_add(1);
                            let count = active
                                .secret_leak_pattern_class_counts
                                .entry(pattern_class.to_string())
                                .or_insert(0);
                            *count = count.saturating_add(1);
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

fn secret_leak_pattern_class(reason_code: &str) -> Option<&str> {
    reason_code.strip_prefix("secret_leak.")
}

pub fn tool_audit_event_json(
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

#[cfg(test)]
mod tests {
    use super::{tool_audit_event_json, PromptTelemetryLogger, ToolAuditLogger};
    use std::{collections::HashMap, time::Instant};
    use tau_agent_core::{AgentEvent, SafetyMode, SafetyStage, ToolExecutionResult};
    use tau_ai::ChatUsage;
    use tempfile::tempdir;

    #[test]
    fn unit_tool_audit_event_json_for_start_has_expected_shape() {
        let mut starts = HashMap::new();
        let event = AgentEvent::ToolExecutionStart {
            tool_call_id: "call-1".to_string(),
            tool_name: "bash".to_string(),
            arguments: serde_json::json!({ "command": "pwd" }),
        };
        let payload = tool_audit_event_json(&event, &mut starts).expect("expected payload");

        assert_eq!(payload["event"], "tool_execution_start");
        assert_eq!(payload["tool_call_id"], "call-1");
        assert_eq!(payload["tool_name"], "bash");
        assert!(payload["arguments_bytes"].as_u64().unwrap_or(0) > 0);
        assert!(starts.contains_key("call-1"));
    }

    #[test]
    fn unit_tool_audit_event_json_for_end_tracks_duration_and_error_state() {
        let mut starts = HashMap::new();
        starts.insert("call-2".to_string(), Instant::now());
        let event = AgentEvent::ToolExecutionEnd {
            tool_call_id: "call-2".to_string(),
            tool_name: "read".to_string(),
            result: ToolExecutionResult::error(serde_json::json!({ "error": "denied" })),
        };
        let payload = tool_audit_event_json(&event, &mut starts).expect("expected payload");

        assert_eq!(payload["event"], "tool_execution_end");
        assert_eq!(payload["tool_call_id"], "call-2");
        assert_eq!(payload["is_error"], true);
        assert!(payload["result_bytes"].as_u64().unwrap_or(0) > 0);
        assert!(payload["duration_ms"].is_number() || payload["duration_ms"].is_null());
        assert!(!starts.contains_key("call-2"));
    }

    #[test]
    fn integration_tool_audit_logger_persists_jsonl_records() {
        let temp = tempdir().expect("tempdir");
        let log_path = temp.path().join("tool-audit.jsonl");
        let logger = ToolAuditLogger::open(log_path.clone()).expect("logger should open");

        let start = AgentEvent::ToolExecutionStart {
            tool_call_id: "call-3".to_string(),
            tool_name: "write".to_string(),
            arguments: serde_json::json!({ "path": "out.txt", "content": "x" }),
        };
        logger.log_event(&start).expect("write start event");

        let end = AgentEvent::ToolExecutionEnd {
            tool_call_id: "call-3".to_string(),
            tool_name: "write".to_string(),
            result: ToolExecutionResult::ok(serde_json::json!({ "bytes_written": 1 })),
        };
        logger.log_event(&end).expect("write end event");

        let raw = std::fs::read_to_string(log_path).expect("read audit log");
        let lines = raw.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);

        let first: serde_json::Value = serde_json::from_str(lines[0]).expect("parse first");
        let second: serde_json::Value = serde_json::from_str(lines[1]).expect("parse second");
        assert_eq!(first["event"], "tool_execution_start");
        assert_eq!(second["event"], "tool_execution_end");
        assert_eq!(second["is_error"], false);
    }

    #[test]
    fn regression_prompt_telemetry_logger_marks_interrupted_runs() {
        let temp = tempdir().expect("tempdir");
        let log_path = temp.path().join("prompt-telemetry.jsonl");
        let logger = PromptTelemetryLogger::open(log_path.clone(), "openai", "gpt-4o-mini")
            .expect("logger open");

        logger
            .log_event(&AgentEvent::AgentStart)
            .expect("first start");
        logger
            .log_event(&AgentEvent::TurnEnd {
                turn: 1,
                tool_results: 0,
                request_duration_ms: 11,
                usage: ChatUsage {
                    input_tokens: 1,
                    output_tokens: 1,
                    total_tokens: 2,
                },
                finish_reason: Some("length".to_string()),
            })
            .expect("first turn");
        logger
            .log_event(&AgentEvent::AgentStart)
            .expect("second start");
        logger
            .log_event(&AgentEvent::AgentEnd { new_messages: 1 })
            .expect("finalize second run");

        let raw = std::fs::read_to_string(log_path).expect("read telemetry log");
        let lines = raw.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);

        let first: serde_json::Value = serde_json::from_str(lines[0]).expect("first record");
        let second: serde_json::Value = serde_json::from_str(lines[1]).expect("second record");
        assert_eq!(first["status"], "interrupted");
        assert_eq!(first["success"], false);
        assert_eq!(second["status"], "completed");
        assert_eq!(second["success"], true);
    }

    #[test]
    fn functional_prompt_telemetry_logger_records_cost_fields_and_budget_alerts() {
        let temp = tempdir().expect("tempdir");
        let log_path = temp.path().join("prompt-telemetry-cost.jsonl");
        let logger = PromptTelemetryLogger::open(log_path.clone(), "openai", "gpt-4o-mini")
            .expect("logger open");

        logger
            .log_event(&AgentEvent::AgentStart)
            .expect("start prompt");
        logger
            .log_event(&AgentEvent::TurnEnd {
                turn: 1,
                tool_results: 0,
                request_duration_ms: 5,
                usage: ChatUsage {
                    input_tokens: 100,
                    output_tokens: 40,
                    total_tokens: 140,
                },
                finish_reason: Some("stop".to_string()),
            })
            .expect("turn end");
        logger
            .log_event(&AgentEvent::CostUpdated {
                turn: 1,
                turn_cost_usd: 0.12,
                cumulative_cost_usd: 0.12,
                budget_usd: Some(0.2),
            })
            .expect("cost update");
        logger
            .log_event(&AgentEvent::CostBudgetAlert {
                turn: 1,
                threshold_percent: 50,
                cumulative_cost_usd: 0.12,
                budget_usd: 0.2,
            })
            .expect("cost alert");
        logger
            .log_event(&AgentEvent::AgentEnd { new_messages: 1 })
            .expect("end prompt");

        let raw = std::fs::read_to_string(log_path).expect("read telemetry log");
        let lines = raw.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 1);
        let record: serde_json::Value = serde_json::from_str(lines[0]).expect("record");
        assert_eq!(record["cost"]["estimated_usd"], 0.12);
        assert_eq!(record["cost"]["budget_usd"], 0.2);
        assert_eq!(record["cost"]["budget_alerts"], 1);
        assert!(record["cost"]["budget_utilization"].as_f64().unwrap_or(0.0) > 0.0);
    }

    #[test]
    fn functional_prompt_telemetry_logger_records_secret_leak_counters_by_pattern_class() {
        let temp = tempdir().expect("tempdir");
        let log_path = temp.path().join("prompt-telemetry-secret-leak.jsonl");
        let logger = PromptTelemetryLogger::open(log_path.clone(), "openai", "gpt-4o-mini")
            .expect("logger open");

        logger
            .log_event(&AgentEvent::AgentStart)
            .expect("start prompt");
        logger
            .log_event(&AgentEvent::SafetyPolicyApplied {
                stage: SafetyStage::ToolOutput,
                mode: SafetyMode::Redact,
                blocked: false,
                matched_rules: vec!["leak.openai_api_key".to_string()],
                reason_codes: vec!["secret_leak.openai_api_key".to_string()],
            })
            .expect("leak event one");
        logger
            .log_event(&AgentEvent::SafetyPolicyApplied {
                stage: SafetyStage::ToolOutput,
                mode: SafetyMode::Block,
                blocked: true,
                matched_rules: vec![
                    "leak.openai_api_key".to_string(),
                    "leak.github_classic_pat".to_string(),
                ],
                reason_codes: vec![
                    "secret_leak.openai_api_key".to_string(),
                    "secret_leak.github_token".to_string(),
                ],
            })
            .expect("leak event two");
        logger
            .log_event(&AgentEvent::AgentEnd { new_messages: 1 })
            .expect("end prompt");

        let raw = std::fs::read_to_string(log_path).expect("read telemetry log");
        let lines = raw.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 1);
        let record: serde_json::Value = serde_json::from_str(lines[0]).expect("record");
        assert_eq!(record["secret_leak"]["detections_total"], 3);
        assert_eq!(
            record["secret_leak"]["pattern_class_counts"]["openai_api_key"],
            2
        );
        assert_eq!(
            record["secret_leak"]["pattern_class_counts"]["github_token"],
            1
        );
    }
}
