# Tools Runtime Split Map (M25)

This guide defines the pre-extraction split plan for:

- `crates/tau-tools/src/tools.rs`

Goal:

- reduce the primary file below 3000 LOC in `#2042` while preserving tool
  behavior and external runtime contracts.

## Generate Artifacts

```bash
scripts/dev/tools-split-map.sh
```

Default outputs:

- `tasks/reports/m25-tools-split-map.json`
- `tasks/reports/m25-tools-split-map.md`

Schema:

- `tasks/schemas/m25-tools-split-map.schema.json`

Deterministic replay:

```bash
scripts/dev/tools-split-map.sh \
  --generated-at 2026-02-16T00:00:00Z \
  --output-json /tmp/m25-tools-split-map.json \
  --output-md /tmp/m25-tools-split-map.md
```

## Validation

```bash
scripts/dev/test-tools-split-map.sh
python3 -m unittest discover -s .github/scripts -p "test_tools_split_map_contract.py"
```

## Public API Impact

- Exported tool type names and trait implementations remain stable for runtime
  callers.
- Existing JSON argument/return contracts for moved tools remain unchanged.
- Policy gate result semantics and error envelopes remain behaviorally
  compatible.

## Import Impact

- Domain modules are extracted into `crates/tau-tools/src/tools/`.
- `tools.rs` keeps targeted re-exports while phased moves are applied.
- Each phase minimizes cross-domain import churn and preserves call-site
  stability.

## Test Migration Plan

- Guardrail update: enforce `tools.rs` split threshold ending at `<3000`.
- Crate-level validation: run `cargo test -p tau-tools` after each extraction
  phase.
- Cross-crate regression: run `cargo test -p tau-coding-agent` after each
  phase to confirm runtime tool consumption parity.
