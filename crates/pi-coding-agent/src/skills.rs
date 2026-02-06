use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub content: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillInstallReport {
    pub installed: usize,
    pub updated: usize,
    pub skipped: usize,
}

pub fn load_catalog(dir: &Path) -> Result<Vec<Skill>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    if !dir.is_dir() {
        bail!("skills path '{}' is not a directory", dir.display());
    }

    let mut skills = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read skill file {}", path.display()))?;
        skills.push(Skill {
            name: stem.to_string(),
            content,
            path,
        });
    }

    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(skills)
}

pub fn install_skills(sources: &[PathBuf], destination_dir: &Path) -> Result<SkillInstallReport> {
    if sources.is_empty() {
        return Ok(SkillInstallReport::default());
    }

    fs::create_dir_all(destination_dir)
        .with_context(|| format!("failed to create {}", destination_dir.display()))?;

    let mut report = SkillInstallReport::default();
    for source in sources {
        if source.extension().and_then(|ext| ext.to_str()) != Some("md") {
            bail!("skill source '{}' must be a .md file", source.display());
        }

        let file_name = source
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("invalid skill source '{}'", source.display()))?;
        let destination = destination_dir.join(file_name);

        let content = fs::read_to_string(source)
            .with_context(|| format!("failed to read skill source {}", source.display()))?;

        if destination.exists() {
            let existing = fs::read_to_string(&destination).with_context(|| {
                format!("failed to read installed skill {}", destination.display())
            })?;
            if existing == content {
                report.skipped += 1;
                continue;
            }

            fs::write(&destination, content.as_bytes())
                .with_context(|| format!("failed to update skill {}", destination.display()))?;
            report.updated += 1;
            continue;
        }

        fs::write(&destination, content.as_bytes())
            .with_context(|| format!("failed to install skill {}", destination.display()))?;
        report.installed += 1;
    }

    Ok(report)
}

pub fn resolve_selected_skills(catalog: &[Skill], selected: &[String]) -> Result<Vec<Skill>> {
    let mut resolved = Vec::new();
    for name in selected {
        let skill = catalog
            .iter()
            .find(|skill| skill.name == *name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown skill '{}'", name))?;
        resolved.push(skill);
    }

    Ok(resolved)
}

pub fn augment_system_prompt(base: &str, skills: &[Skill]) -> String {
    let mut prompt = base.trim_end().to_string();
    for skill in skills {
        if !prompt.is_empty() {
            prompt.push_str("\n\n");
        }

        prompt.push_str("# Skill: ");
        prompt.push_str(&skill.name);
        prompt.push('\n');
        prompt.push_str(skill.content.trim());
    }

    prompt
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::{
        augment_system_prompt, install_skills, load_catalog, resolve_selected_skills, Skill,
        SkillInstallReport,
    };

    #[test]
    fn unit_load_catalog_reads_markdown_files_only() {
        let temp = tempdir().expect("tempdir");
        std::fs::write(temp.path().join("a.md"), "A").expect("write a");
        std::fs::write(temp.path().join("b.txt"), "B").expect("write b");
        std::fs::write(temp.path().join("c.md"), "C").expect("write c");

        let catalog = load_catalog(temp.path()).expect("catalog");
        let names = catalog
            .iter()
            .map(|skill| skill.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["a", "c"]);
    }

    #[test]
    fn functional_augment_system_prompt_preserves_selected_skill_order() {
        let skills = vec![
            Skill {
                name: "first".to_string(),
                content: "one".to_string(),
                path: "first.md".into(),
            },
            Skill {
                name: "second".to_string(),
                content: "two".to_string(),
                path: "second.md".into(),
            },
        ];

        let prompt = augment_system_prompt("base", &skills);
        assert!(prompt.contains("# Skill: first\none"));
        assert!(prompt.contains("# Skill: second\ntwo"));
        assert!(prompt.find("first").expect("first") < prompt.find("second").expect("second"));
    }

    #[test]
    fn regression_resolve_selected_skills_errors_on_unknown_skill() {
        let catalog = vec![Skill {
            name: "known".to_string(),
            content: "x".to_string(),
            path: "known.md".into(),
        }];

        let error = resolve_selected_skills(&catalog, &["missing".to_string()])
            .expect_err("unknown skill should fail");
        assert!(error.to_string().contains("unknown skill 'missing'"));
    }

    #[test]
    fn integration_load_and_resolve_selected_skills_roundtrip() {
        let temp = tempdir().expect("tempdir");
        std::fs::write(temp.path().join("alpha.md"), "alpha body").expect("write alpha");
        std::fs::write(temp.path().join("beta.md"), "beta body").expect("write beta");

        let catalog = load_catalog(temp.path()).expect("catalog");
        let selected =
            resolve_selected_skills(&catalog, &["beta".to_string(), "alpha".to_string()])
                .expect("resolve");
        assert_eq!(
            selected
                .iter()
                .map(|skill| skill.name.as_str())
                .collect::<Vec<_>>(),
            vec!["beta", "alpha"]
        );
    }

    #[test]
    fn unit_install_skills_copies_new_skill_files() {
        let temp = tempdir().expect("tempdir");
        let source = temp.path().join("source.md");
        std::fs::write(&source, "source").expect("write source");
        let install_dir = temp.path().join("skills");

        let report = install_skills(&[source], &install_dir).expect("install");
        assert_eq!(
            report,
            SkillInstallReport {
                installed: 1,
                updated: 0,
                skipped: 0
            }
        );
        assert_eq!(
            std::fs::read_to_string(install_dir.join("source.md")).expect("read installed"),
            "source"
        );
    }

    #[test]
    fn regression_install_skills_skips_when_content_unchanged() {
        let temp = tempdir().expect("tempdir");
        let install_dir = temp.path().join("skills");
        std::fs::create_dir_all(&install_dir).expect("mkdir");
        std::fs::write(install_dir.join("stable.md"), "same").expect("write installed");

        let source = temp.path().join("stable.md");
        std::fs::write(&source, "same").expect("write source");

        let report = install_skills(&[source], &install_dir).expect("install");
        assert_eq!(
            report,
            SkillInstallReport {
                installed: 0,
                updated: 0,
                skipped: 1
            }
        );
    }

    #[test]
    fn integration_install_skills_updates_existing_content() {
        let temp = tempdir().expect("tempdir");
        let install_dir = temp.path().join("skills");
        std::fs::create_dir_all(&install_dir).expect("mkdir");
        std::fs::write(install_dir.join("evolve.md"), "v1").expect("write installed");

        let source = temp.path().join("evolve.md");
        std::fs::write(&source, "v2").expect("write source");

        let report = install_skills(&[PathBuf::from(&source)], &install_dir).expect("install");
        assert_eq!(
            report,
            SkillInstallReport {
                installed: 0,
                updated: 1,
                skipped: 0
            }
        );
        assert_eq!(
            std::fs::read_to_string(install_dir.join("evolve.md")).expect("read installed"),
            "v2"
        );
    }
}
