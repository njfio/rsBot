//! Multi-channel adapter builder facade.
//!
//! Re-exports startup adapter constructors used to bind channel lifecycle/send
//! operations into coding-agent command dispatch.

pub(crate) use tau_startup::{
    build_multi_channel_command_handlers, build_multi_channel_pairing_evaluator,
};
