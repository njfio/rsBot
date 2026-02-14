use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tau_cli::Cli;
use tau_core::{current_unix_timestamp_ms, write_text_atomic};

const PROJECT_INDEX_SCHEMA_VERSION: u32 = 1;
const PROJECT_INDEX_FILE_NAME: &str = "project-index.json";
const MAX_INDEX_FILE_BYTES: usize = 2 * 1024 * 1024;
const MAX_TOKENS_PER_FILE: usize = 512;
const MAX_SYMBOLS_PER_FILE: usize = 128;
const MAX_TOKENIZED_CHARS: usize = 200_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ProjectIndexState {
    schema_version: u32,
    generated_unix_ms: u64,
    root: String,
    files: Vec<ProjectIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ProjectIndexEntry {
    path: String,
    sha256: String,
    bytes: u64,
    lines: u64,
    token_count: usize,
    tokens: Vec<String>,
    symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ProjectIndexBuildReport {
    root: String,
    index_path: String,
    schema_version: u32,
    generated_unix_ms: u64,
    files_discovered: usize,
    files_indexed: usize,
    files_reused: usize,
    files_updated: usize,
    files_removed: usize,
    skipped_non_utf8: usize,
    skipped_large: usize,
    recovered_from_corrupt_state: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ProjectIndexInspectReport {
    root: String,
    index_path: String,
    schema_version: u32,
    generated_unix_ms: u64,
    files_indexed: usize,
    total_bytes: u64,
    total_lines: u64,
    total_tokens: usize,
    extension_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ProjectIndexQueryResult {
    path: String,
    score: u64,
    bytes: u64,
    lines: u64,
    token_count: usize,
    matched_symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ProjectIndexQueryReport {
    root: String,
    index_path: String,
    query: String,
    query_tokens: Vec<String>,
    limit: usize,
    total_matches: usize,
    results: Vec<ProjectIndexQueryResult>,
}

pub fn execute_project_index_command(cli: &Cli) -> Result<()> {
    let root = resolve_project_index_root(&cli.project_index_root)?;
    let index_path = project_index_file_path(&cli.project_index_state_dir);
    if cli.project_index_build {
        let report = build_project_index(&root, &cli.project_index_state_dir)?;
        if cli.project_index_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .context("failed to render project index build report as json")?
            );
        } else {
            println!("{}", render_project_index_build_report(&report));
        }
        return Ok(());
    }

    if let Some(query) = cli.project_index_query.as_deref() {
        let report = query_project_index(&root, &index_path, query, cli.project_index_limit)?;
        if cli.project_index_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .context("failed to render project index query report as json")?
            );
        } else {
            println!("{}", render_project_index_query_report(&report));
        }
        return Ok(());
    }

    if cli.project_index_inspect {
        let report = inspect_project_index(&root, &index_path)?;
        if cli.project_index_json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .context("failed to render project index inspect report as json")?
            );
        } else {
            println!("{}", render_project_index_inspect_report(&report));
        }
        return Ok(());
    }

    bail!("no project index action selected");
}

fn resolve_project_index_root(root: &Path) -> Result<PathBuf> {
    if !root.exists() {
        bail!("--project-index-root '{}' does not exist", root.display());
    }
    if !root.is_dir() {
        bail!(
            "--project-index-root '{}' must point to a directory",
            root.display()
        );
    }
    Ok(fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf()))
}

fn project_index_file_path(state_dir: &Path) -> PathBuf {
    state_dir.join(PROJECT_INDEX_FILE_NAME)
}

fn should_skip_directory(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".hg"
            | ".svn"
            | ".tau"
            | "target"
            | "node_modules"
            | "dist"
            | "build"
            | ".venv"
            | "venv"
            | ".idea"
            | ".vscode"
    )
}

fn should_index_file(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "rs" | "md"
            | "toml"
            | "json"
            | "yaml"
            | "yml"
            | "txt"
            | "py"
            | "go"
            | "java"
            | "kt"
            | "swift"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "sql"
            | "sh"
            | "bash"
            | "zsh"
            | "html"
            | "css"
            | "scss"
            | "c"
            | "cc"
            | "cpp"
            | "h"
            | "hpp"
    )
}

