#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

POLICY_JSON="${REPO_ROOT}/tasks/policies/m25-latency-budget-policy.json"
REPORT_JSON="${REPO_ROOT}/tasks/reports/m25-fast-lane-loop-comparison.json"
OUTPUT_JSON="${REPO_ROOT}/tasks/reports/m25-latency-budget-gate.json"
OUTPUT_MD="${REPO_ROOT}/tasks/reports/m25-latency-budget-gate.md"
GENERATED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
REPO_SLUG=""
QUIET_MODE="false"

usage() {
  cat <<'USAGE'
Usage: latency-budget-gate.sh [options]

Evaluate fast-lane benchmark report against latency-budget policy thresholds.
Writes deterministic JSON + Markdown gate artifacts and exits non-zero on
violations when enforcement mode is `fail`.

Options:
  --policy-json <path>    Policy JSON path
                          (default: tasks/policies/m25-latency-budget-policy.json)
  --report-json <path>    Fast-lane comparison report JSON path
                          (default: tasks/reports/m25-fast-lane-loop-comparison.json)
  --output-json <path>    Output JSON gate artifact path
                          (default: tasks/reports/m25-latency-budget-gate.json)
  --output-md <path>      Output Markdown gate artifact path
                          (default: tasks/reports/m25-latency-budget-gate.md)
  --generated-at <iso>    Generated timestamp override (ISO-8601 UTC)
  --repo <owner/name>     Repository slug override
  --quiet                 Suppress informational output
  --help                  Show this help text
USAGE
}

require_cmd() {
  local name="$1"
  if ! command -v "${name}" >/dev/null 2>&1; then
    echo "error: required command '${name}' not found" >&2
    exit 1
  fi
}

log_info() {
  if [[ "${QUIET_MODE}" != "true" ]]; then
    echo "$@"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --policy-json)
      POLICY_JSON="$2"
      shift 2
      ;;
    --report-json)
      REPORT_JSON="$2"
      shift 2
      ;;
    --output-json)
      OUTPUT_JSON="$2"
      shift 2
      ;;
    --output-md)
      OUTPUT_MD="$2"
      shift 2
      ;;
    --generated-at)
      GENERATED_AT="$2"
      shift 2
      ;;
    --repo)
      REPO_SLUG="$2"
      shift 2
      ;;
    --quiet)
      QUIET_MODE="true"
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument '$1'" >&2
      usage >&2
      exit 1
      ;;
  esac
done

require_cmd python3

if [[ ! -f "${POLICY_JSON}" ]]; then
  echo "error: policy JSON not found: ${POLICY_JSON}" >&2
  exit 1
fi
if [[ ! -f "${REPORT_JSON}" ]]; then
  echo "error: report JSON not found: ${REPORT_JSON}" >&2
  exit 1
fi

mkdir -p "$(dirname "${OUTPUT_JSON}")"
mkdir -p "$(dirname "${OUTPUT_MD}")"

python3 - \
  "${POLICY_JSON}" \
  "${REPORT_JSON}" \
  "${OUTPUT_JSON}" \
  "${OUTPUT_MD}" \
  "${GENERATED_AT}" \
  "${REPO_SLUG}" \
  "${QUIET_MODE}" <<'PY'
from __future__ import annotations

import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

(
    policy_path_raw,
    report_path_raw,
    output_json_raw,
    output_md_raw,
    generated_at_raw,
    repo_slug_raw,
    quiet_mode_raw,
) = sys.argv[1:]

policy_path = Path(policy_path_raw)
report_path = Path(report_path_raw)
output_json_path = Path(output_json_raw)
output_md_path = Path(output_md_raw)
quiet_mode = quiet_mode_raw == "true"


def log(message: str) -> None:
    if not quiet_mode:
        print(message)


def fail(message: str) -> None:
    raise SystemExit(f"error: {message}")


def parse_iso8601_utc(value: str) -> datetime:
    candidate = value.strip()
    if not candidate:
        fail("generated-at value must not be empty")
    if candidate.endswith("Z"):
        candidate = candidate[:-1] + "+00:00"
    try:
        parsed = datetime.fromisoformat(candidate)
    except ValueError as exc:
        fail(f"invalid --generated-at timestamp: {value} ({exc})")
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc).replace(microsecond=0)


