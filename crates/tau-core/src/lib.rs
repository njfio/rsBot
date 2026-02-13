//! Foundational low-level utilities shared across Tau crates.
//!
//! Provides atomic file-write helpers and time utilities used by runtime state,
//! cache persistence, and expiry calculations.

pub mod atomic_io;
pub mod time_utils;

pub use atomic_io::write_text_atomic;
pub use time_utils::{current_unix_timestamp, current_unix_timestamp_ms, is_expired_unix};

#[cfg(test)]
mod tests {
    use std::fs::read_to_string;

    use super::*;

    #[test]
    fn time_utils_round_trip_bounds() {
        let now_s = current_unix_timestamp();
        let now_ms = current_unix_timestamp_ms();
        let now_ms_s = now_ms / 1_000;
        assert!(now_ms_s >= now_s);
        assert!(now_ms_s <= now_s.saturating_add(1));
    }

    #[test]
    fn is_expired_unix_respects_none_and_bounds() {
        let now = current_unix_timestamp();
        assert!(!is_expired_unix(None, now));
        assert!(is_expired_unix(Some(now), now));
        assert!(is_expired_unix(Some(now.saturating_sub(1)), now));
        assert!(!is_expired_unix(Some(now.saturating_add(1)), now));
    }

    #[test]
    fn write_text_atomic_writes_content() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let path = tempdir.path().join("sample.txt");
        write_text_atomic(&path, "hello world").expect("write");
        let contents = read_to_string(&path).expect("read");
        assert_eq!(contents, "hello world");
    }
}
