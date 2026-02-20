# Spec: Issue #2818 - Command-center alert feed list SSR markers

Status: Implemented

## Problem Statement
Tau Ops shell currently exposes only primary alert summary markers. Operators cannot inspect deterministic list-level alert feed rows (code, severity, message) directly in SSR output, which blocks PRD conformance for command-center alert feed behavior.

## Acceptance Criteria

### AC-1 Alert feed list markers reflect live dashboard alerts
Given `/ops` shell render with dashboard alerts present,
When SSR HTML is generated,
Then alert feed list rows are rendered for the snapshot alerts.

### AC-2 Alert row markers expose deterministic metadata contracts
Given rendered alert feed rows,
When operators or tests inspect the SSR payload,
Then each row exposes deterministic id and `data-alert-code` / `data-alert-severity` markers with message text.

### AC-3 Nominal runtime renders informational fallback alert row markers
Given `/ops` shell render where dashboard health has no active warning/critical alerts,
When SSR HTML is generated,
Then alert feed renders an informational fallback row with deterministic `dashboard_healthy/info` metadata markers.

### AC-4 Existing command-center contracts remain stable
Given existing auth, route, shell-control, command-center snapshot, and timeline-range contracts,
When alert feed row markers are integrated,
Then prior suites remain green.

## Scope

### In Scope
- Command-center alert feed list rendering in `tau-dashboard-ui`.
- Gateway mapping of dashboard alerts into UI snapshot row context.
- Conformance/integration tests for populated and fallback alert feed states.

### Out of Scope
- Real-time alert stream hydration.
- New alert generation endpoints.
- Non-command-center dashboard views.

## Conformance Cases
- C-01 (integration): `/ops` render includes alert feed row markers for live snapshot alerts.
- C-02 (functional): alert row markers include deterministic id + `data-alert-code` + `data-alert-severity` + message.
- C-03 (integration): `/ops` render with nominal dashboard state includes fallback `dashboard_healthy/info` marker row.
- C-04 (regression): phase-1A..1I command-center suites remain green.

## Success Metrics / Observable Signals
- `cargo test -p tau-dashboard-ui functional_spec_2818 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2818 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2786 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2794 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2798 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2802 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2806 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2810 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2814 -- --test-threads=1` passes.

## Approval Gate
P1 multi-module slice proceeds with spec marked `Reviewed` per AGENTS.md self-acceptance rule. Human review required in PR.
