//! Shared helpers for Tau GitHub issues bridge/runtime implementations.
//! This crate provides transport helpers, attachment policy utilities, and
//! issue-comment rendering helpers consumed by runtime crates.

pub mod github_issues_helpers;
pub mod github_transport_helpers;
pub mod issue_artifacts_command;
pub mod issue_auth_command;
pub mod issue_auth_helpers;
pub mod issue_chat_command;
pub mod issue_command_usage;
pub mod issue_comment;
pub mod issue_demo_index;
pub mod issue_demo_index_command;
pub mod issue_doctor_command;
pub mod issue_event_action;
pub mod issue_event_collection;
pub mod issue_filter;
pub mod issue_prompt_helpers;
pub mod issue_render;
pub mod issue_run_error_comment;
pub mod issue_runtime_helpers;
pub mod issue_session_helpers;
