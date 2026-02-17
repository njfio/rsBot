# Tasks: Issue #2388 - Implement /tau react dispatch and audit logging

## Ordered Tasks
1. T1 (Conformance/Tests first): add failing tests for C-01..C-05 in
   `multi_channel_runtime/tests.rs` and any required outbound tests.
2. T2: implement parser/render/help updates and command execution metadata plumbing.
3. T3: implement outbound reaction dispatch API and runtime integration on suppression path.
4. T4: pass fmt/clippy/targeted tests for `tau-multi-channel`.
5. T5: run mutation-in-diff and close escapes with additional assertions.
6. T6: update issue process log, create PR, and include AC->tests + tier evidence.

## Tier Mapping
- Unit: C-01, C-02 parser/help coverage.
- Functional: C-03, C-04 runtime behavior and auditable outcomes.
- Regression: C-05 skip-command preservation.
- Integration: outbound reaction dispatch request-shaping tests.
- Mutation: `cargo mutants --in-diff` for changed files.
