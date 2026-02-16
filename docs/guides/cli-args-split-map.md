# CLI Args Split Map (M25)

This guide defines the pre-extraction split plan for:

- `crates/tau-cli/src/cli_args.rs`

Goal:

- reduce the primary file below 3000 LOC in `#2040` while preserving CLI
  behavior and external parser contract.

## Generate Artifacts

```bash
scripts/dev/cli-args-split-map.sh
```

Default outputs:

- `tasks/reports/m25-cli-args-split-map.json`
- `tasks/reports/m25-cli-args-split-map.md`

Schema:

- `tasks/schemas/m25-cli-args-split-map.schema.json`

Deterministic replay:

```bash
scripts/dev/cli-args-split-map.sh \
  --generated-at 2026-02-16T00:00:00Z \
  --output-json /tmp/m25-cli-args-split-map.json \
  --output-md /tmp/m25-cli-args-split-map.md
```

## Validation

```bash
scripts/dev/test-cli-args-split-map.sh
python3 -m unittest discover -s .github/scripts -p "test_cli_args_split_map_contract.py"
```

## Public API Impact

- `pub struct Cli` remains the external parse surface.
- Existing clap flag names, aliases, defaults, and env bindings are preserved.
- New internal domain structs are flattened into `Cli`; they are implementation
  detail boundaries, not external API changes.

## Import Impact

- Domain modules are extracted into `crates/tau-cli/src/cli_args/`.
- `cli_args.rs` keeps root parser helpers and selective `pub use` re-exports
  during phased extraction.
- Each phase minimizes cross-domain import churn before final consolidation.

## Test Migration Plan

- Guardrail update: lower line budget gates from `<4000` to phased thresholds
  ending at `<3000`.
- Crate-level validation: run `cargo test -p tau-cli` after each extraction
  phase.
- Cross-crate regression: run `cargo test -p tau-coding-agent` after each
  phase to confirm CLI consumption parity.
