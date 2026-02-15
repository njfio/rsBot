#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

TODO_PATH="${REPO_ROOT}/tasks/todo.md"
GAP_PATH="${REPO_ROOT}/tasks/tau-vs-ironclaw-gap-list.md"
CONFIG_PATH="${REPO_ROOT}/tasks/roadmap-status-config.json"
FIXTURE_JSON=""
REPO_SLUG=""
CHECK_MODE="false"

TODO_BEGIN="<!-- ROADMAP_STATUS:BEGIN -->"
TODO_END="<!-- ROADMAP_STATUS:END -->"
GAP_BEGIN="<!-- ROADMAP_GAP_STATUS:BEGIN -->"
GAP_END="<!-- ROADMAP_GAP_STATUS:END -->"

declare -a TODO_GROUP_LABELS=()
declare -a TODO_GROUP_IDS=()
EPIC_IDS=""
GAP_CHILD_IDS=""
GAP_CORE_PR_FROM=""
GAP_CORE_PR_TO=""
GAP_EPIC_SUMMARY=""

declare -A ISSUE_STATE=()

usage() {
  cat <<'EOF'
Usage: roadmap-status-sync.sh [options]

Refresh generated status blocks in:
  - tasks/todo.md
  - tasks/tau-vs-ironclaw-gap-list.md

Options:
  --todo-path <path>      Override todo doc path.
  --gap-path <path>       Override gap-list doc path.
  --config-path <path>    Override roadmap config path.
  --repo <owner/name>     Override repository for gh queries.
  --fixture-json <path>   Read issue states from fixture JSON instead of GitHub.
  --check                 Verify docs are up to date (no writes).
  --help                  Show this message.

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

normalize_state() {
  local state="$1"
  echo "${state}" | tr '[:lower:]' '[:upper:]'
}

join_issue_refs() {
  local out=""
  local first="true"
  local id
  for id in "$@"; do
    if [[ "${first}" == "true" ]]; then
      out="#${id}"
      first="false"
    else
      out="${out}, #${id}"
    fi
  done
  printf '%s' "${out}"
}

is_closed_id() {
  local id="$1"
  [[ "${ISSUE_STATE[${id}]:-UNKNOWN}" == "CLOSED" ]]
}

phase_line() {
  local label="$1"
  shift
  local ids=("$@")
  local total="${#ids[@]}"
  local closed=0
  local id
  for id in "${ids[@]}"; do
    if is_closed_id "${id}"; then
      closed=$((closed + 1))
    fi
  done

  local mark=" "
  if [[ "${closed}" -eq "${total}" ]]; then
    mark="x"
  fi

  printf -- "- [%s] %s (closed %d/%d): %s\n" \
    "${mark}" \
    "${label}" \
    "${closed}" \
    "${total}" \
    "$(join_issue_refs "${ids[@]}")"
}

unique_tracked_ids() {
  {
    local group_ids
    for group_ids in "${TODO_GROUP_IDS[@]}"; do
      if [[ -n "${group_ids}" ]]; then
        printf '%s\n' ${group_ids}
      fi
    done
    if [[ -n "${EPIC_IDS}" ]]; then
      printf '%s\n' ${EPIC_IDS}
    fi
  } | sort -n -u
}

load_config() {
  require_cmd jq

  if [[ ! -f "${CONFIG_PATH}" ]]; then
    echo "error: roadmap config not found: ${CONFIG_PATH}" >&2
    exit 1
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

  TODO_GROUP_LABELS=()
  TODO_GROUP_IDS=()
  while IFS=$'\t' read -r label ids; do
    TODO_GROUP_LABELS+=("${label}")
    TODO_GROUP_IDS+=("${ids}")
  done < <(
    jq -r '.todo_groups[] | [.label, (.ids | map(tostring) | join(" "))] | @tsv' "${CONFIG_PATH}"
  )

  EPIC_IDS="$(jq -r '.epic_ids | map(tostring) | join(" ")' "${CONFIG_PATH}")"
  GAP_CHILD_IDS="$(jq -r '.gap.child_story_task_ids | map(tostring) | join(" ")' "${CONFIG_PATH}")"
  GAP_CORE_PR_FROM="$(jq -r '.gap.core_delivery_pr_span.from' "${CONFIG_PATH}")"
  GAP_CORE_PR_TO="$(jq -r '.gap.core_delivery_pr_span.to' "${CONFIG_PATH}")"
  GAP_EPIC_SUMMARY="$(jq -r '.gap.epic_summary' "${CONFIG_PATH}")"
}

load_fixture_states() {
  require_cmd jq
  if [[ ! -f "${FIXTURE_JSON}" ]]; then
    echo "error: fixture JSON not found: ${FIXTURE_JSON}" >&2
    exit 1
  fi

  local default_state
  default_state="$(jq -r '.default_state // "OPEN"' "${FIXTURE_JSON}")"
  default_state="$(normalize_state "${default_state}")"

  local id
  while IFS= read -r id; do
    ISSUE_STATE["${id}"]="${default_state}"
  done < <(unique_tracked_ids)

  while IFS=: read -r issue_number issue_state; do
    ISSUE_STATE["${issue_number}"]="$(normalize_state "${issue_state}")"
  done < <(jq -r '.issues[]? | "\(.number):\(.state)"' "${FIXTURE_JSON}")
}

load_live_issue_states() {
  require_cmd gh
  require_cmd jq

  if [[ -z "${REPO_SLUG}" ]]; then
    REPO_SLUG="$(gh repo view --json nameWithOwner --jq '.nameWithOwner')"
  fi

  local list_json
  list_json="$(gh issue list --repo "${REPO_SLUG}" --state all --limit 500 --json number,state)"

  local id
  while IFS= read -r id; do
    local state
    state="$(jq -r --argjson issue "${id}" '[.[] | select(.number == $issue) | .state][0] // ""' <<<"${list_json}")"
    if [[ -z "${state}" ]]; then
      state="$(gh issue view "${id}" --repo "${REPO_SLUG}" --json state --jq '.state' 2>/dev/null || true)"
    fi
    if [[ -z "${state}" ]]; then
      state="UNKNOWN"
    fi
    ISSUE_STATE["${id}"]="$(normalize_state "${state}")"
  done < <(unique_tracked_ids)
}

render_replaced_file() {
  local file="$1"
  local begin_marker="$2"
  local end_marker="$3"
  local out_file="$4"
  local replacement_body="$5"

  ROADMAP_REPLACEMENT_BODY="${replacement_body}" python3 - "${file}" "${begin_marker}" "${end_marker}" "${out_file}" <<'PY'
import pathlib
import re
import sys
import os

file_path, begin_marker, end_marker, out_path = sys.argv[1:5]
replacement_body = os.environ.get("ROADMAP_REPLACEMENT_BODY", "").rstrip("\n")

text = pathlib.Path(file_path).read_text()
pattern = re.escape(begin_marker) + r".*?" + re.escape(end_marker)
replacement = begin_marker + "\n" + replacement_body + "\n" + end_marker
updated, count = re.subn(pattern, replacement, text, flags=re.S)

if count != 1:
    raise SystemExit(
        f"expected exactly one marker block in {file_path} for {begin_marker}..{end_marker}, found {count}"
    )

pathlib.Path(out_path).write_text(updated)
PY
}

build_todo_status_block() {
  local date_utc
  date_utc="$(date -u +%F)"

  local total=0
  local closed=0
  local open=0
  local unknown=0
  local id
  while IFS= read -r id; do
    total=$((total + 1))
    case "${ISSUE_STATE[${id}]:-UNKNOWN}" in
      CLOSED)
        closed=$((closed + 1))
        ;;
      OPEN)
        open=$((open + 1))
        ;;
      *)
        unknown=$((unknown + 1))
        ;;
    esac
  done < <(unique_tracked_ids)

  local global_mark=" "
  if [[ "${open}" -eq 0 && "${unknown}" -eq 0 ]]; then
    global_mark="x"
  fi

  local phase_lines=""
  local idx
  for idx in "${!TODO_GROUP_LABELS[@]}"; do
    local ids=()
    if [[ -n "${TODO_GROUP_IDS[${idx}]}" ]]; then
      read -r -a ids <<< "${TODO_GROUP_IDS[${idx}]}"
    fi
    phase_lines+=$(phase_line "${TODO_GROUP_LABELS[${idx}]}" "${ids[@]}")
    phase_lines+=$'\n'
  done
  phase_lines="${phase_lines%$'\n'}"

  cat <<EOF
