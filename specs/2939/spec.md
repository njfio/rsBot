# Spec: Issue #2939 - Deploy Agent wizard panel and conformance tests

Status: Implemented

## Problem Statement
The Tau Ops Dashboard PRD acceptance checklist includes Deploy Agent wizard behaviors (`2140-2144`), but the current `tau-dashboard-ui` foundation shell does not expose deploy wizard contract markers. A bounded slice is required so `/ops/deploy` can be validated through deterministic SSR markers and conformance tests.

## Acceptance Criteria

### AC-1 Deploy wizard panel renders on `/ops/deploy`
Given route `/ops/deploy`,
When the Tau Ops shell is rendered,
Then output includes a deploy panel root marker and wizard-step markers.

### AC-2 Deploy wizard contract markers cover model, validation, review, and deploy actions
Given route `/ops/deploy`,
When deploy panel HTML is rendered,
Then output includes markers for model catalog selection, step validation status, review summary, and deploy action controls.

### AC-3 Deploy panel markers are absent for non-deploy routes
Given routes other than `/ops/deploy`,
When the shell is rendered,
Then deploy panel root marker and deploy-step markers are not present.

## Scope

### In Scope
- Extend `tau-dashboard-ui` render API to accept a route and conditionally render deploy panel markers.
- Conformance tests for PRD checklist contracts `2140-2144`.
- Regression test for non-deploy route behavior.

### Out of Scope
- Backend deploy execution or process lifecycle wiring.
- Client hydration, interactivity, or live API calls.
- Full multi-view dashboard implementation.

## Conformance Cases
- C-01 (functional): `/ops/deploy` includes deploy root marker and wizard step markers.
- C-02 (conformance): `/ops/deploy` includes model catalog marker.
- C-03 (conformance): `/ops/deploy` includes validation and review markers.
- C-04 (conformance): `/ops/deploy` includes deploy action marker.
- C-05 (regression): non-deploy route does not include deploy markers.

## Success Metrics / Observable Signals
- `cargo test -p tau-dashboard-ui -- --test-threads=1` passes with added conformance coverage.
- Tests named for `spec_c01..spec_c05` map one-to-one to conformance cases.

## Approval Gate
P1 scope with single-module implementation. Spec is agent-reviewed and implementation proceeds under the user’s explicit instruction to continue contract execution.
