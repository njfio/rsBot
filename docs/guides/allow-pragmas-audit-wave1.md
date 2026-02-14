## `#[allow(...)]` Audit - Wave 1

Date: 2026-02-14  
Issue: #1395

### Scope

- Inventory all `#[allow(...)]` usages under `crates/`.
- Remove safe stale suppressions where possible.
- Document rationale for retained suppressions.

### Inventory Method

Command used:

```bash
rg -n "#\\!?\\[allow\\([^\\]]+\\)\\]" crates -g '*.rs'
```

### Summary

- Baseline before this change: `12` allow pragmas.
- Current after this change: `11` allow pragmas.
- Lint kinds observed: only `clippy::too_many_arguments`.

### Removal Completed

- Removed `#[allow(clippy::too_many_arguments)]` from:
  - `crates/tau-orchestrator/src/orchestrator.rs` (trace emitter helper)
- Change made:
  - Replaced the wide argument list in `emit_route_trace(...)` with a typed `RouteTraceEvent` struct payload.
- Result:
  - Same behavior and output shape, one suppression removed.

### Current Remaining Pragmas (11)

1. `crates/tau-coding-agent/src/commands.rs:204`
   Function: `handle_command_with_session_import_mode`
   Rationale: central command dispatcher depends on many runtime inputs by design.
2. `crates/tau-coding-agent/src/orchestrator_bridge.rs:71`
   Function: `run_plan_first_prompt`
   Rationale: bridge entrypoint explicitly threads orchestrator/runtime controls.
3. `crates/tau-coding-agent/src/orchestrator_bridge.rs:108`
   Function: `run_plan_first_prompt_with_policy_context`
   Rationale: explicit policy context wiring for deterministic orchestration behavior.
4. `crates/tau-coding-agent/src/orchestrator_bridge.rs:147`
   Function: `run_plan_first_prompt_with_policy_context_and_routing`
   Rationale: routing + policy + runtime options remain intentionally explicit.
5. `crates/tau-coding-agent/src/runtime_loop.rs:509`
   Function: `run_plan_first_prompt_with_runtime_hooks`
   Rationale: runtime loop hook entrypoint coordinates several independent controls.
6. `crates/tau-onboarding/src/startup_dispatch.rs:161`
   Function: `resolve_startup_runtime_from_cli`
   Rationale: generic dependency injection API for testable startup resolution.
7. `crates/tau-onboarding/src/startup_dispatch.rs:227`
   Function: `execute_startup_runtime_from_cli_with_modes`
   Rationale: explicit injected callbacks for deterministic startup mode execution.
8. `crates/tau-orchestrator/src/orchestrator.rs:52`
   Function: `run_plan_first_prompt`
   Rationale: public orchestrator API exposes all policy/runtime controls.
9. `crates/tau-orchestrator/src/orchestrator.rs:83`
   Function: `run_plan_first_prompt_with_policy_context`
   Rationale: policy-context variant retains explicit control surfaces.
10. `crates/tau-orchestrator/src/orchestrator.rs:117`
    Function: `run_plan_first_prompt_with_policy_context_and_routing`
    Rationale: routing and fallback behavior require explicit caller-supplied limits.
11. `crates/tau-orchestrator/src/orchestrator.rs:375`
    Function: `run_routed_prompt_with_fallback`
    Rationale: route execution still coordinates multiple independent runtime controls.

### Validation

- `cargo fmt --all`
- `cargo clippy -p tau-orchestrator --all-targets -- -D warnings`
- `cargo clippy -p tau-coding-agent --all-targets -- -D warnings`
- `cargo clippy -p tau-onboarding --all-targets -- -D warnings`
- `cargo test -p tau-orchestrator run_plan_first_prompt -- --nocapture`

### Next Wave

- Evaluate replacing wide argument lists in onboarding/orchestrator bridge with typed option/context structs to further reduce `clippy::too_many_arguments` suppressions.
