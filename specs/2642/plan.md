# Plan: Issue #2642 - G22 skill prompt-mode routing and SKILL.md compatibility hardening

## Approach
1. Add RED tests in `tau-onboarding` and `tau-orchestrator` for summary-vs-full routing and no-skills fallback behavior.
2. Add/refresh SKILL.md compatibility coverage in `tau-skills` for frontmatter + `{baseDir}` invariants.
3. Update startup prompt composition to use summary mode for channel-level prompt sections.
4. Compute selected full-skill context in local runtime startup and pass it through plan-first runtime/orchestrator request contracts.
5. Extend delegated/executor prompt builders to inject full skill context deterministically.
6. Run scoped validation and map AC/C evidence in PR.

## Affected Modules
- `crates/tau-skills/src/lib.rs`
- `crates/tau-onboarding/src/startup_prompt_composition.rs`
- `crates/tau-coding-agent/src/startup_local_runtime.rs`
- `crates/tau-coding-agent/src/runtime_loop.rs`
- `crates/tau-coding-agent/src/orchestrator_bridge.rs`
- `crates/tau-orchestrator/src/orchestrator.rs`
- `specs/2642/spec.md`
- `specs/2642/plan.md`
- `specs/2642/tasks.md`

## Risks / Mitigations
- Risk: prompt contract drift breaks existing template assumptions.
  - Mitigation: keep `skills_section` placeholder contract stable and add regression tests.
- Risk: delegated prompt size grows unexpectedly with full skills.
  - Mitigation: only attach full context in worker/delegated phases; keep channel summary mode.
- Risk: runtime wiring introduces `Option`/lifetime regressions.
  - Mitigation: pass borrowed optional context through existing runtime config structs with targeted compile/test coverage.

## Interfaces / Contracts
- Plan-first request structs in coding-agent/orchestrator gain optional delegated skill context.
- Startup prompt composition behavior changes to summary mode for channel-level prompt building.
- No schema, protocol, or dependency changes.

## ADR
- Not required; this is incremental prompt-routing behavior inside existing contracts.
