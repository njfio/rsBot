#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

FIXTURE_JSON=""
OUTPUT_JSON="${REPO_ROOT}/tasks/reports/m25-ci-cache-parallel-tuning.json"
OUTPUT_MD="${REPO_ROOT}/tasks/reports/m25-ci-cache-parallel-tuning.md"
GENERATED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
REPO_SLUG=""
WORKERS="4"
ITERATIONS="3"
QUIET_MODE="false"
HELPER_COMMAND='python3 -m unittest discover -s .github/scripts -p "test_*.py"'

usage() {
  cat <<'USAGE'
Usage: ci-cache-parallel-tuning-report.sh [options]

Generate deterministic JSON + Markdown artifacts comparing helper-suite
execution medians between serial and parallel unittest scheduling.

Options:
  --fixture-json <path>   Fixture JSON path (skip live command execution).
  --output-json <path>    Output JSON artifact path
                          (default: tasks/reports/m25-ci-cache-parallel-tuning.json)
  --output-md <path>      Output Markdown artifact path
                          (default: tasks/reports/m25-ci-cache-parallel-tuning.md)
  --generated-at <iso>    Generated timestamp override (ISO-8601 UTC).
  --repo <owner/name>     Repository slug override.
  --workers <n>           Parallel worker count for live mode (default: 4).
  --iterations <n>        Live mode iterations per command (default: 3).
  --quiet                 Suppress informational output.
  --help                  Show this help text.

Fixture format:
{
  "serial_ms": [8200, 8000, 7800],
  "parallel_ms": [5200, 5000, 5100],
  "command": "python3 -m unittest discover -s .github/scripts -p \"test_*.py\"",
  "workers": 4
}
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
    --fixture-json)
      FIXTURE_JSON="$2"
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
    --workers)
      WORKERS="$2"
      shift 2
      ;;
    --iterations)
      ITERATIONS="$2"
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

if [[ -n "${FIXTURE_JSON}" && ! -f "${FIXTURE_JSON}" ]]; then
  echo "error: fixture JSON not found: ${FIXTURE_JSON}" >&2
  exit 1
fi

if ! [[ "${WORKERS}" =~ ^[0-9]+$ ]]; then
  echo "error: --workers must be a non-negative integer" >&2
  exit 1
fi
if (( WORKERS <= 0 )); then
  echo "error: --workers must be greater than zero" >&2
  exit 1
fi

if ! [[ "${ITERATIONS}" =~ ^[0-9]+$ ]]; then
  echo "error: --iterations must be a non-negative integer" >&2
  exit 1
fi
if (( ITERATIONS <= 0 )); then
  echo "error: --iterations must be greater than zero" >&2
  exit 1
fi

mkdir -p "$(dirname "${OUTPUT_JSON}")"
mkdir -p "$(dirname "${OUTPUT_MD}")"

python3 - \
  "${FIXTURE_JSON}" \
  "${OUTPUT_JSON}" \
  "${OUTPUT_MD}" \
  "${GENERATED_AT}" \
  "${REPO_SLUG}" \
  "${WORKERS}" \
  "${ITERATIONS}" \
  "${QUIET_MODE}" \
  "${HELPER_COMMAND}" <<'PY'
from __future__ import annotations

import json
import os
import statistics
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

(
    fixture_path_raw,
    output_json_raw,
    output_md_raw,
    generated_at_raw,
    repo_slug_raw,
    workers_raw,
    iterations_raw,
    quiet_mode_raw,
    helper_command_raw,
) = sys.argv[1:]

fixture_path = Path(fixture_path_raw) if fixture_path_raw else None
output_json_path = Path(output_json_raw)
output_md_path = Path(output_md_raw)
workers = int(workers_raw)
iterations = int(iterations_raw)
quiet_mode = quiet_mode_raw == "true"
helper_command = helper_command_raw


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


def detect_repository_slug(explicit_repo: str) -> str:
    candidate = explicit_repo.strip()
    if candidate:
        return candidate
    try:
        completed = subprocess.run(
            ["gh", "repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"],
            text=True,
            capture_output=True,
            check=False,
        )
        if completed.returncode == 0:
            slug = completed.stdout.strip()
            if slug:
                return slug
    except Exception:
        pass
    return f"local/{Path.cwd().name}"


def require_number_list(payload: dict[str, Any], field: str) -> list[int]:
    if field not in payload:
        fail(f"fixture missing required field '{field}'")
    raw = payload[field]
    if not isinstance(raw, list) or not raw:
        fail(f"fixture field '{field}' must be a non-empty array")
    values: list[int] = []
    for value in raw:
        if not isinstance(value, (int, float)):
            fail(f"fixture field '{field}' must contain numeric values")
        values.append(int(round(float(value))))
    return values


