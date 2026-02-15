//! QA-loop command facade for coding-agent preflight/runtime flows.
//!
//! Re-exports QA loop command entrypoints so command dispatch and startup checks
//! share identical maintenance-loop behavior.

pub(crate) use tau_ops::{execute_qa_loop_cli_command, execute_qa_loop_preflight_command};
