//! CLI argument models and validation utilities for Tau binaries.
//!
//! Exposes clap-backed command/flag types plus validation helpers shared by
//! startup, diagnostics, and runtime command dispatch layers.

pub mod cli_args;
pub mod cli_types;
pub mod command_file;
pub mod command_text;
pub mod gateway_remote_profile;
pub mod validation;

pub use cli_args::Cli;
pub use cli_types::*;
pub use command_file::*;
pub use command_text::*;
pub use gateway_remote_profile::*;
pub use validation::*;
