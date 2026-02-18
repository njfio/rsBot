# Plan #2498

## Approach
1. Add C-01..C-04 tests with `spec_2497` naming.
2. Run RED command and capture failing output.
3. Implement #2497 behavior.
4. Run GREEN command and capture passing output.

## Risks / Mitigations
- Risk: RED run passes unexpectedly due weak assertions.
  Mitigation: assert digest length/format, rerun skip counters, and file-retention behavior.

## Interfaces / Contracts
- Evidence-only subtask bound to #2497.
