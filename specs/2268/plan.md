# Plan #2268

Status: Reviewed
Spec: specs/2268/spec.md

## Approach

1. Add shared low-level log-rotation utility in `tau-core`:
   - policy struct (`max_bytes`, `max_files`)
   - env-driven policy resolution with defaults
   - append-with-rotation helper for NDJSON lines.
2. Add RED conformance tests for helper behavior (threshold, retention, env
   parsing).
3. Integrate helper into runtime operational appenders:
   - `tau-runtime`: heartbeat, background jobs, tool audit, prompt telemetry.
   - transport runtimes: dashboard, gateway, deployment, multi-agent,
     multi-channel, custom-command, voice.
4. Add integration/regression tests for representative runtime appenders to
   prove rotation does not break writes.
5. Update runtime/operator docs with controls, defaults, and retained file
   naming.
6. Run scoped verification on touched crates and collect RED/GREEN evidence.

## Affected Modules

- `crates/tau-core/src/*` (new rotation utility)
- `crates/tau-runtime/src/{observability_loggers_runtime.rs,heartbeat_runtime.rs,background_jobs_runtime.rs}`
- `crates/tau-dashboard/src/dashboard_runtime.rs`
- `crates/tau-gateway/src/gateway_runtime.rs`
- `crates/tau-deployment/src/deployment_runtime.rs`
- `crates/tau-orchestrator/src/multi_agent_runtime.rs`
- `crates/tau-multi-channel/src/multi_channel_runtime/routing.rs`
- `crates/tau-custom-command/src/custom_command_runtime.rs`
- `crates/tau-voice/src/voice_runtime.rs`
- `docs/guides/*` (runtime ops guidance)
- `specs/2268/*`

## Risks and Mitigations

- Risk: rotation integration accidentally changes non-operational transcript
  behavior.
  - Mitigation: scope helper calls to runtime/event/audit appenders only.
- Risk: file rename semantics differ across platforms when destination exists.
  - Mitigation: remove destination before rename and validate in unit tests.
- Risk: excessive rotation under low thresholds affects performance.
  - Mitigation: provide tunable max-bytes/max-files controls with safe defaults.

## Interfaces / Contracts

- New environment controls:
  - `TAU_LOG_ROTATION_MAX_BYTES` (u64, default documented)
  - `TAU_LOG_ROTATION_MAX_FILES` (usize, default documented)
- New retained-file convention:
  - active: `<name>.jsonl`
  - backups: `<name>.jsonl.1`, `<name>.jsonl.2`, ...
