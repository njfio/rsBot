//! Deployment contracts and runtime tooling for Tau artifacts.
//!
//! Includes deployment fixture replay plus WASM packaging/inspection flows used
//! for channel and control-plane deployment deliverables.

pub mod deployment_contract;
pub mod deployment_wasm;
pub mod deployment_wasm_identity;
pub mod fly_manifest_contract;

#[cfg(not(target_arch = "wasm32"))]
pub mod deployment_runtime;
#[cfg(not(target_arch = "wasm32"))]
pub mod deployment_wasm_runtime;
