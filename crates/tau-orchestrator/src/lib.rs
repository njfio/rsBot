//! Planning/orchestration and multi-agent runtime components for Tau.
//!
//! Provides planner/executor orchestration, routed multi-agent collaboration,
//! and contract fixtures for orchestrated prompt execution.

pub mod multi_agent_contract;
pub mod multi_agent_router;
pub mod multi_agent_runtime;
pub mod orchestrator;

pub use multi_agent_contract::*;
pub use multi_agent_router::*;
pub use multi_agent_runtime::*;
pub use orchestrator::*;
