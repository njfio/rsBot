#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

REPO_SLUG=""
MILESTONE_NUMBER=21
REPORTS_DIR="${REPO_ROOT}/tasks/reports"
OUTPUT_JSON="${REPO_ROOT}/tasks/reports/m21-validation-matrix.json"
OUTPUT_MD="${REPO_ROOT}/tasks/reports/m21-validation-matrix.md"
SCHEMA_PATH="${REPO_ROOT}/tasks/schemas/m21-validation-matrix.schema.json"
GENERATED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
FIXTURE_ISSUES_JSON=""
FIXTURE_MILESTONE_JSON=""
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: m21-validation-matrix.sh [options]

Generate reproducible M21 validation matrix artifacts from milestone issue state
and local report artifacts.

Options:
  --repo <owner/name>             Repository slug (defaults to current gh repo).
  --milestone-number <n>          Milestone number (default: 21).
  --reports-dir <path>            Directory to scan for local report artifacts.
  --output-json <path>            JSON matrix output path.
  --output-md <path>              Markdown matrix output path.
  --schema-path <path>            Matrix schema file path reference.
  --generated-at <iso>            Override generated timestamp.
  --fixture-issues-json <path>    Fixture issues JSON (skips live GitHub API).
  --fixture-milestone-json <path> Fixture milestone JSON (skips live GitHub API).
  --quiet                         Suppress informational output.
  --help                          Show this help text.
EOF
}

log_info() {
  if [[ "${QUIET_MODE}" != "true" ]]; then
    echo "$@"
  fi
}

require_cmd() {
  local name="$1"
  if ! command -v "${name}" >/dev/null 2>&1; then
    echo "error: required command '${name}' not found" >&2
    exit 1
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo)
      REPO_SLUG="$2"
      shift 2
      ;;
    --milestone-number)
      MILESTONE_NUMBER="$2"
      shift 2
      ;;
    --reports-dir)
      REPORTS_DIR="$2"
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
    --schema-path)
      SCHEMA_PATH="$2"
      shift 2
      ;;
    --generated-at)
      GENERATED_AT="$2"
      shift 2
      ;;
    --fixture-issues-json)
      FIXTURE_ISSUES_JSON="$2"
      shift 2
      ;;
    --fixture-milestone-json)
      FIXTURE_MILESTONE_JSON="$2"
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
      echo "error: unknown option '$1'" >&2
      usage >&2
      exit 1
      ;;
  esac
done

require_cmd jq
require_cmd python3

if ! [[ "${MILESTONE_NUMBER}" =~ ^[0-9]+$ ]]; then
  echo "error: --milestone-number must be a non-negative integer" >&2
  exit 1
fi

using_fixtures="false"
if [[ -n "${FIXTURE_ISSUES_JSON}" || -n "${FIXTURE_MILESTONE_JSON}" ]]; then
  using_fixtures="true"
fi

if [[ "${using_fixtures}" == "true" ]]; then
  if [[ -z "${FIXTURE_ISSUES_JSON}" || -z "${FIXTURE_MILESTONE_JSON}" ]]; then
    echo "error: --fixture-issues-json and --fixture-milestone-json must be provided together" >&2
    exit 1
  fi
  if [[ ! -f "${FIXTURE_ISSUES_JSON}" ]]; then
    echo "error: fixture issues JSON not found: ${FIXTURE_ISSUES_JSON}" >&2
    exit 1
  fi
  if [[ ! -f "${FIXTURE_MILESTONE_JSON}" ]]; then
    echo "error: fixture milestone JSON not found: ${FIXTURE_MILESTONE_JSON}" >&2
    exit 1
  fi
else
  require_cmd gh
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT
issues_path="${tmp_dir}/issues.json"
milestone_path="${tmp_dir}/milestone.json"

if [[ "${using_fixtures}" == "true" ]]; then
  cp "${FIXTURE_ISSUES_JSON}" "${issues_path}"
  cp "${FIXTURE_MILESTONE_JSON}" "${milestone_path}"
  if [[ -z "${REPO_SLUG}" ]]; then
    REPO_SLUG="fixture/repository"
  fi
