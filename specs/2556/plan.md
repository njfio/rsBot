# Plan #2556

1. Set `ToolPolicy::new` default `memory_embedding_provider` to `local`.
2. Keep explicit remote-provider override path intact for env-driven policy resolution.
3. Ensure remote API-base/key fallback logic only applies to remote providers and not local default mode.
4. Add/update conformance tests first (RED), then implement minimal behavior changes (GREEN), then refactor.
5. Update memory operations runbook to document default-local behavior and explicit remote overrides.
6. Run full verification gates and record evidence for PR.

## Risks
- Defaulting provider to local may unintentionally alter diagnostics and downstream assumptions that expected `None`.
- Remote fallback field population may interfere with local defaults if not constrained.

## Mitigations
- Add explicit tests for default-local policy JSON and startup diagnostics.
- Constrain remote fallback wiring to remote providers only.
