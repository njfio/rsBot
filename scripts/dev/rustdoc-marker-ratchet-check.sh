#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

REPO_ROOT="${DEFAULT_REPO_ROOT}"
POLICY_FILE="${DEFAULT_REPO_ROOT}/tasks/policies/m23-doc-ratchet-policy.json"
CURRENT_JSON="${DEFAULT_REPO_ROOT}/tasks/reports/m23-rustdoc-marker-count.json"
OUTPUT_JSON="${DEFAULT_REPO_ROOT}/tasks/reports/m23-rustdoc-marker-ratchet-check.json"
OUTPUT_MD="${DEFAULT_REPO_ROOT}/tasks/reports/m23-rustdoc-marker-ratchet-check.md"
GENERATED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: rustdoc-marker-ratchet-check.sh [options]

Validate current rustdoc marker totals against M23 ratchet floor policy and
emit per-crate regression deltas.

Options:
  --repo-root <path>      Repository root (default: auto-detected).
  --policy-file <path>    Ratchet policy JSON path (default: tasks/policies/m23-doc-ratchet-policy.json).
  --current-json <path>   Current marker count JSON path (default: tasks/reports/m23-rustdoc-marker-count.json).
  --output-json <path>    Output JSON artifact path (default: tasks/reports/m23-rustdoc-marker-ratchet-check.json).
  --output-md <path>      Output Markdown artifact path (default: tasks/reports/m23-rustdoc-marker-ratchet-check.md).
  --generated-at <iso>    Override generated-at timestamp (UTC ISO-8601).
  --quiet                 Suppress informational stdout summary.
  --help                  Show this help text.
EOF
}

fail() {
  echo "error: $*" >&2
  exit 1
}

log_info() {
  if [[ "${QUIET_MODE}" != "true" ]]; then
    echo "$@"
  fi
}

