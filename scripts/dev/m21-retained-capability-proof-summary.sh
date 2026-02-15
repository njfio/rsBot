#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

BINARY_PATH="${REPO_ROOT}/target/debug/tau-coding-agent"
REPORTS_DIR="${REPO_ROOT}/tasks/reports/m21-retained-capability-artifacts"
LOGS_DIR="${REPO_ROOT}/tasks/reports/m21-retained-capability-proof-logs"
OUTPUT_JSON="${REPO_ROOT}/tasks/reports/m21-retained-capability-proof-summary.json"
OUTPUT_MD="${REPO_ROOT}/tasks/reports/m21-retained-capability-proof-summary.md"
MATRIX_JSON=""
GENERATED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: m21-retained-capability-proof-summary.sh [options]

Execute retained-capability proof runs and emit reproducible JSON/Markdown summaries.

By default this runs a bounded proof matrix based on scripts/demo wrappers and writes:
  - tasks/reports/m21-retained-capability-proof-summary.json
  - tasks/reports/m21-retained-capability-proof-summary.md
  - tasks/reports/m21-retained-capability-proof-logs/
  - tasks/reports/m21-retained-capability-artifacts/

Options:
  --repo-root <path>      Repository root (default: detected from script location).
  --binary <path>         tau-coding-agent binary path used by default matrix runs.
  --reports-dir <path>    Artifact directory for proof run outputs.
  --logs-dir <path>       Directory for per-run stdout/stderr logs.
  --output-json <path>    JSON summary output path.
  --output-md <path>      Markdown summary output path.
  --matrix-json <path>    Optional custom proof matrix JSON.
  --generated-at <iso>    Override generated timestamp.
  --quiet                 Suppress informational logs.
  --help                  Show this help text.
EOF
}

log_info() {
  if [[ "${QUIET_MODE}" != "true" ]]; then
    echo "$@"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo-root)
      REPO_ROOT="$2"
      shift 2
      ;;
    --binary)
      BINARY_PATH="$2"
      shift 2
      ;;
    --reports-dir)
      REPORTS_DIR="$2"
      shift 2
      ;;
    --logs-dir)
      LOGS_DIR="$2"
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
    --matrix-json)
      MATRIX_JSON="$2"
      shift 2
      ;;
    --generated-at)
      GENERATED_AT="$2"
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
      exit 2
      ;;
  esac
done

if ! command -v python3 >/dev/null 2>&1; then
  echo "error: required command 'python3' not found" >&2
  exit 1
fi

if [[ -n "${MATRIX_JSON}" && ! -f "${MATRIX_JSON}" ]]; then
  echo "error: matrix JSON not found: ${MATRIX_JSON}" >&2
  exit 1
fi

mkdir -p "$(dirname "${OUTPUT_JSON}")" "$(dirname "${OUTPUT_MD}")" "${REPORTS_DIR}" "${LOGS_DIR}"

python3 - \
  "${REPO_ROOT}" \
  "${BINARY_PATH}" \
  "${REPORTS_DIR}" \
  "${LOGS_DIR}" \
  "${OUTPUT_JSON}" \
  "${OUTPUT_MD}" \
  "${MATRIX_JSON}" \
  "${GENERATED_AT}" \
  "${QUIET_MODE}" <<'PY'
import json
import shlex
import subprocess
import sys
import time
from pathlib import Path

(
    repo_root_raw,
    binary_path_raw,
    reports_dir_raw,
    logs_dir_raw,
    output_json_raw,
    output_md_raw,
    matrix_json_raw,
    generated_at,
    quiet_mode_raw,
) = sys.argv[1:]

repo_root = Path(repo_root_raw).resolve()
binary_path = Path(binary_path_raw)
if not binary_path.is_absolute():
    binary_path = (repo_root / binary_path).resolve()
reports_dir = Path(reports_dir_raw)
if not reports_dir.is_absolute():
    reports_dir = (repo_root / reports_dir).resolve()
logs_dir = Path(logs_dir_raw)
if not logs_dir.is_absolute():
    logs_dir = (repo_root / logs_dir).resolve()
output_json = Path(output_json_raw)
if not output_json.is_absolute():
    output_json = (repo_root / output_json).resolve()
