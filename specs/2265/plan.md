# Plan #2265

Status: Reviewed
Spec: specs/2265/spec.md

## Approach

1. Add CLI completion rendering path:
   - introduce `CliShellCompletion` value enum (`bash|zsh|fish`)
   - add `--shell-completion` flag
   - render completion via `clap_complete` and exit before runtime startup.
2. Add release completion generator tooling:
   - script to invoke binary completion output and write deterministic files.
3. Wire release workflow:
   - extract Linux amd64 artifact
   - generate completion files
   - publish completion assets in release upload step.
4. Add release-helper contract tests:
   - generator script behavior/outputs
   - workflow step/file assertions.
5. Update docs:
   - completion asset locations and install snippets for bash/zsh/fish.

## Affected Modules

- `crates/tau-cli/*` (CLI flag + completion renderer)
- `crates/tau-coding-agent/src/startup_dispatch.rs` (early completion dispatch)
- `scripts/release/*` (generation + tests)
- `.github/workflows/release.yml`
- `.github/workflows/ci.yml`
- `README.md`
- `docs/guides/release-automation-ops.md`
- `specs/2265/*`

## Risks and Mitigations

- Risk: completion generation path accidentally triggers runtime startup.
  - Mitigation: short-circuit dispatch before preflight/runtime model setup.
- Risk: release workflow completion files drift from expected names.
  - Mitigation: contract test asserts exact output filenames and publish list.
- Risk: shell-specific generation regressions.
  - Mitigation: direct per-shell assertions in script tests plus runtime smoke
    execution of `--shell-completion`.

## Interfaces / Contracts

- New CLI contract:
  - `tau-coding-agent --shell-completion <bash|zsh|fish>` prints completion
    script to stdout and exits successfully.
- New release delivery contract:
  - release assets include shell completion files for bash/zsh/fish.
