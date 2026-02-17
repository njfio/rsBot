# Release Automation Operations Guide

This guide covers release build/publish automation, optional signing/notarization hooks, and installer helper scripts.

## Release workflow

Workflow file: [`.github/workflows/release.yml`](../../.github/workflows/release.yml)

Trigger modes:

- Tag push: `v*`
- Manual dispatch: `workflow_dispatch` (required `tag` input)

Default artifact matrix:

- `linux-amd64` (`x86_64-unknown-linux-gnu`)
- `linux-arm64` (`aarch64-unknown-linux-gnu`, cross-compiled on Ubuntu with `gcc-aarch64-linux-gnu`)
- `macos-amd64` (`x86_64-apple-darwin`)
- `macos-arm64` (`aarch64-apple-darwin`)
- `windows-amd64` (`x86_64-pc-windows-msvc`)
- `windows-arm64` (`aarch64-pc-windows-msvc`)

The workflow publishes release archives plus checksum manifests to GitHub Releases:

- `*.tar.gz` / `*.zip`
- `*.sha256`
- `SHA256SUMS`

The workflow also publishes a GHCR container image:

- `ghcr.io/<owner>/tau-coding-agent:<release-tag>`
- `ghcr.io/<owner>/tau-coding-agent:latest`
- Platforms: `linux/amd64`, `linux/arm64`

### Cross-arch smoke policy

Release build lanes execute a `--help` smoke gate only when artifact architecture matches runner architecture.

Cross-arch lanes are compiled/packaged without execution and emit deterministic reason codes:

- `release_smoke_reason=cross_arch_linux_arm64_on_amd64_runner`
- `release_smoke_reason=cross_arch_macos_amd64_on_arm64_runner`
- `release_smoke_reason=cross_arch_windows_arm64_on_amd64_runner`

Native-arch lanes execute smoke with `release_smoke_reason=native_arch` metadata in the matrix.

## Optional signing/notarization hooks

Hook execution is controlled by workflow inputs/vars:

- `enable_signing_hooks` (dispatch input) or `RELEASE_ENABLE_SIGNING_HOOKS` (repo variable)
- `enable_notarization_hooks` (dispatch input) or `RELEASE_ENABLE_NOTARIZATION_HOOKS` (repo variable)

Hook lookup paths:

- Linux signing: `scripts/release/hooks/sign-linux.sh`
- macOS signing: `scripts/release/hooks/sign-macos.sh`
- macOS notarization: `scripts/release/hooks/notarize-macos.sh`
- Windows signing: `scripts/release/hooks/sign-windows.ps1`

If a hook is enabled but missing, release logs include explicit reason codes:

- `release_hook_reason=sign_hook_missing`
- `release_hook_reason=notarize_hook_missing`

If disabled, release logs include:

- `release_hook_reason=sign_hooks_disabled`
- `release_hook_reason=notarization_hooks_disabled`

Windows arm64 signing constraints:

- The release workflow reuses `scripts/release/hooks/sign-windows.ps1` for both windows amd64 and windows arm64 lanes.
- Hook scripts must select cert/signing profile by target triple (`x86_64-pc-windows-msvc` vs `aarch64-pc-windows-msvc`).
- Hosted runners do not execute windows arm64 artifacts; signing hooks should validate signature metadata rather than binary execution.

## Installer helper scripts

Scripts:

- Shell installer: [`scripts/release/install-tau.sh`](../../scripts/release/install-tau.sh)
- Shell updater: [`scripts/release/update-tau.sh`](../../scripts/release/update-tau.sh)
- PowerShell installer: [`scripts/release/install-tau.ps1`](../../scripts/release/install-tau.ps1)
- PowerShell updater: [`scripts/release/update-tau.ps1`](../../scripts/release/update-tau.ps1)

Shell examples:

```bash
# Install latest to ~/.local/bin
./scripts/release/install-tau.sh

# Install an explicit tag and force overwrite
./scripts/release/install-tau.sh --version v0.1.0 --force

# Update an existing install in place
./scripts/release/update-tau.sh --version v0.1.1
```

PowerShell examples:

```powershell
# Install latest (Windows default install dir: $env:LOCALAPPDATA\Tau\bin)
./scripts/release/install-tau.ps1

# Install a specific tag
./scripts/release/install-tau.ps1 -Version v0.1.0

# Update an existing install
./scripts/release/update-tau.ps1 -Version v0.1.1
```

Common behavior:

- Archive + `.sha256` checksum fetch
- SHA256 verification by default (`--no-verify` / `-NoVerify` to bypass)
- Post-install `--help` smoke gate
- Backup/rollback on smoke failure
- Structured reason-coded logs for diagnostics

## Installer telemetry reason codes

Installer scripts emit JSON log lines with `reason_code` values, including:

- `download_started`
- `checksum_fetch_started`
- `checksum_verified`
- `checksum_verification_skipped`
- `install_complete`
- `update_complete`
- `checksum_mismatch`
- `update_target_missing`
- `destination_exists`
- `smoke_test_failed`
- `unsupported_os`
- `unsupported_arch`

## Local validation

Run Docker packaging contract + smoke checks:

```bash
./scripts/dev/test-docker-image-packaging.sh
./scripts/dev/docker-image-smoke.sh --tag tau-coding-agent:local-smoke
```

Run helper-script tests:

```bash
./scripts/release/test-install-helpers.sh
```

Run release workflow lint/validation via PR CI:

- Release helper test scope is automatically triggered when `scripts/release/**` or `.github/workflows/release.yml` changes.
- Workflow contract checks (matrix + smoke policy) run in `scripts/release/test-release-workflow-contract.sh`.
- Docker packaging scope is automatically triggered when `Dockerfile`, `.dockerignore`,
  Docker packaging scripts, or release workflow files change.
