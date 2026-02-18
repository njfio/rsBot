# Plan #2502

## Approach
1. Add C-01..C-05 tests with `spec_2503` naming.
2. Run RED command and capture failing output.
3. Implement #2503 behavior.
4. Rerun same command and capture GREEN output.

## Risks / Mitigations
- Risk: RED run passes unexpectedly due weak assertions.
  Mitigation: assert no-change diagnostics, tool-call parsed output, rerun skip counters, and retry-safe failure behavior.

## Interfaces / Contracts
- Evidence-only subtask bound to #2503.
