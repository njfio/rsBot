# Plan: Issue #2946 - Accessibility contract markers and conformance tests

## Approach
1. Add RED conformance tests for C-01..C-05 in `crates/tau-dashboard-ui/src/tests.rs`.
2. Add an accessibility contract marker section and related attributes in `crates/tau-dashboard-ui/src/lib.rs`.
3. Keep all existing route and widget contracts untouched.
4. Run scoped verify gates and finalize spec status.

## Affected Modules
- `crates/tau-dashboard-ui/src/lib.rs`
- `crates/tau-dashboard-ui/src/tests.rs`
- `specs/milestones/m166/index.md`
- `specs/2946/spec.md`
- `specs/2946/tasks.md`

## Risks / Mitigations
- Risk: marker additions unintentionally alter existing test contracts.
  - Mitigation: isolate new IDs/`data-*` attributes in dedicated sections.
- Risk: ambiguous accessibility semantics in tests.
  - Mitigation: assert explicit ARIA and contract attributes with stable IDs.

## Interfaces / Contracts
- No public Rust API changes.
- Add accessibility marker contracts:
  - conformance section
  - skip-link + keyboard navigation markers
  - live-region announcer markers
  - focus contract marker
  - reduced-motion marker

## ADR
No ADR required: no dependency/protocol/architecture boundary changes.