else
  if [[ -z "${REPO_SLUG}" ]]; then
    REPO_SLUG="$(gh repo view --json nameWithOwner --jq '.nameWithOwner')"
  fi
  gh api "repos/${REPO_SLUG}/milestones/${MILESTONE_NUMBER}" >"${milestone_path}"
  gh api \
    --paginate \
    --slurp \
    "repos/${REPO_SLUG}/issues?state=all&milestone=${MILESTONE_NUMBER}&per_page=100" \
    | jq '[.[][] | select(.pull_request | not)]' >"${issues_path}"
fi

mkdir -p "$(dirname "${OUTPUT_JSON}")" "$(dirname "${OUTPUT_MD}")"

python3 - \
  "${issues_path}" \
  "${milestone_path}" \
  "${REPORTS_DIR}" \
  "${OUTPUT_JSON}" \
  "${OUTPUT_MD}" \
  "${REPO_SLUG}" \
  "${MILESTONE_NUMBER}" \
  "${SCHEMA_PATH}" \
  "${GENERATED_AT}" <<'PY'
import json
import pathlib
import sys
from datetime import datetime, timezone

(
    issues_path,
    milestone_path,
    reports_dir,
    output_json,
    output_md,
    repo_slug,
    milestone_number,
    schema_path,
    generated_at,
) = sys.argv[1:]


