#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

REPO_ROOT="${DEFAULT_REPO_ROOT}"
SCAN_ROOT="crates"
POLICY_FILE="tasks/policies/doc-quality-anti-patterns.json"
OUTPUT_JSON="${DEFAULT_REPO_ROOT}/tasks/reports/m23-doc-quality-audit-helper.json"
OUTPUT_MD="${DEFAULT_REPO_ROOT}/tasks/reports/m23-doc-quality-audit-helper.md"
GENERATED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: doc-quality-audit-helper.sh [options]

Scan rustdoc comments for low-value anti-pattern heuristics and emit findings.

Options:
  --repo-root <path>      Repository root (default: auto-detected).
  --scan-root <path>      Scan root relative to repo root (default: crates).
  --policy-file <path>    Policy file path relative to repo root (default: tasks/policies/doc-quality-anti-patterns.json).
  --output-json <path>    Output JSON artifact path (default: tasks/reports/m23-doc-quality-audit-helper.json).
  --output-md <path>      Output Markdown artifact path (default: tasks/reports/m23-doc-quality-audit-helper.md).
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
    --scan-root)
      SCAN_ROOT="$2"
      shift 2
      ;;
    --policy-file)
      POLICY_FILE="$2"
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

SCAN_ROOT_ABS="$(resolve_path "${REPO_ROOT}" "${SCAN_ROOT}")"
if [[ ! -d "${SCAN_ROOT_ABS}" ]]; then
  fail "scan root not found: ${SCAN_ROOT_ABS}"
fi

POLICY_FILE_ABS="$(resolve_path "${REPO_ROOT}" "${POLICY_FILE}")"
if [[ ! -f "${POLICY_FILE_ABS}" ]]; then
  fail "policy file not found: ${POLICY_FILE_ABS}"
fi

OUTPUT_JSON_ABS="$(resolve_path "${REPO_ROOT}" "${OUTPUT_JSON}")"
OUTPUT_MD_ABS="$(resolve_path "${REPO_ROOT}" "${OUTPUT_MD}")"
mkdir -p "$(dirname "${OUTPUT_JSON_ABS}")"
mkdir -p "$(dirname "${OUTPUT_MD_ABS}")"

python3 - \
  "${REPO_ROOT}" \
  "${SCAN_ROOT_ABS}" \
  "${SCAN_ROOT}" \
  "${POLICY_FILE_ABS}" \
  "${GENERATED_AT}" \
  "${OUTPUT_JSON_ABS}" \
  "${OUTPUT_MD_ABS}" <<'PY'
import json
import pathlib
import re
import sys

(
    repo_root_raw,
    scan_root_abs_raw,
    scan_root_rel,
    policy_path_raw,
    generated_at,
    output_json_raw,
    output_md_raw,
) = sys.argv[1:]

repo_root = pathlib.Path(repo_root_raw)
scan_root_abs = pathlib.Path(scan_root_abs_raw)
policy_path = pathlib.Path(policy_path_raw)
output_json = pathlib.Path(output_json_raw)
output_md = pathlib.Path(output_md_raw)

policy = json.loads(policy_path.read_text(encoding="utf-8"))
patterns = policy.get("patterns", [])
suppressions = policy.get("suppressions", [])

compiled_patterns = []
for entry in patterns:
    pattern_id = entry.get("id")
    if not isinstance(pattern_id, str) or not pattern_id.strip():
        raise SystemExit("error: pattern entry missing non-empty id")
    description = str(entry.get("description", "")).strip()
    contains = entry.get("contains")
    regex = entry.get("regex")
    if contains is None and regex is None:
        raise SystemExit(f"error: pattern '{pattern_id}' requires contains or regex")
    compiled_regex = re.compile(regex) if isinstance(regex, str) else None
    compiled_patterns.append(
        {
            "id": pattern_id,
            "description": description,
            "contains": contains if isinstance(contains, str) else None,
            "regex": compiled_regex,
        }
    )

doc_line_pattern = re.compile(r"^\s*(///|//!)(.*)$")
findings = []
suppressed = []
scanned_files = 0
scanned_doc_lines = 0

