# Tasks: Issue #2642 - G22 skill prompt-mode routing and SKILL.md compatibility hardening

## Ordered Tasks
1. T1 (RED): add failing tests for C-02..C-05 (channel summary mode, delegated/executor full context, no-skill regression).
2. T2 (GREEN): update startup prompt composition to summary mode and preserve compatibility behavior.
3. T3 (GREEN): wire full selected-skill context through local runtime -> runtime loop -> orchestrator bridge -> orchestrator prompt builders.
4. T4 (RED/GREEN): add/refresh SKILL.md compatibility conformance assertions for frontmatter + `{baseDir}` behavior.
5. T5 (VERIFY): run scoped fmt/clippy/tests and record evidence.
6. T6 (CLOSE): update issue/process logs and prepare PR with AC mapping + tier matrix.

## Tier Mapping
- Unit: prompt builder/helper behavior in onboarding/orchestrator modules
- Property: N/A (no randomized invariant surface introduced)
- Contract/DbC: N/A (no `contracts` macro surfaces changed)
- Snapshot: N/A (explicit string assertions)
- Functional: C-02, C-03, C-04
- Conformance: C-01..C-06
- Integration: C-02..C-05 (startup/runtime/orchestrator composition wiring)
- Fuzz: N/A (no new untrusted parser)
- Mutation: N/A (non-critical-path prompt wiring slice)
- Regression: C-05
- Performance: N/A (no performance SLO target changed)
