//! Onboarding and startup bootstrap orchestration for Tau.
//!
//! Implements onboarding flows, startup configuration resolution, model/runtime
//! dispatch, and transport-mode bootstrap helpers.

pub mod onboarding_command;
pub mod onboarding_daemon;
pub mod onboarding_paths;
pub mod onboarding_profile_bootstrap;
pub mod onboarding_release_channel;
pub mod onboarding_report;
pub mod onboarding_wizard;
pub mod profile_commands;
pub mod profile_store;
pub mod startup_config;
pub mod startup_daemon_preflight;
pub mod startup_dispatch;
pub mod startup_local_runtime;
pub mod startup_model_resolution;
pub mod startup_policy;
pub mod startup_preflight;
pub mod startup_prompt_composition;
pub mod startup_resolution;
pub mod startup_skills_bootstrap;
pub mod startup_transport_modes;
