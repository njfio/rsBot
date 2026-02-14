//! Gateway contracts and HTTP/WebSocket runtime surface for Tau.
//!
//! Exposes gateway contract replay, OpenResponses-compatible endpoints,
//! service-mode lifecycle helpers, and remote profile planning utilities.

pub mod gateway_contract;
pub mod gateway_openresponses;
pub mod gateway_runtime;
pub mod gateway_ws_protocol;
pub mod remote_profile;

pub use gateway_contract::*;
pub use gateway_openresponses::*;
pub use gateway_runtime::*;
pub use gateway_ws_protocol::*;
pub use remote_profile::*;
