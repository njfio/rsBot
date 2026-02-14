//! Provider auth, credential, and fallback routing infrastructure for Tau.
//!
//! Includes provider client construction, credential-store operations,
//! integration auth commands, and model fallback/circuit-breaker logic.

mod auth;
mod auth_commands_runtime;
mod claude_cli_client;
mod cli_executable;
mod client;
mod codex_cli_client;
mod credential_store;
mod credentials;
mod fallback;
mod gemini_cli_client;
mod integration_auth;
mod model_catalog;
mod types;

pub use auth::*;
pub use auth_commands_runtime::*;
pub use claude_cli_client::*;
pub use cli_executable::is_executable_available;
pub use client::*;
pub use codex_cli_client::*;
pub use credential_store::*;
pub use credentials::*;
pub use fallback::*;
pub use gemini_cli_client::*;
pub use integration_auth::*;
pub use model_catalog::*;
pub use types::*;
