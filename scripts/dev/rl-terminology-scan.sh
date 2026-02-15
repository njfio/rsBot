#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

REPO_ROOT="${DEFAULT_REPO_ROOT}"
SCAN_ROOT="${DEFAULT_REPO_ROOT}"
ALLOWLIST_FILE="tasks/policies/rl-terms-allowlist.json"
OUTPUT_JSON="${DEFAULT_REPO_ROOT}/tasks/reports/m22-rl-terminology-scan.json"
OUTPUT_MD="${DEFAULT_REPO_ROOT}/tasks/reports/m22-rl-terminology-scan.md"
GENERATED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: rl-terminology-scan.sh [options]

Scan repository text for RL terminology and classify matches as approved
(allowlisted future-RL context) or stale wording.

Options:
  --repo-root <path>       Repository root for policy/docs references.
  --scan-root <path>       Root path to scan (default: repo root).
  --allowlist-file <path>  Allowlist policy path relative to repo root (default: tasks/policies/rl-terms-allowlist.json).
  --output-json <path>     Output JSON report path.
  --output-md <path>       Output Markdown report path.
  --generated-at <iso>     Override generated timestamp (UTC ISO-8601).
  --quiet                  Suppress informational output.
  --help                   Show this help text.
EOF
}

log_info() {
  if [[ "${QUIET_MODE}" != "true" ]]; then
    echo "$@"
  fi
}

fail() {
  echo "error: $*" >&2
  exit 1
}

require_cmd() {
  local name="$1"
  if ! command -v "${name}" >/dev/null 2>&1; then
    fail "required command '${name}' not found"
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
    --scan-root)
      SCAN_ROOT="$2"
      shift 2
      ;;
    --allowlist-file)
      ALLOWLIST_FILE="$2"
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

require_cmd python3

if [[ ! -d "${REPO_ROOT}" ]]; then
  fail "repo root not found: ${REPO_ROOT}"
fi
if [[ ! -d "${SCAN_ROOT}" ]]; then
  fail "scan root not found: ${SCAN_ROOT}"
fi

ALLOWLIST_ABS="$(resolve_path "${REPO_ROOT}" "${ALLOWLIST_FILE}")"
if [[ ! -f "${ALLOWLIST_ABS}" ]]; then
  fail "allowlist file not found: ${ALLOWLIST_ABS}"
fi

mkdir -p "$(dirname "${OUTPUT_JSON}")"
mkdir -p "$(dirname "${OUTPUT_MD}")"

log_info "scanning RL terminology in ${SCAN_ROOT}"

python3 - \
  "${SCAN_ROOT}" \
  "${ALLOWLIST_ABS}" \
  "${OUTPUT_JSON}" \
  "${OUTPUT_MD}" \
  "${GENERATED_AT}" \
  "${ALLOWLIST_FILE}" <<'PY'
import json
import pathlib
import re
import sys

scan_root, allowlist_path, output_json_path, output_md_path, generated_at, allowlist_ref = sys.argv[1:]

scan_root_path = pathlib.Path(scan_root).resolve()
allowlist_abs = pathlib.Path(allowlist_path).resolve()
output_json_abs = pathlib.Path(output_json_path).resolve()
output_md_abs = pathlib.Path(output_md_path).resolve()
excluded_paths = {output_json_abs, output_md_abs}

with allowlist_abs.open(encoding="utf-8") as handle:
    policy = json.load(handle)

approved_terms = policy.get("approved_terms", [])
default_terms = policy.get("disallowed_defaults", [])
scan_terms = []
for entry in approved_terms:
    term = str(entry.get("term", "")).strip()
    if term:
        scan_terms.append(term)
for term in default_terms:
    value = str(term).strip()
    if value:
        scan_terms.append(value)

seen_terms = set()
ordered_terms = []
for term in scan_terms:
    lower = term.lower()
    if lower in seen_terms:
        continue
    seen_terms.add(lower)
    ordered_terms.append(term)