## Execution Status (${date_utc})

Source of truth is GitHub issue and PR history, not this file's original checkbox draft language.
Generated by \`scripts/dev/roadmap-status-sync.sh\`.

${phase_lines}
- [${global_mark}] Tracked roadmap issues closed: ${closed}/${total} (open: ${open}, unknown: ${unknown}).
EOF
}

build_gap_status_block() {
  local date_utc
  date_utc="$(date -u +%F)"

  local child_ids=()
  read -r -a child_ids <<< "${GAP_CHILD_IDS}"
  local child_total="${#child_ids[@]}"
  local child_closed=0
  local id
  for id in "${child_ids[@]}"; do
    if is_closed_id "${id}"; then
      child_closed=$((child_closed + 1))
    fi
  done

  local child_mark=" "
  if [[ "${child_closed}" -eq "${child_total}" ]]; then
    child_mark="x"
  fi

  local epic_ids=()
  read -r -a epic_ids <<< "${EPIC_IDS}"
  local epic_closed=0
  for id in "${epic_ids[@]}"; do
    if is_closed_id "${id}"; then
      epic_closed=$((epic_closed + 1))
    fi
  done

  local epic_mark=" "
  if [[ "${epic_closed}" -eq "${#epic_ids[@]}" ]]; then
    epic_mark="x"
  fi

  cat <<EOF
## Status Snapshot (${date_utc})

This document is the pre-execution baseline used to drive the delivery wave. The gap items are tracked by merged issue and PR history.
Generated by \`scripts/dev/roadmap-status-sync.sh\`.

- [x] Core delivery wave merged in PRs #${GAP_CORE_PR_FROM} through #${GAP_CORE_PR_TO}.
- [${child_mark}] Child stories/tasks referenced by this plan are closed (${child_closed}/${child_total}): $(join_issue_refs "${child_ids[@]}").
- [${epic_mark}] Parent epics closed: ${GAP_EPIC_SUMMARY}.

For current status, use GitHub issues and PRs as source of truth.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --todo-path)
      shift
      TODO_PATH="$1"
      ;;
    --gap-path)
      shift
      GAP_PATH="$1"
      ;;
    --config-path)
      shift
      CONFIG_PATH="$1"
      ;;
    --repo)
      shift
      REPO_SLUG="$1"
      ;;
    --fixture-json)
      shift
      FIXTURE_JSON="$1"
      ;;
    --check)
      CHECK_MODE="true"
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
  shift
done

load_config

if [[ -n "${FIXTURE_JSON}" ]]; then
  load_fixture_states
else
  load_live_issue_states
fi

TODO_BLOCK="$(build_todo_status_block)"
GAP_BLOCK="$(build_gap_status_block)"

tmp_todo="$(mktemp)"
tmp_gap="$(mktemp)"
trap 'rm -f "${tmp_todo}" "${tmp_gap}"' EXIT

render_replaced_file "${TODO_PATH}" "${TODO_BEGIN}" "${TODO_END}" "${tmp_todo}" "${TODO_BLOCK}"
render_replaced_file "${GAP_PATH}" "${GAP_BEGIN}" "${GAP_END}" "${tmp_gap}" "${GAP_BLOCK}"

if [[ "${CHECK_MODE}" == "true" ]]; then
  check_failed="false"
  if ! diff -u "${TODO_PATH}" "${tmp_todo}"; then
    check_failed="true"
  fi
  if ! diff -u "${GAP_PATH}" "${tmp_gap}"; then
    check_failed="true"
  fi

  if [[ "${check_failed}" == "true" ]]; then
    echo "roadmap status docs are out of date; run scripts/dev/roadmap-status-sync.sh" >&2
    exit 1
  fi

  echo "roadmap status docs are up to date"
  exit 0
fi

mv "${tmp_todo}" "${TODO_PATH}"
mv "${tmp_gap}" "${GAP_PATH}"
echo "updated roadmap status blocks:"
echo "  - ${TODO_PATH}"
echo "  - ${GAP_PATH}"
