use std::time::Duration;

use anyhow::Result;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{
    current_unix_timestamp_ms, ChannelContextEntry, ChannelLogEntry, ChannelStore,
    MultiChannelInboundEvent, MultiChannelOutboundDeliveryError, MultiChannelRouteDecision,
};

pub(super) fn log_contains_event_direction(
    logs: &[ChannelLogEntry],
    event_key: &str,
    direction: &str,
) -> bool {
    logs.iter()
        .any(|entry| entry.direction == direction && entry.event_key.as_deref() == Some(event_key))
}

pub(super) fn log_contains_outbound_status(
    logs: &[ChannelLogEntry],
    event_key: &str,
    status: &str,
) -> bool {
    logs.iter().any(|entry| {
        entry.direction == "outbound"
            && entry.event_key.as_deref() == Some(event_key)
            && entry.payload.get("status").and_then(Value::as_str) == Some(status)
    })
}

pub(super) fn log_contains_outbound_response(
    logs: &[ChannelLogEntry],
    event_key: &str,
    response: &str,
) -> bool {
    logs.iter().any(|entry| {
        entry.direction == "outbound"
            && entry.event_key.as_deref() == Some(event_key)
            && entry.payload.get("response").and_then(Value::as_str) == Some(response)
    })
}

pub(super) fn context_contains_entry(
    entries: &[ChannelContextEntry],
    role: &str,
    text: &str,
) -> bool {
    entries
        .iter()
        .any(|entry| entry.role == role && entry.text == text)
}

pub(super) struct DeliveryFailureLogContext<'a> {
    pub event: &'a MultiChannelInboundEvent,
    pub event_key: &'a str,
    pub route_decision: &'a MultiChannelRouteDecision,
    pub route_payload: &'a Value,
    pub pairing_payload: &'a Value,
    pub secure_messaging_payload: &'a Value,
    pub channel_policy_payload: &'a Value,
    pub delivery_mode: &'a str,
    pub command_payload: Option<&'a Value>,
}

pub(super) fn append_delivery_failure_log(
    store: &ChannelStore,
    context: &DeliveryFailureLogContext<'_>,
    error: &MultiChannelOutboundDeliveryError,
) -> Result<()> {
    let mut payload = json!({
        "status": "delivery_failed",
        "reason_code": error.reason_code,
        "detail": error.detail,
        "retryable": error.retryable,
        "chunk_index": error.chunk_index,
        "chunk_count": error.chunk_count,
        "endpoint": error.endpoint,
        "http_status": error.http_status,
        "request_body": error.request_body,
        "delivery_mode": context.delivery_mode,
        "event_key": context.event_key,
        "transport": context.event.transport.as_str(),
        "conversation_id": context.event.conversation_id.trim(),
        "route_session_key": context.route_decision.session_key.as_str(),
        "route": context.route_payload,
        "pairing": context.pairing_payload,
        "secure_messaging": context.secure_messaging_payload,
        "channel_policy": context.channel_policy_payload,
    });
    if let Some(command_payload) = context.command_payload {
        if let Value::Object(map) = &mut payload {
            map.insert("command".to_string(), command_payload.clone());
        }
    }
    store.append_log_entry(&ChannelLogEntry {
        timestamp_unix_ms: current_unix_timestamp_ms(),
        direction: "outbound".to_string(),
        event_key: Some(context.event_key.to_string()),
        source: "tau-multi-channel-runner".to_string(),
        payload,
    })
}

pub(super) fn render_response(event: &MultiChannelInboundEvent) -> String {
    let transport = event.transport.as_str();
    let event_id = event.event_id.trim();
    if matches!(event.event_kind, super::MultiChannelEventKind::Command)
        || event.text.trim().starts_with('/')
    {
        return format!(
            "command acknowledged: transport={} event_id={} conversation={}",
            transport, event_id, event.conversation_id
        );
    }
    format!(
        "message processed: transport={} event_id={} text_chars={}",
        transport,
        event_id,
        event.text.chars().count()
    )
}

pub(super) fn simulated_transient_failures(event: &MultiChannelInboundEvent) -> usize {
    event
        .metadata
        .get("simulate_transient_failures")
        .and_then(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0)
}

pub(super) fn retry_delay_ms(
    base_delay_ms: u64,
    jitter_ms: u64,
    attempt: usize,
    jitter_seed: &str,
) -> u64 {
    if base_delay_ms == 0 {
        return 0;
    }
    let exponent = attempt.saturating_sub(1).min(10) as u32;
    let base_delay = base_delay_ms.saturating_mul(1_u64 << exponent);
    if jitter_ms == 0 {
        return base_delay;
    }
    let mut hasher = Sha256::new();
    hasher.update(jitter_seed.as_bytes());
    hasher.update(attempt.to_le_bytes());
    let digest = hasher.finalize();
    let mut seed_bytes = [0_u8; 8];
    seed_bytes.copy_from_slice(&digest[..8]);
    let deterministic_jitter = u64::from_le_bytes(seed_bytes) % jitter_ms.saturating_add(1);
    base_delay.saturating_add(deterministic_jitter)
}

pub(super) async fn apply_retry_delay(
    base_delay_ms: u64,
    jitter_ms: u64,
    attempt: usize,
    jitter_seed: &str,
) {
    let delay_ms = retry_delay_ms(base_delay_ms, jitter_ms, attempt, jitter_seed);
    if delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
}