resolve_path() {
  local base="$1"
  local path="$2"
  if [[ "${path}" = /* ]]; then
    printf '%s\n' "${path}"
  else
    printf '%s\n' "${base}/${path}"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo-root)
      REPO_ROOT="$2"
      shift 2
      ;;
    --policy-file)
      POLICY_FILE="$2"
      shift 2
      ;;
    --current-json)
      CURRENT_JSON="$2"
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
    --quiet)
      QUIET_MODE="true"
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      fail "unknown option '$1'"
      ;;
  esac
done

if [[ ! -d "${REPO_ROOT}" ]]; then
  fail "repo root not found: ${REPO_ROOT}"
fi

POLICY_FILE_ABS="$(resolve_path "${REPO_ROOT}" "${POLICY_FILE}")"
CURRENT_JSON_ABS="$(resolve_path "${REPO_ROOT}" "${CURRENT_JSON}")"
OUTPUT_JSON_ABS="$(resolve_path "${REPO_ROOT}" "${OUTPUT_JSON}")"
OUTPUT_MD_ABS="$(resolve_path "${REPO_ROOT}" "${OUTPUT_MD}")"

if [[ ! -f "${POLICY_FILE_ABS}" ]]; then
  fail "policy file not found: ${POLICY_FILE_ABS}"
fi
if [[ ! -f "${CURRENT_JSON_ABS}" ]]; then
  fail "current json not found: ${CURRENT_JSON_ABS}"
fi

mkdir -p "$(dirname "${OUTPUT_JSON_ABS}")"
mkdir -p "$(dirname "${OUTPUT_MD_ABS}")"

python3 - \
  "${POLICY_FILE_ABS}" \
  "${CURRENT_JSON_ABS}" \
  "${REPO_ROOT}" \
  "${GENERATED_AT}" \
  "${OUTPUT_JSON_ABS}" \
  "${OUTPUT_MD_ABS}" <<'PY'
import json
import pathlib
import sys

(
    policy_path,
    current_json_path,
    repo_root,
    generated_at,
    output_json_path,
    output_md_path,
) = sys.argv[1:]

policy = json.loads(pathlib.Path(policy_path).read_text(encoding="utf-8"))
current = json.loads(pathlib.Path(current_json_path).read_text(encoding="utf-8"))

if int(policy.get("schema_version", 0)) != 1:
    raise SystemExit("policy schema_version must be 1")

floor = int(policy["floor_markers"])
baseline_artifact_raw = policy["baseline_artifact"]
baseline_path = pathlib.Path(repo_root) / baseline_artifact_raw
if not baseline_path.is_file():
    raise SystemExit(f"baseline artifact not found: {baseline_path}")

baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
baseline_by_crate = {row["crate"]: row for row in baseline.get("crates", [])}
current_by_crate = {row["crate"]: row for row in current.get("crates", [])}
all_crates = sorted(set(baseline_by_crate) | set(current_by_crate))

crate_deltas = []
for crate in all_crates:
    baseline_row = baseline_by_crate.get(crate, {})
    current_row = current_by_crate.get(crate, {})
    baseline_markers = int(baseline_row.get("markers", 0))
    current_markers = int(current_row.get("markers", 0))
    crate_deltas.append(
        {
            "crate": crate,
            "baseline_markers": baseline_markers,
            "current_markers": current_markers,
            "delta_markers": current_markers - baseline_markers,
            "baseline_files_scanned": int(baseline_row.get("files_scanned", 0)),
            "current_files_scanned": int(current_row.get("files_scanned", 0)),
        }
    )

crate_deltas.sort(key=lambda row: row["crate"])
negative = sorted(
    [row for row in crate_deltas if row["delta_markers"] < 0],
    key=lambda row: (row["delta_markers"], row["crate"]),
)

baseline_total = int(baseline.get("total_markers", 0))
current_total = int(current.get("total_markers", 0))
delta_total = current_total - baseline_total
below_floor_by = max(0, floor - current_total)
meets_floor = current_total >= floor

payload = {
    "schema_version": 1,
    "generated_at": generated_at,
    "policy_file": policy_path,
    "baseline_artifact": str(baseline_path),
    "current_artifact": current_json_path,
    "floor_markers": floor,
    "baseline_total_markers": baseline_total,
    "current_total_markers": current_total,
    "delta_total_markers": delta_total,
    "below_floor_by": below_floor_by,
    "meets_floor": meets_floor,
    "fail_on_regression": bool(policy.get("fail_on_regression", True)),
    "crate_deltas": crate_deltas,
    "negative_crate_deltas": negative,
}

pathlib.Path(output_json_path).write_text(
    json.dumps(payload, indent=2) + "\n",
    encoding="utf-8",
)

status = "PASS" if meets_floor else "FAIL"
lines = [
    "# M23 Rustdoc Marker Ratchet Check",
    "",
    f"Generated at: {generated_at}",
    "",
    "## Summary",
    "",
    f"- Floor markers: `{floor}`",
    f"- Baseline total markers: `{baseline_total}`",
    f"- Current total markers: `{current_total}`",
    f"- Delta markers: `{delta_total:+d}`",
    f"- Below floor by: `{below_floor_by}`",
    f"- Ratchet status: `{status}`",
    "",
    "## Negative Crate Deltas",
    "",
]

if negative:
    lines.extend(["| Crate | Baseline | Current | Delta |", "| --- | ---: | ---: | ---: |"])
    for row in negative:
        lines.append(
            f"| {row['crate']} | {row['baseline_markers']} | "
            f"{row['current_markers']} | {row['delta_markers']:+d} |"
        )
else:
    lines.append("- None")

lines.extend(
    [
        "",
        "## Reproduction Command",
        "",
        "```bash",
        "scripts/dev/rustdoc-marker-ratchet-check.sh \\",
        "  --repo-root . \\",
        "  --policy-file tasks/policies/m23-doc-ratchet-policy.json \\",
        "  --current-json tasks/reports/m23-rustdoc-marker-count.json \\",
        "  --output-json tasks/reports/m23-rustdoc-marker-ratchet-check.json \\",
        "  --output-md tasks/reports/m23-rustdoc-marker-ratchet-check.md",
        "```",
        "",
    ]
)

pathlib.Path(output_md_path).write_text("\n".join(lines), encoding="utf-8")
PY

floor_markers="$(jq -r '.floor_markers' "${OUTPUT_JSON_ABS}")"
current_total_markers="$(jq -r '.current_total_markers' "${OUTPUT_JSON_ABS}")"
below_floor_by="$(jq -r '.below_floor_by' "${OUTPUT_JSON_ABS}")"
meets_floor="$(jq -r '.meets_floor' "${OUTPUT_JSON_ABS}")"

log_info "rustdoc ratchet check: floor=${floor_markers} current=${current_total_markers} meets=${meets_floor} below_floor_by=${below_floor_by}"
log_info "json_artifact: ${OUTPUT_JSON_ABS}"
log_info "md_artifact: ${OUTPUT_MD_ABS}"

if [[ "${meets_floor}" != "true" ]]; then
  fail "rustdoc marker total ${current_total_markers} is below floor ${floor_markers} by ${below_floor_by}"
fi
