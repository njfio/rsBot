# Plan #2243

Status: Implemented
Spec: specs/2243/spec.md

## Approach

1. Expand `scripts/dev/live-capability-matrix.sh` case catalog:
   - add explicit cases for Codex direct, OpenRouter Kimi/Minimax, long output,
     stream mode, session continuity, and multi-tool behavior.
2. Add task/check logic for new scenarios:
   - long-output artifact validator
   - session continuity two-phase runner
   - multi-tool minimum call threshold check
   - per-case stream mode override.
3. Extend deterministic script test
   (`scripts/dev/test-live-capability-matrix.sh`) so new case behaviors can be
   verified quickly without live providers.
4. Add a top-level runner script for AC-1..AC-8:
   - executes deterministic harness tests,
   - executes retry/failure-path `tau-ai` tests for AC-5,
   - executes live matrix subset for AC-1/2/3/4/6/7.
5. Run scoped validation and capture pass/fail output summaries.

## Affected Modules

- `scripts/dev/live-capability-matrix.sh`
- `scripts/dev/test-live-capability-matrix.sh`
- `scripts/dev/validate-advanced-capabilities.sh` (new)

## Risks and Mitigations

- Risk: live model variability can cause intermittent failures.
  - Mitigation: artifact-based checks, explicit completion criteria, clear notes.
- Risk: long-output thresholds may be too strict for some models.
  - Mitigation: choose practical lower bound that still validates substantial output.
- Risk: session continuity prompts can be brittle.
  - Mitigation: use file-based phased artifacts with shared session reuse checks.

## Interfaces/Contracts

- No Rust crate API changes.
- Script interface additions only:
  - new case IDs in `live-capability-matrix.sh`
  - validation wrapper script with optional case selection overrides.
