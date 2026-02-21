# Spec: Issue #2963 - Add gateway API reference doc and validate route coverage

Status: Reviewed

## Problem Statement
Gateway exposes a broad HTTP surface (OpenResponses, OpenAI-compatible adapters, sessions,
memory, safety, jobs, dashboard, webchat, cortex), but operators currently rely on scattered runbooks.
A single API reference is needed to reduce integration ambiguity and onboarding risk.

## Acceptance Criteria

### AC-1 API reference documents endpoint inventory with method/path coverage
Given an operator or integrator,
When reading the gateway API reference,
Then they can find grouped endpoint method/path entries for the primary gateway surfaces.

### AC-2 API reference documents auth and policy-gate expectations
Given a protected endpoint,
When reading its section,
Then required auth mode/token behavior and policy-gate requirements are explicitly stated.

### AC-3 API reference is linked in docs index
Given docs consumers,
When browsing `docs/README.md`,
Then the gateway API reference appears as a discoverable entry.

### AC-4 Route coverage is validated against code
Given the gateway route table,
When validating documentation,
Then documented endpoint set aligns with constants/routes in `gateway_openresponses.rs`.

## Scope

### In Scope
- `docs/guides/gateway-api-reference.md`
- `docs/README.md` index entry
- route coverage verification evidence

### Out of Scope
- runtime behavior changes
- endpoint schema redesigns
- auth/policy implementation changes

## Conformance Cases
- C-01: reference includes grouped method/path inventory for gateway endpoint families.
- C-02: reference includes auth + policy-gate semantics for applicable endpoints.
- C-03: docs index includes direct link to new API reference.
- C-04: route mapping check passes against code constants/router table.

## Success Metrics / Observable Signals
- Docs Quality CI passes on the PR.
- Coverage check command confirms no missing major route groups.

## Approval Gate
P1 scope: spec authored and reviewed by agent, implementation proceeds and is flagged for human review in PR.
