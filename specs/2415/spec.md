# Spec: Issue #2415 - SKILL.md compatibility in tau-skills

Status: Accepted

## Problem Statement
`tau-skills` currently loads only top-level markdown files and treats the filename stem as the skill name. It does not support directory-based `SKILL.md` assets, frontmatter-derived metadata, or `baseDir` placeholder expansion required for Spacebot/OpenClaw-style skill packs.

## Acceptance Criteria

### AC-1 Catalog loader supports directory-based `SKILL.md` skills
Given a skills directory containing both legacy top-level `.md` files and subdirectories containing `SKILL.md`,
When `load_catalog` is called,
Then the catalog includes both forms with deterministic naming (frontmatter `name` when present, otherwise sensible fallback).

### AC-2 Frontmatter metadata and base directory placeholders are resolved
Given a `SKILL.md` file with frontmatter fields and body content containing `baseDir` placeholder text,
When `load_catalog` parses the file,
Then `name` and `description` metadata are populated and body content is returned with base directory placeholder expansion applied.

### AC-3 Prompt augmentation exposes summary and full modes
Given selected skills in memory,
When prompt augmentation is requested in summary mode,
Then only per-skill summary metadata is injected.
And when full mode is requested,
Then full skill instructions are injected.

### AC-4 Existing augmentation callsites remain behavior-compatible
Given existing callers of `augment_system_prompt`,
When code is upgraded,
Then behavior remains full-content augmentation by default to avoid runtime regressions.

## Scope

### In Scope
- `tau-skills` catalog loading compatibility for subdirectory `SKILL.md` files.
- Lightweight frontmatter parsing for `name` and `description`.
- `baseDir` placeholder expansion in loaded content.
- Prompt augmentation mode API plus default backward-compatible behavior.
- Tests mapped to conformance cases.

### Out of Scope
- Full YAML parser support for complex nested frontmatter.
- Runtime role-aware prompt routing integration.
- External skill source precedence redesign.

## Conformance Cases
- C-01 (AC-1, functional): `spec_c01_load_catalog_supports_legacy_markdown_and_skill_md_directories`
- C-02 (AC-2, unit): `spec_c02_load_catalog_parses_frontmatter_and_resolves_basedir_placeholder`
- C-03 (AC-3, functional): `spec_c03_augment_system_prompt_summary_mode_injects_metadata_without_full_body`
- C-04 (AC-4, regression): `regression_spec_c04_augment_system_prompt_defaults_to_full_mode`

## Success Metrics / Observable Signals
- New conformance tests pass in `tau-skills`.
- Existing `tau-skills` tests continue passing.
- `cargo fmt --check` and `cargo clippy -p tau-skills -- -D warnings` pass.
