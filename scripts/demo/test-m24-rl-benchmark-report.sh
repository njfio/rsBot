#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VALIDATOR="${SCRIPT_DIR}/validate-m24-rl-benchmark-report.sh"

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "${tmp_dir}"
}
trap cleanup EXIT

valid_json="${tmp_dir}/valid-report.json"
invalid_path_json="${tmp_dir}/invalid-path.json"
invalid_retention_json="${tmp_dir}/invalid-retention.json"

cat >"${valid_json}" <<'EOF'
{
  "schema_version": 1,
  "report_kind": "summary",
  "run_id": "m24-2026-02-15-0001",
  "generated_at": "2026-02-15T00:00:00Z",
  "benchmark_suite": {
    "name": "m24-rl-suite",
    "version": "v1"
  },
  "metrics": {
    "episodes": 200,
    "mean_reward": 0.57,
    "mean_safety_penalty": 0.0
  },
  "publication": {
    "report_path": ".tau/reports/m24/m24-2026-02-15-0001/m24-benchmark-report-summary.json",
    "archive_path": ".tau/reports/archive/m24/2026/02/m24-benchmark-report-m24-2026-02-15-0001-summary.json"
  },
  "retention": {
    "policy": "archive-then-purge",
    "retain_days": 365,
    "archive_after_days": 30,
    "purge_after": "2027-02-15T00:00:00Z"
  }
}
EOF

"${VALIDATOR}" "${valid_json}"

cat >"${invalid_path_json}" <<'EOF'
{
  "schema_version": 1,
  "report_kind": "summary",
  "run_id": "m24-2026-02-15-0001",
  "generated_at": "2026-02-15T00:00:00Z",
  "benchmark_suite": {
    "name": "m24-rl-suite",
    "version": "v1"
  },
  "metrics": {
    "episodes": 200,
    "mean_reward": 0.57,
    "mean_safety_penalty": 0.0
  },
  "publication": {
    "report_path": ".tau/reports/m24/report.json",
    "archive_path": ".tau/reports/archive/m24/2026/02/m24-benchmark-report-m24-2026-02-15-0001-summary.json"
  },
  "retention": {
    "policy": "archive-then-purge",
    "retain_days": 365,
    "archive_after_days": 30,
    "purge_after": "2027-02-15T00:00:00Z"
  }
}
EOF

if "${VALIDATOR}" "${invalid_path_json}" >/dev/null 2>&1; then
  echo "assertion failed: expected invalid report_path fixture to fail validation" >&2
  exit 1
fi

cat >"${invalid_retention_json}" <<'EOF'
{
  "schema_version": 1,
  "report_kind": "summary",
  "run_id": "m24-2026-02-15-0001",
  "generated_at": "2026-02-15T00:00:00Z",
  "benchmark_suite": {
    "name": "m24-rl-suite",
    "version": "v1"
  },
  "metrics": {
    "episodes": 200,
    "mean_reward": 0.57,
    "mean_safety_penalty": 0.0
  },
  "publication": {
    "report_path": ".tau/reports/m24/m24-2026-02-15-0001/m24-benchmark-report-summary.json",
    "archive_path": ".tau/reports/archive/m24/2026/02/m24-benchmark-report-m24-2026-02-15-0001-summary.json"
  },
  "retention": {
    "policy": "archive-then-purge",
    "retain_days": 30,
    "archive_after_days": 60,
    "purge_after": "2027-02-15T00:00:00Z"
  }
}
EOF

if "${VALIDATOR}" "${invalid_retention_json}" >/dev/null 2>&1; then
  echo "assertion failed: expected invalid retention fixture to fail validation" >&2
  exit 1
fi

echo "ok - m24 benchmark report publication contract"
