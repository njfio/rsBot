#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VALIDATOR="${SCRIPT_DIR}/validate-m24-rl-benchmark-proof-template.sh"

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "${tmp_dir}"
}
trap cleanup EXIT

valid_json="${tmp_dir}/valid.json"
invalid_json="${tmp_dir}/invalid.json"

cat >"${valid_json}" <<'EOF'
{
  "schema_version": 1,
  "run_id": "m24-run-001",
  "generated_at": "2026-02-15T00:00:00Z",
  "benchmark_suite": {
    "name": "m24-rl-suite",
    "version": "v1"
  },
  "baseline": {
    "checkpoint_id": "baseline",
    "episodes": 200,
    "mean_reward": 0.42
  },
  "trained": {
    "checkpoint_id": "trained",
    "episodes": 200,
    "mean_reward": 0.57
  },
  "significance": {
    "p_value": 0.01,
    "confidence_level": 0.95,
    "pass": true
  },
  "criteria": {
    "min_reward_delta": 0.05,
    "max_safety_regression": 0.00,
    "max_p_value": 0.05
  },
  "artifacts": {
    "baseline_report": "tasks/reports/m24-benchmark-baseline.json",
    "trained_report": "tasks/reports/m24-benchmark-trained.json",
    "significance_report": "tasks/reports/m24-benchmark-significance.json"
  }
}
EOF

"${VALIDATOR}" "${valid_json}"

cat >"${invalid_json}" <<'EOF'
{
  "schema_version": 1,
  "run_id": "m24-run-001"
}
EOF

if "${VALIDATOR}" "${invalid_json}" >/dev/null 2>&1; then
  echo "assertion failed: expected invalid fixture to fail validation" >&2
  exit 1
fi

echo "ok - m24 benchmark proof template validation"
