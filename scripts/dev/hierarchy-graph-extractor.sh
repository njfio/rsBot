#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

ROOT_ISSUE=1678
REPO_SLUG=""
LABEL_FILTER="roadmap"
ISSUE_STATE="all"
MILESTONE=""
FIXTURE_ISSUES_JSON=""
OUTPUT_JSON="${REPO_ROOT}/tasks/reports/issue-hierarchy-graph.json"
OUTPUT_MD="${REPO_ROOT}/tasks/reports/issue-hierarchy-graph.md"
MAX_RETRIES=4
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: hierarchy-graph-extractor.sh [options]

Extract issue hierarchy graph artifacts (JSON + Markdown tree) for roadmap execution.

Options:
  --root-issue <number>         Root issue number (default: 1678)
  --repo <owner/name>           Repository slug for live GitHub API calls
  --label <name>                Label filter for live issue fetch (default: roadmap)
  --state <open|closed|all>     Issue state filter for live fetch (default: all)
  --milestone <number>          Optional milestone number filter for live fetch
  --fixture-issues-json <path>  Fixture issue JSON input (array or object.issues)
  --output-json <path>          Output JSON artifact path
  --output-md <path>            Output Markdown artifact path
  --max-retries <n>             Live API retry attempts (default: 4)
  --quiet                       Suppress informational output
  --help                        Show this help
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

gh_api_with_retry() {
  local endpoint="$1"
  local output_path="$2"
  local attempt=1
  local delay_seconds=1

  while ((attempt <= MAX_RETRIES)); do
    if gh api "${endpoint}" >"${output_path}"; then
      return 0
    fi
    if ((attempt == MAX_RETRIES)); then
      break
    fi
    sleep "${delay_seconds}"
    delay_seconds=$((delay_seconds * 2))
    attempt=$((attempt + 1))
  done

  echo "error: failed GitHub API request after ${MAX_RETRIES} attempts: ${endpoint}" >&2
  return 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root-issue)
      ROOT_ISSUE="$2"
      shift 2
      ;;
    --repo)
      REPO_SLUG="$2"
      shift 2
      ;;
    --label)
      LABEL_FILTER="$2"
      shift 2
      ;;
    --state)
      ISSUE_STATE="$2"
      shift 2
      ;;
    --milestone)
      MILESTONE="$2"
      shift 2
      ;;
    --fixture-issues-json)
      FIXTURE_ISSUES_JSON="$2"
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
    --max-retries)
      MAX_RETRIES="$2"
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

if ! [[ "${ROOT_ISSUE}" =~ ^[0-9]+$ ]]; then
  echo "error: --root-issue must be numeric" >&2
  exit 1
fi

if ! [[ "${MAX_RETRIES}" =~ ^[0-9]+$ ]] || [[ "${MAX_RETRIES}" -lt 1 ]]; then
  echo "error: --max-retries must be an integer >= 1" >&2
  exit 1
fi

if [[ "${ISSUE_STATE}" != "open" && "${ISSUE_STATE}" != "closed" && "${ISSUE_STATE}" != "all" ]]; then
  echo "error: --state must be one of: open, closed, all" >&2
  exit 1
fi

if [[ -n "${FIXTURE_ISSUES_JSON}" && ! -f "${FIXTURE_ISSUES_JSON}" ]]; then
  echo "error: fixture issues JSON not found: ${FIXTURE_ISSUES_JSON}" >&2
  exit 1
fi

require_cmd python3
require_cmd jq

if [[ -z "${FIXTURE_ISSUES_JSON}" ]]; then
  require_cmd gh
fi

mkdir -p "$(dirname "${OUTPUT_JSON}")"
mkdir -p "$(dirname "${OUTPUT_MD}")"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

issues_input="${tmp_dir}/issues-input.json"

if [[ -n "${FIXTURE_ISSUES_JSON}" ]]; then
  cp "${FIXTURE_ISSUES_JSON}" "${issues_input}"
  SOURCE_MODE="fixture"
  if [[ -z "${REPO_SLUG}" ]]; then
    REPO_SLUG="fixture/repository"
  fi
