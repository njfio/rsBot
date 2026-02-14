use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{bail, Result};
use serde::Serialize;
use tau_core::{current_unix_timestamp, is_expired_unix};

use crate::trust_roots::{
    load_trust_root_records, parse_trust_rotation_spec, parse_trusted_root_spec,
    save_trust_root_records, TrustedRootRecord,
};
use crate::*;

/// Public struct `SkillsSearchMatch` used across Tau components.
pub struct SkillsSearchMatch {
    pub name: String,
    pub file: String,
    pub name_hit: bool,
    pub content_hit: bool,
}

pub fn parse_skills_search_args(command_args: &str) -> Result<(String, usize)> {
    const DEFAULT_MAX_RESULTS: usize = 20;
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("query is required");
    }

    let mut max_results = DEFAULT_MAX_RESULTS;
    let query_tokens = if let Some(last) = tokens.last() {
        match last.parse::<usize>() {
            Ok(parsed_limit) => {
                if parsed_limit == 0 {
                    bail!("max_results must be greater than zero");
                }
                max_results = parsed_limit;
                &tokens[..tokens.len() - 1]
            }
            Err(_) => &tokens[..],
        }
    } else {
        &tokens[..]
    };

    if query_tokens.is_empty() {
        bail!("query is required");
    }
    let query = query_tokens.join(" ");
    if query.trim().is_empty() {
        bail!("query is required");
    }

    Ok((query, max_results))
}

pub fn render_skills_search(
    skills_dir: &Path,
    query: &str,
    max_results: usize,
    matches: &[SkillsSearchMatch],
    total_matches: usize,
) -> String {
    let mut lines = vec![format!(
        "skills search: path={} query={:?} max_results={} matched={} shown={}",
        skills_dir.display(),
        query,
        max_results,
        total_matches,
        matches.len()
    )];
    if matches.is_empty() {
        lines.push("skills: none".to_string());
        return lines.join("\n");
    }

    for entry in matches {
        let match_kind = match (entry.name_hit, entry.content_hit) {
            (true, true) => "name+content",
            (true, false) => "name",
            (false, true) => "content",
            (false, false) => "unknown",
        };
        lines.push(format!(
            "skill: name={} file={} match={}",
            entry.name, entry.file, match_kind
        ));
    }
    lines.join("\n")
}

pub fn execute_skills_search_command(skills_dir: &Path, command_args: &str) -> String {
    let (query, max_results) = match parse_skills_search_args(command_args) {
        Ok(parsed) => parsed,
        Err(error) => {
            return format!(
                "skills search error: path={} args={:?} error={error}",
                skills_dir.display(),
                command_args
            )
        }
    };

    let catalog = match load_catalog(skills_dir) {
        Ok(catalog) => catalog,
        Err(error) => {
            return format!(
                "skills search error: path={} query={:?} error={error}",
                skills_dir.display(),
                query
            )
        }
    };

    let query_lower = query.to_ascii_lowercase();
    let mut matches = Vec::new();
    for skill in catalog {
        let name_hit = skill.name.to_ascii_lowercase().contains(&query_lower);
        let content_hit = skill.content.to_ascii_lowercase().contains(&query_lower);
        if !(name_hit || content_hit) {
            continue;
        }
        let file = skill
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();
        matches.push(SkillsSearchMatch {
            name: skill.name,
            file,
            name_hit,
            content_hit,
        });
    }

    matches.sort_by(|left, right| {
        right
            .name_hit
            .cmp(&left.name_hit)
            .then_with(|| left.name.cmp(&right.name))
    });
    let total_matches = matches.len();
    matches.truncate(max_results);

    render_skills_search(skills_dir, &query, max_results, &matches, total_matches)
}

pub fn render_skills_show(skills_dir: &Path, skill: &Skill) -> String {
    let file = skill
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    format!(
        "skills show: path={} name={} file={} content_bytes={}\n---\n{}",
        skills_dir.display(),
        skill.name,
        file,
        skill.content.len(),
        skill.content
    )
}

pub fn execute_skills_show_command(skills_dir: &Path, skill_name: &str) -> String {
    match load_catalog(skills_dir) {
        Ok(catalog) => match catalog.into_iter().find(|skill| skill.name == skill_name) {
            Some(skill) => render_skills_show(skills_dir, &skill),
            None => format!(
                "skills show error: path={} name={} error=unknown skill '{}'",
                skills_dir.display(),
                skill_name,
                skill_name
            ),
        },
        Err(error) => format!(
            "skills show error: path={} name={} error={error}",
            skills_dir.display(),
            skill_name
        ),
    }
}

pub fn render_skills_list(skills_dir: &Path, catalog: &[Skill]) -> String {
    let mut lines = vec![format!(
        "skills list: path={} count={}",
        skills_dir.display(),
        catalog.len()
    )];
    if catalog.is_empty() {
        lines.push("skills: none".to_string());
    } else {
        for skill in catalog {
            let file = skill
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown");
            lines.push(format!("skill: name={} file={}", skill.name, file));
        }
    }
    lines.join("\n")
}

