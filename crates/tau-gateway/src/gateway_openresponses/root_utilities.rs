//! Utility helpers extracted from gateway_openresponses root module.

#[cfg(test)]
use anyhow::{Context, Result};
#[cfg(test)]
use std::net::SocketAddr;

pub(super) fn derive_gateway_preflight_token_limit(max_input_chars: usize) -> Option<u32> {
    if max_input_chars == 0 {
        return None;
    }
    let chars = u32::try_from(max_input_chars).unwrap_or(u32::MAX);
    Some(chars.saturating_add(3) / 4)
}

#[cfg(test)]
pub(super) fn validate_gateway_openresponses_bind(bind: &str) -> Result<SocketAddr> {
    bind.parse::<SocketAddr>()
        .with_context(|| format!("invalid gateway socket address '{bind}'"))
}