output_md = Path(output_md_raw)
if not output_md.is_absolute():
    output_md = (repo_root / output_md).resolve()
matrix_json = Path(matrix_json_raw).resolve() if matrix_json_raw else None
quiet_mode = quiet_mode_raw == "true"
default_matrix_path = repo_root / "scripts/demo/m21-retained-capability-proof-matrix.json"


def log_info(message: str) -> None:
    if not quiet_mode:
        print(message)


def default_matrix() -> dict:
    return {
        "schema_version": 1,
        "name": "m21-retained-capability-proof-matrix",
        "issues": ["#1746"],
        "runs": [
            {
                "name": "demo-index-retained-scenarios",
                "description": (
                    "Run retained scenario subset via demo index wrapper and emit "
                    "report/manifest artifacts."
                ),
                "command": [
                    "./scripts/demo/index.sh",
                    "--skip-build",
                    "--repo-root",
                    "{repo_root}",
                    "--binary",
                    "{binary}",
                    "--only",
                    "onboarding,gateway-auth,gateway-remote-access,multi-channel-live,deployment-wasm",
                    "--json",
                    "--report-file",
                    "{reports_dir}/demo-index-retained-summary.json",
                    "--manifest-file",
                    "{reports_dir}/demo-index-retained-summary.manifest.json",
                ],
                "expected_exit_code": 0,
                "markers": [
                    {
                        "id": "index-summary-json",
                        "source": "stdout",
                        "contains": '"summary":{"total":',
                    },
                    {
                        "id": "index-summary-failed-zero",
                        "source": "stdout",
                        "contains": '"failed":0',
                    },
                    {
                        "id": "index-report-artifact",
                        "source": "file",
                        "path": "{reports_dir}/demo-index-retained-summary.json",
                        "contains": '"summary":{"total":',
                    },
                    {
                        "id": "index-manifest-artifact",
                        "source": "file",
                        "path": "{reports_dir}/demo-index-retained-summary.manifest.json",
                        "contains": '"pack_name": "demo-index-live-proof-pack"',
                    },
                ],
            },
            {
                "name": "demo-all-retained-demos",
                "description": (
                    "Run retained demo subset via all.sh wrapper and emit report/manifest artifacts."
                ),
                "command": [
                    "./scripts/demo/all.sh",
                    "--skip-build",
                    "--repo-root",
                    "{repo_root}",
                    "--binary",
                    "{binary}",
                    "--only",
                    "local,multi-channel,gateway-auth,gateway-remote-access,deployment",
                    "--json",
                    "--report-file",
                    "{reports_dir}/demo-all-retained-summary.json",
                    "--manifest-file",
                    "{reports_dir}/demo-all-retained-summary.manifest.json",
                ],
                "expected_exit_code": 0,
                "markers": [
                    {
                        "id": "all-summary-json",
                        "source": "stdout",
                        "contains": '"summary":{"total":',
                    },
                    {
                        "id": "all-summary-failed-zero",
                        "source": "stdout",
                        "contains": '"failed":0',
                    },
                    {
                        "id": "all-report-artifact",
                        "source": "file",
                        "path": "{reports_dir}/demo-all-retained-summary.json",
                        "contains": '"summary":{"total":',
                    },
                    {
                        "id": "all-manifest-artifact",
                        "source": "file",
                        "path": "{reports_dir}/demo-all-retained-summary.manifest.json",
                        "contains": '"pack_name": "demo-all-live-proof-pack"',
                    },
                ],
            },
            {
                "name": "proof-pack-rollup-manifest",
                "description": "Emit rollup manifest for retained-capability proof artifacts.",
                "command": [
                    "./scripts/demo/proof-pack-manifest.sh",
                    "--output",
                    "{reports_dir}/m21-retained-capability-proof-pack.manifest.json",
                    "--pack-name",
                    "m21-retained-capability-live-proof-pack",
                    "--producer-script",
                    "scripts/dev/m21-retained-capability-proof-summary.sh",
                    "--mode",
                    "run",
                    "--report-file",
                    "{reports_dir}/demo-all-retained-summary.json",
                    "--artifact",
                    "index-report={reports_dir}/demo-index-retained-summary.json",
                    "--artifact",
                    "index-manifest={reports_dir}/demo-index-retained-summary.manifest.json",
                    "--artifact",
                    "all-manifest={reports_dir}/demo-all-retained-summary.manifest.json",
                    "--issue",
                    "#1746",
                ],
                "expected_exit_code": 0,
                "markers": [
                    {
                        "id": "rollup-manifest-log",
                        "source": "stderr",
                        "contains": "wrote proof-pack manifest:",
                    },
                    {
                        "id": "rollup-manifest-pack-name",
                        "source": "file",
                        "path": "{reports_dir}/m21-retained-capability-proof-pack.manifest.json",
                        "contains": '"pack_name": "m21-retained-capability-live-proof-pack"',
                    },
                    {
                        "id": "rollup-manifest-status-pass",
                        "source": "file",
                        "path": "{reports_dir}/m21-retained-capability-proof-pack.manifest.json",
                        "contains": '"status": "pass"',
                    },
                ],
            },
        ],
    }


