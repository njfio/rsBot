# Spec: Issue #2782 - PRD Phase 1A Leptos crate and /ops shell integration

Status: Implemented

## Problem Statement
The Tau Ops Dashboard PRD requires a Leptos-based dashboard foundation, but the repository currently serves dashboard/webchat shells from static HTML templates. A first migration slice is needed to establish a Leptos SSR crate and wire an `/ops` route in gateway without destabilizing existing runtime APIs.

## Acceptance Criteria

### AC-1 `tau-dashboard-ui` crate exists and is wired into workspace
Given workspace manifests,
When #2782 is implemented,
Then a new crate `crates/tau-dashboard-ui` is present, added to workspace members, and compiles in scoped checks.

### AC-2 Leptos SSR shell renders command-center baseline markers
Given the new dashboard UI crate,
When its SSR render entrypoint is invoked,
Then output includes deterministic shell markers for header/sidebar/command-center sections.

### AC-3 Gateway exposes `/ops` shell endpoint backed by Leptos render function
Given gateway router configuration,
When requesting `/ops`,
Then gateway returns HTML from `tau-dashboard-ui` SSR render function.

### AC-4 Existing dashboard shell route remains stable
Given current `/dashboard` route behavior,
When `/ops` is added,
Then existing `/dashboard` route tests remain green and unchanged behavior is preserved.

## Scope

### In Scope
- Workspace/dependency setup for Leptos SSR dashboard UI crate.
- Minimal SSR command-center shell component/layout.
- Gateway `/ops` route integration.
- Tests for crate render markers and endpoint response.

### Out of Scope
- Full 14-view dashboard implementation.
- WASM hydration pipeline and cargo-leptos production packaging.
- Replacing existing `/dashboard` static shell.

## Conformance Cases
- C-01 (conformance): workspace includes new crate and dependency declarations.
- C-02 (functional): crate render function emits required shell markers.
- C-03 (integration): gateway `/ops` route returns shell HTML and 200 status.
- C-04 (regression): existing `/dashboard` shell endpoint still returns baseline markers.

## Success Metrics / Observable Signals
- `cargo test -p tau-dashboard-ui -- --test-threads=1` passes.
- `cargo test -p tau-gateway -- gateway_openresponses::tests::functional_dashboard_shell_endpoint_returns_html_shell --test-threads=1` remains green.
- New `/ops` endpoint is discoverable and test-covered.

## Approval Gate
This task introduces new dependencies (`leptos`) and proceeds under explicit user direction to continue contract execution end-to-end.
