# M176 - CLI Args Module Split Phase 1 (Runtime Feature Flags)

## Context
`crates/tau-cli/src/cli_args.rs` remains a high-churn hotspot at 3,788 lines. This milestone starts the next decomposition wave by extracting the post-`execution_domain` runtime/deployment flag declarations into dedicated source artifacts while preserving clap CLI behavior.

## Scope
- Keep `Cli` as the external parse surface.
- Extract the runtime feature tail block (events/rpc/deployment domain flags) from root file.
- Preserve all flag names, env bindings, defaults, and help behavior.
- Validate with tau-cli and tau-coding-agent focused regression suites.

## Linked Issues
- Epic: #2990
- Story: #2991
- Task: #2992
