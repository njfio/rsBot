//! Release-channel command facade for coding-agent CLI flows.
//!
//! Re-exports release-channel command APIs so command dispatch uses a canonical
//! channel update/read contract.

pub(crate) use tau_release_channel::{
    default_release_channel_path, execute_release_channel_command,
};
