# M67 - Critical Gap Regression Remediation (Preflight Budget Gate)

Milestone objective: restore fail-closed OpenResponses preflight budget enforcement after
regression so over-budget requests are rejected before provider dispatch.

## Scope
- Reproduce and fix failing gateway preflight conformance checks.
- Ensure over-budget requests return contract-consistent gateway failure payloads.
- Ensure provider dispatch is skipped for preflight-rejected requests.
- Validate via targeted gateway tests and critical-gap verification script.

## Out of Scope
- Provider pricing model updates.
- Broader OpenResponses schema changes.
- Non-gateway budget policies.

## Exit Criteria
- Issue `#2405` AC/C-case mapping implemented.
- `integration_spec_c01_openresponses_preflight_blocks_over_budget_request` passes.
- `integration_spec_c02_openresponses_preflight_skips_provider_dispatch` passes.
- `scripts/dev/verify-critical-gaps.sh` passes for the gateway preflight section.
