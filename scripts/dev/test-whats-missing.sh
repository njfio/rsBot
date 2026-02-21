#!/usr/bin/env bash
set -euo pipefail

REPORT="tasks/whats-missing.md"

assert_contains() {
  local pattern="$1"
  local description="$2"
  if ! rg -q --fixed-strings "$pattern" "$REPORT"; then
    echo "[FAIL] missing ${description}: ${pattern}" >&2
    exit 1
  fi
}

assert_not_contains() {
  local pattern="$1"
  local description="$2"
  if rg -q --fixed-strings "$pattern" "$REPORT"; then
    echo "[FAIL] stale ${description} still present: ${pattern}" >&2
    exit 1
  fi
}

test -f "$REPORT" || {
  echo "[FAIL] report file not found: ${REPORT}" >&2
  exit 1
}

# Required refreshed markers.
assert_contains "# Tau — What's Missing (Current State)" "report title"
assert_contains "Resolved Since Prior Report" "resolved section"
assert_contains "Remaining High-Impact Gaps" "remaining gaps section"
assert_contains "Per-session usage and cost tracking is implemented" "cost tracking resolution marker"
assert_contains "OpenRouter is a first-class provider" "openrouter resolution marker"
assert_contains "PostgreSQL session backend is implemented" "postgres resolution marker"
assert_contains "Dockerfile + release workflow assets are present" "distribution resolution marker"
assert_contains "Fuzz harnesses and deterministic fuzz-conformance tests are present" "fuzz resolution marker"
assert_contains "PPO/GAE runtime optimization is implemented in training and live RL paths" "ppo implementation marker"

# Stale markers that must never reappear.
assert_not_contains "No Per-Session Cost Tracking" "old critical gap"
assert_not_contains "No Token Pre-Flight Estimation" "old critical gap"
assert_not_contains "No Prompt Caching Support" "old critical gap"
assert_not_contains "OpenRouter Is Still an Alias, Not a First-Class Provider" "old functional gap"
assert_not_contains "PostgreSQL Session Backend Is Scaffolded But Not Implemented" "old functional gap"
assert_not_contains "No Docker Image" "old distribution gap"
assert_not_contains "No Homebrew Formula" "old distribution gap"
assert_not_contains "No Shell Completions" "old distribution gap"
assert_not_contains "No Fuzz Testing" "old testing gap"
assert_not_contains "No Log Rotation" "old testing gap"
assert_not_contains "PPO/GAE math is implemented but still not wired into the runtime training loop" "stale ppo claim"

echo "whats-missing conformance passed"
