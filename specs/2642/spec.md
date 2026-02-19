# Spec: Issue #2642 - G22 skill prompt-mode routing and SKILL.md compatibility hardening

Status: Implemented

## Problem Statement
Tau already supports markdown skills and `SKILL.md` directories, including frontmatter parsing and `{baseDir}` substitution. However, startup prompt composition still injects full skill bodies into channel-level prompts, and delegated/worker orchestration paths do not receive an explicit full-skill context channel. This misses the G22 parity target: channel prompts should carry concise skill summaries while worker/delegated execution paths receive full skill content.

## Acceptance Criteria

### AC-1 SKILL.md compatibility behavior remains deterministic for frontmatter + `{baseDir}`
Given a `SKILL.md` file with scalar frontmatter and `{baseDir}` placeholders,
When Tau loads skills,
Then name/description resolve from compatible frontmatter keys and `{baseDir}` is expanded in skill content.

### AC-2 Channel-level startup prompt composition uses summary skill mode
Given selected skills are enabled at startup,
When Tau composes the channel/system startup prompt,
Then skills are injected as summary metadata (`name`, `description`, `path`) and full skill body text is not injected in channel prompt sections.

### AC-3 Worker/delegated orchestration prompts receive full skill context
Given plan-first orchestration runs delegated or executor phases,
When Tau builds delegated/executor prompts,
Then full selected skill content is available in those prompt contexts.

### AC-4 Prompt routing preserves legacy behavior when no skills are selected
Given startup/runtime has no selected skills,
When prompt composition and delegated routing execute,
Then behavior remains valid and prompts omit skill sections without errors.

### AC-5 Scoped verification gates pass
Given this issue scope,
When formatting/linting/tests run,
Then `cargo fmt --check`, `cargo clippy -p tau-skills -- -D warnings`, `cargo clippy -p tau-onboarding -- -D warnings`, `cargo clippy -p tau-coding-agent -- -D warnings`, and targeted tests pass.

## Scope

### In Scope
- Keep/extend SKILL.md compatibility assertions in `tau-skills` for frontmatter + `{baseDir}` handling.
- Switch startup/channel composition in `tau-onboarding` to summary skill prompt mode.
- Pass full selected-skill context into orchestrator delegated/executor prompt builders.
- Add RED/GREEN tests for summary/full routing and compatibility invariants.

### Out of Scope
- Changing skill package schema or registry wire formats.
- New dependency introduction for YAML parsing engines.
- Non-plan-first worker runtime redesign outside current orchestration surfaces.

## Conformance Cases
- C-01 (conformance): SKILL.md frontmatter scalar fields + `{baseDir}` remain compatible and deterministic.
- C-02 (functional): startup channel/system prompt includes summary skill metadata and excludes full body content.
- C-03 (functional): delegated-step prompt includes full skill context when provided.
- C-04 (functional): executor prompt includes full skill context when provided.
- C-05 (regression): no-selected-skills path keeps prompt composition valid with no skill section leakage.
- C-06 (verify): scoped fmt/clippy/targeted tests pass.

## Success Metrics / Observable Signals
- Channel startup prompts shrink from full skill bodies to summary form.
- Delegated/executor orchestration prompts have explicit full skill context when skills are selected.
- Existing SKILL.md compatibility behavior remains green under test.
