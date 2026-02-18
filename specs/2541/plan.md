# Plan #2541

1. Add a local-runtime bridge module that:
   - tracks last effective heartbeat interval,
   - detects profile-store changes,
   - resolves active profile heartbeat policy values,
   - emits deterministic outcomes.
2. On applied interval change, write `<runtime-heartbeat-state>.policy.toml` atomically.
3. Wire bridge lifecycle into `run_local_runtime` startup/shutdown.
4. Implement conformance/regression tests first (RED), then implementation (GREEN), then cleanup.

## Risks
- Profile store may be absent/malformed during runtime.
- Runtime may receive repeated file-system updates with unchanged content.

## Mitigations
- Fail closed to last-known-good interval.
- Use deterministic no-op checks before writing policy artifacts.
- Keep scope to heartbeat interval only.
