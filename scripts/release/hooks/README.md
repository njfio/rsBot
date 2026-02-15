# Release Hook Contracts

Optional signing/notarization hooks are invoked by [`.github/workflows/release.yml`](../../../.github/workflows/release.yml) when enabled.

Supported hook paths:

- `scripts/release/hooks/sign-linux.sh`
  - args: `<packaged_binary_path> <target_triple> <release_tag>`
- `scripts/release/hooks/sign-macos.sh`
  - args: `<packaged_binary_path> <target_triple> <release_tag>`
- `scripts/release/hooks/notarize-macos.sh`
  - args: `<archive_path> <target_triple> <release_tag>`
- `scripts/release/hooks/sign-windows.ps1`
  - params: `-BinaryPath <path> -Target <triple> -Tag <release_tag>`

Hook requirements:

- Exit code `0` on success.
- Non-zero exit code fails the release job.
- Handle secret/material loading internally (for example from GitHub Actions secrets).
