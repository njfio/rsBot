# Plan #2493

## Approach
1. Add C-01..C-04 tests with `spec_2492` naming.
2. Run RED command and capture failing output.
3. Implement #2492 behavior.
4. Run GREEN command and capture passing output.

## Risks / Mitigations
- Risk: RED run passes unexpectedly due weak assertions.
  Mitigation: assert deterministic chunk checkpoint and file lifecycle outcomes.

## Interfaces / Contracts
- Evidence-only subtask bound to #2492.
