# Tasks: Issue #2963 - Gateway API reference documentation

## Ordered Tasks
1. [x] T1 (RED): verify gateway API reference doc path is absent and capture failing evidence.
2. [x] T2 (GREEN): author `docs/guides/gateway-api-reference.md` with grouped method/path/auth inventory.
3. [x] T3 (GREEN): add docs index link in `docs/README.md`.
4. [x] T4 (REGRESSION): run route-coverage extraction checks against `gateway_openresponses.rs`.
5. [x] T5 (VERIFY): run docs-quality validation scripts and finalize issue artifacts.

## Tier Mapping
- Unit: N/A (docs-only)
- Property: N/A (docs-only)
- Contract/DbC: N/A (docs-only)
- Snapshot: N/A (docs-only)
- Functional: reference usability for endpoint lookup
- Conformance: C-01..C-04
- Integration: documentation aligns with live route table
- Fuzz: N/A (no parser/runtime change)
- Mutation: N/A (docs-only)
- Regression: docs-quality script suite
- Performance: N/A (docs-only)
