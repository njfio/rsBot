use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};

use super::{MultiChannelInboundEvent, MULTI_CHANNEL_LIVE_INGRESS_SOURCES};
use crate::multi_channel_live_ingress::parse_multi_channel_live_inbound_envelope;

pub(super) fn load_multi_channel_live_events(
    ingress_dir: &Path,
) -> Result<Vec<MultiChannelInboundEvent>> {
    std::fs::create_dir_all(ingress_dir)
        .with_context(|| format!("failed to create {}", ingress_dir.display()))?;
    let mut events = Vec::new();
    for (transport, file_name) in MULTI_CHANNEL_LIVE_INGRESS_SOURCES {
        let path = ingress_dir.join(file_name);
        if !path.exists() {
            continue;
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        for (index, line) in raw.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match parse_multi_channel_live_inbound_envelope(trimmed) {
                Ok(event) => {
                    if event.transport.as_str() != transport {
                        eprintln!(
                            "multi-channel live ingress skipped event: file={} line={} reason=transport_mismatch expected={} actual={}",
                            path.display(),
                            index + 1,
                            transport,
                            event.transport.as_str()
                        );
                        continue;
                    }
                    events.push(event);
                }
                Err(error) => {
                    eprintln!(
                        "multi-channel live ingress parse failure: file={} line={} reason_code={} detail={}",
                        path.display(),
                        index + 1,
                        error.code.as_str(),
                        error.message
                    );
                }
            }
        }
    }
    Ok(events)
}

pub(super) fn build_user_context_text(
    event: &MultiChannelInboundEvent,
    media_prompt_context: Option<&str>,
) -> Option<String> {
    let text = event.text.trim();
    let media = media_prompt_context.map(str::trim).unwrap_or_default();
    if text.is_empty() && media.is_empty() {
        return None;
    }
    if media.is_empty() {
        return Some(text.to_string());
    }
    if text.is_empty() {
        return Some(media.to_string());
    }
    Some(format!("{text}\n\n{media}"))
}

pub(super) fn normalize_processed_keys(raw: &[String], cap: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for key in raw {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            continue;
        }
        let owned = trimmed.to_string();
        if seen.insert(owned.clone()) {
            normalized.push(owned);
        }
    }
    if cap == 0 {
        return Vec::new();
    }
    if normalized.len() > cap {
        normalized.drain(0..normalized.len().saturating_sub(cap));
    }
    normalized
}
