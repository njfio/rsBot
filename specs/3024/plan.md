# Plan: Issue #3024 - Gateway API route inventory drift guard

## Approach
1. Add RED conformance test for missing inventory script and mismatch handling.
2. Implement inventory script that:
   - counts `.route(...)` calls in router source,
   - counts documented method/path rows in API reference,
   - parses docs marker values,
   - determines drift and emits deterministic artifacts,
   - exits non-zero on mismatch.
3. Update API reference with explicit command contract.
4. Generate report artifacts and rerun tests/checks.

## Affected Paths
- `scripts/dev/gateway-api-route-inventory.sh` (new)
- `scripts/dev/test-gateway-api-route-inventory.sh` (new)
- `scripts/dev/test-docs-capability-archive.sh` (update)
- `docs/guides/gateway-api-reference.md` (update)
- `tasks/reports/gateway-api-route-inventory.json`
- `tasks/reports/gateway-api-route-inventory.md`
- `specs/milestones/m184/index.md`
- `specs/3024/spec.md`
- `specs/3024/plan.md`
- `specs/3024/tasks.md`

## Risks and Mitigations
- Risk: marker parsing brittleness if docs format changes.
  - Mitigation: anchored regex + explicit error output; keep marker phrases stable.
- Risk: absolute path leakage in artifacts.
  - Mitigation: emit repo-relative paths and deterministic timestamp override support.
- Risk: false positives from non-endpoint markdown tables.
  - Mitigation: method-path row pattern anchored to HTTP methods.

## Interfaces / Contracts
Script interface:
- `--router <path>` override router source path
- `--api-doc <path>` override API reference path
- `--output-json <path>` output JSON report
- `--output-md <path>` output markdown report
- `--generated-at <iso>` deterministic timestamp
- `--quiet` suppress stdout summary

Output schema contract:
- `schema_version`
- `generated_at`
- `inputs` (router/api_doc)
- `actual_counts` (`route_calls`, `method_path_rows`)
- `doc_markers` (`route_inventory_marker`, `method_path_inventory_marker`)
- `drift` (`route_calls_match`, `method_path_rows_match`, `ok`)

## ADR
Not required (docs/script quality guard only).
