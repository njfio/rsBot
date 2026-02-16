# Plan #2042

Status: Implemented
Spec: specs/2042/spec.md

## Approach

1. Produce deterministic split-map planning artifacts for `tools.rs` (`#2062`).
2. Execute a high-volume domain extraction (`BashTool` + gate helpers) from
   `tools.rs` into `tools/bash_tool.rs` (`#2063`).
3. Add/execute line-budget guardrail checks plus integration contract evidence.

## Affected Modules

- `crates/tau-tools/src/tools.rs`
- `crates/tau-tools/src/tools/bash_tool.rs`
- `scripts/dev/tools-split-map.sh`
- `scripts/dev/test-tools-split-map.sh`
- `scripts/dev/test-tools-domain-split.sh`
- `tasks/schemas/m25-tools-split-map.schema.json`
- `tasks/reports/m25-tools-split-map.{json,md}`
- `docs/guides/tools-split-map.md`
- `.github/scripts/test_tools_split_map_contract.py`

## Risks and Mitigations

- Risk: split introduces tool behavior/policy regressions.
  - Mitigation: keep API exports stable and validate module/policy markers with
    guardrail checks.
- Risk: crate-scoped cargo checks blocked by existing branch compile drift.
  - Mitigation: anchor parity proof to split guardrails and post-split
    integration contract suite evidence.

## Interfaces and Contracts

- Keep `BashTool` public export stable via root-module re-export.
- Preserve tool JSON contracts and policy gate behavior markers.

## ADR References

- Not required.