pub fn execute_skills_list_command(skills_dir: &Path) -> String {
    match load_catalog(skills_dir) {
        Ok(catalog) => render_skills_list(skills_dir, &catalog),
        Err(error) => format!(
            "skills list error: path={} error={error}",
            skills_dir.display()
        ),
    }
}

pub fn resolve_skills_lock_path(command_args: &str, default_lock_path: &Path) -> PathBuf {
    if command_args.is_empty() {
        default_lock_path.to_path_buf()
    } else {
        PathBuf::from(command_args)
    }
}

pub fn parse_skills_lock_diff_args(
    command_args: &str,
    default_lock_path: &Path,
) -> Result<(PathBuf, bool)> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Ok((default_lock_path.to_path_buf(), false));
    }

    let mut lock_path: Option<PathBuf> = None;
    let mut json_output = false;
    for token in tokens {
        if token == "--json" {
            json_output = true;
            continue;
        }

        if lock_path.is_some() {
            bail!(
                "unexpected argument '{}'; usage: /skills-lock-diff [lockfile_path] [--json]",
                token
            );
        }
        lock_path = Some(PathBuf::from(token));
    }

    Ok((
        lock_path.unwrap_or_else(|| default_lock_path.to_path_buf()),
        json_output,
    ))
}

pub const SKILLS_PRUNE_USAGE: &str = "usage: /skills-prune [lockfile_path] [--dry-run|--apply]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `SkillsPruneMode` values.
pub enum SkillsPruneMode {
    DryRun,
    Apply,
}

impl SkillsPruneMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::DryRun => "dry-run",
            Self::Apply => "apply",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `SkillsPruneCandidate` used across Tau components.
pub struct SkillsPruneCandidate {
    pub file: String,
    pub path: PathBuf,
}

pub fn parse_skills_prune_args(
    command_args: &str,
    default_lock_path: &Path,
) -> Result<(PathBuf, SkillsPruneMode)> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Ok((default_lock_path.to_path_buf(), SkillsPruneMode::DryRun));
    }

    let mut lock_path: Option<PathBuf> = None;
    let mut mode = SkillsPruneMode::DryRun;
    let mut mode_flag_seen = false;
    for token in tokens {
        match token {
            "--dry-run" => {
                if mode_flag_seen && mode != SkillsPruneMode::DryRun {
                    bail!("conflicting flags '--dry-run' and '--apply'; {SKILLS_PRUNE_USAGE}");
                }
                mode = SkillsPruneMode::DryRun;
                mode_flag_seen = true;
            }
            "--apply" => {
                if mode_flag_seen && mode != SkillsPruneMode::Apply {
                    bail!("conflicting flags '--dry-run' and '--apply'; {SKILLS_PRUNE_USAGE}");
                }
                mode = SkillsPruneMode::Apply;
                mode_flag_seen = true;
            }
            _ => {
                if lock_path.is_some() {
                    bail!("unexpected argument '{}'; {SKILLS_PRUNE_USAGE}", token);
                }
                lock_path = Some(PathBuf::from(token));
            }
        }
    }

    Ok((
        lock_path.unwrap_or_else(|| default_lock_path.to_path_buf()),
        mode,
    ))
}

pub fn validate_skills_prune_file_name(file: &str) -> Result<()> {
    if file.contains('\\') {
        bail!(
            "unsafe lockfile entry '{}': path separators are not allowed",
            file
        );
    }

    let path = Path::new(file);
    if path.is_absolute() {
        bail!(
            "unsafe lockfile entry '{}': absolute paths are not allowed",
            file
        );
    }

    let mut components = path.components();
    let first = components.next();
    if components.next().is_some() {
        bail!(
            "unsafe lockfile entry '{}': nested paths are not allowed",
            file
        );
    }

    match first {
        Some(std::path::Component::Normal(component)) => {
            let Some(component) = component.to_str() else {
                bail!("unsafe lockfile entry '{}': path must be valid UTF-8", file);
            };
            if component.is_empty() {
                bail!("unsafe lockfile entry '{}': empty file name", file);
            }
        }
        _ => bail!("unsafe lockfile entry '{}': invalid path component", file),
    }

    if !file.ends_with(".md") {
        bail!(
            "unsafe lockfile entry '{}': only markdown files can be pruned",
            file
        );
    }

    Ok(())
}

