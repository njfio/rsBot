# Plan #2590

## Approach
1. Execute and record #2589 conformance commands.
2. Run scoped quality gates and mutation-in-diff for changed Rust paths.
3. Execute sanitized live smoke and capture summary.
4. Finalize process logs/spec status/checklist updates.

## Affected Modules
- `specs/2589/tasks.md`
- `specs/2590/tasks.md`
- PR evidence artifacts/log references

## Risks & Mitigations
- Risk: mutation escapes reveal assertion weakness.
  - Mitigation: strengthen tests before merge.
- Risk: provider key drift causes flaky live smoke.
  - Mitigation: sanitized key-file execution with deterministic skip behavior.

## Interfaces / Contracts
- No new runtime API contracts; this subtask validates rollout quality gates.