else
  SOURCE_MODE="live"
  if [[ -z "${REPO_SLUG}" ]]; then
    REPO_SLUG="$(gh repo view --json nameWithOwner --jq '.nameWithOwner')"
  fi

  echo '[]' >"${issues_input}"
  page=1
  while true; do
    endpoint="repos/${REPO_SLUG}/issues?state=${ISSUE_STATE}&labels=${LABEL_FILTER}&per_page=100&page=${page}"
    if [[ -n "${MILESTONE}" ]]; then
      endpoint="${endpoint}&milestone=${MILESTONE}"
    fi
    page_payload="${tmp_dir}/page-${page}.json"
    gh_api_with_retry "${endpoint}" "${page_payload}"

    page_count="$(jq 'length' "${page_payload}")"
    if [[ "${page_count}" -eq 0 ]]; then
      break
    fi

    jq -s '.[0] + .[1]' "${issues_input}" "${page_payload}" >"${issues_input}.next"
    mv "${issues_input}.next" "${issues_input}"

    if [[ "${page_count}" -lt 100 ]]; then
      break
    fi
    page=$((page + 1))
  done

  root_payload="${tmp_dir}/root-issue.json"
  gh_api_with_retry "repos/${REPO_SLUG}/issues/${ROOT_ISSUE}" "${root_payload}"
  jq -s '.[0] + [.[1]]' "${issues_input}" "${root_payload}" >"${issues_input}.next"
  mv "${issues_input}.next" "${issues_input}"
fi

python3 - \
  "${issues_input}" \
  "${OUTPUT_JSON}" \
  "${OUTPUT_MD}" \
  "${ROOT_ISSUE}" \
  "${REPO_SLUG}" \
  "${SOURCE_MODE}" \
  "${QUIET_MODE}" <<'PY'
from __future__ import annotations

import json
import re
import sys
from collections import deque
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

(
    issues_input_path,
    output_json_path,
    output_md_path,
    root_issue_raw,
    repository,
    source_mode,
    quiet_mode,
) = sys.argv[1:]

root_issue = int(root_issue_raw)


def log(message: str) -> None:
    if quiet_mode == "true":
        return
    print(message)


def load_issues(path: str) -> list[dict[str, Any]]:
    payload = json.loads(Path(path).read_text(encoding="utf-8"))
    if isinstance(payload, list):
        return [entry for entry in payload if isinstance(entry, dict)]
    if isinstance(payload, dict):
        issues = payload.get("issues")
        if isinstance(issues, list):
            return [entry for entry in issues if isinstance(entry, dict)]
    raise SystemExit("error: issues input must decode to a JSON array")


def normalize_labels(value: Any) -> list[str]:
    labels: list[str] = []
    if not isinstance(value, list):
        return labels
    for entry in value:
        if isinstance(entry, str):
            candidate = entry.strip()
            if candidate:
                labels.append(candidate)
        elif isinstance(entry, dict):
            name = entry.get("name")
            if isinstance(name, str) and name.strip():
                labels.append(name.strip())
    return labels


def parse_issue_number_from_url(url: str | None) -> int | None:
    if not isinstance(url, str):
        return None
    match = re.search(r"/issues/([0-9]+)$", url.strip())
    if not match:
        return None
    return int(match.group(1))


def issue_type(labels: list[str]) -> str:
    for candidate in ("epic", "story", "task"):
        if candidate in labels:
            return candidate
    return "unknown"


def normalize_issue(raw: dict[str, Any]) -> dict[str, Any] | None:
    if "pull_request" in raw:
        return None
    number = raw.get("number")
    if not isinstance(number, int):
        return None

    labels = normalize_labels(raw.get("labels", []))
    parent_issue_url = raw.get("parent_issue_url")
    if not isinstance(parent_issue_url, str):
        parent_issue_url = None

    html_url = raw.get("html_url")
    if not isinstance(html_url, str) or not html_url:
        html_url = f"https://github.com/{repository}/issues/{number}"

    api_url = raw.get("url")
    if not isinstance(api_url, str) or not api_url:
        api_url = f"https://api.github.com/repos/{repository}/issues/{number}"

    return {
        "number": number,
        "title": str(raw.get("title", "")).strip(),
        "state": str(raw.get("state", "unknown")).strip().lower(),
        "labels": sorted(set(labels)),
        "type": issue_type(labels),
        "html_url": html_url,
        "url": api_url,
        "parent_issue_url": parent_issue_url,
        "parent_number": parse_issue_number_from_url(parent_issue_url),
    }


raw_issues = load_issues(issues_input_path)

nodes_by_number: dict[int, dict[str, Any]] = {}
for raw in raw_issues:
    normalized = normalize_issue(raw)
    if normalized is None:
        continue
    nodes_by_number[normalized["number"]] = normalized

if root_issue not in nodes_by_number:
    raise SystemExit(f"error: root issue #{root_issue} not found in input")


def classify_node(number: int) -> str:
    if number == root_issue:
        return "connected"
    visited: set[int] = set()
    current = number
    while True:
        node = nodes_by_number.get(current)
        if node is None:
            return "parent_missing_from_dataset"
        parent_number = node.get("parent_number")
        if not isinstance(parent_number, int):
            if current == number:
                return "missing_parent_link"
            return "chain_terminated_before_root"
        if parent_number == root_issue:
            return "connected"
        if parent_number in visited:
            return "cycle_detected"
        visited.add(parent_number)
        if parent_number not in nodes_by_number:
            return "parent_missing_from_dataset"
        current = parent_number


