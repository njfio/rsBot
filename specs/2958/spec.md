# Spec: Issue #2958 - Create operator deployment guide and validate commands end-to-end

Status: Implemented

## Problem Statement
Tau has multiple operations runbooks, but there is no single operator deployment guide that covers
provider credentials, gateway startup, dashboard authentication, readiness checks, and rollback in one
validated flow. This slows onboarding and increases deployment error risk.

## Acceptance Criteria

### AC-1 Guide documents required deployment prerequisites and credentials
Given an operator preparing deployment,
When reading the guide,
Then they can find required binaries, environment variables, and provider/auth credential setup paths.

### AC-2 Guide provides executable gateway + dashboard startup procedures
Given a clean local environment,
When following the documented startup commands,
Then the operator can start gateway OpenResponses mode and reach dashboard/webchat/status endpoints.

### AC-3 Guide includes troubleshooting and rollback procedures
Given deployment or readiness failures,
When following the guide,
Then operators can use reason-code diagnostics and rollback commands to reach a known-safe posture.

### AC-4 Guide is validated with live command execution
Given the guide updates,
When running documented validation commands in this repository,
Then command syntax and expected health-gate posture are confirmed in output artifacts.

## Scope

### In Scope
- `docs/guides/operator-deployment-guide.md`
- `docs/README.md` index entry for the new guide
- live validation command evidence for guide procedures

### Out of Scope
- Changing runtime behavior, endpoint schemas, or auth implementations
- New deployment orchestration features

## Conformance Cases
- C-01: Guide contains prerequisite and credential sections with concrete command snippets.
- C-02: Guide contains gateway startup and dashboard/status verification commands.
- C-03: Guide contains troubleshooting + rollback section tied to reason-code/status checks.
- C-04: Live validation commands run successfully in local environment (including expected hold-mode overrides).

## Success Metrics / Observable Signals
- `docs/guides/operator-deployment-guide.md` passes Docs Quality CI checks.
- Live validation command set executes with expected pass posture.

## Approval Gate
P0 scope proceeds under explicit user instruction to continue end-to-end; PR will request human acceptance review.
