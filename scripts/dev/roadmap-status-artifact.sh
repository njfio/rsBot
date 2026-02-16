#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

CONFIG_PATH="${REPO_ROOT}/tasks/roadmap-status-config.json"
FIXTURE_JSON=""
REPO_SLUG=""
OUTPUT_JSON="${REPO_ROOT}/tasks/reports/roadmap-status-artifact.json"
OUTPUT_MD="${REPO_ROOT}/tasks/reports/roadmap-status-artifact.md"
GENERATED_AT=""
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: roadmap-status-artifact.sh [options]

Generate deterministic roadmap status artifacts (JSON + Markdown) from tracked
issue state and roadmap config.

Options:
  --config-path <path>    Roadmap status config path (default: tasks/roadmap-status-config.json)
  --repo <owner/name>     Repository slug for live gh issue-state queries
  --fixture-json <path>   Fixture issue-state JSON (deterministic/testing mode)
  --output-json <path>    Output JSON artifact path
  --output-md <path>      Output Markdown artifact path
  --generated-at <iso>    Deterministic generated timestamp (ISO-8601 UTC)
  --quiet                 Suppress informational output
  --help                  Show this help text

Fixture JSON format:
{
  "default_state": "OPEN",
  "issues": [
    { "number": 1425, "state": "CLOSED" }
  ]
}
EOF
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
    --config-path)
      CONFIG_PATH="$2"
      shift 2
      ;;
    --repo)
      REPO_SLUG="$2"
      shift 2
      ;;
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
require_cmd jq

if [[ ! -f "${CONFIG_PATH}" ]]; then
  echo "error: roadmap config not found: ${CONFIG_PATH}" >&2
  exit 1
fi

if [[ -n "${FIXTURE_JSON}" && ! -f "${FIXTURE_JSON}" ]]; then
  echo "error: fixture JSON not found: ${FIXTURE_JSON}" >&2
  exit 1
fi

if [[ -z "${FIXTURE_JSON}" ]]; then
  require_cmd gh
fi

if ! jq -e '
  (.todo_groups | type == "array" and length > 0) and
  all(.todo_groups[]; (.label | type == "string" and length > 0) and (.ids | type == "array" and length > 0 and all(.[]; type == "number"))) and
  (.epic_ids | type == "array" and length > 0 and all(.[]; type == "number")) and
  (.gap | type == "object") and
  (.gap.child_story_task_ids | type == "array" and length > 0 and all(.[]; type == "number")) and
  (.gap.core_delivery_pr_span.from | type == "number") and
  (.gap.core_delivery_pr_span.to | type == "number") and
  (.gap.epic_summary | type == "string" and length > 0)
' "${CONFIG_PATH}" >/dev/null; then
  echo "error: malformed roadmap config: ${CONFIG_PATH}" >&2
  exit 1
fi

mkdir -p "$(dirname "${OUTPUT_JSON}")"
mkdir -p "$(dirname "${OUTPUT_MD}")"

python3 - \
  "${CONFIG_PATH}" \
  "${FIXTURE_JSON}" \
  "${REPO_SLUG}" \
  "${OUTPUT_JSON}" \
  "${OUTPUT_MD}" \
  "${GENERATED_AT}" \
  "${QUIET_MODE}" <<'PY'
from __future__ import annotations

import json
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

(
    config_path_raw,
    fixture_path_raw,
    repo_slug_raw,
    output_json_raw,
    output_md_raw,
    generated_at_raw,
    quiet_mode_raw,
) = sys.argv[1:]

config_path = Path(config_path_raw)
fixture_path = Path(fixture_path_raw) if fixture_path_raw else None
output_json_path = Path(output_json_raw)
output_md_path = Path(output_md_raw)
quiet_mode = quiet_mode_raw == "true"


def log(message: str) -> None:
    if not quiet_mode:
        print(message)


def fail(message: str) -> None:
    raise SystemExit(f"error: {message}")