pub fn resolve_prunable_skill_file_name(skills_dir: &Path, skill_path: &Path) -> Result<String> {
    let relative_path = skill_path.strip_prefix(skills_dir).with_context(|| {
        format!(
            "unsafe skill path '{}': outside skills dir '{}'",
            skill_path.display(),
            skills_dir.display()
        )
    })?;
    let mut components = relative_path.components();
    let first = components.next();
    if components.next().is_some() {
        bail!(
            "unsafe skill path '{}': nested paths are not allowed",
            skill_path.display()
        );
    }
    let Some(std::path::Component::Normal(file_os_str)) = first else {
        bail!(
            "unsafe skill path '{}': invalid file path component",
            skill_path.display()
        );
    };
    let Some(file) = file_os_str.to_str() else {
        bail!(
            "unsafe skill path '{}': file name must be valid UTF-8",
            skill_path.display()
        );
    };
    validate_skills_prune_file_name(file)?;
    Ok(file.to_string())
}

pub fn derive_skills_prune_candidates(
    skills_dir: &Path,
    catalog: &[Skill],
    tracked_files: &HashSet<String>,
) -> Result<Vec<SkillsPruneCandidate>> {
    let mut candidates = Vec::new();
    for skill in catalog {
        let file = resolve_prunable_skill_file_name(skills_dir, &skill.path)?;
        if tracked_files.contains(&file) {
            continue;
        }
        candidates.push(SkillsPruneCandidate {
            file,
            path: skill.path.clone(),
        });
    }
    candidates.sort_by(|left, right| left.file.cmp(&right.file));
    Ok(candidates)
}

pub fn execute_skills_prune_command(
    skills_dir: &Path,
    default_lock_path: &Path,
    command_args: &str,
) -> String {
    let (lock_path, mode) = match parse_skills_prune_args(command_args, default_lock_path) {
        Ok(parsed) => parsed,
        Err(error) => {
            return format!(
                "skills prune error: path={} mode={} error={error}",
                default_lock_path.display(),
                SkillsPruneMode::DryRun.as_str()
            )
        }
    };

    let lockfile = match load_skills_lockfile(&lock_path) {
        Ok(lockfile) => lockfile,
        Err(error) => {
            return format!(
                "skills prune error: path={} mode={} error={error}",
                lock_path.display(),
                mode.as_str()
            )
        }
    };

    let mut tracked_files = HashSet::new();
    for entry in &lockfile.entries {
        if let Err(error) = validate_skills_prune_file_name(&entry.file) {
            return format!(
                "skills prune error: path={} mode={} error={error}",
                lock_path.display(),
                mode.as_str()
            );
        }
        tracked_files.insert(entry.file.clone());
    }

    let catalog = match load_catalog(skills_dir) {
        Ok(catalog) => catalog,
        Err(error) => {
            return format!(
                "skills prune error: path={} mode={} error={error}",
                lock_path.display(),
                mode.as_str()
            )
        }
    };

    let candidates = match derive_skills_prune_candidates(skills_dir, &catalog, &tracked_files) {
        Ok(candidates) => candidates,
        Err(error) => {
            return format!(
                "skills prune error: path={} mode={} error={error}",
                lock_path.display(),
                mode.as_str()
            )
        }
    };

    let mut lines = vec![format!(
        "skills prune: mode={} lockfile={} skills_dir={} tracked_entries={} installed_skills={} prune_candidates={}",
        mode.as_str(),
        lock_path.display(),
        skills_dir.display(),
        tracked_files.len(),
        catalog.len(),
        candidates.len()
    )];

    if candidates.is_empty() {
        lines.push("prune: none".to_string());
        return lines.join("\n");
    }

    for candidate in &candidates {
        let action = match mode {
            SkillsPruneMode::DryRun => "would_delete",
            SkillsPruneMode::Apply => "delete",
        };
        lines.push(format!("prune: file={} action={action}", candidate.file));
    }

    if mode == SkillsPruneMode::DryRun {
        return lines.join("\n");
    }

    let mut deleted = 0usize;
    let mut failed = 0usize;
    for candidate in &candidates {
        match std::fs::remove_file(&candidate.path) {
            Ok(()) => {
                deleted += 1;
                lines.push(format!("prune: file={} status=deleted", candidate.file));
            }
            Err(error) => {
                failed += 1;
                lines.push(format!(
                    "prune: file={} status=error error={error}",
                    candidate.file
                ));
            }
        }
    }
    lines.push(format!(
        "skills prune result: mode=apply deleted={} failed={}",
        deleted, failed
    ));
    lines.join("\n")
}

pub const SKILLS_TRUST_LIST_USAGE: &str = "usage: /skills-trust-list [trust_root_file]";
pub const SKILLS_TRUST_ADD_USAGE: &str =
    "usage: /skills-trust-add <id=base64_key> [trust_root_file]";
pub const SKILLS_TRUST_REVOKE_USAGE: &str = "usage: /skills-trust-revoke <id> [trust_root_file]";
pub const SKILLS_TRUST_ROTATE_USAGE: &str =
    "usage: /skills-trust-rotate <old_id:new_id=base64_key> [trust_root_file]";

