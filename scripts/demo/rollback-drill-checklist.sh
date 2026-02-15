#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

PROOF_SUMMARY_JSON="${REPO_ROOT}/tasks/reports/m21-retained-capability-proof-summary.json"
VALIDATION_MATRIX_JSON="${REPO_ROOT}/tasks/reports/m21-validation-matrix.json"
OUTPUT_JSON="${REPO_ROOT}/tasks/reports/m21-rollback-drill-checklist.json"
OUTPUT_MD="${REPO_ROOT}/tasks/reports/m21-rollback-drill-checklist.md"
GENERATED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
FAIL_ON_TRIGGER="false"
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: rollback-drill-checklist.sh [options]

Generate a standardized rollback drill checklist for consolidated runtime surfaces.

Options:
  --repo-root <path>              Repository root (default: detected from script location).
  --proof-summary-json <path>     Retained-capability proof summary JSON.
  --validation-matrix-json <path> M21 validation matrix JSON.
  --output-json <path>            Output JSON checklist report path.
  --output-md <path>              Output Markdown checklist report path.
  --generated-at <iso>            Override generated timestamp.
  --fail-on-trigger               Exit non-zero when rollback_required is true.
  --quiet                         Suppress informational output.
  --help                          Show this help text.
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
    --proof-summary-json)
      PROOF_SUMMARY_JSON="$2"
      shift 2
      ;;
    --validation-matrix-json)
      VALIDATION_MATRIX_JSON="$2"
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
    --fail-on-trigger)
      FAIL_ON_TRIGGER="true"
      shift
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

mkdir -p "$(dirname "${OUTPUT_JSON}")" "$(dirname "${OUTPUT_MD}")"

python3 - \
  "${REPO_ROOT}" \
  "${PROOF_SUMMARY_JSON}" \
  "${VALIDATION_MATRIX_JSON}" \
  "${OUTPUT_JSON}" \
  "${OUTPUT_MD}" \
  "${GENERATED_AT}" \
  "${FAIL_ON_TRIGGER}" \
  "${QUIET_MODE}" <<'PY'
import json
import sys
from pathlib import Path

(
    repo_root_raw,
    proof_summary_raw,
    validation_matrix_raw,
    output_json_raw,
    output_md_raw,
    generated_at,
    fail_on_trigger_raw,
    quiet_mode_raw,
) = sys.argv[1:]

repo_root = Path(repo_root_raw).resolve()
proof_summary_path = Path(proof_summary_raw)
if not proof_summary_path.is_absolute():
    proof_summary_path = (repo_root / proof_summary_path).resolve()
validation_matrix_path = Path(validation_matrix_raw)
if not validation_matrix_path.is_absolute():
    validation_matrix_path = (repo_root / validation_matrix_path).resolve()
output_json = Path(output_json_raw)
if not output_json.is_absolute():
    output_json = (repo_root / output_json).resolve()
output_md = Path(output_md_raw)
if not output_md.is_absolute():
    output_md = (repo_root / output_md).resolve()
fail_on_trigger = fail_on_trigger_raw == "true"
quiet_mode = quiet_mode_raw == "true"


def log_info(message: str) -> None:
    if not quiet_mode:
        print(message)


def load_json_if_present(path: Path):
    if not path.is_file():
        return None
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


proof_summary = load_json_if_present(proof_summary_path)
validation_matrix = load_json_if_present(validation_matrix_path)

triggers = []

def add_trigger(trigger_id: str, description: str, severity: str, active: bool, evidence: str):
    triggers.append(
        {
            "id": trigger_id,
            "description": description,
            "severity": severity,
            "active": active,
            "evidence": evidence,
        }
    )


if proof_summary is None:
    add_trigger(
        "proof-summary-missing",
        "Retained-capability proof summary artifact is missing.",
        "high",
        True,
        f"missing file: {proof_summary_path}",
    )
    proof_failed_runs = None
    proof_marker_missing = None
else:
    summary = proof_summary.get("summary", {})
    proof_failed_runs = int(summary.get("failed_runs", 0) or 0)
    proof_marker_missing = sum(
        int((run.get("marker_summary") or {}).get("missing", 0) or 0)
        for run in proof_summary.get("runs", [])
        if isinstance(run, dict)
    )
    add_trigger(
        "proof-runs-failed",
        "Retained-capability proof run(s) failed.",
        "high",
        proof_failed_runs > 0,
        f"failed_runs={proof_failed_runs}",
    )
    add_trigger(
        "proof-markers-missing",
        "Expected proof markers were missing in run output/artifacts.",
        "high",
        proof_marker_missing > 0,
        f"marker_missing={proof_marker_missing}",
    )

if validation_matrix is None:
    add_trigger(
        "validation-matrix-missing",
        "M21 validation matrix artifact is missing.",
        "medium",
        True,
        f"missing file: {validation_matrix_path}",
    )
