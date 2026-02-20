# Milestone M165 - Tau Ops Dashboard Deploy Wizard Contracts

Status: InProgress

## Scope
Implement the Deploy Agent wizard contract slice from the Tau Ops Dashboard PRD in the Leptos SSR shell:
- add `/ops/deploy` panel contract markers,
- render wizard step and navigation markers,
- render model catalog and review/deploy action markers,
- verify non-deploy routes do not expose deploy panel markers.

## Linked Issues
- Epic: #2937
- Story: #2938
- Task: #2939

## Success Signals
- `tau-dashboard-ui` render output includes deploy wizard contract markers.
- Conformance tests cover PRD acceptance checklist items `2140-2144`.
- Regression tests verify deploy panel markers are absent on non-deploy routes.