def suppression_matches(path_rel: str, pattern_id: str, comment_text: str) -> dict | None:
    for rule in suppressions:
        if rule.get("pattern_id") not in (None, pattern_id):
            continue
        path_contains = rule.get("path_contains")
        if isinstance(path_contains, str) and path_contains not in path_rel:
            continue
        line_contains = rule.get("line_contains")
        if isinstance(line_contains, str) and line_contains not in comment_text:
            continue
        return rule
    return None

for rust_file in sorted(scan_root_abs.rglob("*.rs")):
    if "/src/" not in str(rust_file).replace("\\", "/"):
        continue
    scanned_files += 1
    rel_path = rust_file.relative_to(repo_root).as_posix()
    with rust_file.open(encoding="utf-8") as handle:
        for line_no, raw_line in enumerate(handle, start=1):
            match = doc_line_pattern.match(raw_line)
            if not match:
                continue
            scanned_doc_lines += 1
            comment_text = match.group(2).strip()
            for pattern in compiled_patterns:
                matched = False
                if pattern["contains"] is not None and pattern["contains"] in comment_text:
                    matched = True
                if pattern["regex"] is not None and pattern["regex"].search(comment_text):
                    matched = True
                if not matched:
                    continue
                suppression = suppression_matches(rel_path, pattern["id"], comment_text)
                if suppression is not None:
                    suppressed.append(
                        {
                            "suppression_id": suppression.get("id", "suppression"),
                            "pattern_id": pattern["id"],
                            "path": rel_path,
                            "line": line_no,
                        }
                    )
                    continue
                findings.append(
                    {
                        "pattern_id": pattern["id"],
                        "pattern_description": pattern["description"],
                        "path": rel_path,
                        "line": line_no,
                        "comment": comment_text,
                    }
                )

findings.sort(key=lambda item: (item["path"], item["line"], item["pattern_id"]))
suppressed.sort(key=lambda item: (item["path"], item["line"], item["pattern_id"]))

payload = {
    "schema_version": 1,
    "generated_at": generated_at,
    "repo_root": str(repo_root),
    "scan_root": scan_root_rel,
    "policy_file": (
        str(policy_path.relative_to(repo_root))
        if policy_path.is_relative_to(repo_root)
        else str(policy_path)
    ),
    "summary": {
        "scanned_files": scanned_files,
        "scanned_doc_lines": scanned_doc_lines,
        "patterns_configured": len(compiled_patterns),
        "findings_count": len(findings),
        "suppressed_count": len(suppressed),
    },
    "findings": findings,
    "suppressed": suppressed,
}

output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

md_lines = [
    "# M23 Doc Quality Audit Helper Report",
    "",
    f"Generated at: {generated_at}",
    "",
    "## Summary",
    "",
    f"- Scan root: `{scan_root_rel}`",
    f"- Policy file: `{payload['policy_file']}`",
    f"- Scanned files: `{scanned_files}`",
    f"- Scanned rustdoc lines: `{scanned_doc_lines}`",
    f"- Findings: `{len(findings)}`",
    f"- Suppressed: `{len(suppressed)}`",
    "",
    "## Findings",
    "",
    "| Pattern | Path | Line | Comment |",
    "| --- | --- | ---: | --- |",
]
for finding in findings:
    comment = finding["comment"].replace("|", "\\|")
    md_lines.append(
        f"| {finding['pattern_id']} | {finding['path']} | {finding['line']} | {comment} |"
    )

md_lines.append("")
md_lines.append("## Suppressed")
md_lines.append("")
md_lines.append("| Suppression ID | Pattern | Path | Line |")
md_lines.append("| --- | --- | --- | ---: |")
for row in suppressed:
    md_lines.append(
        f"| {row['suppression_id']} | {row['pattern_id']} | {row['path']} | {row['line']} |"
    )
md_lines.append("")

output_md.write_text("\n".join(md_lines), encoding="utf-8")
PY

findings_count="$(jq -r '.summary.findings_count' "${OUTPUT_JSON_ABS}")"
suppressed_count="$(jq -r '.summary.suppressed_count' "${OUTPUT_JSON_ABS}")"
scanned_doc_lines="$(jq -r '.summary.scanned_doc_lines' "${OUTPUT_JSON_ABS}")"

log_info "doc quality audit helper: findings=${findings_count} suppressed=${suppressed_count} scanned_doc_lines=${scanned_doc_lines}"
log_info "json_artifact: ${OUTPUT_JSON_ABS}"
log_info "md_artifact: ${OUTPUT_MD_ABS}"
