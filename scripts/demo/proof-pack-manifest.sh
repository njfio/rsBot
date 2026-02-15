#!/usr/bin/env bash
set -euo pipefail

OUTPUT_PATH=""
PACK_NAME="m21-live-proof-pack"
PRODUCER_SCRIPT=""
MODE="run"
REPORT_FILE=""
MILESTONE="Gap Closure Wave 2026-03: Structural Runtime Hardening"
STATUS_OVERRIDE=""
GENERATED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
QUIET_MODE="false"
ISSUES=("#1742")
ARTIFACT_ENTRIES=()

usage() {
  cat <<'EOF'
Usage: proof-pack-manifest.sh --output PATH [options]

Generate a standard artifact manifest for M21 live proof packs.

Options:
  --output PATH            Manifest output path (required).
  --pack-name NAME         Logical proof-pack name.
  --producer-script PATH   Producing wrapper/script path.
  --mode MODE              Emission mode (for example: list/run).
  --report-file PATH       Primary report artifact path.
  --artifact NAME=PATH     Additional artifact entry (repeatable).
  --issue ID               Issue reference (repeatable; default: #1742).
  --milestone NAME         Milestone label.
  --status STATUS          Override computed status (pass/fail/unknown).
  --generated-at ISO       Override generated timestamp.
  --quiet                  Suppress informational logs.
  --help                   Show this help text.
EOF
}

log_info() {
  if [[ "${QUIET_MODE}" != "true" ]]; then
    echo "$@" >&2
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output)
      OUTPUT_PATH="$2"
      shift 2
      ;;
    --pack-name)
      PACK_NAME="$2"
      shift 2
      ;;
    --producer-script)
      PRODUCER_SCRIPT="$2"
      shift 2
      ;;
    --mode)
      MODE="$2"
      shift 2
      ;;
    --report-file)
      REPORT_FILE="$2"
      shift 2
      ;;
    --artifact)
      ARTIFACT_ENTRIES+=("$2")
      shift 2
      ;;
    --issue)
      ISSUES+=("$2")
      shift 2
      ;;
    --milestone)
      MILESTONE="$2"
      shift 2
      ;;
    --status)
      STATUS_OVERRIDE="$2"
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

if [[ -z "${OUTPUT_PATH}" ]]; then
  echo "error: --output is required" >&2
  usage >&2
  exit 2
fi

if [[ -n "${REPORT_FILE}" ]]; then
  ARTIFACT_ENTRIES=("report=${REPORT_FILE}" "${ARTIFACT_ENTRIES[@]}")
fi

tmp_artifacts="$(mktemp)"
tmp_issues="$(mktemp)"
cleanup() {
  rm -f "${tmp_artifacts}" "${tmp_issues}"
}
trap cleanup EXIT

for issue in "${ISSUES[@]}"; do
  if [[ -n "${issue}" ]]; then
    echo "${issue}" >>"${tmp_issues}"
  fi
done

for artifact in "${ARTIFACT_ENTRIES[@]}"; do
  if [[ "${artifact}" != *=* ]]; then
    echo "error: --artifact must be NAME=PATH, got '${artifact}'" >&2
    exit 2
  fi
  name="${artifact%%=*}"
  path="${artifact#*=}"
  if [[ -z "${name}" || -z "${path}" ]]; then
    echo "error: --artifact must include non-empty NAME and PATH, got '${artifact}'" >&2
    exit 2
  fi
  printf '%s\t%s\n' "${name}" "${path}" >>"${tmp_artifacts}"
done

mkdir -p "$(dirname "${OUTPUT_PATH}")"

python3 - \
  "${tmp_artifacts}" \
  "${tmp_issues}" \
  "${OUTPUT_PATH}" \
  "${PACK_NAME}" \
  "${PRODUCER_SCRIPT}" \
  "${MODE}" \
  "${MILESTONE}" \
  "${STATUS_OVERRIDE}" \
  "${GENERATED_AT}" <<'PY'
import json
import sys
from pathlib import Path

(
    artifacts_path,
    issues_path,
    output_path,
    pack_name,
    producer_script,
    mode,
    milestone,
    status_override,
    generated_at,
) = sys.argv[1:]


def load_summary(report_path: Path):
    if not report_path.exists():
        return None
    try:
        payload = json.loads(report_path.read_text(encoding="utf-8"))
    except Exception:
        return None
    summary = payload.get("summary")
    if not isinstance(summary, dict):
        return None
    total = summary.get("total")
    passed = summary.get("passed")
    failed = summary.get("failed")
    if not all(isinstance(value, int) for value in (total, passed, failed)):
        return None
    return {"total": total, "passed": passed, "failed": failed}


issues = []
seen_issues = set()
with open(issues_path, encoding="utf-8") as handle:
    for raw in handle:
        issue = raw.strip()
        if issue and issue not in seen_issues:
            seen_issues.add(issue)
            issues.append(issue)

artifacts = []
missing_required = False
summary_from_report = None
seen_artifact_names = set()
with open(artifacts_path, encoding="utf-8") as handle:
    for raw in handle:
        row = raw.rstrip("\n")
        if not row:
            continue
        name, path = row.split("\t", 1)
        if name in seen_artifact_names:
            raise SystemExit(f"duplicate artifact name '{name}'")
        seen_artifact_names.add(name)
        artifact_path = Path(path)
        exists = artifact_path.exists()
        status = "present" if exists else "missing"
        if not exists:
            missing_required = True
        entry = {
            "name": name,
            "path": path,
            "required": True,
            "status": status,
        }
        artifacts.append(entry)
        if name == "report":
            summary_from_report = load_summary(artifact_path)

if status_override:
    status = status_override
elif missing_required:
    status = "fail"
elif summary_from_report is not None:
    status = "pass" if summary_from_report["failed"] == 0 else "fail"
else:
    status = "unknown"

summary_payload = {
    "status": status,
    "total": None,
    "passed": None,
    "failed": None,
}
if summary_from_report is not None:
    summary_payload.update(summary_from_report)

payload = {
    "schema_version": 1,
    "generated_at": generated_at,
    "pack_name": pack_name,
    "milestone": milestone,
    "issues": issues,
    "producer": {
        "script": producer_script,
        "mode": mode,
    },
    "artifacts": artifacts,
    "summary": summary_payload,
}

output = Path(output_path)
output.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
PY

log_info "wrote proof-pack manifest: ${OUTPUT_PATH}"
