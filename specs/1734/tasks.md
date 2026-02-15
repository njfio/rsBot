# Issue 1734 Tasks

Status: Implemented

## Ordered Tasks

T1 (tests-first): add helper contract test fixture for anti-pattern detection
and suppression behavior.

T2: implement policy-driven helper command with JSON/Markdown outputs.

T3: document false-positive handling workflow in remediation guide.

T4: run helper tests and publish baseline helper artifacts.

## Tier Mapping

- Functional: helper detects configured anti-patterns in fixture and repo scan
- Conformance: JSON output contract includes deterministic finding fields
- Integration: remediation guide documents helper and suppression process
- Regression: suppression entries prevent known false-positive findings
