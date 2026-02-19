# Plan #2544

1. Reproduce failing heartbeat hot-reload convergence tests to capture RED baseline.
2. Add deterministic policy-file fingerprint fallback for watcher-miss scenarios in hot-reload state.
3. Preserve fail-closed semantics when no watcher/polling context exists.
4. Add targeted regression/conformance tests for fallback behavior.
5. Run scoped and workspace verification gates.

## Risks
- Platform-specific file notification behavior can be nondeterministic.
- Over-eager fallback polling could break pending-reload guard semantics.

## Mitigations
- Gate fallback polling behind active watcher context.
- Keep existing pending-reload contract test and add explicit regression for guard behavior.
