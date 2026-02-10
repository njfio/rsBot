# `tau-coding-agent` Contributor Code Map

This guide maps the `tau-coding-agent` source tree to responsibility areas so contributors can land changes quickly and safely.

## Entrypoint and Startup Pipeline

- Entrypoint: `crates/tau-coding-agent/src/main.rs`
- Startup orchestrator: `crates/tau-coding-agent/src/startup_dispatch.rs` (`run_cli`)

`run_cli` executes startup in this order:

1. `startup_preflight::execute_startup_preflight`
2. `startup_model_resolution::resolve_startup_models`
3. `provider_fallback::build_client_with_fallbacks`
4. `startup_skills_bootstrap::run_startup_skills_bootstrap`
5. `startup_prompt_composition::compose_startup_system_prompt`
6. `startup_policy::resolve_startup_policy`
7. `startup_transport_modes::run_transport_mode_if_requested`
8. `startup_local_runtime::run_local_runtime`

If a preflight or transport mode completes, runtime startup exits early.

## Module Map by Concern

### CLI surface and parsing

- `cli_args.rs`: all CLI flags and clap definitions.
- `cli_types.rs`: clap-facing enums and value types.
- `onboarding.rs`: first-run onboarding wizard and bootstrap report flow.
- `runtime_types.rs`: shared runtime/config structs used across modules.

Use this area when adding or changing flags, defaults, and typed CLI input behavior.

### Command system

- `commands.rs`: slash-command parsing/dispatch and command-file execution.
- `session_commands.rs`: session stats, diff, search command behaviors.
- `session_graph_commands.rs`: session graph export formats.
- `session_navigation_commands.rs`: bookmarks and branch aliases.
- `skills_commands.rs`: skills lifecycle commands.
- `auth_commands.rs`: provider auth command handling.
- `release_channel_commands.rs`: `/release-channel` command parsing/rendering and dispatch.
- `release_channel_commands/cache.rs`: release lookup cache load/save/prune helpers.
- `diagnostics_commands.rs`: doctor/audit/policy diagnostics commands.
- `macro_profile_commands.rs`: macro/profile command workflows.

Use this area when command UX, parser behavior, or command output changes.

### Runtime and output

- `runtime_loop.rs`: interactive loop, one-shot prompt execution, cancellation.
- `runtime_output.rs`: rendering helpers, JSON event conversion, persistence helpers.
- `startup_local_runtime.rs`: runtime assembly and handoff from startup.

Use this area for prompt-loop behavior, output formatting, and turn execution changes.

### Provider/auth and model routing

- `provider_client.rs`: provider client construction.
- `provider_fallback.rs`: model fallback routing and retry-aware client composition.
- `provider_auth.rs`: auth mode resolution and API key candidate logic.
- `provider_credentials.rs`: CLI/store-backed provider credential resolution.
- `credentials.rs`: encrypted credential store and integration credential flows.
- `startup_model_resolution.rs`: `--model` parse and fallback model resolution.

Use this area for provider onboarding, credential behavior, or fallback policy updates.

### Session and persistence

- `session.rs`: session storage and lineage model.
- `session_runtime_helpers.rs`: session initialization and reload helpers.
- `atomic_io.rs`: safe atomic file writes.
- `channel_store.rs`: channel-specific persisted logs/context state.
- `channel_store_admin.rs`: inspect/repair operations for ChannelStore.

Use this area for storage formats, integrity behavior, and session/channel durability.

### Skills and trust roots

- `skills.rs`: skill catalog loading, selection, install/sync, registry integration.
- `startup_skills_bootstrap.rs`: startup installs/sync and lockfile orchestration.
- `startup_prompt_composition.rs`: base prompt + selected skill prompt augmentation.
- `trust_roots.rs`: signed skill trust root parsing/mutations/persistence.

Use this area for skill packaging, verification, registry support, and lock workflows.

### Transports and integrations

- `github_issues.rs`: GitHub Issues bridge transport.
- `slack.rs`: Slack Socket Mode bridge transport.
- `events.rs`: scheduler runner and webhook immediate-event ingestion.
- `dashboard_contract.rs`: web dashboard/operator control-plane fixture/schema contract definitions and validators.
- `dashboard_runtime.rs`: dashboard runtime loop (state transitions, retries, dedupe, channel-store writes).
- `gateway_openresponses.rs`: OpenResponses HTTP server (`/v1/responses`) plus gateway auth/session (`/gateway/auth/session`) and webchat/status endpoints.
- `deployment_contract.rs`: cloud deployment + WASM deliverable fixture/schema contract definitions and validators.
- `deployment_runtime.rs`: deployment/WASM runtime loop (queueing, retries, dedupe, channel-store writes).
- `deployment_wasm.rs`: WASM artifact packaging, manifest verification, and deployment state deliverable tracking.
- `browser_automation_contract.rs`: browser automation fixture/schema contract definitions, capability checks, and replay evaluation.
- `browser_automation_runtime.rs`: browser automation runtime loop (queueing, retry, guardrails, dedupe, channel-store writes).
- `memory_contract.rs`: semantic-memory fixture/schema contract definitions and validators.
- `memory_runtime.rs`: semantic-memory runtime loop (state transitions, retries, dedupe, channel-store writes).
- `multi_channel_contract.rs`: multi-channel (Telegram/Discord/WhatsApp) fixture/schema contract.
- `multi_channel_runtime.rs`: multi-channel runtime loop (queueing, retry, dedupe, channel-store writes).
- `multi_channel_media.rs`: media understanding envelope normalization, bounded attachment processing, and reason-coded media summary/transcription contracts.
- `multi_channel_live_connectors.rs`: live provider ingress bridges (Telegram polling/webhook, Discord polling, WhatsApp webhook), connector liveness/error counters, and webhook server.
- `voice_contract.rs`: voice interaction + wake-word fixture/schema contract definitions and validators.
- `voice_runtime.rs`: voice runtime loop (wake-word/turn replay, retries, dedupe, channel-store writes).
- `runtime_cli_validation.rs`: validation for integration runtime flags.
- `startup_transport_modes.rs`: transport mode dispatch entry.

