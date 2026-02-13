//! Access-control primitives for Tau runtimes.
//!
//! Provides approval workflows, pairing policy checks, RBAC authorization,
//! and trust-root management shared across local and transport runtimes.

pub mod approvals;
pub mod pairing;
pub mod rbac;
pub mod trust_roots;

pub use approvals::*;
pub use pairing::*;
pub use rbac::*;
pub use trust_roots::*;
