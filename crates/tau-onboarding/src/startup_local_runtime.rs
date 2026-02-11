use serde_json::Value;
use tau_agent_core::AgentEvent;
use tau_core::current_unix_timestamp_ms;

const EXTENSION_TOOL_HOOK_PAYLOAD_SCHEMA_VERSION: u32 = 1;

pub fn extension_tool_hook_dispatch(event: &AgentEvent) -> Option<(&'static str, Value)> {
    match event {
        AgentEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            arguments,
        } => Some((
            "pre-tool-call",
            extension_tool_hook_payload(
                "pre-tool-call",
                serde_json::json!({
                    "tool_call_id": tool_call_id,
                    "tool_name": tool_name,
                    "arguments": arguments,
                }),
            ),
        )),
        AgentEvent::ToolExecutionEnd {
            tool_call_id,
            tool_name,
            result,
        } => Some((
            "post-tool-call",
            extension_tool_hook_payload(
                "post-tool-call",
                serde_json::json!({
                    "tool_call_id": tool_call_id,
                    "tool_name": tool_name,
                    "result": {
                        "is_error": result.is_error,
                        "content": result.content,
                    },
                }),
            ),
        )),
        _ => None,
    }
}

fn extension_tool_hook_payload(hook: &str, data: Value) -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert(
        "schema_version".to_string(),
        serde_json::Value::Number(EXTENSION_TOOL_HOOK_PAYLOAD_SCHEMA_VERSION.into()),
    );
    payload.insert(
        "hook".to_string(),
        serde_json::Value::String(hook.to_string()),
    );
    payload.insert(
        "emitted_at_ms".to_string(),
        serde_json::Value::Number(current_unix_timestamp_ms().into()),
    );
    payload.insert("data".to_string(), data.clone());
    if let Some(object) = data.as_object() {
        for (key, value) in object {
            payload.insert(key.clone(), value.clone());
        }
    }
    Value::Object(payload)
}

#[cfg(test)]
mod tests {
    use super::extension_tool_hook_dispatch;
    use tau_agent_core::{AgentEvent, ToolExecutionResult};

    #[test]
    fn unit_extension_tool_hook_dispatch_maps_start_event_payload() {
        let event = AgentEvent::ToolExecutionStart {
            tool_call_id: "call-1".to_string(),
            tool_name: "read".to_string(),
            arguments: serde_json::json!({"path":"README.md"}),
        };
        let (hook, payload) = extension_tool_hook_dispatch(&event).expect("dispatch payload");
        assert_eq!(hook, "pre-tool-call");
        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["hook"], "pre-tool-call");
        assert!(payload["emitted_at_ms"].as_u64().is_some());
        assert_eq!(payload["data"]["tool_call_id"], "call-1");
        assert_eq!(payload["data"]["tool_name"], "read");
        assert_eq!(payload["data"]["arguments"]["path"], "README.md");
    }

    #[test]
    fn functional_extension_tool_hook_dispatch_maps_end_event_payload() {
        let event = AgentEvent::ToolExecutionEnd {
            tool_call_id: "call-1".to_string(),
            tool_name: "read".to_string(),
            result: ToolExecutionResult::ok(serde_json::json!({"content":"hello"})),
        };
        let (hook, payload) = extension_tool_hook_dispatch(&event).expect("dispatch payload");
        assert_eq!(hook, "post-tool-call");
        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["hook"], "post-tool-call");
        assert!(payload["emitted_at_ms"].as_u64().is_some());
        assert_eq!(payload["data"]["tool_call_id"], "call-1");
        assert_eq!(payload["data"]["tool_name"], "read");
        assert_eq!(payload["data"]["result"]["is_error"], false);
        assert_eq!(payload["data"]["result"]["content"]["content"], "hello");
    }

    #[test]
    fn regression_extension_tool_hook_dispatch_ignores_non_tool_events() {
        let event = AgentEvent::AgentStart;
        assert!(extension_tool_hook_dispatch(&event).is_none());
    }
}
