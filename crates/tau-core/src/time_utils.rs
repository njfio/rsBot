/// Returns the current Unix timestamp in milliseconds.
pub fn current_unix_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

/// Returns the current Unix timestamp in seconds.
pub fn current_unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Returns true when `expires_unix` is present and no longer in the future.
pub fn is_expired_unix(expires_unix: Option<u64>, now_unix: u64) -> bool {
    matches!(expires_unix, Some(value) if value <= now_unix)
}
