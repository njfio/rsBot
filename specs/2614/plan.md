# Plan: Issue #2614 - Build production dashboard UI (G18) with auth and live status views

## Approach
1. Add RED assertions in gateway webchat rendering tests for new dashboard UI controls/elements.
2. Extend `render_gateway_webchat_page` HTML + client JS with:
   - dashboard tab panel,
   - authenticated dashboard snapshot/action fetch helpers,
   - live polling toggle + interval handling,
   - in-panel success/error rendering.
3. Keep backend dashboard endpoint contracts unchanged and rely on existing integration tests for behavior continuity.
4. Run scoped verification gates for `tau-gateway` plus dashboard-focused tests.

## Affected Modules
- `crates/tau-gateway/src/gateway_openresponses/webchat_page.rs`
- `crates/tau-gateway/src/gateway_openresponses/tests.rs`
- `specs/2614/spec.md`
- `specs/2614/plan.md`
- `specs/2614/tasks.md`

## Risks / Mitigations
- Risk: UI polling introduces noisy requests.
  - Mitigation: operator-controlled live toggle + bounded minimum interval.
- Risk: auth failures degrade UX without clarity.
  - Mitigation: show errors directly in dashboard panel and preserve raw payload detail.
- Risk: UI additions regress existing webchat/test expectations.
  - Mitigation: extend existing gateway tests and keep route contracts untouched.

## Interfaces / Contracts
- Reuse existing endpoints:
  - `GET /dashboard/health`
  - `GET /dashboard/widgets`
  - `GET /dashboard/queue-timeline`
  - `GET /dashboard/alerts`
  - `POST /dashboard/actions`
  - `GET /dashboard/stream` (backend continuity, no JS EventSource requirement in this slice)
- UI control IDs + JS handlers in `webchat_page.rs` become the dashboard surface contract.

## ADR
- Not required (UI extension over existing endpoint contract, no new dependency/protocol).