def require_string(payload: dict[str, Any], field: str) -> str:
    value = payload.get(field)
    if not isinstance(value, str) or not value.strip():
        fail(f"fixture field '{field}' must be a non-empty string")
    return value


def run_timed_command(command: str) -> tuple[int, int]:
    start = time.perf_counter()
    completed = subprocess.run(command, shell=True, check=False)
    elapsed_ms = int(round((time.perf_counter() - start) * 1000))
    return elapsed_ms, completed.returncode


def median_ms(values: list[int]) -> int:
    return int(round(float(statistics.median(values))))


def status_for_delta(improvement_ms: int) -> str:
    if improvement_ms > 0:
        return "improved"
    if improvement_ms < 0:
        return "regressed"
    return "unchanged"


def render_markdown(report: dict[str, Any]) -> str:
    serial_samples = ", ".join(str(value) for value in report["serial_ms"])
    parallel_samples = ", ".join(str(value) for value in report["parallel_ms"])
    lines: list[str] = []
    lines.append("# M25 CI Cache + Parallel Tuning")
    lines.append("")
    lines.append(f"Generated: `{report['generated_at']}`")
    lines.append(f"Repository: `{report['repository']}`")
    lines.append(f"Source mode: `{report['source_mode']}`")
    lines.append("")
    lines.append("## Summary")
    lines.append("")
    lines.append("| Status | Serial median ms | Parallel median ms | Improvement ms | Improvement % |")
    lines.append("|---|---:|---:|---:|---:|")
    lines.append(
        f"| {report['status']} | {report['serial_median_ms']} | {report['parallel_median_ms']} | "
        f"{report['improvement_ms']} | {report['improvement_percent']:.2f} |"
    )
    lines.append("")
    lines.append("## Timing Samples")
    lines.append("")
    lines.append(f"- command: `{report['command']}`")
    lines.append(f"- parallel command: `{report['parallel_command']}`")
    lines.append(f"- workers: {report['workers']}")
    lines.append(f"- serial samples ms: [{serial_samples}]")
    lines.append(f"- parallel samples ms: [{parallel_samples}]")
    lines.append("")
    return "\n".join(lines)


generated_at = iso_utc(parse_iso8601_utc(generated_at_raw))
repository = detect_repository_slug(repo_slug_raw)

if fixture_path:
    payload_raw = json.loads(fixture_path.read_text(encoding="utf-8"))
    if not isinstance(payload_raw, dict):
        fail("fixture JSON must decode to an object")
    payload = payload_raw
    serial_ms = require_number_list(payload, "serial_ms")
    parallel_ms = require_number_list(payload, "parallel_ms")
    command = require_string(payload, "command")
    workers_value = payload.get("workers", workers)
    if not isinstance(workers_value, int) or workers_value <= 0:
        fail("fixture field 'workers' must be a positive integer when present")
    source_mode = "fixture"
else:
    serial_ms = []
    parallel_ms = []
    command = helper_command
    parallel_command = (
        "python3 .github/scripts/ci_helper_parallel_runner.py "
        f"--workers {workers} --start-dir .github/scripts --pattern \"test_*.py\""
    )
    for _ in range(iterations):
        duration_ms, exit_code = run_timed_command(command)
        if exit_code != 0:
            fail(f"serial helper command failed with exit code {exit_code}")
        serial_ms.append(duration_ms)
    for _ in range(iterations):
        duration_ms, exit_code = run_timed_command(parallel_command)
        if exit_code != 0:
            fail(f"parallel helper command failed with exit code {exit_code}")
        parallel_ms.append(duration_ms)
    workers_value = workers
    source_mode = "live"

parallel_command = (
    "python3 .github/scripts/ci_helper_parallel_runner.py "
    f"--workers {workers_value} --start-dir .github/scripts --pattern \"test_*.py\""
)
serial_median = median_ms(serial_ms)
parallel_median = median_ms(parallel_ms)
improvement_ms = serial_median - parallel_median
improvement_percent = round((improvement_ms / serial_median) * 100.0, 2) if serial_median > 0 else 0.0

report = {
    "schema_version": 1,
    "generated_at": generated_at,
    "repository": repository,
    "source_mode": source_mode,
    "command": command,
    "parallel_command": parallel_command,
    "workers": workers_value,
    "iterations": len(serial_ms),
    "serial_ms": serial_ms,
    "parallel_ms": parallel_ms,
    "serial_median_ms": serial_median,
    "parallel_median_ms": parallel_median,
    "improvement_ms": improvement_ms,
    "improvement_percent": improvement_percent,
    "status": status_for_delta(improvement_ms),
}

output_json_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
output_md_path.write_text(render_markdown(report), encoding="utf-8")

log(f"wrote {output_json_path}")
log(f"wrote {output_md_path}")
PY