def load_matrix(path: Path | None) -> tuple[dict, str]:
    source = "embedded-default"
    matrix_path = path
    if matrix_path is None and default_matrix_path.is_file():
        matrix_path = default_matrix_path
    if matrix_path is None:
        return default_matrix(), source
    source = str(matrix_path)
    with matrix_path.open(encoding="utf-8") as handle:
        matrix = json.load(handle)
    if not isinstance(matrix, dict):
        raise SystemExit("error: matrix JSON must decode to an object")
    return matrix, source


def sanitize_name(raw: str) -> str:
    sanitized = "".join(ch if ch.isalnum() or ch in {"-", "_"} else "-" for ch in raw)
    sanitized = sanitized.strip("-")
    return sanitized or "run"


def ensure_path(path_text: str) -> Path:
    path = Path(path_text)
    if not path.is_absolute():
        path = (repo_root / path).resolve()
    return path


def render_template(raw: str) -> str:
    rendered = raw
    replacements = {
        "{repo_root}": str(repo_root),
        "{binary}": str(binary_path),
        "{reports_dir}": str(reports_dir),
        "{logs_dir}": str(logs_dir),
        "{output_json}": str(output_json),
        "{output_md}": str(output_md),
    }
    for token, value in replacements.items():
        rendered = rendered.replace(token, value)
    return rendered


def render_command(args: list[str]) -> list[str]:
    return [render_template(item) for item in args]


def evaluate_marker(marker: dict, stdout_text: str, stderr_text: str) -> dict:
    marker_id = marker.get("id") or "marker"
    source = marker.get("source")
    contains = marker.get("contains")
    marker_path_raw = marker.get("path")
    marker_path = render_template(marker_path_raw) if isinstance(marker_path_raw, str) else None
    matched = False
    detail = ""

    if source == "stdout":
        if not isinstance(contains, str) or not contains:
            raise SystemExit(f"error: marker '{marker_id}' stdout markers require non-empty contains")
        matched = contains in stdout_text
        detail = "matched stdout substring" if matched else "stdout substring missing"
    elif source == "stderr":
        if not isinstance(contains, str) or not contains:
            raise SystemExit(f"error: marker '{marker_id}' stderr markers require non-empty contains")
        matched = contains in stderr_text
        detail = "matched stderr substring" if matched else "stderr substring missing"
    elif source == "file":
        if not isinstance(marker_path, str) or not marker_path:
            raise SystemExit(f"error: marker '{marker_id}' file markers require path")
        path = ensure_path(marker_path)
        if not path.exists():
            matched = False
            detail = f"file missing: {path}"
        else:
            if contains is None:
                matched = True
                detail = f"file present: {path}"
            else:
                if not isinstance(contains, str) or not contains:
                    raise SystemExit(
                        f"error: marker '{marker_id}' file marker contains must be non-empty when set"
                    )
                text = path.read_text(encoding="utf-8")
                matched = contains in text
                detail = (
                    f"matched file substring in {path}"
                    if matched
                    else f"file substring missing in {path}"
                )
    else:
        raise SystemExit(f"error: marker '{marker_id}' has unsupported source '{source}'")

    payload = {
        "id": marker_id,
        "source": source,
        "contains": contains,
        "path": marker_path,
        "matched": matched,
        "detail": detail,
    }
    return payload


