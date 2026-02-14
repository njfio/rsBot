//! Voice contract fixtures and runtime logic for Tau.
//!
//! Provides voice transport contract replay and runtime helpers used by voice
//! interaction and wake-word related integrations.

#[cfg(feature = "runtime")]
pub mod voice_contract;
pub mod voice_provider;
#[cfg(feature = "runtime")]
pub mod voice_runtime;
