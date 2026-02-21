# M228 - Panic Policy Audit Classifier Alignment

Status: In Progress

## Context
`scripts/dev/panic-unsafe-audit.sh` currently classifies test-context via coarse heuristics (`line >= first #[cfg(test)]`). This can misclassify non-test occurrences that appear later in mixed files and skew policy gating counters.

## Scope
- Add RED fixture case that exposes cfg(test)-line false classification.
- Update classifier to evaluate per-line test context by parsing source structure.
- Preserve JSON schema and guard compatibility.

## Linked Issues
- Epic: #3202
- Story: #3203
- Task: #3204

## Success Signals
- `scripts/dev/test-panic-unsafe-audit.sh`
- `scripts/dev/test-panic-unsafe-guard.sh`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
