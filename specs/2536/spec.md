# Spec #2536 - Task: add G15 profile routing schema + dispatch override policy

Status: Implemented

## Problem Statement
G15 checklist items for process model routing, task overrides, and complexity-based dispatch remain unimplemented outside the role-route-table path.

## Acceptance Criteria
### AC-1 routing schema availability
Given profile defaults deserialize from persisted store, when routing fields are present, then `channel_model`, `branch_model`, `worker_model`, `compactor_model`, `cortex_model`, and `task_overrides` are loaded with backward-compatible defaults.

### AC-2 complexity + override policy
Given a prompt and process type, when policy resolves effective model, then deterministic keyword-based complexity classification and task override selection produce stable model resolution.

### AC-3 dispatch-time scoped application
Given an effective model differs from baseline, when prompt dispatch executes, then agent dispatch swaps to the routed model for that run and restores baseline after completion.

### AC-4 fail-closed backward compatibility
Given routing config is absent or incomplete, when prompt dispatch executes, then baseline model behavior remains unchanged.

## Scope
In scope:
- `crates/tau-onboarding/src/startup_config.rs`
- `crates/tau-coding-agent/src/runtime_loop.rs`
- `crates/tau-coding-agent/src/startup_local_runtime.rs`
- related tests

Out of scope:
- Provider catalog/model availability validation.
- Non-local runtime surfaces.

## Conformance Cases
- C-01 (AC-1, functional): `spec_2536_c01_profile_defaults_parse_routing_fields`
- C-02 (AC-2, unit): `spec_2536_c02_prompt_complexity_and_task_override_select_model`
- C-03 (AC-3, integration): `spec_2536_c03_dispatch_uses_scoped_model_override_and_restores_baseline`
- C-04 (AC-4, regression): `regression_2536_default_profile_without_routing_keeps_baseline_model`

## Success Metrics
- C-01..C-04 pass.
- Full workspace tests pass.
- Diff-scoped mutation run reports zero missed mutants.
- Live validation script passes.