pub fn parse_skills_trust_mutation_args(
    command_args: &str,
    default_trust_root_path: Option<&Path>,
    usage: &str,
) -> Result<(String, PathBuf)> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("{usage}");
    }
    if tokens.len() > 2 {
        bail!("unexpected argument '{}'; {usage}", tokens[2]);
    }

    let trust_root_path = if tokens.len() == 2 {
        PathBuf::from(tokens[1])
    } else {
        default_trust_root_path
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow!("trust root file is required; {usage}"))?
    };
    Ok((tokens[0].to_string(), trust_root_path))
}

pub fn parse_skills_trust_list_args(
    command_args: &str,
    default_trust_root_path: Option<&Path>,
) -> Result<PathBuf> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return default_trust_root_path
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow!("trust root file is required; {SKILLS_TRUST_LIST_USAGE}"));
    }

    if tokens.len() > 1 {
        bail!(
            "unexpected argument '{}'; {SKILLS_TRUST_LIST_USAGE}",
            tokens[1]
        );
    }

    Ok(PathBuf::from(tokens[0]))
}

pub fn execute_skills_trust_add_command(
    default_trust_root_path: Option<&Path>,
    command_args: &str,
) -> String {
    let (spec, trust_root_path) = match parse_skills_trust_mutation_args(
        command_args,
        default_trust_root_path,
        SKILLS_TRUST_ADD_USAGE,
    ) {
        Ok(parsed) => parsed,
        Err(error) => {
            let configured_path = default_trust_root_path
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string());
            return format!(
                "skills trust add error: path={} error={error}",
                configured_path
            );
        }
    };

    let key = match parse_trusted_root_spec(&spec) {
        Ok(key) => key,
        Err(error) => {
            return format!(
                "skills trust add error: path={} error={error}",
                trust_root_path.display()
            );
        }
    };

    let mut records = match load_trust_root_records(&trust_root_path) {
        Ok(records) => records,
        Err(error) => {
            return format!(
                "skills trust add error: path={} error={error}",
                trust_root_path.display()
            );
        }
    };
    let add_specs = vec![spec];
    let report = match apply_trust_root_mutation_specs(&mut records, &add_specs, &[], &[]) {
        Ok(report) => report,
        Err(error) => {
            return format!(
                "skills trust add error: path={} error={error}",
                trust_root_path.display()
            );
        }
    };

    match save_trust_root_records(&trust_root_path, &records) {
        Ok(()) => format!(
            "skills trust add: path={} id={} added={} updated={} revoked={} rotated={}",
            trust_root_path.display(),
            key.id,
            report.added,
            report.updated,
            report.revoked,
            report.rotated
        ),
        Err(error) => format!(
            "skills trust add error: path={} error={error}",
            trust_root_path.display()
        ),
    }
}

pub fn execute_skills_trust_revoke_command(
    default_trust_root_path: Option<&Path>,
    command_args: &str,
) -> String {
    let (spec, trust_root_path) = match parse_skills_trust_mutation_args(
        command_args,
        default_trust_root_path,
        SKILLS_TRUST_REVOKE_USAGE,
    ) {
        Ok(parsed) => parsed,
        Err(error) => {
            let configured_path = default_trust_root_path
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string());
            return format!(
                "skills trust revoke error: path={} error={error}",
                configured_path
            );
        }
    };

    let mut records = match load_trust_root_records(&trust_root_path) {
        Ok(records) => records,
        Err(error) => {
            return format!(
                "skills trust revoke error: path={} error={error}",
                trust_root_path.display()
            );
        }
    };
    let revoke_ids = vec![spec.clone()];
    let report = match apply_trust_root_mutation_specs(&mut records, &[], &revoke_ids, &[]) {
        Ok(report) => report,
        Err(error) => {
            return format!(
                "skills trust revoke error: path={} error={error}",
                trust_root_path.display()
            );
        }
    };

    match save_trust_root_records(&trust_root_path, &records) {
        Ok(()) => format!(
            "skills trust revoke: path={} id={} added={} updated={} revoked={} rotated={}",
            trust_root_path.display(),
            spec,
            report.added,
            report.updated,
            report.revoked,
            report.rotated
        ),
        Err(error) => format!(
            "skills trust revoke error: path={} error={error}",
            trust_root_path.display()
        ),
    }
}