fn collect_index_candidate_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_index_candidate_files_recursive(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_index_candidate_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries = fs::read_dir(dir)
        .with_context(|| format!("failed to read directory '{}'", dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to list directory entries for '{}'", dir.display()))?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect '{}'", path.display()))?;
        if file_type.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if should_skip_directory(&name) {
                continue;
            }
            collect_index_candidate_files_recursive(&path, files)?;
            continue;
        }
        if file_type.is_file() && should_index_file(&path) {
            files.push(path);
        }
    }

    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}

fn normalize_relative_path(root: &Path, path: &Path) -> Result<String> {
    let relative = path.strip_prefix(root).with_context(|| {
        format!(
            "failed to compute path relative to root '{}'",
            root.display()
        )
    })?;
    let normalized = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/");
    Ok(normalized)
}

fn extract_tokens(text: &str) -> Vec<String> {
    let mut tokens = BTreeSet::new();
    let mut current = String::new();

    for character in text.chars().take(MAX_TOKENIZED_CHARS) {
        if character.is_ascii_alphanumeric() || character == '_' {
            current.push(character.to_ascii_lowercase());
            continue;
        }
        if !current.is_empty() {
            if current.len() >= 2 && !is_stopword(&current) {
                tokens.insert(current.clone());
                if tokens.len() >= MAX_TOKENS_PER_FILE {
                    break;
                }
            }
            current.clear();
        }
    }
    if !current.is_empty() && current.len() >= 2 && !is_stopword(&current) {
        tokens.insert(current);
    }

    tokens.into_iter().take(MAX_TOKENS_PER_FILE).collect()
}

fn is_stopword(token: &str) -> bool {
    matches!(
        token,
        "the"
            | "and"
            | "for"
            | "with"
            | "that"
            | "this"
            | "from"
            | "into"
            | "true"
            | "false"
            | "none"
            | "null"
            | "let"
            | "pub"
            | "use"
    )
}

fn extract_symbols(text: &str) -> Vec<String> {
    let prefixes = [
        "pub fn ",
        "fn ",
        "pub struct ",
        "struct ",
        "pub enum ",
        "enum ",
        "pub trait ",
        "trait ",
        "pub mod ",
        "mod ",
        "class ",
        "interface ",
        "type ",
        "impl ",
    ];
    let mut symbols = BTreeSet::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        for prefix in prefixes {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                let name = parse_identifier(rest);
                if !name.is_empty() {
                    symbols.insert(name.to_string());
                    if symbols.len() >= MAX_SYMBOLS_PER_FILE {
                        return symbols.into_iter().collect();
                    }
                }
                break;
            }
        }
    }
    symbols.into_iter().collect()
}

fn parse_identifier(raw: &str) -> &str {
    let trimmed = raw.trim_start();
    let mut end = 0usize;
    for (index, character) in trimmed.char_indices() {
        if character.is_ascii_alphanumeric() || character == '_' || character == ':' {
            end = index + character.len_utf8();
        } else {
            break;
        }
    }
    trimmed.get(..end).unwrap_or_default()
}

fn load_project_index_state(index_path: &Path) -> Result<ProjectIndexState> {
    let raw = fs::read_to_string(index_path)
        .with_context(|| format!("failed reading '{}'", index_path.display()))?;
    let state: ProjectIndexState = serde_json::from_str(&raw)
        .with_context(|| format!("failed parsing '{}'", index_path.display()))?;
    if state.schema_version != PROJECT_INDEX_SCHEMA_VERSION {
        bail!(
            "project index schema mismatch in '{}': expected {}, got {}",
            index_path.display(),
            PROJECT_INDEX_SCHEMA_VERSION,
            state.schema_version
        );
    }
    Ok(state)
}

