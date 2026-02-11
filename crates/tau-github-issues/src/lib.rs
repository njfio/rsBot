//! Shared helpers for Tau GitHub issues bridge/runtime implementations.
//! This crate provides transport helpers, attachment policy utilities, and
//! issue-comment rendering helpers consumed by runtime crates.

pub mod github_issues_helpers;
pub mod github_transport_helpers;
pub mod issue_comment;
pub mod issue_filter;
pub mod issue_render;
