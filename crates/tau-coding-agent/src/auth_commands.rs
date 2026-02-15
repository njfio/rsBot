//! Auth command facade for provider/login workflows.
//!
//! Re-exports provider auth command APIs consumed by coding-agent command
//! dispatch to preserve one canonical auth contract surface.

pub(crate) use tau_provider::*;
