//! Access-control primitives for Tau runtimes.
//!
//! Provides approval workflows, pairing policy checks, RBAC authorization,
//! and trust-root management shared across local and transport runtimes.

pub mod approvals;
pub mod identity;
pub mod pairing;
pub mod rbac;
pub mod rl_control_plane;
pub mod signed_envelope;
pub mod trust_roots;

pub use approvals::*;
pub use identity::*;
pub use pairing::*;
pub use rbac::*;
pub use rl_control_plane::*;
pub use signed_envelope::*;
pub use trust_roots::*;