def iso_utc(dt: datetime) -> str:
    return dt.astimezone(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def detect_repository_slug(explicit_repo: str, report_repo: str | None) -> str:
    if explicit_repo.strip():
        return explicit_repo.strip()
    if report_repo:
        return report_repo
    try:
        completed = subprocess.run(
            ["gh", "repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"],
            text=True,
            capture_output=True,
            check=False,
        )
        if completed.returncode == 0:
            candidate = completed.stdout.strip()
            if candidate:
                return candidate
    except Exception:
        pass
    return f"local/{Path.cwd().name}"


def require_dict(payload: Any, label: str) -> dict[str, Any]:
    if not isinstance(payload, dict):
        fail(f"{label} must decode to an object")
    return payload


def require_report_field(report: dict[str, Any], field: str, expected_type: type) -> Any:
    if field not in report:
        fail(f"missing required report field '{field}'")
    value = report[field]
    if not isinstance(value, expected_type):
        fail(f"report field '{field}' must be {expected_type.__name__}")
    return value


def require_policy_field(policy: dict[str, Any], field: str, expected_type: type) -> Any:
    if field not in policy:
        fail(f"missing required policy field '{field}'")
    value = policy[field]
    if not isinstance(value, expected_type):
        fail(f"policy field '{field}' must be {expected_type.__name__}")
    return value


def render_markdown(gate_report: dict[str, Any]) -> str:
    lines: list[str] = []
    lines.append("# M25 Latency Budget Gate")
    lines.append("")
    lines.append(f"Generated: `{gate_report['generated_at']}`")
    lines.append(f"Repository: `{gate_report['repository']}`")
    lines.append(f"Policy: `{gate_report['policy_path']}`")
    lines.append(f"Report: `{gate_report['report_path']}`")
    lines.append("")
    lines.append("## Summary")
    lines.append("")
    lines.append("| Status | Violations |")
    lines.append("|---|---:|")
    lines.append(f"| {gate_report['status']} | {len(gate_report['violations'])} |")
    lines.append("")
    lines.append("## Report Metrics")
    lines.append("")
    report_summary = gate_report["report_summary"]
    lines.append("| Metric | Value |")
    lines.append("|---|---:|")
    lines.append(f"| baseline_median_ms | {report_summary['baseline_median_ms']} |")
    lines.append(f"| fast_lane_median_ms | {report_summary['fast_lane_median_ms']} |")
    lines.append(f"| improvement_percent | {report_summary['improvement_percent']} |")
    lines.append("")
    lines.append("## Checks")
    lines.append("")
    lines.append("| Metric | Result | Threshold | Observed | Remediation |")
    lines.append("|---|---|---|---:|---|")
    for check in gate_report["checks"]:
        remediation = str(check["remediation"]).replace("|", "\\|")
        lines.append(
            f"| {check['metric']} | {check['result']} | {check['threshold']} | {check['observed']} | {remediation} |"
        )
    return "\n".join(lines) + "\n"


def main() -> int:
    generated_at = iso_utc(parse_iso8601_utc(generated_at_raw))

    try:
        policy = require_dict(json.loads(policy_path.read_text(encoding="utf-8")), "policy")
    except Exception as exc:
        fail(f"unable to parse policy JSON: {exc}")

    try:
        report = require_dict(json.loads(report_path.read_text(encoding="utf-8")), "report")
    except Exception as exc:
        fail(f"unable to parse report JSON: {exc}")

    max_fast_lane_median_ms = require_policy_field(policy, "max_fast_lane_median_ms", int)
    min_improvement_percent = require_policy_field(policy, "min_improvement_percent", (int, float))
    max_regression_percent = require_policy_field(policy, "max_regression_percent", (int, float))
    enforcement_mode = require_policy_field(policy, "enforcement_mode", str)
    remediation = require_policy_field(policy, "remediation", dict)

    baseline_median_ms = require_report_field(report, "baseline_median_ms", int)
    fast_lane_median_ms = require_report_field(report, "fast_lane_median_ms", int)
    improvement_percent = require_report_field(report, "improvement_percent", (int, float))

    checks: list[dict[str, Any]] = []

    def add_check(metric: str, threshold: str, observed: float, passed: bool, remediation_key: str) -> None:
        checks.append(
            {
                "metric": metric,
                "threshold": threshold,
                "observed": observed,
                "result": "pass" if passed else "fail",
                "remediation": remediation.get(remediation_key, "Review latency policy and optimize command path."),
            }
        )

    add_check(
        metric="fast_lane_median_ms",
        threshold=f"<= {max_fast_lane_median_ms}",
        observed=fast_lane_median_ms,
        passed=fast_lane_median_ms <= max_fast_lane_median_ms,
        remediation_key="fast_lane_median_ms",
    )
    add_check(
        metric="improvement_percent",
        threshold=f">= {min_improvement_percent}",
        observed=float(improvement_percent),
        passed=float(improvement_percent) >= float(min_improvement_percent),
        remediation_key="improvement_percent",
    )
    add_check(
        metric="regression_percent",
        threshold=f"<= {max_regression_percent}",
        observed=max(0.0, -float(improvement_percent)),
        passed=max(0.0, -float(improvement_percent)) <= float(max_regression_percent),
        remediation_key="regression_percent",
    )

    violations = [check for check in checks if check["result"] == "fail"]

    status = "pass"
    exit_code = 0
    if violations and enforcement_mode.strip().lower() == "fail":
        status = "fail"
        exit_code = 1
    elif violations:
        status = "warn"

    repository = detect_repository_slug(repo_slug_raw, report.get("repository"))

    gate_report = {
        "schema_version": 1,
        "generated_at": generated_at,
        "repository": repository,
        "policy_path": str(policy_path),
        "report_path": str(report_path),
        "status": status,
        "checks": checks,
        "violations": violations,
        "report_summary": {
            "baseline_median_ms": baseline_median_ms,
            "fast_lane_median_ms": fast_lane_median_ms,
            "improvement_percent": float(improvement_percent),
        },
    }

    output_json_path.write_text(json.dumps(gate_report, indent=2) + "\n", encoding="utf-8")
    output_md_path.write_text(render_markdown(gate_report), encoding="utf-8")

    log(
        "[latency-budget-gate] "
        f"status={status} violations={len(violations)} "
        f"fast_lane_median_ms={fast_lane_median_ms}"
    )

    if exit_code != 0:
        print(f"budget violations detected: {len(violations)}", file=sys.stderr)
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
PY