pub fn execute_skills_trust_rotate_command(
    default_trust_root_path: Option<&Path>,
    command_args: &str,
) -> String {
    let (spec, trust_root_path) = match parse_skills_trust_mutation_args(
        command_args,
        default_trust_root_path,
        SKILLS_TRUST_ROTATE_USAGE,
    ) {
        Ok(parsed) => parsed,
        Err(error) => {
            let configured_path = default_trust_root_path
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string());
            return format!(
                "skills trust rotate error: path={} error={error}",
                configured_path
            );
        }
    };

    let (old_id, new_key) = match parse_trust_rotation_spec(&spec) {
        Ok(parsed) => parsed,
        Err(error) => {
            return format!(
                "skills trust rotate error: path={} error={error}",
                trust_root_path.display()
            );
        }
    };

    let mut records = match load_trust_root_records(&trust_root_path) {
        Ok(records) => records,
        Err(error) => {
            return format!(
                "skills trust rotate error: path={} error={error}",
                trust_root_path.display()
            );
        }
    };
    let rotate_specs = vec![spec];
    let report = match apply_trust_root_mutation_specs(&mut records, &[], &[], &rotate_specs) {
        Ok(report) => report,
        Err(error) => {
            return format!(
                "skills trust rotate error: path={} error={error}",
                trust_root_path.display()
            );
        }
    };

    match save_trust_root_records(&trust_root_path, &records) {
        Ok(()) => format!(
            "skills trust rotate: path={} old_id={} new_id={} added={} updated={} revoked={} rotated={}",
            trust_root_path.display(),
            old_id,
            new_key.id,
            report.added,
            report.updated,
            report.revoked,
            report.rotated
        ),
        Err(error) => format!(
            "skills trust rotate error: path={} error={error}",
            trust_root_path.display()
        ),
    }
}

pub fn trust_record_status(record: &TrustedRootRecord, now_unix: u64) -> &'static str {
    if record.revoked {
        "revoked"
    } else if is_expired_unix(record.expires_unix, now_unix) {
        "expired"
    } else {
        "active"
    }
}

pub fn render_skills_trust_list(path: &Path, records: &[TrustedRootRecord]) -> String {
    let now_unix = current_unix_timestamp();
    let mut lines = vec![format!(
        "skills trust list: path={} count={}",
        path.display(),
        records.len()
    )];

    if records.is_empty() {
        lines.push("roots: none".to_string());
        return lines.join("\n");
    }

    for record in records {
        lines.push(format!(
            "root: id={} revoked={} expires_unix={} rotated_from={} status={}",
            record.id,
            record.revoked,
            record
                .expires_unix
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            record.rotated_from.as_deref().unwrap_or("none"),
            trust_record_status(record, now_unix)
        ));
    }

    lines.join("\n")
}

pub fn execute_skills_trust_list_command(
    default_trust_root_path: Option<&Path>,
    command_args: &str,
) -> String {
    let trust_root_path = match parse_skills_trust_list_args(command_args, default_trust_root_path)
    {
        Ok(path) => path,
        Err(error) => {
            let configured_path = default_trust_root_path
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string());
            return format!(
                "skills trust list error: path={} error={error}",
                configured_path
            );
        }
    };

    match load_trust_root_records(&trust_root_path) {
        Ok(mut records) => {
            records.sort_by(|left, right| left.id.cmp(&right.id));
            render_skills_trust_list(&trust_root_path, &records)
        }
        Err(error) => format!(
            "skills trust list error: path={} error={error}",
            trust_root_path.display()
        ),
    }
}

pub fn render_skills_lock_diff_in_sync(path: &Path, report: &SkillsSyncReport) -> String {
    format!(
        "skills lock diff: in-sync path={} expected_entries={} actual_entries={}",
        path.display(),
        report.expected_entries,
        report.actual_entries
    )
}

pub fn render_skills_lock_diff_drift(path: &Path, report: &SkillsSyncReport) -> String {
    format!(
        "skills lock diff: drift path={} {}",
        path.display(),
        render_skills_sync_drift_details(report)
    )
}

pub fn execute_skills_lock_diff_command(
    skills_dir: &Path,
    default_lock_path: &Path,
    command_args: &str,
) -> String {
    let (lock_path, json_output) =
        match parse_skills_lock_diff_args(command_args, default_lock_path) {
            Ok(parsed) => parsed,
            Err(error) => {
                return format!(
                    "skills lock diff error: path={} error={error}",
                    default_lock_path.display()
                )
            }
        };

    match sync_skills_with_lockfile(skills_dir, &lock_path) {
        Ok(report) => {
            if json_output {
                return serde_json::json!({
                    "path": lock_path.display().to_string(),
                    "status": if report.in_sync() { "in_sync" } else { "drift" },
                    "in_sync": report.in_sync(),
                    "expected_entries": report.expected_entries,
                    "actual_entries": report.actual_entries,
                    "missing": report.missing,
                    "extra": report.extra,
                    "changed": report.changed,
                    "metadata_mismatch": report.metadata_mismatch,
                })
                .to_string();
            }
            if report.in_sync() {
                render_skills_lock_diff_in_sync(&lock_path, &report)
            } else {
                render_skills_lock_diff_drift(&lock_path, &report)
            }
        }
        Err(error) => {
            if json_output {
                return serde_json::json!({
                    "path": lock_path.display().to_string(),
                    "status": "error",
                    "error": error.to_string(),
                })
                .to_string();
            }
            format!(
                "skills lock diff error: path={} error={error}",
                lock_path.display()
            )
        }
    }
}

pub fn render_skills_lock_write_success(path: &Path, entries: usize) -> String {
    format!(
        "skills lock write: path={} entries={entries}",
        path.display()
    )
}

