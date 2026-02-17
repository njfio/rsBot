# Tasks: Issue #2415 - SKILL.md compatibility in tau-skills

## Ordered Tasks
1. T1 (RED): add C-01/C-02/C-03/C-04 conformance tests in `tau-skills` and capture failing output.
2. T2 (GREEN): implement mixed catalog loader for top-level `.md` and subdirectory `SKILL.md` files.
3. T3 (GREEN): implement frontmatter extraction (`name`, `description`) and baseDir placeholder expansion.
4. T4 (GREEN): implement prompt augmentation modes with default full-mode compatibility.
5. T5 (VERIFY): run `cargo fmt --check`, `cargo clippy -p tau-skills -- -D warnings`, and `cargo test -p tau-skills`.
6. T6 (CLOSE): prepare PR with AC mapping, RED/GREEN evidence, and test-tier matrix.

## Tier Mapping
- Unit: C-02 frontmatter and placeholder parsing behavior
- Functional: C-01 mixed catalog loading and C-03 summary prompt behavior
- Regression: C-04 default augmentation behavior compatibility
- Integration: N/A (single-crate change)
- Property/Contract/Snapshot/Fuzz/Mutation/Performance: N/A for this scoped compatibility slice
