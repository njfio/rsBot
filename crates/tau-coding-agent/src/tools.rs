//! Built-in tool facade for coding-agent command/runtime use.
//!
//! Re-exports tool registration and policy types from `tau-tools` to keep tool
//! dispatch contracts uniform across startup and runtime loops.

pub use tau_tools::tools::*;
