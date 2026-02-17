# M69 - Spacebot G22 SKILL.md Compatibility

Milestone objective: deliver first-class SKILL.md compatibility in Tau skills loading and prompt rendering while preserving current startup behavior.

## Scope
- Add SKILL.md loader compatibility in `tau-skills` for directory-based skills.
- Parse YAML-style frontmatter fields (`name`, `description`) and resolve `baseDir` placeholders.
- Add explicit prompt rendering modes for channel-style summaries versus full worker instructions.
- Keep existing default augmentation behavior stable for current runtime flow.

## Out of Scope
- Runtime-wide process-role prompt wiring beyond current startup prompt composition.
- Multi-source skill precedence (instance/workspace) architecture changes.
- New dependencies for YAML parsing.

## Exit Criteria
- Task `#2415` ACs implemented and verified.
- Conformance tests for SKILL.md loading, frontmatter parsing, and prompt rendering modes pass.
- Scoped quality gates pass (`fmt`, `clippy`, `tau-skills` tests).