pub fn execute_skills_lock_write_command(
    skills_dir: &Path,
    default_lock_path: &Path,
    command_args: &str,
) -> String {
    let lock_path = resolve_skills_lock_path(command_args, default_lock_path);
    match write_skills_lockfile(skills_dir, &lock_path, &[]) {
        Ok(lockfile) => render_skills_lock_write_success(&lock_path, lockfile.entries.len()),
        Err(error) => format!(
            "skills lock write error: path={} error={error}",
            lock_path.display()
        ),
    }
}

pub fn render_skills_sync_field(items: &[String], separator: &str) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(separator)
    }
}

pub fn render_skills_sync_drift_details(report: &SkillsSyncReport) -> String {
    format!(
        "expected_entries={} actual_entries={} missing={} extra={} changed={} metadata={}",
        report.expected_entries,
        report.actual_entries,
        render_skills_sync_field(&report.missing, ","),
        render_skills_sync_field(&report.extra, ","),
        render_skills_sync_field(&report.changed, ","),
        render_skills_sync_field(&report.metadata_mismatch, ";")
    )
}

pub fn render_skills_sync_in_sync(path: &Path, report: &SkillsSyncReport) -> String {
    format!(
        "skills sync: in-sync path={} expected_entries={} actual_entries={}",
        path.display(),
        report.expected_entries,
        report.actual_entries
    )
}

pub fn render_skills_sync_drift(path: &Path, report: &SkillsSyncReport) -> String {
    format!(
        "skills sync: drift path={} {}",
        path.display(),
        render_skills_sync_drift_details(report)
    )
}

pub fn execute_skills_sync_command(
    skills_dir: &Path,
    default_lock_path: &Path,
    command_args: &str,
) -> String {
    let lock_path = resolve_skills_lock_path(command_args, default_lock_path);
    match sync_skills_with_lockfile(skills_dir, &lock_path) {
        Ok(report) => {
            if report.in_sync() {
                render_skills_sync_in_sync(&lock_path, &report)
            } else {
                render_skills_sync_drift(&lock_path, &report)
            }
        }
        Err(error) => format!(
            "skills sync error: path={} error={error}",
            lock_path.display()
        ),
    }
}