def run_capture(command: list[str]) -> str:
    completed = subprocess.run(
        command,
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        fail(
            "command failed: "
            + " ".join(command)
            + f" | stderr={completed.stderr.strip() or 'n/a'}"
        )
    return completed.stdout


def parse_iso8601_utc(value: str) -> datetime:
    candidate = value.strip()
    if not candidate:
        fail("generated-at value must not be empty")
    if candidate.endswith("Z"):
        candidate = candidate[:-1] + "+00:00"
    try:
        parsed = datetime.fromisoformat(candidate)
    except ValueError as exc:  # pragma: no cover - exercised via regression shell tests
        fail(f"invalid --generated-at timestamp: {value} ({exc})")
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc).replace(microsecond=0)


def iso_utc(dt: datetime) -> str:
    return dt.astimezone(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def normalize_state(state: Any) -> str:
    if not isinstance(state, str):
        return "UNKNOWN"
    candidate = state.strip().upper()
    if candidate in {"OPEN", "CLOSED"}:
        return candidate
    return "UNKNOWN"


def load_config() -> dict[str, Any]:
    try:
        payload = json.loads(config_path.read_text(encoding="utf-8"))
    except Exception as exc:  # pragma: no cover - shell-gated before this stage
        fail(f"unable to parse config JSON: {exc}")
    if not isinstance(payload, dict):
        fail("roadmap config must decode to an object")
    return payload


def tracked_issue_ids(config: dict[str, Any]) -> list[int]:
    ids: set[int] = set()
    for group in config.get("todo_groups", []):
        if isinstance(group, dict):
            for value in group.get("ids", []):
                if isinstance(value, int):
                    ids.add(value)
    for value in config.get("epic_ids", []):
        if isinstance(value, int):
            ids.add(value)
    gap = config.get("gap", {})
    if isinstance(gap, dict):
        for value in gap.get("child_story_task_ids", []):
            if isinstance(value, int):
                ids.add(value)
    return sorted(ids)


def load_fixture_states(issue_ids: list[int], fixture: dict[str, Any]) -> dict[int, str]:
    default_state = normalize_state(fixture.get("default_state", "OPEN"))
    result = {issue_id: default_state for issue_id in issue_ids}

    overrides = fixture.get("issues", [])
    if not isinstance(overrides, list):
        fail("fixture JSON field 'issues' must be an array")

    for entry in overrides:
        if not isinstance(entry, dict):
            fail("fixture issues entries must be objects")
        number = entry.get("number")
        if not isinstance(number, int):
            fail("fixture issues entries require integer 'number'")
        if number in result:
            result[number] = normalize_state(entry.get("state"))
    return result


def load_live_states(issue_ids: list[int], repo_slug: str) -> dict[int, str]:
    baseline_payload = run_capture(
        [
            "gh",
            "issue",
            "list",
            "--repo",
            repo_slug,
            "--state",
            "all",
            "--limit",
            "500",
            "--json",
            "number,state",
        ]
    )
    try:
        baseline = json.loads(baseline_payload)
    except json.JSONDecodeError as exc:
        fail(f"unable to decode gh issue list payload: {exc}")
    if not isinstance(baseline, list):
        fail("gh issue list payload must be an array")

    lookup: dict[int, str] = {}
    for entry in baseline:
        if not isinstance(entry, dict):
            continue
        number = entry.get("number")
        if isinstance(number, int):
            lookup[number] = normalize_state(entry.get("state"))

    result: dict[int, str] = {}
    for issue_id in issue_ids:
        if issue_id in lookup:
            result[issue_id] = lookup[issue_id]
            continue
        fallback = subprocess.run(
            [
                "gh",
                "issue",
                "view",
                str(issue_id),
                "--repo",
                repo_slug,
                "--json",
                "state",
                "--jq",
                ".state",
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        if fallback.returncode != 0:
            result[issue_id] = "UNKNOWN"
            continue
        result[issue_id] = normalize_state(fallback.stdout)
    return result


@dataclass
class CountSummary:
    closed: int
    open: int
    unknown: int
    total: int

    @property
    def all_closed(self) -> bool:
        return self.total > 0 and self.open == 0 and self.unknown == 0


def summarize(ids: list[int], states: dict[int, str]) -> CountSummary:
    closed = 0
    open_count = 0
    unknown = 0
    for issue_id in ids:
        state = states.get(issue_id, "UNKNOWN")
        if state == "CLOSED":
            closed += 1
        elif state == "OPEN":
            open_count += 1
        else:
            unknown += 1
    return CountSummary(
        closed=closed,
        open=open_count,
        unknown=unknown,
        total=len(ids),
    )


config = load_config()
issue_ids = tracked_issue_ids(config)
if not issue_ids:
    fail("tracked issue set from config resolved to empty")

if generated_at_raw.strip():
    generated_at = parse_iso8601_utc(generated_at_raw)
else:
    generated_at = datetime.now(timezone.utc).replace(microsecond=0)
generated_at_iso = iso_utc(generated_at)

if fixture_path is not None:
    try:
        fixture_payload = json.loads(fixture_path.read_text(encoding="utf-8"))
    except Exception as exc:
        fail(f"unable to parse fixture JSON: {exc}")
    if not isinstance(fixture_payload, dict):
        fail("fixture JSON must decode to an object")
    states = load_fixture_states(issue_ids, fixture_payload)
    repository = repo_slug_raw.strip() or "fixture/repository"
    source_mode = "fixture"
else:
    if repo_slug_raw.strip():
        repository = repo_slug_raw.strip()
    else:
        repository = run_capture(
            ["gh", "repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"]
        ).strip()
        if not repository:
            fail("unable to resolve repository slug from gh repo view")
    states = load_live_states(issue_ids, repository)
    source_mode = "live"

summary = summarize(issue_ids, states)
summary_payload = {
    "tracked_issue_count": summary.total,
    "closed_count": summary.closed,
    "open_count": summary.open,
    "unknown_count": summary.unknown,
    "all_closed": summary.all_closed,
}

todo_groups_payload: list[dict[str, Any]] = []
for group in config.get("todo_groups", []):
    if not isinstance(group, dict):
        continue
    label = str(group.get("label", "")).strip()
    ids = [value for value in group.get("ids", []) if isinstance(value, int)]
    counts = summarize(ids, states)
    todo_groups_payload.append(
        {
            "label": label,
            "issue_ids": ids,
            "closed_count": counts.closed,
            "open_count": counts.open,
            "unknown_count": counts.unknown,
            "total_count": counts.total,
            "all_closed": counts.all_closed,
        }
    )

epic_ids = [value for value in config.get("epic_ids", []) if isinstance(value, int)]
epic_counts = summarize(epic_ids, states)
epics_payload = {
    "label": "Parent epics",
    "issue_ids": epic_ids,
    "closed_count": epic_counts.closed,
    "open_count": epic_counts.open,
    "unknown_count": epic_counts.unknown,
    "total_count": epic_counts.total,
    "all_closed": epic_counts.all_closed,
}

gap_config = config.get("gap", {})
if not isinstance(gap_config, dict):
    fail("config gap block must be an object")
gap_child_ids = [value for value in gap_config.get("child_story_task_ids", []) if isinstance(value, int)]
gap_counts = summarize(gap_child_ids, states)
gap_payload = {
    "core_delivery_pr_span": {
        "from": int(gap_config.get("core_delivery_pr_span", {}).get("from", 0)),
        "to": int(gap_config.get("core_delivery_pr_span", {}).get("to", 0)),
    },
    "child_story_task_ids": gap_child_ids,
    "child_story_task_closed_count": gap_counts.closed,
    "child_story_task_open_count": gap_counts.open,
    "child_story_task_unknown_count": gap_counts.unknown,
    "child_story_task_total_count": gap_counts.total,
    "child_story_task_all_closed": gap_counts.all_closed,
    "epic_summary": str(gap_config.get("epic_summary", "")),
}

payload = {
    "schema_version": 1,
    "generated_at": generated_at_iso,
    "repository": repository,
    "source_mode": source_mode,
    "config_path": str(config_path),
    "summary": summary_payload,
    "todo_groups": todo_groups_payload,
    "epics": epics_payload,
    "gap": gap_payload,
    "issue_states": [{"number": number, "state": states[number]} for number in issue_ids],
}

output_json_path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")

def issue_refs(ids: list[int]) -> str:
    if not ids:
        return "-"
    return ", ".join(f"#{issue_id}" for issue_id in ids)

markdown_lines: list[str] = [
    "# Roadmap Status Artifact",
    "",
    f"- Generated at (UTC): `{generated_at_iso}`",
    f"- Repository: `{repository}`",
    f"- Source mode: `{source_mode}`",
    f"- Config: `{config_path}`",
    "",
    "## Summary",
    "",
    f"- Tracked issues: `{summary.total}`",
    f"- Closed: `{summary.closed}`",
    f"- Open: `{summary.open}`",
    f"- Unknown: `{summary.unknown}`",
    f"- All closed: `{'yes' if summary.all_closed else 'no'}`",
    "",
    "## Todo Groups",
    "",
    "| Group | Closed | Open | Unknown | Total | Complete | Issues |",
    "| --- | ---: | ---: | ---: | ---: | --- | --- |",
]
for group in todo_groups_payload:
    markdown_lines.append(
        "| "
        f"{group['label']} | "
        f"{group['closed_count']} | "
        f"{group['open_count']} | "
        f"{group['unknown_count']} | "
        f"{group['total_count']} | "
        f"{'yes' if group['all_closed'] else 'no'} | "
        f"{issue_refs(group['issue_ids'])} |"
    )

markdown_lines.extend(
    [
        "",
        "## Epic Status",
        "",
        "| Label | Closed | Open | Unknown | Total | Complete | Issues |",
        "| --- | ---: | ---: | ---: | ---: | --- | --- |",
        "| "
        f"{epics_payload['label']} | "
        f"{epics_payload['closed_count']} | "
        f"{epics_payload['open_count']} | "
        f"{epics_payload['unknown_count']} | "
        f"{epics_payload['total_count']} | "
        f"{'yes' if epics_payload['all_closed'] else 'no'} | "
        f"{issue_refs(epics_payload['issue_ids'])} |",
        "",
        "## Gap Snapshot",
        "",
        f"- Core delivery PR range: `#{gap_payload['core_delivery_pr_span']['from']}` through `#{gap_payload['core_delivery_pr_span']['to']}`",
        f"- Child stories/tasks closed: `{gap_payload['child_story_task_closed_count']}/{gap_payload['child_story_task_total_count']}`",
        f"- Child stories/tasks open: `{gap_payload['child_story_task_open_count']}`",
        f"- Child stories/tasks unknown: `{gap_payload['child_story_task_unknown_count']}`",
        f"- Child stories/tasks all closed: `{'yes' if gap_payload['child_story_task_all_closed'] else 'no'}`",
        f"- Child story/task issue refs: `{issue_refs(gap_payload['child_story_task_ids'])}`",
        f"- Epic summary: `{gap_payload['epic_summary']}`",
        "",
        "## Issue States",
        "",
        "| Issue | State |",
        "| --- | --- |",
    ]
)
for row in payload["issue_states"]:
    markdown_lines.append(f"| #{row['number']} | {row['state']} |")

output_md_path.write_text("\n".join(markdown_lines) + "\n", encoding="utf-8")
log(
    "[roadmap-status-artifact] "
    f"repository={repository} source_mode={source_mode} "
    f"tracked={summary.total} closed={summary.closed} open={summary.open} unknown={summary.unknown}"
)
PY

log_info "wrote roadmap status artifacts:"
log_info "  - ${OUTPUT_JSON}"
log_info "  - ${OUTPUT_MD}"
