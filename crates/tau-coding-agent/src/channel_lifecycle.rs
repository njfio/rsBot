//! Multi-channel lifecycle command facade.
//!
//! Re-exports channel lifecycle command entrypoints for login/probe/logout-style
//! operations while preserving shared lifecycle safeguards.

pub(crate) use tau_startup::execute_multi_channel_channel_lifecycle_command;