fn build_project_index(root: &Path, state_dir: &Path) -> Result<ProjectIndexBuildReport> {
    fs::create_dir_all(state_dir).with_context(|| {
        format!(
            "failed creating project index state directory '{}'",
            state_dir.display()
        )
    })?;
    let index_path = project_index_file_path(state_dir);

    let mut recovered_from_corrupt_state = false;
    let existing = if index_path.exists() {
        match load_project_index_state(&index_path) {
            Ok(state) => Some(state),
            Err(_) => {
                recovered_from_corrupt_state = true;
                None
            }
        }
    } else {
        None
    };

    let existing_entries = existing
        .map(|state| {
            state
                .files
                .into_iter()
                .map(|entry| (entry.path.clone(), entry))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    let candidate_files = collect_index_candidate_files(root)?;
    let files_discovered = candidate_files.len();
    let mut files_reused = 0usize;
    let mut files_updated = 0usize;
    let mut skipped_non_utf8 = 0usize;
    let mut skipped_large = 0usize;
    let mut indexed_entries = Vec::new();
    let mut indexed_paths = BTreeSet::new();

    for file_path in candidate_files {
        let relative_path = normalize_relative_path(root, &file_path)?;
        let bytes = fs::read(&file_path)
            .with_context(|| format!("failed reading '{}'", file_path.display()))?;
        if bytes.len() > MAX_INDEX_FILE_BYTES {
            skipped_large = skipped_large.saturating_add(1);
            continue;
        }

        let text = match String::from_utf8(bytes.clone()) {
            Ok(value) => value,
            Err(_) => {
                skipped_non_utf8 = skipped_non_utf8.saturating_add(1);
                continue;
            }
        };

        let sha = sha256_hex(&bytes);
        if let Some(existing_entry) = existing_entries.get(&relative_path) {
            if existing_entry.sha256 == sha {
                files_reused = files_reused.saturating_add(1);
                indexed_paths.insert(relative_path);
                indexed_entries.push(existing_entry.clone());
                continue;
            }
        }

        files_updated = files_updated.saturating_add(1);
        indexed_paths.insert(relative_path.clone());
        let tokens = extract_tokens(&text);
        let symbols = extract_symbols(&text);
        indexed_entries.push(ProjectIndexEntry {
            path: relative_path,
            sha256: sha,
            bytes: bytes.len() as u64,
            lines: text.lines().count() as u64,
            token_count: tokens.len(),
            tokens,
            symbols,
        });
    }

    indexed_entries.sort_by(|left, right| left.path.cmp(&right.path));
    let files_removed = existing_entries
        .keys()
        .filter(|path| !indexed_paths.contains(*path))
        .count();
    let generated_unix_ms = current_unix_timestamp_ms();
    let state = ProjectIndexState {
        schema_version: PROJECT_INDEX_SCHEMA_VERSION,
        generated_unix_ms,
        root: root.display().to_string(),
        files: indexed_entries.clone(),
    };
    write_text_atomic(
        &index_path,
        &serde_json::to_string_pretty(&state)
            .context("failed to serialize project index state payload")?,
    )
    .with_context(|| format!("failed writing '{}'", index_path.display()))?;

    Ok(ProjectIndexBuildReport {
        root: root.display().to_string(),
        index_path: index_path.display().to_string(),
        schema_version: PROJECT_INDEX_SCHEMA_VERSION,
        generated_unix_ms,
        files_discovered,
        files_indexed: indexed_entries.len(),
        files_reused,
        files_updated,
        files_removed,
        skipped_non_utf8,
        skipped_large,
        recovered_from_corrupt_state,
    })
}

fn inspect_project_index(root: &Path, index_path: &Path) -> Result<ProjectIndexInspectReport> {
    let state = load_project_index_state(index_path).with_context(|| {
        format!(
            "failed loading project index from '{}'; run --project-index-build to regenerate",
            index_path.display()
        )
    })?;
    let mut extension_counts = BTreeMap::new();
    let mut total_bytes = 0u64;
    let mut total_lines = 0u64;
    let mut total_tokens = 0usize;
    for entry in &state.files {
        let extension = Path::new(&entry.path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("(none)")
            .to_ascii_lowercase();
        *extension_counts.entry(extension).or_insert(0usize) += 1;
        total_bytes = total_bytes.saturating_add(entry.bytes);
        total_lines = total_lines.saturating_add(entry.lines);
        total_tokens = total_tokens.saturating_add(entry.token_count);
    }
    Ok(ProjectIndexInspectReport {
        root: root.display().to_string(),
        index_path: index_path.display().to_string(),
        schema_version: state.schema_version,
        generated_unix_ms: state.generated_unix_ms,
        files_indexed: state.files.len(),
        total_bytes,
        total_lines,
        total_tokens,
        extension_counts,
    })
}

fn query_project_index(
    root: &Path,
    index_path: &Path,
    query: &str,
    limit: usize,
) -> Result<ProjectIndexQueryReport> {
    let state = load_project_index_state(index_path).with_context(|| {
        format!(
            "failed loading project index from '{}'; run --project-index-build to regenerate",
            index_path.display()
        )
    })?;
    let query_trimmed = query.trim();
    if query_trimmed.is_empty() {
        bail!("--project-index-query cannot be empty");
    }
    let query_lower = query_trimmed.to_ascii_lowercase();
    let mut query_tokens = extract_tokens(query_trimmed);
    if query_tokens.is_empty() {
        query_tokens.push(query_lower.clone());
    }

    let mut results = state
        .files
        .iter()
        .filter_map(|entry| score_project_index_entry(entry, &query_lower, &query_tokens))
        .collect::<Vec<_>>();
    results.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.path.cmp(&right.path))
    });
    let total_matches = results.len();
    results.truncate(limit);

    Ok(ProjectIndexQueryReport {
        root: root.display().to_string(),
        index_path: index_path.display().to_string(),
        query: query_trimmed.to_string(),
        query_tokens,
        limit,
        total_matches,
        results,
    })
}

