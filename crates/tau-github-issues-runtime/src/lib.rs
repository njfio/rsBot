//! Runtime-source crate for the Tau GitHub issues bridge.
//!
//! The GitHub issues runtime implementation is consumed by `tau-coding-agent`
//! via source include during incremental crate extraction.

/// Marker type used to keep this crate non-empty while extraction is in progress.
pub struct GithubIssuesRuntimeCrate;
