# Tasks #2265

Status: In Progress
Spec: specs/2265/spec.md
Plan: specs/2265/plan.md

- T1 (tests first): add RED completion contract tests for C-01..C-04
  (`scripts/release/test-shell-completions.sh` + workflow contract assertions).
- T2: add CLI completion flag/type and completion renderer helper.
- T3: add startup dispatch short-circuit for completion generation.
- T4: add release completion generation script and make RED tests pass.
- T5: wire release workflow + CI release-helper scope for completion assets.
- T6: update docs (`README.md`, `docs/guides/release-automation-ops.md`) for
  completion install guidance.
- T7: run scoped verification and collect PR evidence:
  - `./scripts/release/test-shell-completions.sh`
  - `./scripts/release/test-release-workflow-contract.sh`
  - `./scripts/release/test-install-helpers.sh`
  - `CARGO_TARGET_DIR=target-fast-2265 cargo check -p tau-cli`
  - `CARGO_TARGET_DIR=target-fast-2265 cargo test -p tau-coding-agent unit_normalize_daemon_subcommand_args_maps_action_and_alias_flags`
  - `CARGO_TARGET_DIR=target-fast-2265 cargo run -q -p tau-coding-agent -- --shell-completion bash`
  - `cargo fmt --check`.