pub const SKILLS_VERIFY_USAGE: &str =
    "usage: /skills-verify [lockfile_path] [trust_root_file] [--json]";

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `SkillsVerifyArgs` used across Tau components.
pub struct SkillsVerifyArgs {
    pub lock_path: PathBuf,
    pub trust_root_path: Option<PathBuf>,
    pub json_output: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `SkillsVerifyStatus` values.
pub enum SkillsVerifyStatus {
    Pass,
    Warn,
    Fail,
}

impl SkillsVerifyStatus {
    fn severity(self) -> u8 {
        match self {
            Self::Pass => 0,
            Self::Warn => 1,
            Self::Fail => 2,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
/// Public struct `SkillsVerifyEntry` used across Tau components.
pub struct SkillsVerifyEntry {
    pub file: String,
    pub name: String,
    pub status: SkillsVerifyStatus,
    pub checks: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
/// Public struct `SkillsVerifyTrustSummary` used across Tau components.
pub struct SkillsVerifyTrustSummary {
    pub total: usize,
    pub active: usize,
    pub revoked: usize,
    pub expired: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
/// Public struct `SkillsVerifySummary` used across Tau components.
pub struct SkillsVerifySummary {
    pub entries: usize,
    pub pass: usize,
    pub warn: usize,
    pub fail: usize,
    pub status: SkillsVerifyStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
/// Public struct `SkillsVerifyReport` used across Tau components.
pub struct SkillsVerifyReport {
    pub lock_path: String,
    pub trust_root_path: Option<String>,
    pub expected_entries: usize,
    pub actual_entries: usize,
    pub missing: Vec<String>,
    pub extra: Vec<String>,
    pub changed: Vec<String>,
    pub metadata_mismatch: Vec<String>,
    pub trust: Option<SkillsVerifyTrustSummary>,
    pub summary: SkillsVerifySummary,
    pub entries: Vec<SkillsVerifyEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `TrustRootState` values.
pub enum TrustRootState {
    Active,
    Revoked,
    Expired,
}

pub fn parse_skills_verify_args(
    command_args: &str,
    default_lock_path: &Path,
    default_trust_root_path: Option<&Path>,
) -> Result<SkillsVerifyArgs> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let mut positional = Vec::new();
    let mut json_output = false;
    for token in tokens {
        if token == "--json" {
            json_output = true;
            continue;
        }
        positional.push(token);
    }

    if positional.len() > 2 {
        bail!(
            "unexpected argument '{}'; {SKILLS_VERIFY_USAGE}",
            positional[2]
        );
    }

    let lock_path = positional
        .first()
        .map(|token| PathBuf::from(*token))
        .unwrap_or_else(|| default_lock_path.to_path_buf());
    let trust_root_path = positional
        .get(1)
        .map(|token| PathBuf::from(*token))
        .or_else(|| default_trust_root_path.map(Path::to_path_buf));

    Ok(SkillsVerifyArgs {
        lock_path,
        trust_root_path,
        json_output,
    })
}

pub fn update_verify_status(
    status: &mut SkillsVerifyStatus,
    checks: &mut Vec<String>,
    next_status: SkillsVerifyStatus,
    check: String,
) {
    if next_status.severity() > status.severity() {
        *status = next_status;
    }
    checks.push(check);
}

pub fn build_skills_verify_report(
    skills_dir: &Path,
    lock_path: &Path,
    trust_root_path: Option<&Path>,
) -> Result<SkillsVerifyReport> {
    let lockfile = load_skills_lockfile(lock_path)?;
    let sync_report = sync_skills_with_lockfile(skills_dir, lock_path)?;

    let trust_data = if let Some(path) = trust_root_path {
        let records = load_trust_root_records(path)?;
        let now_unix = current_unix_timestamp();
        let mut trust_index = HashMap::new();
        let mut summary = SkillsVerifyTrustSummary {
            total: records.len(),
            active: 0,
            revoked: 0,
            expired: 0,
        };
        for record in records {
            let state = if record.revoked {
                summary.revoked += 1;
                TrustRootState::Revoked
            } else if is_expired_unix(record.expires_unix, now_unix) {
                summary.expired += 1;
                TrustRootState::Expired
            } else {
                summary.active += 1;
                TrustRootState::Active
            };
            trust_index.insert(record.id, state);
        }
        Some((summary, trust_index))
    } else {
        None
    };

    let mut metadata_by_file: HashMap<String, Vec<String>> = HashMap::new();
    for item in &sync_report.metadata_mismatch {
        if let Some((file, reason)) = item.split_once(": ") {
            metadata_by_file
                .entry(file.to_string())
                .or_default()
                .push(reason.to_string());
        }
    }

    let missing_files = sync_report.missing.iter().cloned().collect::<HashSet<_>>();
    let changed_files = sync_report.changed.iter().cloned().collect::<HashSet<_>>();
    let extra_files = sync_report.extra.iter().cloned().collect::<HashSet<_>>();

    let mut lock_entries = lockfile.entries.clone();
    lock_entries.sort_by(|left, right| left.file.cmp(&right.file));

    let mut entries = Vec::new();
    for lock_entry in lock_entries {
        let mut status = SkillsVerifyStatus::Pass;
        let mut checks = Vec::new();

        if missing_files.contains(&lock_entry.file) {
            update_verify_status(
                &mut status,
                &mut checks,
                SkillsVerifyStatus::Fail,
                "sync=missing".to_string(),
            );
        } else if changed_files.contains(&lock_entry.file) {
            update_verify_status(
                &mut status,
                &mut checks,
                SkillsVerifyStatus::Fail,
                "sync=changed".to_string(),
            );
        } else {
            checks.push("sync=ok".to_string());
        }

        if let Some(reasons) = metadata_by_file.get(&lock_entry.file) {
            for reason in reasons {
                update_verify_status(
                    &mut status,
                    &mut checks,
                    SkillsVerifyStatus::Fail,
                    format!("metadata={reason}"),
                );
            }
        }

        match &lock_entry.source {
            crate::SkillLockSource::Remote {
                signing_key_id,
                signature,
                ..
            }
            | crate::SkillLockSource::Registry {
                signing_key_id,
                signature,
                ..
            } => match (signing_key_id.as_deref(), signature.as_deref()) {
                (None, None) => update_verify_status(
                    &mut status,
                    &mut checks,
                    SkillsVerifyStatus::Warn,
                    "signature=unsigned".to_string(),
                ),
                (Some(_), None) | (None, Some(_)) => update_verify_status(
                    &mut status,
                    &mut checks,
                    SkillsVerifyStatus::Fail,
                    "signature=incomplete_metadata".to_string(),
                ),
                (Some(key_id), Some(_)) => {
                    if let Some((_, trust_index)) = &trust_data {
                        match trust_index.get(key_id) {
                            Some(TrustRootState::Active) => {
                                checks.push(format!("signature=trusted key={key_id}"));
                            }
                            Some(TrustRootState::Revoked) => update_verify_status(
                                &mut status,
                                &mut checks,
                                SkillsVerifyStatus::Fail,
                                format!("signature=revoked key={key_id}"),
                            ),
                            Some(TrustRootState::Expired) => update_verify_status(
                                &mut status,
                                &mut checks,
                                SkillsVerifyStatus::Fail,
                                format!("signature=expired key={key_id}"),
                            ),
                            None => update_verify_status(
                                &mut status,
                                &mut checks,
                                SkillsVerifyStatus::Fail,
                                format!("signature=untrusted key={key_id}"),
                            ),
                        }
                    } else {
                        update_verify_status(
                            &mut status,
                            &mut checks,
                            SkillsVerifyStatus::Warn,
                            format!("signature=unverified key={key_id} trust_root=none"),
                        );
                    }
                }
            },
            crate::SkillLockSource::Unknown => checks.push("source=unknown".to_string()),
            crate::SkillLockSource::Local { .. } => checks.push("source=local".to_string()),
        }

        entries.push(SkillsVerifyEntry {
            file: lock_entry.file,
            name: lock_entry.name,
            status,
            checks,
        });
    }

    for file in extra_files {
        entries.push(SkillsVerifyEntry {
            name: file.trim_end_matches(".md").to_string(),
            file,
            status: SkillsVerifyStatus::Fail,
            checks: vec!["sync=extra_not_in_lockfile".to_string()],
        });
    }
    entries.sort_by(|left, right| left.file.cmp(&right.file));

    let mut pass = 0usize;
    let mut warn = 0usize;
    let mut fail = 0usize;
    for entry in &entries {
        match entry.status {
            SkillsVerifyStatus::Pass => pass += 1,
            SkillsVerifyStatus::Warn => warn += 1,
            SkillsVerifyStatus::Fail => fail += 1,
        }
    }
    let overall_status = if fail > 0 {
        SkillsVerifyStatus::Fail
    } else if warn > 0 {
        SkillsVerifyStatus::Warn
    } else {
        SkillsVerifyStatus::Pass
    };

    Ok(SkillsVerifyReport {
        lock_path: lock_path.display().to_string(),
        trust_root_path: trust_root_path.map(|path| path.display().to_string()),
        expected_entries: sync_report.expected_entries,
        actual_entries: sync_report.actual_entries,
        missing: sync_report.missing,
        extra: sync_report.extra,
        changed: sync_report.changed,
        metadata_mismatch: sync_report.metadata_mismatch,
        trust: trust_data.map(|(summary, _)| summary),
        summary: SkillsVerifySummary {
            entries: entries.len(),
            pass,
            warn,
            fail,
            status: overall_status,
        },
        entries,
    })
}

pub fn render_skills_verify_report(report: &SkillsVerifyReport) -> String {
    let mut lines = vec![format!(
        "skills verify: status={} lock_path={} trust_root_path={} entries={} pass={} warn={} fail={}",
        report.summary.status.as_str(),
        report.lock_path,
        report.trust_root_path.as_deref().unwrap_or("none"),
        report.summary.entries,
        report.summary.pass,
        report.summary.warn,
        report.summary.fail
    )];
    lines.push(format!(
        "sync: expected_entries={} actual_entries={} missing={} extra={} changed={} metadata={}",
        report.expected_entries,
        report.actual_entries,
        render_skills_sync_field(&report.missing, ","),
        render_skills_sync_field(&report.extra, ","),
        render_skills_sync_field(&report.changed, ","),
        render_skills_sync_field(&report.metadata_mismatch, ";")
    ));
    if let Some(trust) = report.trust {
        lines.push(format!(
            "trust: total={} active={} revoked={} expired={}",
            trust.total, trust.active, trust.revoked, trust.expired
        ));
    } else {
        lines.push("trust: none".to_string());
    }

    if report.entries.is_empty() {
        lines.push("entry: none".to_string());
        return lines.join("\n");
    }

    for entry in &report.entries {
        lines.push(format!(
            "entry: file={} name={} status={} checks={}",
            entry.file,
            entry.name,
            entry.status.as_str(),
            entry.checks.join(";")
        ));
    }
    lines.join("\n")
}

pub fn execute_skills_verify_command(
    skills_dir: &Path,
    default_lock_path: &Path,
    default_trust_root_path: Option<&Path>,
    command_args: &str,
) -> String {
    let args =
        match parse_skills_verify_args(command_args, default_lock_path, default_trust_root_path) {
            Ok(args) => args,
            Err(error) => {
                return format!(
                    "skills verify error: path={} error={error}",
                    default_lock_path.display()
                );
            }
        };

    match build_skills_verify_report(skills_dir, &args.lock_path, args.trust_root_path.as_deref()) {
        Ok(report) => {
            if args.json_output {
                serde_json::to_string(&report).unwrap_or_else(|error| {
                    serde_json::json!({
                        "status": "error",
                        "path": args.lock_path.display().to_string(),
                        "error": format!("failed to serialize skills verify report: {error}"),
                    })
                    .to_string()
                })
            } else {
                render_skills_verify_report(&report)
            }
        }
        Err(error) => {
            if args.json_output {
                serde_json::json!({
                    "status": "error",
                    "path": args.lock_path.display().to_string(),
                    "error": error.to_string(),
                })
                .to_string()
            } else {
                format!(
                    "skills verify error: path={} error={error}",
                    args.lock_path.display()
                )
            }
        }
    }
}