approved_matches = []
stale_findings = []
scanned_files_total = 0

def classify(term: str, rel_path: str, line_text: str, full_text: str):
    candidates = [entry for entry in approved_terms if str(entry.get("term", "")).lower() == term.lower()]
    if not candidates:
        return ("stale", "term is not allowlisted")

    for entry in candidates:
        allowed_paths = [str(path) for path in entry.get("allowed_paths", []) if str(path).strip()]
        required_context = [str(pattern) for pattern in entry.get("required_context", []) if str(pattern).strip()]

        path_ok = any(rel_path.startswith(path) for path in allowed_paths) if allowed_paths else False
        context_ok = True
        if required_context:
            context_ok = any(
                re.search(pattern, line_text, flags=re.IGNORECASE)
                or re.search(pattern, full_text, flags=re.IGNORECASE)
                for pattern in required_context
            )

        if path_ok and context_ok:
            return ("approved", str(entry.get("rationale", "allowlisted")))

    return ("stale", "allowlisted term used outside approved path/context")


for candidate in sorted(scan_root_path.rglob("*")):
    if not candidate.is_file():
        continue
    if candidate.resolve() in excluded_paths:
        continue
    if candidate.suffix.lower() not in {".md", ".txt", ".rst"}:
        continue
    scanned_files_total += 1

    text = candidate.read_text(encoding="utf-8", errors="ignore")
    rel_path = candidate.relative_to(scan_root_path).as_posix()
    lines = text.splitlines()

    for line_no, line in enumerate(lines, start=1):
        lowered = line.lower()
        for term in ordered_terms:
            if term.lower() not in lowered:
                continue
            classification, reason = classify(term, rel_path, line, text)
            finding = {
                "path": rel_path,
                "line": line_no,
                "term": term,
                "excerpt": line.strip(),
                "reason": reason,
            }
            if classification == "approved":
                approved_matches.append(finding)
            else:
                stale_findings.append(finding)

payload = {
    "schema_version": 1,
    "generated_at": generated_at,
    "policy_path": allowlist_ref,
    "scan_root": str(scan_root_path),
    "approved_matches": approved_matches,
    "stale_findings": stale_findings,
    "summary": {
        "scanned_files_total": scanned_files_total,
        "approved_count": len(approved_matches),
        "stale_count": len(stale_findings),
        "total_matches": len(approved_matches) + len(stale_findings),
    },
}

output_json = pathlib.Path(output_json_path)
output_json.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")

lines = [
    "# RL Terminology Scan Report",
    "",
    f"- Generated at: `{generated_at}`",
    f"- Policy: `{allowlist_ref}`",
    f"- Scan root: `{scan_root_path}`",
    "",
    "## Summary",
    "",
    f"- Scanned files: `{payload['summary']['scanned_files_total']}`",
    f"- Approved matches: `{payload['summary']['approved_count']}`",
    f"- Stale findings: `{payload['summary']['stale_count']}`",
    "",
    "## Approved Matches",
    "",
    "| Path | Line | Term | Reason |",
    "| --- | ---: | --- | --- |",
]
for finding in approved_matches:
    lines.append(
        f"| {finding['path']} | {finding['line']} | {finding['term']} | {finding['reason']} |"
    )
if not approved_matches:
    lines.append("| - | - | - | none |")

lines.extend(
    [
        "",
        "## Stale Findings",
        "",
        "| Path | Line | Term | Reason |",
        "| --- | ---: | --- | --- |",
    ]
)
for finding in stale_findings:
    lines.append(
        f"| {finding['path']} | {finding['line']} | {finding['term']} | {finding['reason']} |"
    )
if not stale_findings:
    lines.append("| - | - | - | none |")

output_md = pathlib.Path(output_md_path)
output_md.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY

log_info "wrote JSON report: ${OUTPUT_JSON}"
log_info "wrote Markdown report: ${OUTPUT_MD}"
