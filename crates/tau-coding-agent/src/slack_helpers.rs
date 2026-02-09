use std::time::Duration;

pub(crate) fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
}

pub(crate) fn retry_delay(
    base_delay_ms: u64,
    attempt: usize,
    retry_after_seconds: Option<u64>,
) -> Duration {
    if let Some(retry_after_seconds) = retry_after_seconds {
        return Duration::from_secs(retry_after_seconds);
    }
    let exponent = attempt.saturating_sub(1).min(6) as u32;
    let scale = 2_u64.pow(exponent);
    Duration::from_millis(base_delay_ms.max(1).saturating_mul(scale))
}

pub(crate) fn is_retryable_slack_status(status: u16) -> bool {
    status == 429 || (500..600).contains(&status)
}

pub(crate) fn is_retryable_transport_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request() || error.is_body()
}

pub(crate) fn truncate_for_error(value: &str, max_chars: usize) -> String {
    truncate_for_slack(value, max_chars)
}

pub(crate) fn truncate_for_slack(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut truncated = String::new();
    for ch in value.chars().take(max_chars) {
        truncated.push(ch);
    }
    truncated.push_str("...");
    truncated
}

pub(crate) fn sanitize_for_path(raw: &str) -> String {
    let sanitized = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "channel".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        is_retryable_slack_status, parse_retry_after, retry_delay, sanitize_for_path,
        truncate_for_slack,
    };
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
    use std::time::Duration;

    #[test]
    fn unit_parse_retry_after_accepts_numeric_and_rejects_invalid_values() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("15"));
        assert_eq!(parse_retry_after(&headers), Some(15));

        headers.insert(RETRY_AFTER, HeaderValue::from_static("invalid"));
        assert_eq!(parse_retry_after(&headers), None);

        let empty = HeaderMap::new();
        assert_eq!(parse_retry_after(&empty), None);
    }

    #[test]
    fn unit_retry_delay_prefers_retry_after_and_uses_exponential_backoff() {
        assert_eq!(retry_delay(50, 1, Some(3)), Duration::from_secs(3));
        assert_eq!(retry_delay(100, 1, None), Duration::from_millis(100));
        assert_eq!(retry_delay(100, 2, None), Duration::from_millis(200));
        assert_eq!(retry_delay(100, 3, None), Duration::from_millis(400));
    }

    #[test]
    fn unit_is_retryable_slack_status_handles_rate_limit_and_server_errors() {
        assert!(is_retryable_slack_status(429));
        assert!(is_retryable_slack_status(500));
        assert!(is_retryable_slack_status(503));
        assert!(!is_retryable_slack_status(400));
        assert!(!is_retryable_slack_status(404));
    }

    #[test]
    fn regression_truncate_for_slack_preserves_unicode_boundaries() {
        let value = "ta🌊u-message";
        assert_eq!(truncate_for_slack(value, 20), value);
        assert_eq!(truncate_for_slack(value, 3), "ta🌊...");
        assert_eq!(truncate_for_slack(value, 0), "...");
    }

    #[test]
    fn regression_sanitize_for_path_replaces_unsafe_characters() {
        assert_eq!(sanitize_for_path("C123/topic alpha"), "C123_topic_alpha");
        assert_eq!(sanitize_for_path("___"), "channel");
    }
}
