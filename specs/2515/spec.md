# Spec #2515 - Subtask: RED/GREEN + live validation evidence for G12

Status: Accepted

## Problem Statement
G12 closure requires explicit RED/GREEN and validation artifacts attached to the issue/PR chain.

## Acceptance Criteria
### AC-1
Given G12 conformance tests, when run in RED/GREEN sequence, then evidence is captured in issue/PR notes.

### AC-2
Given merged implementation, when live validation executes, then demo script passes.

## Conformance Cases
- C-01 (AC-1): RED/GREEN command outputs recorded.
- C-02 (AC-2): `./scripts/demo/local.sh ...` passes.

## Success Metrics
- Evidence posted in issue comments and PR template.
