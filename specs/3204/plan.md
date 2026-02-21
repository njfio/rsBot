# Plan: Issue #3204 - align panic policy audit classifier with per-line test context

## Approach
1. Add RED fixture marker in `test-panic-unsafe-audit.sh` dataset to expose cfg(test)-line false classification.
2. Implement per-line test-context parser in `panic-unsafe-audit.sh` classifier path.
3. Re-run audit + guard fixture scripts and standard verify gates.

## Affected Modules
- `scripts/dev/panic-unsafe-audit.sh`
- `scripts/dev/test-panic-unsafe-audit.sh`
- `specs/milestones/m228/index.md`
- `specs/3204/spec.md`
- `specs/3204/plan.md`
- `specs/3204/tasks.md`

## Risks & Mitigations
- Risk: parser heuristic complexity may introduce false negatives.
  - Mitigation: explicit fixture for mixed file ordering; keep path-based + attribute signals additive.
- Risk: JSON output contract drift.
  - Mitigation: do not change output keys; validate guard test.

## Interfaces / Contracts
- Output keys in `panic-unsafe-audit.sh` JSON `counters` unchanged.
- Guard script input contract remains compatible.

## ADR
No ADR required (script classifier correction only).
