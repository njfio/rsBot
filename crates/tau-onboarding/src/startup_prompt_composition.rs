use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tau_cli::Cli;
use tau_skills::{augment_system_prompt, load_catalog, resolve_selected_skills};

use crate::onboarding_paths::resolve_tau_root;
use crate::startup_resolution::resolve_system_prompt;

const STARTUP_IDENTITY_SCHEMA_VERSION: u32 = 1;
const STARTUP_IDENTITY_PROMPT_HEADER: &str = "## Tau Startup Identity Files";
const STARTUP_IDENTITY_PROMPT_PREAMBLE: &str =
    "Resolved from `.tau` during startup and composed deterministically.";
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
    let mut system_prompt = augment_system_prompt(&base_system_prompt, &selected_skills);

    let (identity_report, sections) = resolve_startup_identity_report_with_sections(cli);
    append_identity_sections(&mut system_prompt, &sections);

    Ok(StartupPromptComposition {
        system_prompt,
        identity_report,
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

fn append_identity_sections(system_prompt: &mut String, loaded_sections: &[LoadedIdentitySection]) {
    if loaded_sections.is_empty() {
        return;
    }

    system_prompt.push_str("\n\n");
    system_prompt.push_str(STARTUP_IDENTITY_PROMPT_HEADER);
    system_prompt.push('\n');
    system_prompt.push_str(STARTUP_IDENTITY_PROMPT_PREAMBLE);

    for section in loaded_sections {
        system_prompt.push_str("\n\n### ");
        system_prompt.push_str(&section.file_name);
        system_prompt.push_str(" (");
        system_prompt.push_str(&section.path);
        system_prompt.push_str(")\n");
        system_prompt.push_str(&section.content);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        compose_startup_system_prompt_with_report, resolve_startup_identity_report,
        StartupIdentityFileStatus,
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
