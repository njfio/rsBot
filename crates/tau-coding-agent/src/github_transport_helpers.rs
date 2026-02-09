use std::time::Duration;

pub(crate) fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let raw = headers.get("retry-after")?.to_str().ok()?;
    let seconds = raw.trim().parse::<u64>().ok()?;
    Some(Duration::from_secs(seconds))
}

pub(crate) fn retry_delay(
    base_delay_ms: u64,
    attempt: usize,
    retry_after: Option<Duration>,
) -> Duration {
    if let Some(delay) = retry_after {
        return delay.max(Duration::from_millis(base_delay_ms));
    }
    let exponent = attempt.saturating_sub(1).min(10) as u32;
    let scaled = base_delay_ms.saturating_mul(2_u64.saturating_pow(exponent));
    Duration::from_millis(scaled.min(30_000))
}

pub(crate) fn is_retryable_transport_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request()
}

pub(crate) fn is_retryable_github_status(status: u16) -> bool {
    status == 429 || status >= 500
}

pub(crate) fn truncate_for_error(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut truncated = text.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

#[cfg(test)]
mod tests {
    use super::{is_retryable_github_status, parse_retry_after, retry_delay, truncate_for_error};
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
    use std::time::Duration;

    #[test]
    fn unit_parse_retry_after_parses_seconds_and_rejects_invalid_values() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("4"));
        assert_eq!(parse_retry_after(&headers), Some(Duration::from_secs(4)));

        headers.insert(RETRY_AFTER, HeaderValue::from_static("bad-value"));
        assert_eq!(parse_retry_after(&headers), None);
    }

    #[test]
    fn unit_retry_delay_applies_retry_after_floor_and_exponential_backoff() {
        assert_eq!(
            retry_delay(200, 2, Some(Duration::from_millis(100))),
            Duration::from_millis(200)
        );
        assert_eq!(retry_delay(100, 1, None), Duration::from_millis(100),);
        assert_eq!(retry_delay(100, 3, None), Duration::from_millis(400),);
    }

    #[test]
    fn unit_retry_delay_caps_backoff_growth() {
        assert_eq!(retry_delay(2_000, 11, None), Duration::from_millis(30_000));
        assert_eq!(retry_delay(20_000, 2, None), Duration::from_millis(30_000));
    }

    #[test]
    fn unit_is_retryable_github_status_matches_expected_ranges() {
        assert!(is_retryable_github_status(429));
        assert!(is_retryable_github_status(500));
        assert!(!is_retryable_github_status(404));
    }

    #[test]
    fn regression_truncate_for_error_preserves_unicode_boundaries() {
        assert_eq!(truncate_for_error("ta🌊u", 3), "ta🌊...");
        assert_eq!(truncate_for_error("ok", 10), "ok");
    }
}
