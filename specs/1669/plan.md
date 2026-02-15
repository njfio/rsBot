# Issue 1669 Plan

Status: Reviewed

## Approach

1. Add a new `gae` module in `tau-algorithm` with:
   - `GaeConfig` and parser helper(s)
   - slice-based GAE computation function
   - trajectory-to-`AdvantageBatch` adapter
2. Implement backward-recursive GAE with configurable gamma/lambda and optional
   normalization/clipping for advantages and returns.
3. Add deterministic tests for:
   - known example conformance
   - normalization/clipping behavior
   - invalid input/missing-value fail-closed paths.

## Affected Areas

- `crates/tau-algorithm/src/lib.rs`
- `crates/tau-algorithm/src/gae.rs` (new)
- `specs/1669/{spec,plan,tasks}.md`

## Risks And Mitigations

- Risk: ambiguity around bootstrap behavior for terminal steps.
  - Mitigation: explicit done-mask semantics and deterministic test vectors.
- Risk: normalization instability on near-constant advantages.
  - Mitigation: epsilon guard in std-dev denominator.

## ADR

No architecture/protocol/dependency boundary change. ADR not required.
