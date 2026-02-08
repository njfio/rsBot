use std::path::Path;

use super::*;

pub(crate) fn compose_startup_system_prompt(cli: &Cli, skills_dir: &Path) -> Result<String> {
    let base_system_prompt = resolve_system_prompt(cli)?;
    let catalog = load_catalog(skills_dir)
        .with_context(|| format!("failed to load skills from {}", skills_dir.display()))?;
    let selected_skills = resolve_selected_skills(&catalog, &cli.skills)?;
    Ok(augment_system_prompt(&base_system_prompt, &selected_skills))
}
