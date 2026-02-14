use chrono::{DateTime, Utc};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const BASE_BACKOFF_MS: u64 = 200;

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);
static JITTER_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn should_retry_status(status: u16) -> bool {
    status == 408 || status == 409 || status == 425 || status == 429 || status >= 500
}

pub fn next_backoff_ms(attempt: usize) -> u64 {
    let shift = attempt.min(6);
    BASE_BACKOFF_MS.saturating_mul(1_u64 << shift)
}

pub fn next_backoff_ms_with_jitter(attempt: usize, jitter_enabled: bool) -> u64 {
    let base = next_backoff_ms(attempt);
    if !jitter_enabled || base <= 1 {
        return base;
    }

    // Bounded jitter in [50%, 100%] of the deterministic backoff.
    let low = base / 2;
    let width = base.saturating_sub(low);
    let seed = JITTER_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mixed = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(17) ^ 0xA24B_AED4_963E_E407;
    let jitter = if width == 0 {
        0
    } else {
        mixed % width.saturating_add(1)
    };
    low.saturating_add(jitter)
}

pub fn parse_retry_after_ms(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    let raw = headers.get("retry-after")?.to_str().ok()?.trim();
    if raw.is_empty() {
        return None;
    }

    if let Ok(seconds) = raw.parse::<u64>() {
        return Some(seconds.saturating_mul(1000));
    }

    let retry_at = DateTime::parse_from_rfc2822(raw).ok()?.with_timezone(&Utc);
    let now = Utc::now();
    let delay_ms = retry_at.signed_duration_since(now).num_milliseconds();
    if delay_ms <= 0 {
        return Some(0);
    }

    u64::try_from(delay_ms).ok()
}

pub fn provider_retry_delay_ms(
    attempt: usize,
    jitter_enabled: bool,
    retry_after_ms: Option<u64>,
) -> u64 {
    let backoff_ms = next_backoff_ms_with_jitter(attempt, jitter_enabled);
    match retry_after_ms {
        Some(retry_after_ms) => backoff_ms.max(retry_after_ms),
        None => backoff_ms,
    }
}

pub fn retry_budget_allows_delay(elapsed_ms: u64, delay_ms: u64, retry_budget_ms: u64) -> bool {
    if retry_budget_ms == 0 {
        return true;
    }
    elapsed_ms.saturating_add(delay_ms) <= retry_budget_ms
}

pub fn is_retryable_http_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request() || error.is_body()
}

pub fn new_request_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let count = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("tau-rs-{millis}-{count}")
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use reqwest::header::{HeaderMap, HeaderValue};

    use super::{
        new_request_id, next_backoff_ms, next_backoff_ms_with_jitter, parse_retry_after_ms,
        provider_retry_delay_ms, retry_budget_allows_delay, should_retry_status,
    };

    #[test]
    fn retry_status_selection_is_correct() {
        assert!(should_retry_status(429));
        assert!(should_retry_status(503));
        assert!(!should_retry_status(400));
        assert!(!should_retry_status(404));
    }

    #[test]
    fn backoff_increases_per_attempt() {
        assert_eq!(next_backoff_ms(0), 200);
        assert_eq!(next_backoff_ms(1), 400);
        assert_eq!(next_backoff_ms(2), 800);
    }

    #[test]
    fn jittered_backoff_stays_within_expected_bounds() {
        let attempt = 3;
        let base = next_backoff_ms(attempt);
        let low = base / 2;
        for _ in 0..64 {
            let value = next_backoff_ms_with_jitter(attempt, true);
            assert!(value >= low, "expected {value} >= {low}");
            assert!(value <= base, "expected {value} <= {base}");
        }
    }

    #[test]
    fn unit_parse_retry_after_ms_accepts_seconds_and_rejects_invalid_values() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", HeaderValue::from_static("3"));
        assert_eq!(parse_retry_after_ms(&headers), Some(3_000));

        headers.insert("retry-after", HeaderValue::from_static("not-a-number"));
        assert_eq!(parse_retry_after_ms(&headers), None);
    }

    #[test]
    fn functional_parse_retry_after_ms_accepts_http_dates() {
        let mut headers = HeaderMap::new();
        let raw = (Utc::now() + Duration::seconds(2))
            .to_rfc2822()
            .replace("+0000", "GMT");
        headers.insert(
            "retry-after",
            HeaderValue::from_str(raw.as_str()).expect("retry-after date"),
        );
        let delay = parse_retry_after_ms(&headers).expect("delay from date");
        assert!(delay <= 2_500, "delay should be close to 2s, got {delay}");
        assert!(
            delay >= 500,
            "delay should be positive and non-trivial, got {delay}"
        );
    }

    #[test]
    fn regression_provider_retry_delay_honors_retry_after_floor() {
        let without_header = provider_retry_delay_ms(0, false, None);
        assert_eq!(without_header, 200);

        let smaller_header = provider_retry_delay_ms(2, false, Some(100));
        assert_eq!(smaller_header, 800);

        let larger_header = provider_retry_delay_ms(0, false, Some(1_500));
        assert_eq!(larger_header, 1_500);
    }

    #[test]
    fn retry_budget_math_respects_zero_and_bounded_budgets() {
        assert!(retry_budget_allows_delay(50, 100, 0));
        assert!(retry_budget_allows_delay(50, 50, 100));
        assert!(!retry_budget_allows_delay(50, 60, 100));
    }

    #[test]
    fn request_ids_are_unique() {
        let a = new_request_id();
        let b = new_request_id();
        assert_ne!(a, b);
        assert!(a.starts_with("tau-rs-"));
    }
}
