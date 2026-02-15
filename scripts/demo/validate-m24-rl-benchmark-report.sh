#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: validate-m24-rl-benchmark-report.sh <report-json>" >&2
  exit 2
fi

report_json="$1"
if [[ ! -f "${report_json}" ]]; then
  echo "error: report JSON not found: ${report_json}" >&2
  exit 2
fi

python3 - "${report_json}" <<'PY'
import json
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
payload = json.loads(path.read_text(encoding="utf-8"))

RUN_ID_RE = re.compile(r"^m24-[a-z0-9-]+$")
REPORT_KIND_ALLOWED = {"baseline", "trained", "significance", "summary"}

def fail(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)

def require_obj(root: dict, key: str) -> dict:
    value = root.get(key)
    if not isinstance(value, dict):
        fail(f"missing object field '{key}'")
    return value

def require_str(root: dict, key: str) -> str:
    value = root.get(key)
    if not isinstance(value, str) or not value.strip():
        fail(f"missing string field '{key}'")
    return value

def require_num(root: dict, key: str) -> float:
    value = root.get(key)
    if not isinstance(value, (int, float)):
        fail(f"missing numeric field '{key}'")
    return float(value)

if payload.get("schema_version") != 1:
    fail("schema_version must be 1")

report_kind = require_str(payload, "report_kind")
if report_kind not in REPORT_KIND_ALLOWED:
    fail("report_kind must be one of baseline|trained|significance|summary")

run_id = require_str(payload, "run_id")
if not RUN_ID_RE.match(run_id):
    fail("run_id must match ^m24-[a-z0-9-]+$")

require_str(payload, "generated_at")

suite = require_obj(payload, "benchmark_suite")
require_str(suite, "name")
require_str(suite, "version")

metrics = require_obj(payload, "metrics")
if int(require_num(metrics, "episodes")) <= 0:
    fail("metrics.episodes must be > 0")
require_num(metrics, "mean_reward")
require_num(metrics, "mean_safety_penalty")

publication = require_obj(payload, "publication")
report_path = require_str(publication, "report_path")
archive_path = require_str(publication, "archive_path")

expected_report_path = (
    f".tau/reports/m24/{run_id}/m24-benchmark-report-{report_kind}.json"
)
if report_path != expected_report_path:
    fail(
        "publication.report_path must be "
        f"'{expected_report_path}'"
    )

archive_re = re.compile(
    rf"^\.tau/reports/archive/m24/\d{{4}}/\d{{2}}/"
    rf"m24-benchmark-report-{re.escape(run_id)}-{re.escape(report_kind)}\.json$"
)
if not archive_re.match(archive_path):
    fail(
        "publication.archive_path must match "
        "'.tau/reports/archive/m24/YYYY/MM/"
        f"m24-benchmark-report-{run_id}-{report_kind}.json'"
    )

retention = require_obj(payload, "retention")
policy = require_str(retention, "policy")
if policy not in {"archive-then-purge", "retain-only"}:
    fail("retention.policy must be archive-then-purge or retain-only")
retain_days = int(require_num(retention, "retain_days"))
archive_after_days = int(require_num(retention, "archive_after_days"))
if retain_days <= 0:
    fail("retention.retain_days must be > 0")
if archive_after_days < 0:
    fail("retention.archive_after_days must be >= 0")
if archive_after_days > retain_days:
    fail("retention.archive_after_days must be <= retention.retain_days")
require_str(retention, "purge_after")

print("ok - m24 benchmark report artifact valid")
PY
