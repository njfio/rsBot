# Plan: Issue #2415 - SKILL.md compatibility in tau-skills

## Approach
1. Add conformance tests first (RED) for loader compatibility and prompt mode behavior.
2. Extend `Skill` metadata model to carry parsed `description` and base directory path.
3. Refactor `load_catalog` to ingest both legacy top-level `.md` files and directory-based `SKILL.md` files.
4. Implement lightweight frontmatter parsing and baseDir placeholder substitution.
5. Add explicit prompt augmentation mode API while preserving current default full behavior.
6. Re-run scoped quality gates and confirm GREEN.

## Affected Modules
- `crates/tau-skills/src/lib.rs`
- `crates/tau-skills/src/load_registry.rs`

## Risks / Mitigations
- Risk: Existing callsites depend on full-content augmentation semantics.
  - Mitigation: keep `augment_system_prompt` defaulted to full mode and add separate mode-aware API.
- Risk: Loader changes introduce ambiguity or duplicate skill names.
  - Mitigation: deterministic ordering and explicit tests for mixed catalogs.
- Risk: Naive frontmatter parsing mishandles complex YAML.
  - Mitigation: scope parser to supported scalar fields (`name`, `description`) and ignore unsupported complex structures.

## Interfaces / Contracts
- Extend `Skill` with metadata required for SKILL.md compatibility.
- Add `SkillPromptMode` enum.
- Add `augment_system_prompt_with_mode(base, skills, mode)`.
- Preserve existing `augment_system_prompt(base, skills)` behavior as full-mode wrapper.

## ADR
- Not required; no dependency, wire-protocol, or architecture-level changes.
