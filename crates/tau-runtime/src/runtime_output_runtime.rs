use std::io::Write;

use anyhow::Result;
use tau_agent_core::AgentEvent;
use tau_ai::{Message, MessageRole};
use tau_session::SessionRuntime;

pub fn summarize_message(message: &Message) -> String {
    let text = message.text_content().replace('\n', " ");
    if text.trim().is_empty() {
        return format!(
            "{:?} (tool_calls={})",
            message.role,
            message.tool_calls().len()
        );
    }

    let max = 60;
    if text.chars().count() <= max {
        text
    } else {
        let summary = text.chars().take(max).collect::<String>();
        format!("{summary}...")
    }
}

pub fn persist_messages(
    session_runtime: &mut Option<SessionRuntime>,
    new_messages: &[Message],
) -> Result<()> {
    let Some(runtime) = session_runtime.as_mut() else {
        return Ok(());
    };

    runtime.active_head = runtime
        .store
        .append_messages(runtime.active_head, new_messages)?;
    Ok(())
}

pub fn print_assistant_messages(
    messages: &[Message],
    stream_output: bool,
    _stream_delay_ms: u64,
    suppress_first_streamed_text: bool,
) {
    let mut suppressed_once = false;
    for message in messages {
        if message.role != MessageRole::Assistant {
            continue;
        }

        let text = message.text_content();
        if !text.trim().is_empty() {
            if stream_output && suppress_first_streamed_text && !suppressed_once {
                suppressed_once = true;
                println!("\n");
                continue;
            }
            println!();
            if stream_output {
                let mut stdout = std::io::stdout();
                for chunk in stream_text_chunks(&text) {
                    print!("{chunk}");
                    let _ = stdout.flush();
                }
                println!("\n");
            } else {
                println!("{text}\n");
            }
            continue;
        }

        let tool_calls = message.tool_calls();
        if !tool_calls.is_empty() {
            println!(
                "\n[assistant requested {} tool call(s)]\n",
                tool_calls.len()
            );
        }
    }
}

pub fn stream_text_chunks(text: &str) -> Vec<&str> {
    text.split_inclusive(char::is_whitespace).collect()
}

