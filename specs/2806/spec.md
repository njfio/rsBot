# Spec: Issue #2806 - Command-center live-data SSR markers from dashboard snapshot

Status: Implemented

## Problem Statement
Tau Ops shell currently renders command-center placeholders instead of snapshot-backed values. The health badge and KPI widgets do not reflect live dashboard data, and alert/timeline sections are static placeholders. This blocks PRD validation for command-center live observability.

## Acceptance Criteria

### AC-1 Health badge markers reflect dashboard snapshot health data
Given `/ops` shell render with dashboard snapshot state,
When SSR HTML is generated,
Then health badge markers expose snapshot health state and reason values.

### AC-2 Six KPI stat-card markers expose live snapshot metrics
Given `/ops` shell render,
When SSR HTML is generated,
Then exactly six command-center KPI stat cards expose numeric markers derived from dashboard snapshot metrics.

### AC-3 Alerts feed and queue timeline markers reflect snapshot data
Given `/ops` shell render,
When SSR HTML is generated,
Then alert and queue timeline sections expose marker counts/entries derived from dashboard snapshot alerts/timeline payloads.

### AC-4 Existing phase-1B/1C/1D/1E/1F contracts remain stable
Given existing auth, route, control-marker, and query-state contracts,
When command-center live-data markers are integrated,
Then those tests remain green.

## Scope

### In Scope
- SSR shell context extension for command-center snapshot data.
- Gateway mapping from `collect_gateway_dashboard_snapshot` output into shell context.
- Integration tests for health/KPI/alerts/timeline markers in `/ops` shell output.

### Out of Scope
- Client-side realtime polling/stream hydration.
- Visual redesign of command center layout.
- Additional gateway APIs.

## Conformance Cases
- C-01 (integration): `/ops` shell includes health badge markers matching snapshot health state/reason.
- C-02 (integration): `/ops` shell includes six KPI markers with snapshot-derived values.
- C-03 (integration): `/ops` shell includes alert-feed and queue-timeline markers with snapshot-derived counts/content.
- C-04 (regression): phase-1B/1C/1D/1E/1F suites remain green.

## Success Metrics / Observable Signals
- `cargo test -p tau-gateway functional_spec_2806 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2786 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2794 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2798 -- --test-threads=1` passes.
- `cargo test -p tau-gateway functional_spec_2802 -- --test-threads=1` passes.

## Approval Gate
P1 multi-module slice proceeds with spec marked `Reviewed` per AGENTS.md self-acceptance rule.