def load_json(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def issue_type(labels):
    names = set(labels)
    if "epic" in names:
        return "epic"
    if "story" in names:
        return "story"
    if "task" in names:
        return "task"
    return "issue"


def completion_percent(closed, total):
    if total == 0:
        return 0.0
    return round((closed / total) * 100.0, 2)


issues_raw = load_json(issues_path)
if isinstance(issues_raw, dict):
    issues_raw = issues_raw.get("issues", [])
if not isinstance(issues_raw, list):
    raise SystemExit("issues fixture/source must decode to a JSON array")

milestone_raw = load_json(milestone_path)
if not isinstance(milestone_raw, dict):
    raise SystemExit("milestone fixture/source must decode to a JSON object")

issues = []
for issue in issues_raw:
    if not isinstance(issue, dict):
        continue
    if "pull_request" in issue:
        continue
    labels = sorted(
        [label.get("name", "") for label in issue.get("labels", []) if isinstance(label, dict)]
    )
    state = issue.get("state", "unknown")
    entry = {
        "number": issue.get("number"),
        "title": issue.get("title", ""),
        "state": state,
        "url": issue.get("html_url", ""),
        "labels": labels,
        "type": issue_type(labels),
        "comments": int(issue.get("comments", 0) or 0),
        "has_comments": int(issue.get("comments", 0) or 0) > 0,
        "testing_matrix_required": "testing-matrix" in labels,
        "parent_issue_url": issue.get("parent_issue_url"),
    }
    if entry["number"] is None:
        continue
    issues.append(entry)

issues.sort(key=lambda entry: int(entry["number"]))

total = len(issues)
closed = sum(1 for issue in issues if issue["state"] == "closed")
open_count = sum(1 for issue in issues if issue["state"] == "open")
epics = sum(1 for issue in issues if issue["type"] == "epic")
stories = sum(1 for issue in issues if issue["type"] == "story")
tasks = sum(1 for issue in issues if issue["type"] == "task")
with_comments = sum(1 for issue in issues if issue["has_comments"])
testing_required = sum(1 for issue in issues if issue["testing_matrix_required"])
testing_closed = sum(
    1
    for issue in issues
    if issue["testing_matrix_required"] and issue["state"] == "closed"
)

reports_path = pathlib.Path(reports_dir)
local_artifacts = []
if reports_path.exists() and reports_path.is_dir():
    for artifact_path in sorted(reports_path.iterdir(), key=lambda item: item.name):
        if not artifact_path.is_file():
            continue
        if artifact_path.suffix not in {".json", ".md"}:
            continue
        stat = artifact_path.stat()
        local_artifacts.append(
            {
                "path": str(artifact_path),
                "name": artifact_path.name,
                "bytes": stat.st_size,
                "modified_at": datetime.fromtimestamp(
                    stat.st_mtime, tz=timezone.utc
                ).strftime("%Y-%m-%dT%H:%M:%SZ"),
                "status": "present",
            }
        )

matrix = {
    "schema_version": 1,
    "generated_at": generated_at,
    "repository": repo_slug,
    "schema_path": schema_path,
    "milestone": {
        "number": int(milestone_number),
        "title": milestone_raw.get("title", ""),
        "state": milestone_raw.get("state", ""),
        "open_issues": int(milestone_raw.get("open_issues", 0) or 0),
        "closed_issues": int(milestone_raw.get("closed_issues", 0) or 0),
        "due_on": milestone_raw.get("due_on"),
    },
    "summary": {
        "total_issues": total,
        "open_issues": open_count,
        "closed_issues": closed,
        "completion_percent": completion_percent(closed, total),
        "epics": epics,
        "stories": stories,
        "tasks": tasks,
        "with_comments": with_comments,
        "testing_matrix_required": testing_required,
        "testing_matrix_closed": testing_closed,
        "local_artifacts_total": len(local_artifacts),
    },
    "local_artifacts": local_artifacts,
    "issues": issues,
}

with open(output_json, "w", encoding="utf-8") as handle:
    json.dump(matrix, handle, indent=2)
    handle.write("\n")

lines = []
lines.append("# M21 Validation Matrix")
lines.append("")
lines.append(f"- Generated: {matrix['generated_at']}")
lines.append(f"- Repository: {matrix['repository']}")
lines.append(
    f"- Milestone: #{matrix['milestone']['number']} {matrix['milestone']['title']}".rstrip()
)
lines.append(
    f"- Progress: {matrix['summary']['closed_issues']}/{matrix['summary']['total_issues']} "
    f"closed ({matrix['summary']['completion_percent']:.2f}%)"
)
lines.append("")
lines.append("## Summary")
lines.append("")
lines.append("| Metric | Value |")
lines.append("| --- | ---: |")
for metric, value in (
    ("Open issues", matrix["summary"]["open_issues"]),
    ("Closed issues", matrix["summary"]["closed_issues"]),
    ("Epics", matrix["summary"]["epics"]),
    ("Stories", matrix["summary"]["stories"]),
    ("Tasks", matrix["summary"]["tasks"]),
    ("Issues with comments", matrix["summary"]["with_comments"]),
    ("Testing-matrix required", matrix["summary"]["testing_matrix_required"]),
    ("Testing-matrix closed", matrix["summary"]["testing_matrix_closed"]),
    ("Local artifacts", matrix["summary"]["local_artifacts_total"]),
):
    lines.append(f"| {metric} | {value} |")
lines.append("")

lines.append("## Issue Matrix")
lines.append("")
lines.append("| Issue | State | Type | Labels | Comments | Testing Matrix |")
lines.append("| --- | --- | --- | --- | ---: | --- |")
for issue in issues:
    labels = ", ".join(issue["labels"]) if issue["labels"] else "-"
    testing_required_cell = "required" if issue["testing_matrix_required"] else "-"
    lines.append(
        f"| #{issue['number']} | {issue['state']} | {issue['type']} | "
        f"{labels} | {issue['comments']} | {testing_required_cell} |"
    )
lines.append("")

lines.append("## Local Artifacts")
lines.append("")
lines.append("| Artifact | Bytes | Modified (UTC) | Status |")
lines.append("| --- | ---: | --- | --- |")
for artifact in local_artifacts:
    lines.append(
        f"| `{artifact['path']}` | {artifact['bytes']} | "
        f"{artifact['modified_at']} | {artifact['status']} |"
    )
if not local_artifacts:
    lines.append("| (none) | 0 | n/a | n/a |")
lines.append("")

with open(output_md, "w", encoding="utf-8") as handle:
    handle.write("\n".join(lines))
    handle.write("\n")
PY

log_info "wrote validation matrix artifacts:"
log_info "- ${OUTPUT_JSON}"
log_info "- ${OUTPUT_MD}"
