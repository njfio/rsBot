# Issue 1645 Plan

Status: Reviewed

## Approach

1. Tests-first:
   - add `scripts/dev/test-safety-live-run-validation-contract.sh` to check
     demo wrapper/index, CI manifest/workflow, and docs contract markers
   - run harness for RED against missing demo-index safety docs section
2. Update `docs/guides/demo-index.md` with explicit `safety-smoke` scenario
   section and CI-light coverage notes.
3. Run harness for GREEN.
4. Run targeted verification:
   - `scripts/demo/test-safety-smoke.sh`
5. Run scoped quality checks:
   - `scripts/dev/roadmap-status-sync.sh --check --quiet`
   - `cargo fmt --check`
   - `cargo clippy -p tau-coding-agent -- -D warnings`

## Affected Areas

- `scripts/dev/test-safety-live-run-validation-contract.sh`
- `docs/guides/demo-index.md`
- `specs/1645/spec.md`
- `specs/1645/plan.md`
- `specs/1645/tasks.md`

## Risks And Mitigations

- Risk: docs drift from actual smoke wiring.
  - Mitigation: harness validates concrete marker/command tokens across docs and
    CI/demo source files.
- Risk: CI wiring regresses silently.
  - Mitigation: harness includes workflow + manifest marker checks.

## Interfaces / Contracts

- Safety live-run contract:
  - wrapper must include expected fail-closed command/marker.
  - index must expose safety-smoke scenario with marker and troubleshooting hint.
  - manifest/workflow must include safety smoke entry and validation step.

## ADR

No dependency/protocol/architecture decision changes; ADR not required.
