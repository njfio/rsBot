# Channel Store Admin Split Map (M25)

- Generated at (UTC): `2026-02-16T00:00:00Z`
- Source file: `crates/tau-ops/src/channel_store_admin.rs`
- Target line budget: `2200`
- Current line count: `2898`
- Current gap to target: `698`
- Estimated lines to extract: `1340`
- Estimated post-split line count: `1558`

## Extraction Phases

| Phase | Owner | Est. Reduction | Depends On | Modules | Notes |
| --- | --- | ---: | --- | --- | --- |
| phase-1-operator-control-summary-diff (Operator control summary and diff workflows) | ops-runtime-control-plane | 760 | - | channel_store_admin/operator_control_helpers.rs | Move summary/diff aggregation logic without changing status gates, reason codes, or snapshot semantics. |
| phase-2-status-collector-domains (Status collector domains for dashboard/multi/gateway/deployment) | ops-runtime-collectors | 360 | phase-1-operator-control-summary-diff | channel_store_admin/status_collectors.rs, channel_store_admin/status_types.rs | Preserve report fields and rollout-gate classification behavior for each component status collector. |
| phase-3-cycle-summary-loaders (Cycle-report and log-summary loader helpers) | ops-runtime-observability | 220 | phase-2-status-collector-domains | channel_store_admin/cycle_summary_loaders.rs, channel_store_admin/log_summary_loaders.rs | Keep fail-closed parse behavior, invalid-line counting, and summary counters stable. |

## Public API Impact

- Keep execute_channel_store_admin_command entrypoint behavior and CLI contract stable.
- Preserve rendered/JSON report field names for dashboard, multi-channel, multi-agent, gateway, custom-command, voice, deployment, and operator summary views.
- Retain operator summary snapshot save/load and drift diff semantics.

## Import Impact

- Introduce focused modules under crates/tau-ops/src/channel_store_admin/ with selective imports from channel_store_admin.rs.
- Keep existing helper-module boundaries for command parsing, rendering, and transport health while adding operator-control extraction boundaries.
- Limit cross-module coupling by centralizing shared count/normalization helpers in channel_store_admin.rs.

## Test Migration Plan

| Order | Step | Command | Expected Signal |
| ---: | --- | --- | --- |
| 1 | guardrail-threshold-enforcement: Raise channel_store_admin split guardrail from legacy <3000 to M25 target <2200 and assert extracted module markers. | scripts/dev/test-channel-store-admin-domain-split.sh | channel_store_admin.rs fails closed when line budget or module markers regress |
| 2 | ops-crate-targeted-regression: Run focused channel-store admin tests by unit/functional/integration/regression slices. | cargo test -p tau-ops channel_store_admin::tests::<targeted_test> | operator summary/reporting behavior remains stable after extraction |
| 3 | operator-summary-roundtrip-validation: Validate operator summary snapshot roundtrip and compare flows remain deterministic. | cargo test -p tau-ops channel_store_admin::tests::integration_operator_control_summary_snapshot_roundtrip_and_compare -- --nocapture | snapshot persistence and diff generation parity is maintained |
