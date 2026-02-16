# Channel Store Admin Split Map (M25)

This guide defines the pre-extraction split plan for:

- `crates/tau-ops/src/channel_store_admin.rs`

Goal:

- reduce the primary file below 2200 LOC in `#2044` while preserving
  channel-store admin command behavior and operator-control status contracts.

## Generate Artifacts

```bash
scripts/dev/channel-store-admin-split-map.sh
```

Default outputs:

- `tasks/reports/m25-channel-store-admin-split-map.json`
- `tasks/reports/m25-channel-store-admin-split-map.md`

Schema:

- `tasks/schemas/m25-channel-store-admin-split-map.schema.json`

Deterministic replay:

```bash
scripts/dev/channel-store-admin-split-map.sh \
  --generated-at 2026-02-16T00:00:00Z \
  --output-json /tmp/m25-channel-store-admin-split-map.json \
  --output-md /tmp/m25-channel-store-admin-split-map.md
```

## Validation

```bash
scripts/dev/test-channel-store-admin-split-map.sh
python3 -m unittest discover -s .github/scripts -p "test_channel_store_admin_split_map_contract.py"
```

## Public API Impact

- `execute_channel_store_admin_command` behavior and CLI argument handling stay
  stable.
- Rendered and JSON status reports preserve field names/semantics.
- Operator-control summary snapshot and compare workflows keep existing
  contracts.

## Import Impact

- Domain modules are extracted into
  `crates/tau-ops/src/channel_store_admin/`.
- `channel_store_admin.rs` keeps stable imports/entrypoints while delegating to
  extracted helper modules.
- Shared helper patterns remain centralized to control import fan-out.

## Test Migration Plan

- Guardrail update: enforce `channel_store_admin.rs` split threshold ending at
  `<2200`.
- Crate-level validation: run targeted channel-store admin tests for
  unit/functional/integration/regression slices.
- Snapshot roundtrip parity: run operator-control snapshot compare integration
  test after extraction.
