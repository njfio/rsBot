//! Request translation helpers converting OpenResponses payloads into Tau runtime requests.

use serde_json::Value;

use super::{
    OpenResponsesApiError, OpenResponsesPrompt, OpenResponsesRequest, DEFAULT_SESSION_KEY,
};

pub(super) fn translate_openresponses_request(
    request: &OpenResponsesRequest,
    max_input_chars: usize,
) -> Result<OpenResponsesPrompt, OpenResponsesApiError> {
    let mut ignored_fields = request.extra.keys().cloned().collect::<Vec<_>>();
    ignored_fields.sort();

    let mut segments = Vec::new();
    if let Some(instructions) = non_empty_trimmed(request.instructions.as_deref()) {
        segments.push(format!("System instructions:\n{instructions}"));
    }

    if let Some(previous_response_id) = non_empty_trimmed(request.previous_response_id.as_deref()) {
        segments.push(format!(
            "Continuation context (previous_response_id):\n{previous_response_id}"
        ));
    }

    let mut extracted = 0usize;
    extract_openresponses_input_segments(
        &request.input,
        &mut segments,
        &mut extracted,
        &mut ignored_fields,
    )?;

    if extracted == 0 {
        return Err(OpenResponsesApiError::bad_request(
            "missing_input",
            "input must include at least one textual message or function_call_output item",
        ));
    }

    let prompt = segments
        .iter()
        .filter_map(|segment| {
            let trimmed = segment.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    if prompt.is_empty() {
        return Err(OpenResponsesApiError::bad_request(
            "missing_input",
            "input did not contain usable text",
        ));
    }

    if prompt.chars().count() > max_input_chars {
        return Err(OpenResponsesApiError::payload_too_large(format!(
            "translated input exceeds max {} characters",
            max_input_chars
        )));
    }

    let session_seed = metadata_string(&request.metadata, "session_id")
        .or_else(|| non_empty_trimmed(request.conversation.as_deref()))
        .or_else(|| non_empty_trimmed(request.previous_response_id.as_deref()))
        .unwrap_or(DEFAULT_SESSION_KEY);

    Ok(OpenResponsesPrompt {
        prompt,
        session_key: sanitize_session_key(session_seed),
        ignored_fields,
    })
}

fn extract_openresponses_input_segments(
    input: &Value,
    segments: &mut Vec<String>,
    extracted: &mut usize,
    ignored_fields: &mut Vec<String>,
) -> Result<(), OpenResponsesApiError> {
    match input {
        Value::Null => Err(OpenResponsesApiError::bad_request(
            "missing_input",
            "input is required",
        )),
        Value::String(text) => {
            let text = text.trim();
            if !text.is_empty() {
                segments.push(format!("User:\n{text}"));
                *extracted = extracted.saturating_add(1);
            }
            Ok(())
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                extract_openresponses_item(item, index, segments, extracted, ignored_fields)?;
            }
            Ok(())
        }
        Value::Object(_) => {
            extract_openresponses_item(input, 0, segments, extracted, ignored_fields)
        }
        _ => Err(OpenResponsesApiError::bad_request(
            "invalid_input",
            "input must be a string, object, or array",
        )),
    }
}

fn extract_openresponses_item(
    item: &Value,
    index: usize,
    segments: &mut Vec<String>,
    extracted: &mut usize,
    ignored_fields: &mut Vec<String>,
) -> Result<(), OpenResponsesApiError> {
    match item {
        Value::String(text) => {
            let text = text.trim();
            if !text.is_empty() {
                segments.push(format!("User:\n{text}"));
                *extracted = extracted.saturating_add(1);
            }
            Ok(())
        }
        Value::Object(map) => {
            let item_type = map.get("type").and_then(Value::as_str).unwrap_or_default();
            if item_type == "function_call_output" {
                let output = stringify_output(map.get("output").unwrap_or(&Value::Null));
                if output.is_empty() {
                    return Err(OpenResponsesApiError::bad_request(
                        "invalid_function_call_output",
                        format!(
                            "input[{index}] function_call_output item requires non-empty output"
                        ),
                    ));
                }
                let call_id = map
                    .get("call_id")
                    .or_else(|| map.get("id"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("unknown");
                segments.push(format!("Function output (call_id={call_id}):\n{output}"));
                *extracted = extracted.saturating_add(1);
                return Ok(());
            }

            if item_type == "message" || map.contains_key("role") || map.contains_key("content") {
                let role = map.get("role").and_then(Value::as_str).unwrap_or("user");
                let text = extract_message_content_text(map.get("content"));
                if !text.is_empty() {
                    segments.push(format!("{}:\n{}", role_label(role), text));
                    *extracted = extracted.saturating_add(1);
                } else {
                    ignored_fields.push(format!("input[{index}].content"));
                }
                return Ok(());
            }

            ignored_fields.push(format!("input[{index}]"));
            Ok(())
        }
        _ => {
            ignored_fields.push(format!("input[{index}]"));
            Ok(())
        }
    }
}

fn extract_message_content_text(content: Option<&Value>) -> String {
    let Some(content) = content else {
        return String::new();
    };

    match content {
        Value::String(text) => text.trim().to_string(),
        Value::Array(parts) => {
            let mut segments = Vec::new();
            for part in parts {
                if let Some(text) = extract_message_content_part(part) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        segments.push(trimmed.to_string());
                    }
                }
            }
            segments.join("\n")
        }
        Value::Object(_) => extract_message_content_part(content).unwrap_or_default(),
        _ => String::new(),
    }
}

fn extract_message_content_part(part: &Value) -> Option<String> {
    match part {
        Value::String(text) => Some(text.to_string()),
        Value::Object(map) => {
            let part_type = map.get("type").and_then(Value::as_str).unwrap_or("text");
            match part_type {
                "input_text" | "output_text" | "text" => map
                    .get("text")
                    .and_then(Value::as_str)
                    .map(|value| value.to_string()),
                "function_call_output" => {
                    let output = stringify_output(map.get("output").unwrap_or(&Value::Null));
                    if output.trim().is_empty() {
                        return None;
                    }
                    let call_id = map
                        .get("call_id")
                        .or_else(|| map.get("id"))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or("unknown");
                    Some(format!("Function output (call_id={call_id}):\n{output}"))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn stringify_output(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.trim().to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn role_label(role: &str) -> &'static str {
    match role.trim().to_ascii_lowercase().as_str() {
        "assistant" => "Assistant context",
        "system" => "System context",
        "tool" => "Tool context",
        _ => "User",
    }
}

fn metadata_string<'a>(metadata: &'a Value, key: &str) -> Option<&'a str> {
    metadata
        .as_object()?
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn non_empty_trimmed(raw: Option<&str>) -> Option<&str> {
    raw.map(str::trim).filter(|value| !value.is_empty())
}

pub(super) fn sanitize_session_key(raw: &str) -> String {
    let mut normalized = String::new();
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            normalized.push(ch);
        } else {
            normalized.push('_');
        }
    }
    let normalized = normalized.trim_matches('_').to_string();
    if normalized.is_empty() {
        DEFAULT_SESSION_KEY.to_string()
    } else {
        normalized
    }
}