pub fn event_to_json(event: &AgentEvent) -> serde_json::Value {
    match event {
        AgentEvent::AgentStart => serde_json::json!({ "type": "agent_start" }),
        AgentEvent::AgentEnd { new_messages } => {
            serde_json::json!({ "type": "agent_end", "new_messages": new_messages })
        }
        AgentEvent::TurnStart { turn } => serde_json::json!({ "type": "turn_start", "turn": turn }),
        AgentEvent::TurnEnd {
            turn,
            tool_results,
            request_duration_ms,
            usage,
            finish_reason,
        } => serde_json::json!({
            "type": "turn_end",
            "turn": turn,
            "tool_results": tool_results,
            "request_duration_ms": request_duration_ms,
            "usage": usage,
            "finish_reason": finish_reason,
        }),
        AgentEvent::MessageAdded { message } => serde_json::json!({
            "type": "message_added",
            "role": format!("{:?}", message.role).to_lowercase(),
            "text": message.text_content(),
            "tool_calls": message.tool_calls().len(),
        }),
        AgentEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            arguments,
        } => serde_json::json!({
            "type": "tool_execution_start",
            "tool_call_id": tool_call_id,
            "tool_name": tool_name,
            "arguments": arguments,
        }),
        AgentEvent::ToolExecutionEnd {
            tool_call_id,
            tool_name,
            result,
        } => serde_json::json!({
            "type": "tool_execution_end",
            "tool_call_id": tool_call_id,
            "tool_name": tool_name,
            "is_error": result.is_error,
            "content": result.content,
        }),
        AgentEvent::ReplanTriggered { turn, reason } => serde_json::json!({
            "type": "replan_triggered",
            "turn": turn,
            "reason": reason,
        }),
        AgentEvent::CostUpdated {
            turn,
            turn_cost_usd,
            cumulative_cost_usd,
            budget_usd,
        } => serde_json::json!({
            "type": "cost_updated",
            "turn": turn,
            "turn_cost_usd": turn_cost_usd,
            "cumulative_cost_usd": cumulative_cost_usd,
            "budget_usd": budget_usd,
        }),
        AgentEvent::CostBudgetAlert {
            turn,
            threshold_percent,
            cumulative_cost_usd,
            budget_usd,
        } => serde_json::json!({
            "type": "cost_budget_alert",
            "turn": turn,
            "threshold_percent": threshold_percent,
            "cumulative_cost_usd": cumulative_cost_usd,
            "budget_usd": budget_usd,
        }),
        AgentEvent::SafetyPolicyApplied {
            stage,
            mode,
            blocked,
            matched_rules,
            reason_codes,
        } => serde_json::json!({
            "type": "safety_policy_applied",
            "stage": stage.as_str(),
            "mode": mode,
            "blocked": blocked,
            "matched_rules": matched_rules,
            "reason_codes": reason_codes,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{event_to_json, print_assistant_messages, stream_text_chunks, summarize_message};
    use tau_agent_core::{AgentEvent, SafetyMode, SafetyStage, ToolExecutionResult};
    use tau_ai::{ContentBlock, Message, ToolCall};

    #[test]
    fn unit_stream_text_chunks_preserve_whitespace_boundaries() {
        let chunks = stream_text_chunks("hello world\nnext");
        assert_eq!(chunks, vec!["hello ", "world\n", "next"]);
    }

    #[test]
    fn regression_stream_text_chunks_handles_empty_and_single_word() {
        assert!(stream_text_chunks("").is_empty());
        assert_eq!(stream_text_chunks("token"), vec!["token"]);
    }

    #[test]
    fn unit_summarize_message_truncates_long_text_and_reports_tool_calls_for_empty_text() {
        let short = Message::assistant_text("short text");
        assert_eq!(summarize_message(&short), "short text");

        let long = Message::assistant_text("a".repeat(80));
        assert!(summarize_message(&long).ends_with("..."));

        let tool_call = Message::assistant_blocks(vec![ContentBlock::tool_call(ToolCall {
            id: "call-1".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({ "path": "README.md" }),
        })]);
        assert_eq!(summarize_message(&tool_call), "Assistant (tool_calls=1)");
    }

    #[test]
    fn unit_event_to_json_maps_tool_execution_end_shape() {
        let event = AgentEvent::ToolExecutionEnd {
            tool_call_id: "call-1".to_string(),
            tool_name: "write".to_string(),
            result: ToolExecutionResult::ok(serde_json::json!({ "ok": true })),
        };
        let value = event_to_json(&event);
        assert_eq!(value["type"], "tool_execution_end");
        assert_eq!(value["tool_call_id"], "call-1");
        assert_eq!(value["tool_name"], "write");
        assert_eq!(value["is_error"], false);
        assert_eq!(value["content"]["ok"], true);
    }

    #[test]
    fn unit_event_to_json_maps_replan_triggered_shape() {
        let event = AgentEvent::ReplanTriggered {
            turn: 2,
            reason: "tool failure".to_string(),
        };
        let value = event_to_json(&event);
        assert_eq!(value["type"], "replan_triggered");
        assert_eq!(value["turn"], 2);
        assert_eq!(value["reason"], "tool failure");
    }

    #[test]
    fn unit_event_to_json_maps_cost_budget_alert_shape() {
        let event = AgentEvent::CostBudgetAlert {
            turn: 3,
            threshold_percent: 80,
            cumulative_cost_usd: 1.25,
            budget_usd: 1.5,
        };
        let value = event_to_json(&event);
        assert_eq!(value["type"], "cost_budget_alert");
        assert_eq!(value["turn"], 3);
        assert_eq!(value["threshold_percent"], 80);
        assert_eq!(value["cumulative_cost_usd"], 1.25);
        assert_eq!(value["budget_usd"], 1.5);
    }

    #[test]
    fn unit_event_to_json_maps_safety_policy_applied_shape() {
        let event = AgentEvent::SafetyPolicyApplied {
            stage: SafetyStage::ToolOutput,
            mode: SafetyMode::Block,
            blocked: true,
            matched_rules: vec!["literal.ignore_previous_instructions".to_string()],
            reason_codes: vec!["prompt_injection.ignore_instructions".to_string()],
        };
        let value = event_to_json(&event);
        assert_eq!(value["type"], "safety_policy_applied");
        assert_eq!(value["stage"], "tool_output");
        assert_eq!(value["mode"], "block");
        assert_eq!(value["blocked"], true);
        assert_eq!(
            value["reason_codes"][0],
            "prompt_injection.ignore_instructions"
        );
    }

    #[test]
    fn unit_print_assistant_messages_stream_fallback_avoids_blocking_delay() {
        let started = Instant::now();
        print_assistant_messages(
            &[Message::assistant_text("fallback stream render")],
            true,
            300,
            false,
        );
        assert!(
            started.elapsed() < Duration::from_millis(350),
            "fallback render should not sleep per chunk in sync path"
        );
    }
}