in_scope_numbers: set[int] = set()
orphan_nodes: list[dict[str, Any]] = []
missing_links: list[dict[str, Any]] = []

for number in sorted(nodes_by_number):
    classification = classify_node(number)
    node = nodes_by_number[number]
    if classification == "connected":
        in_scope_numbers.add(number)
        continue
    orphan_entry = {
        "number": number,
        "title": node["title"],
        "reason": classification,
        "parent_issue_url": node["parent_issue_url"],
        "parent_number": node["parent_number"],
        "url": node["html_url"],
    }
    orphan_nodes.append(orphan_entry)
    if classification in {"missing_parent_link", "parent_missing_from_dataset"}:
        missing_links.append(orphan_entry)

in_scope_nodes = [nodes_by_number[number] for number in sorted(in_scope_numbers)]
in_scope_set = set(in_scope_numbers)

edges: list[dict[str, Any]] = []
children_by_parent: dict[int, list[int]] = {}
for node in in_scope_nodes:
    parent_number = node.get("parent_number")
    number = node["number"]
    if isinstance(parent_number, int) and parent_number in in_scope_set:
        edges.append({"from": parent_number, "to": number, "kind": "parent_child"})
        children_by_parent.setdefault(parent_number, []).append(number)
    children_by_parent.setdefault(number, [])

for parent in list(children_by_parent):
    children_by_parent[parent] = sorted(children_by_parent[parent])

depth_by_number: dict[int, int] = {root_issue: 0}
queue: deque[int] = deque([root_issue])
while queue:
    current = queue.popleft()
    current_depth = depth_by_number[current]
    for child in children_by_parent.get(current, []):
        if child in depth_by_number:
            continue
        depth_by_number[child] = current_depth + 1
        queue.append(child)

for node in in_scope_nodes:
    node["depth"] = depth_by_number.get(node["number"], 0)

generated_at = datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")

json_payload = {
    "schema_version": 1,
    "generated_at": generated_at,
    "repository": repository,
    "source_mode": source_mode,
    "root_issue_number": root_issue,
    "nodes": in_scope_nodes,
    "edges": edges,
    "missing_links": missing_links,
    "orphan_nodes": orphan_nodes,
    "summary": {
        "in_scope_nodes": len(in_scope_nodes),
        "in_scope_edges": len(edges),
        "missing_links": len(missing_links),
        "orphan_nodes": len(orphan_nodes),
    },
}

Path(output_json_path).write_text(json.dumps(json_payload, indent=2) + "\n", encoding="utf-8")


def render_tree_lines(number: int, depth: int, seen: set[int]) -> list[str]:
    node = nodes_by_number[number]
    indent = "  " * depth
    lines = [f"{indent}- #{number} [{node['state']}] {node['title']} ({node['type']})"]
    if number in seen:
        lines.append(f"{indent}  - (cycle-detected)")
        return lines
    seen.add(number)
    for child in children_by_parent.get(number, []):
        lines.extend(render_tree_lines(child, depth + 1, seen.copy()))
    return lines


markdown_lines: list[str] = [
    "# Issue Hierarchy Graph",
    "",
    f"- Repository: `{repository}`",
    f"- Root issue: `#{root_issue}`",
    f"- Generated at (UTC): `{generated_at}`",
    f"- In-scope nodes: `{len(in_scope_nodes)}`",
    f"- In-scope edges: `{len(edges)}`",
    f"- Missing links: `{len(missing_links)}`",
    f"- Orphan nodes: `{len(orphan_nodes)}`",
    "",
    "## Tree",
    "",
]

if root_issue in in_scope_set:
    markdown_lines.extend(render_tree_lines(root_issue, 0, set()))
else:
    markdown_lines.append(f"- Root issue `#{root_issue}` is not connected in extracted graph.")

markdown_lines.extend(["", "## Missing Links", ""])
if missing_links:
    for entry in missing_links:
        markdown_lines.append(
            f"- #{entry['number']} {entry['title']} | reason={entry['reason']} | parent={entry['parent_issue_url'] or 'none'}"
        )
else:
    markdown_lines.append("- none")

markdown_lines.extend(["", "## Orphan Nodes", ""])
if orphan_nodes:
    for entry in orphan_nodes:
        markdown_lines.append(
            f"- #{entry['number']} {entry['title']} | reason={entry['reason']} | parent={entry['parent_issue_url'] or 'none'}"
        )
else:
    markdown_lines.append("- none")

Path(output_md_path).write_text("\n".join(markdown_lines) + "\n", encoding="utf-8")

log(
    "[hierarchy-graph-extractor] "
    f"in_scope_nodes={len(in_scope_nodes)} "
    f"in_scope_edges={len(edges)} "
    f"missing_links={len(missing_links)} "
    f"orphan_nodes={len(orphan_nodes)} "
    f"source_mode={source_mode}"
)
PY
