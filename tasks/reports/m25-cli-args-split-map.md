# CLI Args Split Map (M25)

- Generated at (UTC): `2026-02-16T08:47:54Z`
- Source file: `crates/tau-cli/src/cli_args.rs`
- Target line budget: `3000`
- Current line count: `3954`
- Current gap to target: `954`
- Estimated lines to extract: `1070`
- Estimated post-split line count: `2884`

## Extraction Phases

| Phase | Owner | Est. Reduction | Depends On | Modules | Notes |
| --- | --- | ---: | --- | --- | --- |
| phase-1-provider-auth (Provider/auth and model catalog flags) | cli-platform | 300 | - | cli_args/provider_model_flags.rs, cli_args/provider_auth_flags.rs | Preserve Cli top-level field names via #[command(flatten)] wrappers. |
| phase-2-gateway-runtime (Gateway remote/service and transport flags) | runtime-gateway | 260 | phase-1-provider-auth | cli_args/gateway_remote_flags.rs, cli_args/gateway_service_flags.rs | Keep existing gateway daemon sub-struct wiring intact and additive. |
| phase-3-package-events-extension (Package/events/extensions and skill policy flags) | runtime-packaging | 230 | phase-2-gateway-runtime | cli_args/package_flags.rs, cli_args/events_flags.rs, cli_args/extension_flags.rs, cli_args/skills_flags.rs | Group related command surfaces to reduce import fan-out in cli_args.rs. |
| phase-4-multichannel-dashboard-voice (Multi-channel, dashboard, memory, and voice surfaces) | runtime-integrations | 280 | phase-3-package-events-extension | cli_args/multi_channel_flags.rs, cli_args/dashboard_flags.rs, cli_args/memory_flags.rs, cli_args/voice_flags.rs | Final phase targets high-volume flag groups to push below the 3000 line budget. |

## Public API Impact

- Keep pub struct Cli as the single externally consumed parser type.
- Retain existing flag names, clap aliases, defaults, and env bindings.
- Introduce internal flattened sub-structs only; no external crate API renames.

## Import Impact

- Add new module declarations under crates/tau-cli/src/cli_args/ with targeted pub re-exports.
- Move domain-specific clap argument definitions from cli_args.rs into phase modules.
- Keep root-level helper parsers in cli_args.rs until all phases are complete to avoid churn.

## Test Migration Plan

| Order | Step | Command | Expected Signal |
| ---: | --- | --- | --- |
| 1 | update-guardrail-threshold: Lower cli_args split guardrail from <4000 to staged thresholds ending at <3000. | scripts/dev/test-cli-args-domain-split.sh | line budget checks enforce progressive reduction and final <3000 gate |
| 2 | cli-crate-coverage: Run crate-scoped CLI parsing and validation tests after each phase extraction. | cargo test -p tau-cli | all clap parser and validation tests pass |
| 3 | workspace-integration: Run cross-crate runtime command integration tests that consume Cli fields. | cargo test -p tau-coding-agent | no regressions in command wiring and runtime behavior |
