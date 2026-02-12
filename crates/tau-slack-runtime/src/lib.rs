//! Runtime-source crate for the Tau Slack bridge.
//!
//! The Slack runtime implementation is consumed by `tau-coding-agent` via
//! source include during incremental crate extraction.

/// Marker type used to keep this crate non-empty while extraction is in progress.
pub struct SlackRuntimeCrate;
