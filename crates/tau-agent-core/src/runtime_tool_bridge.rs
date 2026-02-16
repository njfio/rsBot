//! Tool-bridge helpers for argument validation and tool execution.

use std::{sync::Arc, time::Duration};

use jsonschema::validator_for;
use serde_json::{json, Value};
use tau_ai::{ToolCall, ToolDefinition};

use crate::{AgentTool, CooperativeCancellationToken, ToolExecutionResult};

pub(crate) async fn execute_tool_call_inner(
    call: ToolCall,
    registered: Option<(ToolDefinition, Arc<dyn AgentTool>)>,
    tool_timeout: Option<Duration>,
    cancellation_token: Option<CooperativeCancellationToken>,
) -> ToolExecutionResult {
    if cancellation_token
        .as_ref()
        .map(CooperativeCancellationToken::is_cancelled)
        .unwrap_or(false)
    {
        return ToolExecutionResult::error(json!({
            "error": format!("tool '{}' cancelled before execution", call.name)
        }));
    }

    if let Some((definition, tool)) = registered {
        if let Err(error) = validate_tool_arguments(&definition, &call.arguments) {
            return ToolExecutionResult::error(json!({ "error": error }));
        }

        let tool_name = definition.name.clone();
        let execution = async move {
            if let Some(timeout) = tool_timeout {
                match tokio::time::timeout(timeout, tool.execute(call.arguments)).await {
                    Ok(result) => result,
                    Err(_) => ToolExecutionResult::error(json!({
                        "error": format!(
                            "tool '{}' timed out after {}ms",
                            tool_name,
                            timeout.as_millis()
                        )
                    })),
                }
            } else {
                tool.execute(call.arguments).await
            }
        };

        if let Some(token) = cancellation_token {
            tokio::select! {
                _ = token.cancelled() => ToolExecutionResult::error(json!({
                    "error": format!("tool '{}' cancelled", definition.name)
                })),
                result = execution => result,
            }
        } else {
            execution.await
        }
    } else {
        ToolExecutionResult::error(json!({
            "error": format!("Tool '{}' is not registered", call.name)
        }))
    }
}
pub(crate) fn validate_tool_arguments(
    definition: &ToolDefinition,
    arguments: &Value,
) -> Result<(), String> {
    let validator = validator_for(&definition.parameters)
        .map_err(|error| format!("invalid JSON schema for '{}': {error}", definition.name))?;

    let mut errors = validator.iter_errors(arguments);
    if let Some(first) = errors.next() {
        return Err(format!(
            "invalid arguments for '{}': {}",
            definition.name, first
        ));
    }

    Ok(())
}
