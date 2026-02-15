//! Multi-channel send command facade.
//!
//! Re-exports send command execution helpers for coding-agent command routing so
//! transport send semantics remain aligned with startup/runtime contracts.

pub(crate) use tau_startup::execute_multi_channel_send_command;
