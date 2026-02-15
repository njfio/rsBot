#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

POLICY_JSON="${REPO_ROOT}/tasks/policies/critical-path-update-cadence-policy.json"
REPO_SLUG=""
ISSUE_NUMBER=""
FIXTURE_COMMENTS_JSON=""
NOW_UTC=""
JSON_MODE="false"
QUIET_MODE="false"

usage() {
  cat <<'USAGE'
Usage: critical-path-cadence-check.sh [options]

Validate #1678 critical-path update cadence against policy thresholds.

Options:
  --policy-json <path>           Cadence policy JSON path
  --repo <owner/name>            Repository slug for live comment fetch
  --issue-number <number>        Tracker issue number (default from policy)
  --fixture-comments-json <path> Fixture comments JSON (list or object.comments)
  --now-utc <ISO8601>            Deterministic current UTC time for tests
  --json                         Emit JSON output
  --quiet                        Suppress informational stderr logs
  --help                         Show this help
USAGE
}

log_info() {
  if [[ "${QUIET_MODE}" != "true" ]]; then
    echo "$@" >&2
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
    --policy-json)
      POLICY_JSON="$2"
      shift 2
      ;;
    --repo)
      REPO_SLUG="$2"
      shift 2
      ;;
    --issue-number)
      ISSUE_NUMBER="$2"
      shift 2
      ;;
    --fixture-comments-json)
      FIXTURE_COMMENTS_JSON="$2"
      shift 2
      ;;
    --now-utc)
      NOW_UTC="$2"
      shift 2
      ;;
    --json)
      JSON_MODE="true"
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
      echo "error: unknown option '$1'" >&2
      usage >&2
      exit 1
      ;;
  esac
done

require_cmd python3
require_cmd jq

if [[ ! -f "${POLICY_JSON}" ]]; then
  echo "error: cadence policy not found: ${POLICY_JSON}" >&2
  exit 1
fi

if [[ -n "${FIXTURE_COMMENTS_JSON}" && ! -f "${FIXTURE_COMMENTS_JSON}" ]]; then
  echo "error: fixture comments JSON not found: ${FIXTURE_COMMENTS_JSON}" >&2
  exit 1
fi

if [[ -z "${ISSUE_NUMBER}" ]]; then
  ISSUE_NUMBER="$(jq -r '.tracker_issue_number // empty' "${POLICY_JSON}")"
fi
if ! [[ "${ISSUE_NUMBER}" =~ ^[0-9]+$ ]]; then
  echo "error: issue number must be numeric" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT
comments_json="${tmp_dir}/comments.json"

if [[ -n "${FIXTURE_COMMENTS_JSON}" ]]; then
  cp "${FIXTURE_COMMENTS_JSON}" "${comments_json}"
  SOURCE_MODE="fixture"
else
  require_cmd gh
  SOURCE_MODE="live"
  if [[ -z "${REPO_SLUG}" ]]; then
    REPO_SLUG="$(gh repo view --json nameWithOwner --jq '.nameWithOwner')"
  fi

  echo '[]' >"${comments_json}"
  page=1
  while true; do
    endpoint="repos/${REPO_SLUG}/issues/${ISSUE_NUMBER}/comments?per_page=100&page=${page}"
    page_payload="${tmp_dir}/comments-page-${page}.json"
    gh api "${endpoint}" >"${page_payload}"

    page_count="$(jq 'length' "${page_payload}")"
    if [[ "${page_count}" -eq 0 ]]; then
      break
    fi

    jq -s '.[0] + .[1]' "${comments_json}" "${page_payload}" >"${comments_json}.next"
    mv "${comments_json}.next" "${comments_json}"

    if [[ "${page_count}" -lt 100 ]]; then
      break
    fi
    page=$((page + 1))
  done
fi

python3 - \
  "${POLICY_JSON}" \
  "${comments_json}" \
  "${NOW_UTC}" \
  "${JSON_MODE}" \
  "${SOURCE_MODE}" <<'PY'
from __future__ import annotations

import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

policy_path, comments_path, now_utc_raw, json_mode, source_mode = sys.argv[1:]


def parse_utc(value: str) -> datetime:
    candidate = value.strip()
    if candidate.endswith("Z"):
        candidate = candidate[:-1] + "+00:00"
    dt = datetime.fromisoformat(candidate)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt.astimezone(timezone.utc)


policy = json.loads(Path(policy_path).read_text(encoding="utf-8"))
comments_payload = json.loads(Path(comments_path).read_text(encoding="utf-8"))
if isinstance(comments_payload, list):
    comments: list[dict[str, Any]] = [entry for entry in comments_payload if isinstance(entry, dict)]
elif isinstance(comments_payload, dict) and isinstance(comments_payload.get("comments"), list):
    comments = [entry for entry in comments_payload["comments"] if isinstance(entry, dict)]
else:
    raise SystemExit("error: comments payload must be a JSON array or object.comments array")

header = str(policy.get("update_header", "## Critical-Path Update"))
cadence_hours = int(policy.get("cadence_hours", 24))
grace_period_hours = int(policy.get("grace_period_hours", 0))
warning_after_hours = int(policy.get("warning_after_hours", cadence_hours + grace_period_hours))
escalate_after_hours = int(policy.get("escalate_after_hours", warning_after_hours * 2))

if now_utc_raw.strip():
    now_utc = parse_utc(now_utc_raw)
else:
    now_utc = datetime.now(timezone.utc)

matching: list[dict[str, Any]] = []
for entry in comments:
    body = entry.get("body")
    created_at = entry.get("created_at")
    if not isinstance(body, str) or not isinstance(created_at, str):
        continue
    if header not in body:
        continue
    try:
        created_dt = parse_utc(created_at)
    except Exception:
        continue
    matching.append({"created_at": created_at, "created_dt": created_dt, "id": entry.get("id")})

status = "critical"
reason_code = "no_update_found"
exit_code = 1
last_update_at = None
age_hours = None

if matching:
    latest = max(matching, key=lambda item: item["created_dt"])
    last_dt = latest["created_dt"]
    last_update_at = last_dt.replace(microsecond=0).isoformat().replace("+00:00", "Z")
    age_hours = round((now_utc - last_dt).total_seconds() / 3600.0, 2)

    if age_hours <= cadence_hours + grace_period_hours:
        status = "ok"
        reason_code = "within_cadence"
        exit_code = 0
    elif age_hours <= escalate_after_hours:
        status = "warning"
        reason_code = "stale_update_warning"
        exit_code = 1
    else:
        status = "critical"
        reason_code = "stale_update_escalation"
        exit_code = 1

payload = {
    "schema_version": 1,
    "policy_id": str(policy.get("policy_id", "critical-path-update-cadence-policy")),
    "source_mode": source_mode,
    "status": status,
    "reason_code": reason_code,
    "last_update_at": last_update_at,
    "age_hours": age_hours,
    "cadence_hours": cadence_hours,
    "grace_period_hours": grace_period_hours,
    "warning_after_hours": warning_after_hours,
    "escalate_after_hours": escalate_after_hours,
    "matched_updates": len(matching),
    "evaluated_at": now_utc.replace(microsecond=0).isoformat().replace("+00:00", "Z"),
}

if json_mode == "true":
    print(json.dumps(payload))
else:
    print(
        "[critical-path-cadence-check] "
        f"status={payload['status']} "
        f"reason={payload['reason_code']} "
        f"age_hours={payload['age_hours']} "
        f"matched_updates={payload['matched_updates']}"
    )

sys.exit(exit_code)
PY
