use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_RETRIES: usize = 2;
pub const BASE_BACKOFF_MS: u64 = 200;

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn should_retry_status(status: u16) -> bool {
    status == 408 || status == 409 || status == 425 || status == 429 || status >= 500
}

pub fn next_backoff_ms(attempt: usize) -> u64 {
    let shift = attempt.min(6);
    BASE_BACKOFF_MS.saturating_mul(1_u64 << shift)
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
    format!("pi-rs-{millis}-{count}")
}

#[cfg(test)]
mod tests {
    use super::{new_request_id, next_backoff_ms, should_retry_status};

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
    fn request_ids_are_unique() {
        let a = new_request_id();
        let b = new_request_id();
        assert_ne!(a, b);
        assert!(a.starts_with("pi-rs-"));
    }
}
