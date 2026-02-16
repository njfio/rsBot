# Plan #2057

Status: Implemented
Spec: specs/2057/spec.md

## Approach

Implement a dedicated artifact generator and contract schema:

1. Add `tasks/schemas/roadmap-status-artifact.schema.json` with required
   top-level fields and nested shape for roadmap phase, epic, and gap status.
2. Add `scripts/dev/roadmap-status-artifact.sh` that:
   - loads `tasks/roadmap-status-config.json`,
   - resolves issue states from fixture JSON or live GitHub,
   - writes deterministic JSON + Markdown outputs.
3. Add a script test harness for deterministic output and fail-closed paths.

## Affected Modules

- `scripts/dev/roadmap-status-artifact.sh`
- `scripts/dev/test-roadmap-status-artifact.sh`
- `tasks/schemas/roadmap-status-artifact.schema.json`

## Risks and Mitigations

- Risk: Live GitHub calls introduce nondeterminism.
  - Mitigation: fixture mode + `--generated-at` override for deterministic test
    runs; live mode used only for operational generation.
- Risk: Schema and generator drift over time.
  - Mitigation: contract tests validate schema existence and deterministic
    generator behavior.

## Interfaces and Contracts

- CLI contract:
  `scripts/dev/roadmap-status-artifact.sh --output-json <path> --output-md <path>`
- Deterministic controls:
  `--fixture-json <path>` and `--generated-at <iso-8601-utc>`.
- Schema contract:
  `tasks/schemas/roadmap-status-artifact.schema.json`.

## ADR References

- Not required.