Use this area when adding a new integration channel or adjusting bridge behavior.

### Tool policy and observability

- `tools.rs`: built-in tool registration and tool policy primitives.
- `tool_policy_config.rs`: CLI/preset/env policy assembly.
- `observability_loggers.rs`: telemetry and tool audit log subscribers.
- `startup_policy.rs`: startup policy packaging and optional policy printing.

Use this area for tool permissions, audit semantics, and telemetry output changes.

### Shared helpers

- `startup_config.rs`: startup-time config bundles for auth/profile/doctor commands.
- `startup_resolution.rs`: shared prompt/trust root resolution helpers.
- `bootstrap_helpers.rs`: tracing bootstrap and CLI helper labels.
- `time_utils.rs`: timestamp and expiration utilities.

Use this area for narrow utility behavior reused across startup/runtime modules.

### Test surfaces

- `tests.rs`: large integration/regression suite for `tau-coding-agent`.
- `dashboard_contract.rs`: dashboard contract schema/fixture validation and replay contract tests.
- `dashboard_runtime.rs`: dashboard runtime tests for queueing, retries, idempotency, and health signals.
- `deployment_contract.rs`: deployment/WASM fixture/schema compatibility and replay contract tests.
- `deployment_runtime.rs`: deployment/WASM runtime tests for retries, idempotency, and health signals.
- `deployment_wasm.rs`: deployment WASM package/manifest tests (hash validation, constraints, regression guards).
- `browser_automation_contract.rs`: browser automation schema/fixture compatibility and replay contract tests.
- `browser_automation_runtime.rs`: fixture-driven browser automation runtime tests for guardrails, retries, and idempotency.
- `memory_contract.rs`: semantic-memory schema/fixture compatibility and replay contract tests.
- `memory_runtime.rs`: semantic-memory runtime tests for retries, idempotency, and health signals.
- `transport_conformance.rs`: replay conformance fixtures for bridge/scheduler flows.
- `multi_channel_contract.rs`: multi-channel (Telegram/Discord/WhatsApp) schema and fixture validation contract.
- `multi_channel_runtime.rs`: fixture-driven runtime tests covering queueing, retries, and replay idempotency.
- `multi_channel_live_connectors.rs`: connector module tests for polling/webhook ingest, signature verification, dedupe, and status reporting.
- `voice_contract.rs`: voice fixture/schema compatibility and replay contract tests.
- `voice_runtime.rs`: fixture-driven voice runtime tests covering queueing, retries, and replay idempotency.
- `#[cfg(test)]` exports in `main.rs`: test-only visibility for parser/helpers.

Prefer adding tests next to the module behavior being changed, plus regression coverage in `tests.rs` when behavior spans modules.

## Common Change Paths

- Add a new CLI flag:
  1. Update `cli_args.rs` (flag definition).
  2. Add/adjust type in `cli_types.rs` if needed.
  3. Wire behavior in the appropriate startup/runtime/command module.
  4. Add unit tests for parsing + integration/regression tests for behavior.

- Add a new slash command:
  1. Add parser/dispatch hooks in `commands.rs`.
  2. Implement command logic in a focused `*_commands.rs` module.
  3. Add command rendering and error-path tests.
  4. Add command-file coverage if command should work in `--command-file` mode.

- Add a new transport/integration mode:
  1. Add CLI flags in `cli_args.rs`.
  2. Add validation in `runtime_cli_validation.rs`.
  3. Add runtime config + bridge runner wiring in `startup_transport_modes.rs`.
  4. Add conformance fixtures/tests in `transport_conformance.rs` and regression tests in `tests.rs`.

- Change provider/auth behavior:
  1. Update auth resolution in `provider_auth.rs` / `provider_credentials.rs` / `credentials.rs`.
  2. Keep model and fallback behavior aligned in `startup_model_resolution.rs` and `provider_fallback.rs`.
  3. Add regression coverage for required/missing credentials and fallback routing.

## Quality Gate (Local)

Run this repository matrix before opening a PR:

```bash
cargo fmt --all -- --check
cargo test -p tau-coding-agent --quiet
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

For transport-heavy changes, also run focused conformance tests in `transport_conformance.rs`.