matrix, matrix_source = load_matrix(matrix_json)
schema_version = matrix.get("schema_version", 1)
if schema_version != 1:
    raise SystemExit(
        f"error: unsupported matrix schema_version {schema_version}; expected 1"
    )

matrix_name = matrix.get("name", "retained-capability-proof-matrix")
issues = matrix.get("issues", [])
if issues is None:
    issues = []
if not isinstance(issues, list):
    raise SystemExit("error: matrix issues must be an array when present")
issues = [str(issue) for issue in issues if str(issue).strip()]

runs = matrix.get("runs")
if not isinstance(runs, list) or not runs:
    raise SystemExit("error: matrix runs must be a non-empty array")

reports_dir.mkdir(parents=True, exist_ok=True)
logs_dir.mkdir(parents=True, exist_ok=True)
output_json.parent.mkdir(parents=True, exist_ok=True)
output_md.parent.mkdir(parents=True, exist_ok=True)

results: list[dict] = []
for index, run in enumerate(runs, start=1):
    if not isinstance(run, dict):
        raise SystemExit(f"error: runs[{index - 1}] must be an object")
    name = run.get("name")
    if not isinstance(name, str) or not name.strip():
        raise SystemExit(f"error: runs[{index - 1}].name must be a non-empty string")
    name = name.strip()
    description = run.get("description", "")
    if description is None:
        description = ""
    if not isinstance(description, str):
        raise SystemExit(f"error: runs[{index - 1}].description must be a string when set")
    command = run.get("command")
    if not isinstance(command, list) or not command:
        raise SystemExit(f"error: runs[{index - 1}].command must be a non-empty array")
    command_rendered: list[str] = []
    for arg_index, arg in enumerate(command):
        if not isinstance(arg, str) or not arg:
            raise SystemExit(
                f"error: runs[{index - 1}].command[{arg_index}] must be a non-empty string"
            )
        command_rendered.append(render_template(arg))

    expected_exit_code = run.get("expected_exit_code", 0)
    if not isinstance(expected_exit_code, int):
        raise SystemExit(
            f"error: runs[{index - 1}].expected_exit_code must be an integer when set"
        )

    if any("{binary}" in arg for arg in command) and not binary_path.is_file():
        raise SystemExit(
            f"error: binary path does not exist for run '{name}': {binary_path}"
        )

    safe_name = sanitize_name(name)
    stdout_log = logs_dir / f"{index:02d}-{safe_name}.stdout.log"
    stderr_log = logs_dir / f"{index:02d}-{safe_name}.stderr.log"

    started = time.perf_counter()
    completed = subprocess.run(
        command_rendered,
        cwd=repo_root,
        text=True,
        capture_output=True,
        check=False,
    )
    duration_ms = int((time.perf_counter() - started) * 1000)

    stdout_log.write_text(completed.stdout, encoding="utf-8")
    stderr_log.write_text(completed.stderr, encoding="utf-8")

    markers = run.get("markers", [])
    if not isinstance(markers, list):
        raise SystemExit(f"error: runs[{index - 1}].markers must be an array when set")

    marker_results = []
    matched_markers = 0
    for marker_index, marker in enumerate(markers):
        if not isinstance(marker, dict):
            raise SystemExit(
                f"error: runs[{index - 1}].markers[{marker_index}] must be an object"
            )
        if "id" not in marker:
            marker = dict(marker)
            marker["id"] = f"{safe_name}-marker-{marker_index + 1}"
        marker_result = evaluate_marker(marker, completed.stdout, completed.stderr)
        marker_results.append(marker_result)
        if marker_result["matched"]:
            matched_markers += 1

    failure_reasons: list[str] = []
    if completed.returncode != expected_exit_code:
        failure_reasons.append(
            f"exit-code-mismatch expected={expected_exit_code} actual={completed.returncode}"
        )
    for marker in marker_results:
        if not marker["matched"]:
            failure_reasons.append(f"marker-missing:{marker['id']}")

    status = "pass" if not failure_reasons else "fail"
    results.append(
        {
            "name": name,
            "description": description,
            "status": status,
            "command": command_rendered,
            "command_line": shlex.join(command_rendered),
            "expected_exit_code": expected_exit_code,
            "exit_code": completed.returncode,
            "duration_ms": duration_ms,
            "stdout_log": str(stdout_log),
            "stderr_log": str(stderr_log),
            "marker_summary": {
                "total": len(marker_results),
                "matched": matched_markers,
                "missing": len(marker_results) - matched_markers,
            },
            "markers": marker_results,
            "failure_reasons": failure_reasons,
        }
    )

    log_info(
        f"[m21-proof-summary] run={name} status={status} "
        f"exit={completed.returncode} markers={matched_markers}/{len(marker_results)}"
    )

