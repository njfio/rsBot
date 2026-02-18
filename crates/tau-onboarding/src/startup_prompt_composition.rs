//! Startup prompt composition from resolved system prompt and identity artifacts.
//!
//! This module composes deterministic prompt sections (identity files, selected
//! skills, and optional bootstrap context) after startup resolution. Composition
//! failures are reported with file-path context for operator debugging.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use tau_cli::Cli;
use tau_skills::{augment_system_prompt, load_catalog, resolve_selected_skills, Skill};

use crate::onboarding_paths::resolve_tau_root;
use crate::startup_resolution::resolve_system_prompt;

const STARTUP_IDENTITY_SCHEMA_VERSION: u32 = 1;
const STARTUP_IDENTITY_PROMPT_HEADER: &str = "## Tau Startup Identity Files";
const STARTUP_IDENTITY_PROMPT_PREAMBLE: &str =
    "Resolved from `.tau` during startup and composed deterministically.";
const STARTUP_SYSTEM_PROMPT_TEMPLATE_PATH: &str = "prompts/system.md.j2";
const STARTUP_BUILTIN_SYSTEM_PROMPT_TEMPLATE: &str =
    include_str!("../templates/startup-system.md.j2");
const STARTUP_IDENTITY_SPECS: &[(&str, &str)] = &[
    ("soul", "SOUL.md"),
    ("agents", "AGENTS.md"),
    ("user", "USER.md"),
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `StartupIdentityFileStatus` values.
pub enum StartupIdentityFileStatus {
    Loaded,
    Missing,
    Empty,
    ReadError,
    InvalidType,
}

impl StartupIdentityFileStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Loaded => "loaded",
            Self::Missing => "missing",
            Self::Empty => "empty",
            Self::ReadError => "read_error",
            Self::InvalidType => "invalid_type",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `StartupIdentityFileReportEntry` used across Tau components.
pub struct StartupIdentityFileReportEntry {
    pub key: String,
    pub file_name: String,
    pub path: String,
    pub status: StartupIdentityFileStatus,
    pub reason_code: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `StartupIdentityCompositionReport` used across Tau components.
pub struct StartupIdentityCompositionReport {
    pub schema_version: u32,
    pub tau_root: String,
    pub loaded_count: usize,
    pub missing_count: usize,
    pub skipped_count: usize,
    pub entries: Vec<StartupIdentityFileReportEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `StartupPromptComposition` used across Tau components.
pub struct StartupPromptComposition {
    pub system_prompt: String,
    pub identity_report: StartupIdentityCompositionReport,
    pub template_report: StartupPromptTemplateReport,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `StartupPromptTemplateSource` values.
pub enum StartupPromptTemplateSource {
    Workspace,
    BuiltIn,
    DefaultFallback,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `StartupPromptTemplateReport` used across Tau components.
pub struct StartupPromptTemplateReport {
    pub source: StartupPromptTemplateSource,
    pub template_path: Option<String>,
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LoadedIdentitySection {
    file_name: String,
    path: String,
    content: String,
}

pub fn compose_startup_system_prompt(cli: &Cli, skills_dir: &Path) -> Result<String> {
    Ok(compose_startup_system_prompt_with_report(cli, skills_dir)?.system_prompt)
}

pub fn compose_startup_system_prompt_with_report(
    cli: &Cli,
    skills_dir: &Path,
) -> Result<StartupPromptComposition> {
    let base_system_prompt = resolve_system_prompt(cli)?;
    let catalog = load_catalog(skills_dir)
        .with_context(|| format!("failed to load skills from {}", skills_dir.display()))?;
    let selected_skills = resolve_selected_skills(&catalog, &cli.skills)?;
    let (identity_report, sections) = resolve_startup_identity_report_with_sections(cli);
    let default_system_prompt =
        compose_default_system_prompt(&base_system_prompt, &selected_skills, &sections);
    let (system_prompt, template_report) = render_template_with_report(
        cli,
        &base_system_prompt,
        &selected_skills,
        &sections,
        &default_system_prompt,
    );

    Ok(StartupPromptComposition {
        system_prompt,
        identity_report,
        template_report,
    })
}

pub fn resolve_startup_identity_report(cli: &Cli) -> StartupIdentityCompositionReport {
    resolve_startup_identity_report_with_sections(cli).0
}

fn resolve_startup_identity_report_with_sections(
    cli: &Cli,
) -> (StartupIdentityCompositionReport, Vec<LoadedIdentitySection>) {
    let tau_root = resolve_tau_root(cli);
    let mut entries = Vec::with_capacity(STARTUP_IDENTITY_SPECS.len());
    let mut loaded_sections = Vec::new();

    for (key, file_name) in STARTUP_IDENTITY_SPECS {
        let path = tau_root.join(file_name);
        let path_text = path.display().to_string();
        let metadata = std::fs::metadata(&path);
        let entry = match metadata {
            Ok(metadata) if !metadata.is_file() => StartupIdentityFileReportEntry {
                key: (*key).to_string(),
                file_name: (*file_name).to_string(),
                path: path_text,
                status: StartupIdentityFileStatus::InvalidType,
                reason_code: "identity_file_not_regular".to_string(),
                bytes: 0,
            },
            Ok(_) => {
                read_identity_file_entry(key, file_name, path, path_text, &mut loaded_sections)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                StartupIdentityFileReportEntry {
                    key: (*key).to_string(),
                    file_name: (*file_name).to_string(),
                    path: path_text,
                    status: StartupIdentityFileStatus::Missing,
                    reason_code: "identity_file_missing".to_string(),
                    bytes: 0,
                }
            }
            Err(_) => StartupIdentityFileReportEntry {
                key: (*key).to_string(),
                file_name: (*file_name).to_string(),
                path: path_text,
                status: StartupIdentityFileStatus::ReadError,
                reason_code: "identity_file_metadata_read_failed".to_string(),
                bytes: 0,
            },
        };
        entries.push(entry);
    }

    let loaded_count = entries
        .iter()
        .filter(|entry| entry.status == StartupIdentityFileStatus::Loaded)
        .count();
    let missing_count = entries
        .iter()
        .filter(|entry| entry.status == StartupIdentityFileStatus::Missing)
        .count();
    let skipped_count = entries.len().saturating_sub(loaded_count + missing_count);
    let report = StartupIdentityCompositionReport {
        schema_version: STARTUP_IDENTITY_SCHEMA_VERSION,
        tau_root: tau_root.display().to_string(),
        loaded_count,
        missing_count,
        skipped_count,
        entries,
    };
    (report, loaded_sections)
}

fn read_identity_file_entry(
    key: &str,
    file_name: &str,
    path: PathBuf,
    path_text: String,
    loaded_sections: &mut Vec<LoadedIdentitySection>,
) -> StartupIdentityFileReportEntry {
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let bytes = content.len();
            if content.trim().is_empty() {
                StartupIdentityFileReportEntry {
                    key: key.to_string(),
                    file_name: file_name.to_string(),
                    path: path_text,
                    status: StartupIdentityFileStatus::Empty,
                    reason_code: "identity_file_empty".to_string(),
                    bytes,
                }
            } else {
                loaded_sections.push(LoadedIdentitySection {
                    file_name: file_name.to_string(),
                    path: path.display().to_string(),
                    content: content.trim_end().to_string(),
                });
                StartupIdentityFileReportEntry {
                    key: key.to_string(),
                    file_name: file_name.to_string(),
                    path: path_text,
                    status: StartupIdentityFileStatus::Loaded,
                    reason_code: "identity_file_loaded".to_string(),
                    bytes,
                }
            }
        }
        Err(_) => StartupIdentityFileReportEntry {
            key: key.to_string(),
            file_name: file_name.to_string(),
            path: path_text,
            status: StartupIdentityFileStatus::ReadError,
            reason_code: "identity_file_read_failed".to_string(),
            bytes: 0,
        },
    }
}

fn compose_default_system_prompt(
    base_system_prompt: &str,
    selected_skills: &[Skill],
    loaded_sections: &[LoadedIdentitySection],
) -> String {
    let mut system_prompt = augment_system_prompt(base_system_prompt, selected_skills);
    append_identity_sections(&mut system_prompt, loaded_sections);
    system_prompt
}

fn render_template_with_report(
    cli: &Cli,
    base_system_prompt: &str,
    selected_skills: &[Skill],
    loaded_sections: &[LoadedIdentitySection],
    default_system_prompt: &str,
) -> (String, StartupPromptTemplateReport) {
    let template_path = resolve_tau_root(cli).join(STARTUP_SYSTEM_PROMPT_TEMPLATE_PATH);
    let skills_section = render_skills_section(selected_skills);
    let identity_sections = build_identity_sections_text(loaded_sections);

    match load_workspace_template(&template_path) {
        WorkspaceTemplateLoad::Loaded(template) => match render_prompt_template(
            &template,
            base_system_prompt,
            &skills_section,
            &identity_sections,
            default_system_prompt,
        ) {
            Ok(rendered) if !rendered.trim().is_empty() => (
                rendered,
                StartupPromptTemplateReport {
                    source: StartupPromptTemplateSource::Workspace,
                    template_path: Some(template_path.display().to_string()),
                    reason_code: "workspace_template_rendered".to_string(),
                },
            ),
            Ok(_) | Err(_) => render_builtin_or_default(
                base_system_prompt,
                &skills_section,
                &identity_sections,
                default_system_prompt,
                "workspace_template_render_failed_fallback_builtin",
            ),
        },
        WorkspaceTemplateLoad::Missing => render_builtin_or_default(
            base_system_prompt,
            &skills_section,
            &identity_sections,
            default_system_prompt,
            "workspace_template_missing_fallback_builtin",
        ),
        WorkspaceTemplateLoad::Empty => render_builtin_or_default(
            base_system_prompt,
            &skills_section,
            &identity_sections,
            default_system_prompt,
            "workspace_template_empty_fallback_builtin",
        ),
        WorkspaceTemplateLoad::InvalidType => render_builtin_or_default(
            base_system_prompt,
            &skills_section,
            &identity_sections,
            default_system_prompt,
            "workspace_template_not_regular_fallback_builtin",
        ),
        WorkspaceTemplateLoad::ReadError => render_builtin_or_default(
            base_system_prompt,
            &skills_section,
            &identity_sections,
            default_system_prompt,
            "workspace_template_read_failed_fallback_builtin",
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WorkspaceTemplateLoad {
    Loaded(String),
    Missing,
    Empty,
    InvalidType,
    ReadError,
}

fn load_workspace_template(template_path: &Path) -> WorkspaceTemplateLoad {
    match std::fs::metadata(template_path) {
        Ok(metadata) if !metadata.is_file() => WorkspaceTemplateLoad::InvalidType,
        Ok(_) => match std::fs::read_to_string(template_path) {
            Ok(content) if content.trim().is_empty() => WorkspaceTemplateLoad::Empty,
            Ok(content) => WorkspaceTemplateLoad::Loaded(content),
            Err(_) => WorkspaceTemplateLoad::ReadError,
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            WorkspaceTemplateLoad::Missing
        }
        Err(_) => WorkspaceTemplateLoad::ReadError,
    }
}

fn render_builtin_or_default(
    base_system_prompt: &str,
    skills_section: &str,
    identity_sections: &str,
    default_system_prompt: &str,
    fallback_reason_code: &str,
) -> (String, StartupPromptTemplateReport) {
    if STARTUP_BUILTIN_SYSTEM_PROMPT_TEMPLATE.trim().is_empty() {
        return (
            default_system_prompt.to_string(),
            StartupPromptTemplateReport {
                source: StartupPromptTemplateSource::DefaultFallback,
                template_path: None,
                reason_code: "builtin_template_empty_fallback_default".to_string(),
            },
        );
    }

    match render_prompt_template(
        STARTUP_BUILTIN_SYSTEM_PROMPT_TEMPLATE,
        base_system_prompt,
        skills_section,
        identity_sections,
        default_system_prompt,
    ) {
        Ok(rendered) if !rendered.trim().is_empty() => (
            rendered,
            StartupPromptTemplateReport {
                source: StartupPromptTemplateSource::BuiltIn,
                template_path: None,
                reason_code: fallback_reason_code.to_string(),
            },
        ),
        Ok(_) | Err(_) => (
            default_system_prompt.to_string(),
            StartupPromptTemplateReport {
                source: StartupPromptTemplateSource::DefaultFallback,
                template_path: None,
                reason_code: "builtin_template_render_failed_fallback_default".to_string(),
            },
        ),
    }
}

fn render_skills_section(selected_skills: &[Skill]) -> String {
    augment_system_prompt("", selected_skills)
        .trim()
        .to_string()
}

fn build_identity_sections_text(loaded_sections: &[LoadedIdentitySection]) -> String {
    if loaded_sections.is_empty() {
        return String::new();
    }

    let mut identity_sections = String::new();
    identity_sections.push_str(STARTUP_IDENTITY_PROMPT_HEADER);
    identity_sections.push('\n');
    identity_sections.push_str(STARTUP_IDENTITY_PROMPT_PREAMBLE);

    for section in loaded_sections {
        identity_sections.push_str("\n\n### ");
        identity_sections.push_str(&section.file_name);
        identity_sections.push_str(" (");
        identity_sections.push_str(&section.path);
        identity_sections.push_str(")\n");
        identity_sections.push_str(&section.content);
    }

    identity_sections
}

fn render_prompt_template(
    template: &str,
    base_system_prompt: &str,
    skills_section: &str,
    identity_sections: &str,
    default_system_prompt: &str,
) -> Result<String> {
    let mut rendered = String::with_capacity(template.len());
    let mut cursor = 0usize;

    while let Some(open_offset) = template[cursor..].find("{{") {
        let open_index = cursor + open_offset;
        rendered.push_str(&template[cursor..open_index]);
        let close_offset = template[open_index + 2..]
            .find("}}")
            .ok_or_else(|| anyhow!("startup prompt template contains unterminated placeholder"))?;
        let close_index = open_index + 2 + close_offset;
        let placeholder = template[open_index + 2..close_index].trim();
        if placeholder.is_empty() {
            bail!("startup prompt template contains empty placeholder");
        }

        let value = match placeholder {
            "base_system_prompt" => base_system_prompt,
            "skills_section" => skills_section,
            "identity_sections" => identity_sections,
            "default_system_prompt" => default_system_prompt,
            _ => bail!(
                "startup prompt template placeholder '{}' is not supported",
                placeholder
            ),
        };
        rendered.push_str(value);
        cursor = close_index + 2;
    }

    rendered.push_str(&template[cursor..]);
    Ok(rendered)
}

fn append_identity_sections(system_prompt: &mut String, loaded_sections: &[LoadedIdentitySection]) {
    let identity_sections = build_identity_sections_text(loaded_sections);
    if identity_sections.is_empty() {
        return;
    }

    system_prompt.push_str("\n\n");
    system_prompt.push_str(&identity_sections);
}

#[cfg(test)]
mod tests {
    use super::{
        compose_startup_system_prompt_with_report, render_prompt_template,
        resolve_startup_identity_report, StartupIdentityFileStatus, StartupPromptTemplateSource,
    };
    use clap::Parser;
    use std::path::Path;
    use tau_cli::Cli;
    use tempfile::tempdir;

    fn parse_cli_with_stack() -> Cli {
        std::thread::Builder::new()
            .name("tau-cli-parse".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(|| Cli::parse_from(["tau-rs"]))
            .expect("spawn cli parse thread")
            .join()
            .expect("join cli parse thread")
    }

    fn apply_workspace_paths(cli: &mut Cli, workspace: &Path) {
        let tau_root = workspace.join(".tau");
        cli.session = tau_root.join("sessions/default.sqlite");
        cli.credential_store = tau_root.join("credentials.json");
        cli.skills_dir = tau_root.join("skills");
        std::fs::create_dir_all(&cli.skills_dir).expect("create skills dir");
    }

    #[test]
    fn unit_resolve_startup_identity_report_marks_missing_files() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());

        let report = resolve_startup_identity_report(&cli);
        assert_eq!(report.loaded_count, 0);
        assert_eq!(report.missing_count, 3);
        assert_eq!(report.skipped_count, 0);
        assert_eq!(report.entries.len(), 3);
        assert!(report
            .entries
            .iter()
            .all(|entry| entry.status == StartupIdentityFileStatus::Missing));
    }

    #[test]
    fn functional_compose_startup_system_prompt_with_report_includes_identity_sections_in_order() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        cli.system_prompt = "base prompt".to_string();
        let tau_root = temp.path().join(".tau");
        std::fs::create_dir_all(&tau_root).expect("create tau root");
        std::fs::write(tau_root.join("SOUL.md"), "Soul rules").expect("write SOUL");
        std::fs::write(tau_root.join("AGENTS.md"), "Agent constraints").expect("write AGENTS");
        std::fs::write(tau_root.join("USER.md"), "User preferences").expect("write USER");

        let composition =
            compose_startup_system_prompt_with_report(&cli, &cli.skills_dir).expect("compose");
        assert!(composition.system_prompt.contains("base prompt"));
        assert!(composition
            .system_prompt
            .contains("## Tau Startup Identity Files"));
        assert!(composition.system_prompt.contains("Soul rules"));
        assert!(composition.system_prompt.contains("Agent constraints"));
        assert!(composition.system_prompt.contains("User preferences"));
        let soul_index = composition
            .system_prompt
            .find("### SOUL.md")
            .expect("SOUL section");
        let agents_index = composition
            .system_prompt
            .find("### AGENTS.md")
            .expect("AGENTS section");
        let user_index = composition
            .system_prompt
            .find("### USER.md")
            .expect("USER section");
        assert!(soul_index < agents_index && agents_index < user_index);
        assert_eq!(composition.identity_report.loaded_count, 3);
        assert_eq!(composition.identity_report.missing_count, 0);
        assert_eq!(composition.identity_report.skipped_count, 0);
    }

    #[test]
    fn integration_compose_startup_system_prompt_with_report_keeps_skill_augmentation() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        cli.system_prompt = "base prompt".to_string();
        cli.skills = vec!["checks".to_string()];
        std::fs::write(
            cli.skills_dir.join("checks.md"),
            "Always run tests before reporting completion.",
        )
        .expect("write skill");
        let tau_root = temp.path().join(".tau");
        std::fs::write(
            tau_root.join("SOUL.md"),
            "Preserve deterministic behavior for startup composition.",
        )
        .expect("write SOUL");

        let composition =
            compose_startup_system_prompt_with_report(&cli, &cli.skills_dir).expect("compose");
        assert!(composition.system_prompt.contains("base prompt"));
        assert!(composition
            .system_prompt
            .contains("Always run tests before reporting completion."));
        assert!(composition
            .system_prompt
            .contains("Preserve deterministic behavior for startup composition."));
        assert_eq!(composition.identity_report.loaded_count, 1);
        assert_eq!(composition.identity_report.missing_count, 2);
    }

    #[test]
    fn integration_spec_2471_c01_compose_startup_system_prompt_renders_workspace_template_placeholders(
    ) {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        cli.system_prompt = "base prompt".to_string();
        cli.skills = vec!["checks".to_string()];

        std::fs::write(
            cli.skills_dir.join("checks.md"),
            "Always run tests before reporting completion.",
        )
        .expect("write skill");

        let tau_root = temp.path().join(".tau");
        std::fs::create_dir_all(tau_root.join("prompts")).expect("create prompts dir");
        std::fs::write(tau_root.join("SOUL.md"), "Soul section").expect("write SOUL");
        std::fs::write(
            tau_root.join("prompts/system.md.j2"),
            "TEMPLATE\nbase={{base_system_prompt}}\nskills={{skills_section}}\nidentity={{identity_sections}}\ndefault={{default_system_prompt}}\n",
        )
        .expect("write template");

        let composition =
            compose_startup_system_prompt_with_report(&cli, &cli.skills_dir).expect("compose");
        assert!(
            composition
                .system_prompt
                .starts_with("TEMPLATE\nbase=base prompt"),
            "template output should drive composition"
        );
        assert!(composition.system_prompt.contains("skills=# Skill: checks"));
        assert!(composition
            .system_prompt
            .contains("identity=## Tau Startup Identity Files"));
        assert!(composition.system_prompt.contains("default=base prompt"));
        assert!(!composition.system_prompt.contains("}}"));
    }

    #[test]
    fn regression_spec_2471_c02_compose_startup_system_prompt_without_template_preserves_legacy_composition(
    ) {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        cli.system_prompt = "base prompt".to_string();
        cli.skills = vec!["checks".to_string()];

        std::fs::write(
            cli.skills_dir.join("checks.md"),
            "Always run tests before reporting completion.",
        )
        .expect("write skill");
        let tau_root = temp.path().join(".tau");
        std::fs::write(tau_root.join("SOUL.md"), "Soul section").expect("write SOUL");

        let composition =
            compose_startup_system_prompt_with_report(&cli, &cli.skills_dir).expect("compose");
        assert!(composition.system_prompt.contains("base prompt"));
        assert!(composition.system_prompt.contains("# Skill: checks"));
        assert!(composition
            .system_prompt
            .contains("## Tau Startup Identity Files"));
        assert!(composition.system_prompt.contains("Soul section"));
    }

    #[test]
    fn unit_spec_2471_render_prompt_template_replaces_all_placeholders_without_residue() {
        let rendered = render_prompt_template(
            "A={{base_system_prompt}};B={{skills_section}};C={{identity_sections}};D={{default_system_prompt}}",
            "base",
            "skills",
            "identity",
            "default",
        )
        .expect("render");

        assert_eq!(rendered, "A=base;B=skills;C=identity;D=default");
    }

    #[test]
    fn regression_spec_2471_c03_compose_startup_system_prompt_invalid_template_placeholder_falls_back_to_default(
    ) {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        cli.system_prompt = "base prompt".to_string();

        let tau_root = temp.path().join(".tau");
        std::fs::create_dir_all(tau_root.join("prompts")).expect("create prompts dir");
        std::fs::write(tau_root.join("SOUL.md"), "Soul section").expect("write SOUL");
        std::fs::write(
            tau_root.join("prompts/system.md.j2"),
            "BROKEN TEMPLATE {{unknown_placeholder}}",
        )
        .expect("write template");

        let composition =
            compose_startup_system_prompt_with_report(&cli, &cli.skills_dir).expect("compose");
        assert!(!composition.system_prompt.contains("BROKEN TEMPLATE"));
        assert!(composition.system_prompt.contains("base prompt"));
        assert!(composition
            .system_prompt
            .contains("## Tau Startup Identity Files"));
        assert!(composition.system_prompt.contains("Soul section"));
    }

    #[test]
    fn integration_spec_2476_c01_compose_startup_system_prompt_reports_workspace_template_source() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        cli.system_prompt = "base prompt".to_string();

        let tau_root = temp.path().join(".tau");
        std::fs::create_dir_all(tau_root.join("prompts")).expect("create prompts dir");
        std::fs::write(tau_root.join("SOUL.md"), "Soul section").expect("write SOUL");
        std::fs::write(
            tau_root.join("prompts/system.md.j2"),
            "W={{base_system_prompt}}|{{identity_sections}}",
        )
        .expect("write template");

        let composition =
            compose_startup_system_prompt_with_report(&cli, &cli.skills_dir).expect("compose");
        assert_eq!(
            composition.template_report.source,
            StartupPromptTemplateSource::Workspace
        );
        assert_eq!(
            composition.template_report.reason_code,
            "workspace_template_rendered"
        );
        assert_eq!(
            composition.template_report.template_path.as_deref(),
            Some(
                tau_root
                    .join("prompts/system.md.j2")
                    .to_string_lossy()
                    .as_ref()
            )
        );
    }

    #[test]
    fn integration_spec_2476_c02_compose_startup_system_prompt_without_workspace_template_uses_builtin_source(
    ) {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        cli.system_prompt = "base prompt".to_string();
        std::fs::write(temp.path().join(".tau/SOUL.md"), "Soul section").expect("write SOUL");

        let composition =
            compose_startup_system_prompt_with_report(&cli, &cli.skills_dir).expect("compose");
        assert_eq!(
            composition.template_report.source,
            StartupPromptTemplateSource::BuiltIn
        );
        assert_eq!(
            composition.template_report.reason_code,
            "workspace_template_missing_fallback_builtin"
        );
        assert_eq!(composition.template_report.template_path, None);
        assert!(composition.system_prompt.contains("base prompt"));
        assert!(composition.system_prompt.contains("Soul section"));
    }

    #[test]
    fn regression_spec_2476_c03_invalid_workspace_template_falls_back_to_builtin_source() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        cli.system_prompt = "base prompt".to_string();

        let tau_root = temp.path().join(".tau");
        std::fs::create_dir_all(tau_root.join("prompts")).expect("create prompts dir");
        std::fs::write(tau_root.join("SOUL.md"), "Soul section").expect("write SOUL");
        std::fs::write(
            tau_root.join("prompts/system.md.j2"),
            "BROKEN TEMPLATE {{unknown_placeholder}}",
        )
        .expect("write template");

        let composition =
            compose_startup_system_prompt_with_report(&cli, &cli.skills_dir).expect("compose");
        assert_eq!(
            composition.template_report.source,
            StartupPromptTemplateSource::BuiltIn
        );
        assert_eq!(
            composition.template_report.reason_code,
            "workspace_template_render_failed_fallback_builtin"
        );
        assert!(!composition.system_prompt.contains("BROKEN TEMPLATE"));
        assert!(composition.system_prompt.contains("base prompt"));
        assert!(composition.system_prompt.contains("Soul section"));
    }

    #[test]
    fn regression_template_source_workspace_rendered_empty_falls_back_to_builtin() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        cli.system_prompt = "base prompt".to_string();

        let tau_root = temp.path().join(".tau");
        std::fs::create_dir_all(tau_root.join("prompts")).expect("create prompts dir");
        std::fs::write(tau_root.join("prompts/system.md.j2"), "{{skills_section}}")
            .expect("write template");

        let composition =
            compose_startup_system_prompt_with_report(&cli, &cli.skills_dir).expect("compose");
        assert_eq!(
            composition.template_report.source,
            StartupPromptTemplateSource::BuiltIn
        );
        assert_eq!(
            composition.template_report.reason_code,
            "workspace_template_render_failed_fallback_builtin"
        );
        assert_eq!(composition.system_prompt, "base prompt\n");
    }

    #[test]
    fn regression_template_source_workspace_empty_file_uses_empty_reason_code() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        cli.system_prompt = "base prompt".to_string();

        let tau_root = temp.path().join(".tau");
        std::fs::create_dir_all(tau_root.join("prompts")).expect("create prompts dir");
        std::fs::write(tau_root.join("prompts/system.md.j2"), " \n\t").expect("write template");

        let composition =
            compose_startup_system_prompt_with_report(&cli, &cli.skills_dir).expect("compose");
        assert_eq!(
            composition.template_report.source,
            StartupPromptTemplateSource::BuiltIn
        );
        assert_eq!(
            composition.template_report.reason_code,
            "workspace_template_empty_fallback_builtin"
        );
    }

    #[test]
    fn regression_template_source_workspace_not_regular_uses_invalid_type_reason_code() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        cli.system_prompt = "base prompt".to_string();

        let tau_root = temp.path().join(".tau");
        std::fs::create_dir_all(tau_root.join("prompts/system.md.j2"))
            .expect("create invalid template path as directory");

        let composition =
            compose_startup_system_prompt_with_report(&cli, &cli.skills_dir).expect("compose");
        assert_eq!(
            composition.template_report.source,
            StartupPromptTemplateSource::BuiltIn
        );
        assert_eq!(
            composition.template_report.reason_code,
            "workspace_template_not_regular_fallback_builtin"
        );
    }

    #[test]
    fn regression_template_source_workspace_metadata_read_error_is_not_missing() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        cli.system_prompt = "base prompt".to_string();

        let tau_root = temp.path().join(".tau");
        std::fs::write(tau_root.join("prompts"), "not a directory").expect("write prompts file");

        let composition =
            compose_startup_system_prompt_with_report(&cli, &cli.skills_dir).expect("compose");
        assert_eq!(
            composition.template_report.source,
            StartupPromptTemplateSource::BuiltIn
        );
        assert_eq!(
            composition.template_report.reason_code,
            "workspace_template_read_failed_fallback_builtin"
        );
    }

    #[test]
    fn regression_template_source_builtin_empty_render_falls_back_to_default() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        cli.system_prompt = "".to_string();

        let composition =
            compose_startup_system_prompt_with_report(&cli, &cli.skills_dir).expect("compose");
        assert_eq!(
            composition.template_report.source,
            StartupPromptTemplateSource::DefaultFallback
        );
        assert_eq!(
            composition.template_report.reason_code,
            "builtin_template_render_failed_fallback_default"
        );
        assert_eq!(composition.system_prompt, "");
    }

    #[test]
    fn regression_resolve_startup_identity_report_marks_invalid_type_entries() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        apply_workspace_paths(&mut cli, temp.path());
        let tau_root = temp.path().join(".tau");
        std::fs::create_dir_all(tau_root.join("SOUL.md")).expect("create invalid SOUL path");

        let report = resolve_startup_identity_report(&cli);
        let soul = report
            .entries
            .iter()
            .find(|entry| entry.key == "soul")
            .expect("soul entry");
        assert_eq!(soul.status, StartupIdentityFileStatus::InvalidType);
        assert_eq!(soul.reason_code, "identity_file_not_regular");
    }
}
