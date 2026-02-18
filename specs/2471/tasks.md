# Tasks #2471 - add startup prompt template loader/renderer in tau-onboarding

1. T1 (RED): add C-01..C-03 conformance tests to `startup_prompt_composition` and capture failing run.
2. T2 (GREEN): implement template loader + renderer + fallback behavior.
3. T3 (REFACTOR): keep helpers deterministic and isolate rendering logic.
4. T4 (VERIFY): run scoped fmt/clippy/tests/mutation and record evidence.