fn score_project_index_entry(
    entry: &ProjectIndexEntry,
    query_lower: &str,
    query_tokens: &[String],
) -> Option<ProjectIndexQueryResult> {
    let path_lower = entry.path.to_ascii_lowercase();
    let symbol_rows = entry
        .symbols
        .iter()
        .map(|symbol| symbol.to_ascii_lowercase())
        .collect::<Vec<_>>();

    let mut score = 0u64;
    if path_lower.contains(query_lower) {
        score = score.saturating_add(120);
    }
    for token in query_tokens {
        if path_lower.contains(token) {
            score = score.saturating_add(40);
        }
        if symbol_rows.iter().any(|symbol| symbol.contains(token)) {
            score = score.saturating_add(30);
        }
        if entry.tokens.binary_search(token).is_ok() {
            score = score.saturating_add(10);
        }
    }
    if score == 0 {
        return None;
    }

    let matched_symbols = entry
        .symbols
        .iter()
        .filter(|symbol| {
            let lower = symbol.to_ascii_lowercase();
            lower.contains(query_lower) || query_tokens.iter().any(|token| lower.contains(token))
        })
        .take(3)
        .cloned()
        .collect::<Vec<_>>();

    Some(ProjectIndexQueryResult {
        path: entry.path.clone(),
        score,
        bytes: entry.bytes,
        lines: entry.lines,
        token_count: entry.token_count,
        matched_symbols,
    })
}

fn render_project_index_build_report(report: &ProjectIndexBuildReport) -> String {
    format!(
        "project index build: root={} index_path={} schema_version={} generated_unix_ms={} discovered={} indexed={} reused={} updated={} removed={} skipped_non_utf8={} skipped_large={} recovered_from_corrupt_state={}",
        report.root,
        report.index_path,
        report.schema_version,
        report.generated_unix_ms,
        report.files_discovered,
        report.files_indexed,
        report.files_reused,
        report.files_updated,
        report.files_removed,
        report.skipped_non_utf8,
        report.skipped_large,
        report.recovered_from_corrupt_state
    )
}

