//! Aggregates tooling + skills CLI integration coverage into domain-focused modules.

use super::*;

#[path = "tooling_skills/extensions_packages.rs"]
mod extensions_packages;
#[path = "tooling_skills/prompt_audit.rs"]
mod prompt_audit;
#[path = "tooling_skills/skills_cli.rs"]
mod skills_cli;
#[path = "tooling_skills/skills_registry_install.rs"]
mod skills_registry_install;
