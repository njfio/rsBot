//! Canvas command facade for coding-agent command dispatch.
//!
//! Re-exports canvas command contracts from `tau-ops` so runtime command routing
//! uses one canonical canvas persistence and repair behavior.

pub(crate) use tau_ops::{
    execute_canvas_command, CanvasCommandConfig, CanvasEventOrigin, CanvasSessionLinkContext,
};
