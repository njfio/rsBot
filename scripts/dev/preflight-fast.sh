#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
cd "${REPO_ROOT}"

ROADMAP_SYNC_BIN="${TAU_ROADMAP_SYNC_BIN:-${SCRIPT_DIR}/roadmap-status-sync.sh}"
PANIC_UNSAFE_GUARD_BIN="${TAU_PANIC_UNSAFE_GUARD_BIN:-${SCRIPT_DIR}/panic-unsafe-guard.sh}"
FAST_VALIDATE_BIN="${TAU_FAST_VALIDATE_BIN:-${SCRIPT_DIR}/fast-validate.sh}"

usage() {
  cat <<'EOF'
Usage: preflight-fast.sh [fast-validate args...]

Runs high-signal local blockers in order:
  1) scripts/dev/roadmap-status-sync.sh --check --quiet
  2) scripts/dev/panic-unsafe-guard.sh --quiet
  3) scripts/dev/fast-validate.sh <args...>

All arguments are forwarded unchanged to fast-validate.

Environment overrides (for testing):
  TAU_ROADMAP_SYNC_BIN  Override roadmap sync script path.
  TAU_PANIC_UNSAFE_GUARD_BIN Override panic/unsafe guard script path.
  TAU_FAST_VALIDATE_BIN Override fast-validate script path.
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

if [[ ! -x "${ROADMAP_SYNC_BIN}" ]]; then
  echo "error: roadmap sync script is not executable: ${ROADMAP_SYNC_BIN}" >&2
  exit 1
fi

if [[ ! -x "${FAST_VALIDATE_BIN}" ]]; then
  echo "error: fast-validate script is not executable: ${FAST_VALIDATE_BIN}" >&2
  exit 1
fi

if [[ ! -x "${PANIC_UNSAFE_GUARD_BIN}" ]]; then
  echo "error: panic/unsafe guard script is not executable: ${PANIC_UNSAFE_GUARD_BIN}" >&2
  exit 1
fi

echo "preflight-fast: checking roadmap freshness..."
"${ROADMAP_SYNC_BIN}" --check --quiet
echo "preflight-fast: roadmap check passed"

echo "preflight-fast: running panic/unsafe guard..."
"${PANIC_UNSAFE_GUARD_BIN}" --quiet
echo "preflight-fast: panic/unsafe guard passed"

echo "preflight-fast: running fast-validate..."
exec "${FAST_VALIDATE_BIN}" "$@"
