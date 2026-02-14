#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"

(
  cd "${repo_root}"
  python3 ".github/scripts/live_run_unified_runner.py" \
    --repo-root "${repo_root}" \
    --surfaces-manifest "${repo_root}/.github/live-run-unified-manifest.json" \
    "$@"
)