passed_runs = sum(1 for entry in results if entry["status"] == "pass")
failed_runs = len(results) - passed_runs
summary_status = "pass" if failed_runs == 0 else "fail"

payload = {
    "schema_version": 1,
    "generated_at": generated_at,
    "matrix_name": matrix_name,
    "matrix_source": matrix_source,
    "issues": issues,
    "repository_root": str(repo_root),
    "report_paths": {
        "json": str(output_json),
        "markdown": str(output_md),
        "logs_dir": str(logs_dir),
        "artifacts_dir": str(reports_dir),
    },
    "summary": {
        "total_runs": len(results),
        "passed_runs": passed_runs,
        "failed_runs": failed_runs,
        "status": summary_status,
    },
    "runs": results,
}

output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

lines: list[str] = []
lines.append("# M21 Retained-Capability Proof Summary")
lines.append("")
lines.append(f"- Generated: {generated_at}")
lines.append(f"- Matrix: {matrix_name}")
lines.append(f"- Matrix source: {matrix_source}")
if issues:
    lines.append(f"- Issues: {', '.join(issues)}")
lines.append(f"- Status: {summary_status}")
lines.append("")
lines.append("## Report Paths")
lines.append("")
lines.append(f"- JSON: `{output_json}`")
lines.append(f"- Markdown: `{output_md}`")
lines.append(f"- Logs directory: `{logs_dir}`")
lines.append(f"- Artifact directory: `{reports_dir}`")
lines.append("")
lines.append("## Summary")
lines.append("")
lines.append("| Metric | Value |")
lines.append("| --- | ---: |")
lines.append(f"| Total runs | {len(results)} |")
lines.append(f"| Passed runs | {passed_runs} |")
lines.append(f"| Failed runs | {failed_runs} |")
lines.append("")
lines.append("## Run Matrix")
lines.append("")
lines.append("| Run | Status | Exit | Markers | Stdout Log | Stderr Log |")
lines.append("| --- | --- | ---: | ---: | --- | --- |")
for entry in results:
    marker_summary = entry["marker_summary"]
    lines.append(
        f"| {entry['name']} | {entry['status']} | {entry['exit_code']} | "
        f"{marker_summary['matched']}/{marker_summary['total']} | "
        f"`{entry['stdout_log']}` | `{entry['stderr_log']}` |"
    )
lines.append("")

failed_entries = [entry for entry in results if entry["status"] == "fail"]
if failed_entries:
    lines.append("## Failure Diagnostics")
    lines.append("")
    for entry in failed_entries:
        lines.append(f"### {entry['name']}")
        lines.append("")
        lines.append(f"- Command: `{entry['command_line']}`")
        lines.append(f"- Exit: {entry['exit_code']} (expected {entry['expected_exit_code']})")
        if entry["failure_reasons"]:
            lines.append(f"- Reasons: {', '.join(entry['failure_reasons'])}")
        lines.append(f"- Stdout: `{entry['stdout_log']}`")
        lines.append(f"- Stderr: `{entry['stderr_log']}`")
        lines.append("")

output_md.write_text("\n".join(lines) + "\n", encoding="utf-8")

log_info(f"[m21-proof-summary] wrote JSON summary: {output_json}")
log_info(f"[m21-proof-summary] wrote Markdown summary: {output_md}")

if failed_runs > 0:
    raise SystemExit(1)
PY

log_info "retained-capability proof summary generation complete"
