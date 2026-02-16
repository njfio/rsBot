# Issue 1639 Plan

Status: Reviewed

## Approach

1. Tests-first:
   - add `scripts/dev/test-oversized-file-guardrail-contract.sh` validating
     source/tests/CI/docs contract markers
   - run harness for RED against missing docs guardrail workflow section
2. Add CI-guardrail workflow contract section to
   `docs/guides/oversized-file-policy.md`.
3. Run harness for GREEN.
4. Run targeted tests:
   - `python3 -m unittest .github/scripts/test_oversized_file_guard.py`
   - `scripts/dev/test-oversized-file-policy.sh`
5. Run scoped checks:
   - `scripts/dev/roadmap-status-sync.sh --check --quiet`
   - `cargo fmt --check`
   - `cargo clippy -p tau-coding-agent -- -D warnings`

## Affected Areas

- `scripts/dev/test-oversized-file-guardrail-contract.sh`
- `docs/guides/oversized-file-policy.md`
- `specs/1639/spec.md`
- `specs/1639/plan.md`
- `specs/1639/tasks.md`

## Risks And Mitigations

- Risk: docs drift from actual workflow wiring.
  - Mitigation: harness checks CI workflow and script tokens directly.
- Risk: parent closure omits test evidence.
  - Mitigation: targeted python + bash policy tests are mandatory in issue gate.

## Interfaces / Contracts

- Guardrail contract:
  - `.github/scripts/oversized_file_guard.py` thresholds + annotation/report behavior.
  - `.github/workflows/ci.yml` step wiring for guard invocation and artifact upload.
  - `tasks/policies/oversized-file-exemptions.json` audited by policy tests.

## ADR

No dependency/protocol/architecture decision changes; ADR not required.
