# Spec: Issue #2822 - Command-center connector health table SSR markers

Status: Implemented

## Problem Statement
Tau Ops shell currently surfaces dashboard timeline/control/alert contracts, but does not expose connector health table rows for multi-channel state. Operators cannot inspect channel-level connector mode/liveness status from command-center SSR output, which leaves PRD command-center connector visibility incomplete.

## Acceptance Criteria

### AC-1 Connector table rows reflect live multi-channel channel state
Given `/ops` shell render with multi-channel connector state present,
When SSR HTML is generated,
Then connector health table rows are rendered for each live channel.

### AC-2 Connector row markers expose deterministic metadata contracts
Given rendered connector rows,
When operators/tests inspect SSR payload,
Then each row exposes deterministic id and marker attributes for `channel`, `mode`, `liveness`, and counter values.

### AC-3 Missing connector state renders deterministic fallback row
Given `/ops` shell render when connector state is unavailable,
When SSR HTML is generated,
Then connector table renders a fallback row with deterministic `none/unknown` markers and zero counters.

### AC-4 Existing command-center contracts remain stable
Given existing auth, route, shell-control, command-center snapshot, timeline-range, and alert-feed contracts,
When connector row markers are integrated,
Then prior suites remain green.

## Scope

### In Scope
- Command-center connector health table row rendering in `tau-dashboard-ui`.
- Gateway mapping of multi-channel connector channel state into command-center context.
- Conformance/integration tests for populated and fallback connector table rows.

### Out of Scope
- Real-time connector stream hydration updates.
- New multi-channel lifecycle endpoints.
- Non-command-center routes.

## Conformance Cases
- C-01 (integration): `/ops` render includes connector row markers for live multi-channel channels.
- C-02 (functional): connector row markers include deterministic id + `data-channel` + `data-mode` + `data-liveness` + counters.
- C-03 (integration): `/ops` render with missing connector state includes fallback connector row markers.
- C-04 (regression): phase-1A..1J command-center suites remain green.

## Success Metrics / Observable Signals
- `cargo test -p tau-dashboard-ui functional_spec_2822 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2822 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2786 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2794 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2798 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2802 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2806 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2810 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2814 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2818 -- --test-threads=1` passes.

## Approval Gate
P1 multi-module slice proceeds with spec marked `Reviewed` per AGENTS.md self-acceptance rule. Human review required in PR.

## Implementation Notes
- Added `TauOpsDashboardConnectorHealthRow` and `connector_health_rows` to command-center snapshot contracts in `tau-dashboard-ui`.
- Rendered SSR connector health table markers:
  - `id="tau-ops-connector-health-table"`
  - `id="tau-ops-connector-table-body"`
  - `id="tau-ops-connector-row-<index>"` with `data-channel`, `data-mode`, `data-liveness`, `data-events-ingested`, `data-provider-failures`.
- Gateway now maps multi-channel connector status (`collect_gateway_multi_channel_status_report`) into command-center connector rows with deterministic `none/unknown` fallback.
