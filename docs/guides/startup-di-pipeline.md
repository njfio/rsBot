# Startup Dependency Injection Pipeline

<!-- architecture-doc:startup-di -->

This guide documents the startup dependency-injection (DI) path used by Tau runtime entrypoints.

Source of truth:

- `crates/tau-coding-agent/src/startup_dispatch.rs`
- `crates/tau-onboarding/src/startup_dispatch.rs`
- `crates/tau-onboarding/src/startup_preflight.rs`
- `crates/tau-startup/src/lib.rs`

## 3-stage resolution overview

| Stage | Entry API | Primary output |
| --- | --- | --- |
| Stage 1 | `execute_startup_preflight` | Early command handling and short-circuit |
| Stage 2 | `resolve_startup_model_runtime_from_cli` + `resolve_startup_runtime_dispatch_context_from_cli` | `StartupRuntimeResolution` (model, catalog, client, runtime context) |
| Stage 3 | `execute_startup_runtime_modes` | Transport runtime path or local runtime path |

## Stage 1: Preflight command gate

`run_cli` in `tau-coding-agent` calls `execute_startup_preflight` first.  
If any preflight command is selected, startup ends immediately with no model/client boot.

```mermaid
sequenceDiagram
    participant CLI as Cli
    participant Entry as run_cli
    participant Onboarding as tau_onboarding::execute_startup_preflight
    participant Startup as tau_startup::execute_startup_preflight
    participant Actions as StartupPreflightActions

    CLI->>Entry: parse + run
    Entry->>Onboarding: execute_startup_preflight(&cli, callbacks)
    Onboarding->>Startup: execute_startup_preflight(&cli, actions)
    Startup->>Actions: execute_<command>() when a preflight flag matches
    alt command handled
        Startup-->>Entry: Ok(true)
        Entry-->>CLI: return Ok(())
    else no preflight command
        Startup-->>Entry: Ok(false)
    end
```

## Stage 2: Dependency and context resolution

When Stage 1 returns `false`, startup builds runtime dependencies in `execute_startup_runtime_from_cli_with_modes`.

Resolution fan-in:

1. Model stack: `resolve_startup_models`, `resolve_startup_model_catalog`, `validate_startup_model_catalog`
2. Client: `build_client_with_fallbacks`
3. Runtime context: `build_startup_runtime_dispatch_context` (skills dir/lock, system prompt, startup policy)

```mermaid
sequenceDiagram
    participant Entry as run_cli
    participant Resolver as execute_startup_runtime_from_cli_with_modes
    participant Model as resolve_startup_model_runtime_from_cli
    participant Dispatch as resolve_startup_runtime_dispatch_context_from_cli

    Entry->>Resolver: request (callbacks for model/client/context)
    Resolver->>Model: resolve model_ref + fallbacks + catalog + client
    Model-->>Resolver: StartupModelRuntimeResolution
    Resolver->>Dispatch: resolve skills + prompt + policy context
    Dispatch-->>Resolver: StartupRuntimeDispatchContext
    Resolver-->>Entry: StartupRuntimeResolution
```

## Stage 3: Runtime mode dispatch

`execute_startup_runtime_modes` performs the final decision:

1. Try `run_transport_mode_if_requested`
2. If not selected, execute local runtime closure (`run_training_proxy_mode_if_requested`, `run_training_mode_if_requested`, then `run_local_runtime`)

```mermaid
sequenceDiagram
    participant Modes as execute_startup_runtime_modes
    participant Transport as run_transport_mode_if_requested
    participant Local as run_local_runtime closure

    Modes->>Transport: check transport flags
    alt transport mode selected
        Transport-->>Modes: Ok(true)
        Modes-->>Modes: return Ok(())
    else no transport mode
        Transport-->>Modes: Ok(false)
        Modes->>Local: execute local/training path
        Local-->>Modes: Ok(())
    end
```

## Validation snippets

```bash
# Stage 1 command short-circuit behavior
cargo test -p tau-onboarding \
  startup_preflight::tests::unit_execute_startup_preflight_onboard_calls_callback

# Stage 2 model/client/context resolution behavior
cargo test -p tau-onboarding \
  startup_dispatch::tests::unit_resolve_startup_model_runtime_from_cli_returns_composed_outputs

# Stage 3 mode split behavior
cargo test -p tau-onboarding \
  startup_dispatch::tests::functional_execute_startup_runtime_modes_short_circuits_local_when_transport_handles
```

