# Plan #2043

Status: Implemented
Spec: specs/2043/spec.md

## Approach

1. Complete split-map artifact planning for decomposition boundaries and test
   migration sequencing (`#2064`).
2. Execute phased extraction from the root file while preserving runtime
   contracts (`#2065`).
3. Enforce threshold and parity via split guardrail + targeted tests.
4. Close parent task after subtask evidence is posted.

## Affected Modules

- `crates/tau-github-issues-runtime/src/github_issues_runtime.rs`
- `crates/tau-github-issues-runtime/src/github_issues_runtime/*`
- `scripts/dev/test-github-issues-runtime-domain-split.sh`
- split-map artifacts under `scripts/dev/`, `tasks/schemas/`,
  `tasks/reports/`, and `docs/guides/`

## Risks and Mitigations

- Risk: extraction changes runtime reason-code/error-envelope behavior.
  - Mitigation: preserve helper semantics and run targeted runtime tests.
- Risk: planning/execution drift between split-map and actual extraction.
  - Mitigation: maintain tested artifact contracts and post closure evidence.

## Interfaces and Contracts

- Runtime entrypoints preserved for GitHub issue processing.
- Guardrails enforce file-size threshold and extracted module markers.

## ADR References

- Not required.
