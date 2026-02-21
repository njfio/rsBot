# Plan: Issue #3208 - expand kamn-sdk browser DID init/report coverage

## Approach
1. Add RED conformance tests for SDK error-context and write-boundary behavior in `crates/kamn-sdk/src/lib.rs`.
2. Update `initialize_browser_did` failure mapping to include request diagnostics without entropy leakage.
3. Re-run crate and repo quality gates.

## Affected Modules
- `crates/kamn-sdk/src/lib.rs`
- `specs/milestones/m229/index.md`
- `specs/3208/spec.md`
- `specs/3208/plan.md`
- `specs/3208/tasks.md`

## Risks & Mitigations
- Risk: overexposing request values in error strings.
  - Mitigation: include method/network/subject only; exclude entropy from diagnostics and test this explicitly.
- Risk: brittle assertions on filesystem error wording.
  - Mitigation: assert stable contextual prefixes and path components controlled by SDK context layer.

## Interfaces / Contracts
- `initialize_browser_did` still returns `anyhow::Result<BrowserDidInitReport>`.
- `write_browser_did_init_report` still returns `anyhow::Result<()>`.
- `BrowserDidInitReport` schema remains unchanged.

## ADR
No ADR required (single-crate QA and diagnostics hardening).