else:
    summary = validation_matrix.get("summary", {})
    open_issues = int(summary.get("open_issues", 0) or 0)
    completion_percent = float(summary.get("completion_percent", 0.0) or 0.0)
    add_trigger(
        "validation-open-issues",
        "Validation matrix still has open tracked issues.",
        "medium",
        open_issues > 0,
        f"open_issues={open_issues}",
    )
    add_trigger(
        "validation-completion-below-100",
        "Validation matrix completion is below 100%.",
        "low",
        completion_percent < 100.0,
        f"completion_percent={completion_percent:.2f}",
    )

rollback_required = any(trigger["active"] and trigger["severity"] in {"high", "medium"} for trigger in triggers)

artifact_capture = [
    {
        "name": "proof-summary-json",
        "path": str(proof_summary_path),
        "required": True,
        "status": "present" if proof_summary_path.is_file() else "missing",
    },
    {
        "name": "validation-matrix-json",
        "path": str(validation_matrix_path),
        "required": True,
        "status": "present" if validation_matrix_path.is_file() else "missing",
    },
]

if isinstance(proof_summary, dict):
    report_paths = proof_summary.get("report_paths") or {}
    for key in ("markdown", "logs_dir", "artifacts_dir"):
        value = report_paths.get(key)
        if isinstance(value, str) and value:
            artifact_capture.append(
                {
                    "name": f"proof-{key}",
                    "path": value,
                    "required": True,
                    "status": "present" if Path(value).exists() else "missing",
                }
            )

steps = [
    {
        "order": 1,
        "title": "Freeze rollout and capture gate state",
        "action": "Pause promotion and snapshot current rollout gate diagnostics.",
        "command": "./scripts/demo/rollback-drill-checklist.sh --output-json tasks/reports/m21-rollback-drill-checklist.json --output-md tasks/reports/m21-rollback-drill-checklist.md",
    },
    {
        "order": 2,
        "title": "Collect required rollback artifacts",
        "action": "Archive proof summaries, validation matrix output, and per-run logs before any revert.",
        "command": "tar -czf tasks/reports/m21-rollback-artifacts.tgz tasks/reports/m21-retained-capability-proof-summary.json tasks/reports/m21-validation-matrix.json tasks/reports/m21-retained-capability-proof-logs",
    },
    {
        "order": 3,
        "title": "Execute bounded rollback",
        "action": "Revert the candidate consolidation commit(s) and rerun retained-capability proof matrix.",
        "command": "git revert <commit-sha> && ./scripts/dev/m21-retained-capability-proof-summary.sh --binary ./target/debug/tau-coding-agent",
    },
    {
        "order": 4,
        "title": "Verify rollback exit criteria",
        "action": "Confirm proof summary failed_runs=0 and marker_missing=0 before reopening rollout.",
        "command": "jq '.summary' tasks/reports/m21-retained-capability-proof-summary.json",
    },
]

report = {
    "schema_version": 1,
    "generated_at": generated_at,
    "rollback_required": rollback_required,
    "inputs": {
        "proof_summary_json": str(proof_summary_path),
        "validation_matrix_json": str(validation_matrix_path),
    },
    "triggers": triggers,
    "checklist": {
        "steps": steps,
        "artifact_capture": artifact_capture,
        "exit_criteria": [
            "All high/medium rollback triggers are inactive.",
            "Retained-capability proof summary reports failed_runs == 0.",
            "Retained-capability proof summary marker missing count == 0.",
        ],
    },
}

output_json.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

lines = []
lines.append("# M21 Rollback Drill Checklist")
lines.append("")
lines.append(f"- Generated: {generated_at}")
lines.append(f"- Rollback required: {'yes' if rollback_required else 'no'}")
lines.append("")
lines.append("## Trigger Conditions")
lines.append("")
lines.append("| Trigger | Severity | Active | Evidence |")
lines.append("| --- | --- | --- | --- |")
for trigger in triggers:
    lines.append(
        f"| {trigger['id']} | {trigger['severity']} | "
        f"{'yes' if trigger['active'] else 'no'} | {trigger['evidence']} |"
    )
lines.append("")
lines.append("## Rollback Drill Steps")
lines.append("")
for step in steps:
    lines.append(f"{step['order']}. **{step['title']}**")
    lines.append(f"   - Action: {step['action']}")
    lines.append(f"   - Command: `{step['command']}`")
lines.append("")
lines.append("## Artifact Capture Checklist")
lines.append("")
lines.append("| Artifact | Required | Status | Path |")
lines.append("| --- | --- | --- | --- |")
for artifact in artifact_capture:
    lines.append(
        f"| {artifact['name']} | {'yes' if artifact['required'] else 'no'} | "
        f"{artifact['status']} | `{artifact['path']}` |"
    )
lines.append("")
lines.append("## Exit Criteria")
lines.append("")
for criterion in report["checklist"]["exit_criteria"]:
    lines.append(f"- {criterion}")
lines.append("")

output_md.write_text("\n".join(lines), encoding="utf-8")

log_info(f"[rollback-drill] wrote JSON: {output_json}")
log_info(f"[rollback-drill] wrote Markdown: {output_md}")
log_info(f"[rollback-drill] rollback_required={'yes' if rollback_required else 'no'}")

if fail_on_trigger and rollback_required:
    raise SystemExit(1)
PY

log_info "rollback drill checklist generation complete"
