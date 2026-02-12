use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use tau_cli::Cli;
use tau_skills::{
    build_local_skill_lock_hints, build_registry_skill_lock_hints, build_remote_skill_lock_hints,
    default_skills_cache_dir, default_skills_lock_path, fetch_registry_manifest_with_cache,
    install_remote_skills_with_cache, install_skills, resolve_registry_skill_sources,
    resolve_remote_skill_sources, sync_skills_with_lockfile, write_skills_lockfile,
    SkillsDownloadOptions, SkillsSyncReport,
};

use crate::startup_resolution::resolve_skill_trust_roots;

/// Public struct `StartupSkillsBootstrapOutput` used across Tau components.
pub struct StartupSkillsBootstrapOutput {
    pub skills_lock_path: PathBuf,
}

pub async fn run_startup_skills_bootstrap(cli: &Cli) -> Result<StartupSkillsBootstrapOutput> {
    let mut skill_lock_hints = Vec::new();
    if !cli.install_skill.is_empty() {
        let report = install_skills(&cli.install_skill, &cli.skills_dir)?;
        skill_lock_hints.extend(build_local_skill_lock_hints(&cli.install_skill)?);
        println!(
            "skills install: installed={} updated={} skipped={}",
            report.installed, report.updated, report.skipped
        );
    }

    let skills_download_options = SkillsDownloadOptions {
        cache_dir: Some(
            cli.skills_cache_dir
                .clone()
                .unwrap_or_else(|| default_skills_cache_dir(&cli.skills_dir)),
        ),
        offline: cli.skills_offline,
    };
    let remote_skill_sources =
        resolve_remote_skill_sources(&cli.install_skill_url, &cli.install_skill_sha256)?;
    if !remote_skill_sources.is_empty() {
        let report = install_remote_skills_with_cache(
            &remote_skill_sources,
            &cli.skills_dir,
            &skills_download_options,
        )
        .await?;
        skill_lock_hints.extend(build_remote_skill_lock_hints(&remote_skill_sources)?);
        println!(
            "remote skills install: installed={} updated={} skipped={}",
            report.installed, report.updated, report.skipped
        );
    }

    let trusted_skill_roots = resolve_skill_trust_roots(cli)?;
    if !cli.install_skill_from_registry.is_empty() {
        let registry_url = cli.skill_registry_url.as_deref().ok_or_else(|| {
            anyhow!("--skill-registry-url is required when using --install-skill-from-registry")
        })?;
        let manifest = fetch_registry_manifest_with_cache(
            registry_url,
            cli.skill_registry_sha256.as_deref(),
            &skills_download_options,
        )
        .await?;
        let sources = resolve_registry_skill_sources(
            &manifest,
            &cli.install_skill_from_registry,
            &trusted_skill_roots,
            cli.require_signed_skills,
        )?;
        let report =
            install_remote_skills_with_cache(&sources, &cli.skills_dir, &skills_download_options)
                .await?;
        skill_lock_hints.extend(build_registry_skill_lock_hints(
            registry_url,
            &cli.install_skill_from_registry,
            &sources,
        )?);
        println!(
            "registry skills install: installed={} updated={} skipped={}",
            report.installed, report.updated, report.skipped
        );
    }

    let skills_lock_path = cli
        .skills_lock_file
        .clone()
        .unwrap_or_else(|| default_skills_lock_path(&cli.skills_dir));
    if cli.skills_lock_write {
        let lockfile =
            write_skills_lockfile(&cli.skills_dir, &skills_lock_path, &skill_lock_hints)?;
        println!(
            "{}",
            render_skills_lock_write_success(&skills_lock_path, lockfile.entries.len())
        );
    }
    if cli.skills_sync {
        let report = sync_skills_with_lockfile(&cli.skills_dir, &skills_lock_path)?;
        if report.in_sync() {
            println!("{}", render_skills_sync_in_sync(&skills_lock_path, &report));
        } else {
            bail!(
                "skills sync drift detected: path={} {}",
                skills_lock_path.display(),
                render_skills_sync_drift_details(&report)
            );
        }
    }

    Ok(StartupSkillsBootstrapOutput { skills_lock_path })
}

fn render_skills_lock_write_success(path: &Path, entries: usize) -> String {
    format!(
        "skills lock write: path={} entries={entries}",
        path.display()
    )
}

fn render_skills_sync_field(items: &[String], separator: &str) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(separator)
    }
}

fn render_skills_sync_drift_details(report: &SkillsSyncReport) -> String {
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

fn render_skills_sync_in_sync(path: &Path, report: &SkillsSyncReport) -> String {
    format!(
        "skills sync: in-sync path={} expected_entries={} actual_entries={}",
        path.display(),
        report.expected_entries,
        report.actual_entries
    )
}