fn render_project_index_inspect_report(report: &ProjectIndexInspectReport) -> String {
    let extension_summary = if report.extension_counts.is_empty() {
        "none".to_string()
    } else {
        report
            .extension_counts
            .iter()
            .map(|(extension, count)| format!("{extension}:{count}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "project index inspect: root={} index_path={} schema_version={} generated_unix_ms={} files={} total_bytes={} total_lines={} total_tokens={} extensions={}",
        report.root,
        report.index_path,
        report.schema_version,
        report.generated_unix_ms,
        report.files_indexed,
        report.total_bytes,
        report.total_lines,
        report.total_tokens,
        extension_summary
    )
}

fn render_project_index_query_report(report: &ProjectIndexQueryReport) -> String {
    let mut lines = vec![format!(
        "project index query: root={} query={} matches={} limit={}",
        report.root, report.query, report.total_matches, report.limit
    )];
    for result in &report.results {
        let symbols = if result.matched_symbols.is_empty() {
            "none".to_string()
        } else {
            result.matched_symbols.join("|")
        };
        lines.push(format!(
            "score={} path={} bytes={} lines={} token_count={} symbols={}",
            result.score, result.path, result.bytes, result.lines, result.token_count, symbols
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn unit_extract_tokens_and_symbols_are_deterministic() {
        let text = r#"
            /// Public struct `IndexReport` used across Tau components.
            pub struct IndexReport { value: usize }
            pub fn build_index() {}
            fn private_helper() {}
        "#;
        let first_tokens = extract_tokens(text);
        let second_tokens = extract_tokens(text);
        assert_eq!(first_tokens, second_tokens);
        assert!(first_tokens.iter().any(|token| token == "indexreport"));
        assert!(first_tokens.iter().any(|token| token == "build_index"));

        let symbols = extract_symbols(text);
        assert_eq!(
            symbols,
            vec![
                "IndexReport".to_string(),
                "build_index".to_string(),
                "private_helper".to_string()
            ]
        );
    }

    #[test]
    fn functional_build_project_index_writes_state_and_counts_files() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("workspace");
        let state_dir = temp.path().join("state");
        fs::create_dir_all(root.join("src")).expect("create src");
        fs::write(
            root.join("src").join("lib.rs"),
            "pub fn parse_value() -> usize { 1 }\n",
        )
        .expect("write rust file");
        fs::write(root.join("README.md"), "Tau project index demo\n").expect("write readme");

        let report = build_project_index(&root, &state_dir).expect("build should succeed");
        assert_eq!(report.files_discovered, 2);
        assert_eq!(report.files_indexed, 2);
        assert_eq!(report.files_updated, 2);
        assert_eq!(report.files_reused, 0);
        assert!(
            project_index_file_path(&state_dir).exists(),
            "index file should exist after build"
        );

        let inspect = inspect_project_index(&root, &project_index_file_path(&state_dir))
            .expect("inspect should succeed");
        assert_eq!(inspect.files_indexed, 2);
        assert_eq!(inspect.extension_counts.get("rs"), Some(&1usize));
        assert_eq!(inspect.extension_counts.get("md"), Some(&1usize));
    }

    #[test]
    fn integration_build_then_query_reuses_unchanged_entries_and_ranks_matches() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("workspace");
        let state_dir = temp.path().join("state");
        fs::create_dir_all(root.join("src")).expect("create src");
        fs::write(
            root.join("src").join("alpha.rs"),
            "pub fn alpha_runner() {}\n",
        )
        .expect("write alpha");
        fs::write(
            root.join("src").join("beta.rs"),
            "pub fn beta_runner() {}\n",
        )
        .expect("write beta");

        let first = build_project_index(&root, &state_dir).expect("first build should succeed");
        assert_eq!(first.files_updated, 2);
        assert_eq!(first.files_reused, 0);

        fs::write(
            root.join("src").join("beta.rs"),
            "pub fn beta_runner_v2() {}\n",
        )
        .expect("update beta");

        let second = build_project_index(&root, &state_dir).expect("second build should succeed");
        assert_eq!(second.files_updated, 1);
        assert_eq!(second.files_reused, 1);

        let query = query_project_index(
            &root,
            &project_index_file_path(&state_dir),
            "beta_runner_v2",
            5,
        )
        .expect("query should succeed");
        assert_eq!(query.total_matches, 1);
        assert_eq!(query.results[0].path, "src/beta.rs");
        assert!(query.results[0].score > 0);
    }

    #[test]
    fn regression_query_fails_on_corrupt_state_with_repair_guidance() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("workspace");
        let state_dir = temp.path().join("state");
        fs::create_dir_all(&root).expect("create root");
        fs::create_dir_all(&state_dir).expect("create state");
        fs::write(project_index_file_path(&state_dir), "{invalid json").expect("write corrupt");

        let error = query_project_index(&root, &project_index_file_path(&state_dir), "alpha", 5)
            .expect_err("query should fail on corrupt state");
        let message = error.to_string();
        assert!(message.contains("run --project-index-build to regenerate"));

        let report = build_project_index(&root, &state_dir).expect("build should recover");
        assert!(report.recovered_from_corrupt_state);
    }
}
