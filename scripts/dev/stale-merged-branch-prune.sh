#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

DEFAULT_OUTPUT_JSON="${REPO_ROOT}/tasks/reports/stale-merged-branch-prune.json"
DEFAULT_OUTPUT_MD="${REPO_ROOT}/tasks/reports/stale-merged-branch-prune.md"
DELETE_CONFIRMATION_TOKEN="DELETE_REMOTE_BRANCHES"

TARGET_REPO_ROOT=""
REMOTE_NAME="origin"
BASE_BRANCH="master"
MIN_AGE_DAYS=7
OUTPUT_JSON="${DEFAULT_OUTPUT_JSON}"
OUTPUT_MD="${DEFAULT_OUTPUT_MD}"
GENERATED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
EXECUTE_MODE="false"
CONFIRM_DELETE=""
QUIET_MODE="false"
PROTECTED_PATTERNS=("master" "main" "develop" "release/*" "hotfix/*")

usage() {
  cat <<'EOF'
Usage: stale-merged-branch-prune.sh [options]

Inventory and optionally prune stale merged remote branches.
Default mode is dry-run and never deletes branches.

Options:
  --repo-root <path>         Target git repository root (defaults to current repo root).
  --remote <name>            Remote name (default: origin).
  --base-branch <name>       Base branch for merged checks (default: master).
  --min-age-days <n>         Minimum branch age in days for prune eligibility (default: 7).
  --protect-pattern <glob>   Additional protected branch glob; repeatable.
  --output-json <path>       JSON report output path.
  --output-md <path>         Markdown report output path.
  --generated-at <iso>       Generated timestamp (ISO-8601 UTC).
  --execute                  Enable deletion mode (requires --confirm-delete token).
  --confirm-delete <token>   Must be exactly DELETE_REMOTE_BRANCHES when --execute is set.
  --quiet                    Suppress informational output.
  --help                     Show this help text.
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

iso_to_unix() {
  local iso_value="$1"
  python3 - "${iso_value}" <<'PY'
import sys
from datetime import datetime, timezone

value = sys.argv[1].strip()
if value.endswith("Z"):
    value = value[:-1] + "+00:00"
try:
    dt = datetime.fromisoformat(value)
except ValueError as exc:
    raise SystemExit(f"invalid ISO timestamp: {exc}")
if dt.tzinfo is None:
    dt = dt.replace(tzinfo=timezone.utc)
print(int(dt.timestamp()))
PY
}

is_protected_branch() {
  local branch="$1"
  local pattern
  for pattern in "${PROTECTED_PATTERNS[@]}"; do
    if [[ "${branch}" == ${pattern} ]]; then
      return 0
    fi
  done
  return 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo-root)
      TARGET_REPO_ROOT="$2"
      shift 2
      ;;
    --remote)
      REMOTE_NAME="$2"
      shift 2
      ;;
    --base-branch)
      BASE_BRANCH="$2"
      shift 2
      ;;
    --min-age-days)
      MIN_AGE_DAYS="$2"
      shift 2
      ;;
    --protect-pattern)
      PROTECTED_PATTERNS+=("$2")
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
    --execute)
      EXECUTE_MODE="true"
      shift
      ;;
    --confirm-delete)
      CONFIRM_DELETE="$2"
      shift 2
      ;;
    --quiet)
      QUIET_MODE="true"
      shift
      ;;
    --help)
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

require_cmd git
require_cmd jq
require_cmd python3

if ! [[ "${MIN_AGE_DAYS}" =~ ^[0-9]+$ ]]; then
  echo "error: --min-age-days must be a non-negative integer" >&2
  exit 1
fi

if [[ "${EXECUTE_MODE}" == "true" && "${CONFIRM_DELETE}" != "${DELETE_CONFIRMATION_TOKEN}" ]]; then
  echo "error: --execute requires --confirm-delete ${DELETE_CONFIRMATION_TOKEN}" >&2
  exit 1
fi

if [[ -n "${TARGET_REPO_ROOT}" ]]; then
  REPO_ROOT="${TARGET_REPO_ROOT}"
fi

if [[ ! -d "${REPO_ROOT}" ]]; then
  echo "error: repository root does not exist: ${REPO_ROOT}" >&2
  exit 1
fi

if ! git -C "${REPO_ROOT}" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "error: ${REPO_ROOT} is not a git work tree" >&2
  exit 1
fi

if ! git -C "${REPO_ROOT}" remote get-url "${REMOTE_NAME}" >/dev/null 2>&1; then
  echo "error: remote '${REMOTE_NAME}' not found in ${REPO_ROOT}" >&2
  exit 1
fi

if ! git -C "${REPO_ROOT}" show-ref --verify --quiet "refs/remotes/${REMOTE_NAME}/${BASE_BRANCH}"; then
  echo "error: remote base ref refs/remotes/${REMOTE_NAME}/${BASE_BRANCH} not found" >&2
  exit 1
fi

generated_unix="$(iso_to_unix "${GENERATED_AT}")"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT
rows_path="${tmp_dir}/rows.tsv"
: >"${rows_path}"

while IFS=$'\t' read -r remote_ref last_commit_unix tip_sha; do
  if [[ -z "${remote_ref}" ]]; then
    continue
  fi

  if [[ "${remote_ref}" == "${REMOTE_NAME}/HEAD" ]]; then
    continue
  fi

  branch="${remote_ref#${REMOTE_NAME}/}"
  if [[ -z "${branch}" || "${branch}" == "${remote_ref}" ]]; then
    continue
  fi

  if [[ -z "${last_commit_unix}" ]]; then
    last_commit_unix=0
  fi
  if (( generated_unix > last_commit_unix )); then
    age_days=$(((generated_unix - last_commit_unix) / 86400))
  else
    age_days=0
  fi

  merged_into_base="false"
  if git -C "${REPO_ROOT}" merge-base --is-ancestor "${remote_ref}" "${REMOTE_NAME}/${BASE_BRANCH}" >/dev/null 2>&1; then
    merged_into_base="true"
  fi

  protected="false"
  if is_protected_branch "${branch}"; then
    protected="true"
  fi

  eligible="false"
  action="skipped"
  skip_reason=""

  if [[ "${branch}" == "${BASE_BRANCH}" ]]; then
    skip_reason="base_branch"
  elif [[ "${merged_into_base}" != "true" ]]; then
    skip_reason="not_merged"
  elif [[ "${protected}" == "true" ]]; then
    skip_reason="protected"
  elif (( age_days < MIN_AGE_DAYS )); then
    skip_reason="age_below_threshold"
  else
    eligible="true"
    action="candidate"
  fi

  if [[ "${eligible}" == "true" && "${EXECUTE_MODE}" == "true" ]]; then
    if git -C "${REPO_ROOT}" push "${REMOTE_NAME}" --delete "${branch}" >/dev/null 2>&1; then
      action="deleted"
      skip_reason=""
      log_info "deleted remote branch ${REMOTE_NAME}/${branch}"
    else
      action="delete_failed"
      skip_reason="push_delete_failed"
      log_info "failed to delete remote branch ${REMOTE_NAME}/${branch}"
    fi
  fi

  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "${branch}" \
    "${remote_ref}" \
    "${tip_sha}" \
    "${last_commit_unix}" \
    "${age_days}" \
    "${merged_into_base}" \
    "${protected}" \
    "${eligible}" \
    "${action}" \
    "${skip_reason}" >>"${rows_path}"
done < <(git -C "${REPO_ROOT}" for-each-ref \
  --format='%(refname:short)%09%(committerdate:unix)%09%(objectname)' \
  "refs/remotes/${REMOTE_NAME}")

analysis_json="$(
  jq -Rn \
    --arg generated_at "${GENERATED_AT}" \
    --arg repository_root "${REPO_ROOT}" \
    --arg remote "${REMOTE_NAME}" \
    --arg base_branch "${BASE_BRANCH}" \
    --arg min_age_days "${MIN_AGE_DAYS}" \
    --arg mode "$(if [[ "${EXECUTE_MODE}" == "true" ]]; then echo "execute"; else echo "dry_run"; fi)" \
    --arg execute_requested "${EXECUTE_MODE}" \
    --arg deletion_confirmed "$(if [[ "${CONFIRM_DELETE}" == "${DELETE_CONFIRMATION_TOKEN}" ]]; then echo "true"; else echo "false"; fi)" \
    '
      [
        inputs
        | select(length > 0)
        | split("\t")
        | {
            branch: .[0],
            remote_ref: .[1],
            tip_sha: .[2],
            last_commit_unix: (.[3] | tonumber),
            age_days: (.[4] | tonumber),
            merged_into_base: (.[5] == "true"),
            protected: (.[6] == "true"),
            eligible: (.[7] == "true"),
            action: .[8],
            skip_reason: (if .[9] == "" then null else .[9] end)
          }
      ] as $rows
      | {
          schema_version: 1,
          generated_at: $generated_at,
          repository_root: $repository_root,
          remote: $remote,
          base_branch: $base_branch,
          min_age_days: ($min_age_days | tonumber),
          mode: $mode,
          execute_requested: ($execute_requested == "true"),
          deletion_confirmed: ($deletion_confirmed == "true"),
          summary: {
            total_branches: ($rows | length),
            eligible_count: ($rows | map(select(.eligible)) | length),
            candidate_count: ($rows | map(select(.action == "candidate")) | length),
            deleted_count: ($rows | map(select(.action == "deleted")) | length),
            skipped_count: ($rows | map(select(.action == "skipped")) | length),
            delete_failed_count: ($rows | map(select(.action == "delete_failed")) | length)
          },
          branches: $rows
        }
    ' <"${rows_path}"
)"

mkdir -p "$(dirname "${OUTPUT_JSON}")" "$(dirname "${OUTPUT_MD}")"
printf '%s\n' "${analysis_json}" | jq '.' >"${OUTPUT_JSON}"

{
  echo "# Stale Merged Branch Prune Report"
  echo
  echo "- Generated at: \`${GENERATED_AT}\`"
  echo "- Repository root: \`${REPO_ROOT}\`"
  echo "- Remote: \`${REMOTE_NAME}\`"
  echo "- Base branch: \`${BASE_BRANCH}\`"
  echo "- Min age days: \`${MIN_AGE_DAYS}\`"
  echo "- Mode: \`$(if [[ "${EXECUTE_MODE}" == "true" ]]; then echo "execute"; else echo "dry_run"; fi)\`"
  echo
  echo "## Summary"
  echo
  echo "| Metric | Value |"
  echo "| --- | ---: |"
  echo "| Total branches | $(printf '%s\n' "${analysis_json}" | jq -r '.summary.total_branches') |"
  echo "| Eligible branches | $(printf '%s\n' "${analysis_json}" | jq -r '.summary.eligible_count') |"
  echo "| Candidate branches | $(printf '%s\n' "${analysis_json}" | jq -r '.summary.candidate_count') |"
  echo "| Deleted branches | $(printf '%s\n' "${analysis_json}" | jq -r '.summary.deleted_count') |"
  echo "| Skipped branches | $(printf '%s\n' "${analysis_json}" | jq -r '.summary.skipped_count') |"
  echo "| Delete failures | $(printf '%s\n' "${analysis_json}" | jq -r '.summary.delete_failed_count') |"
  echo
  echo "## Branch Rows"
  echo
  echo "| Branch | Age (days) | Merged | Protected | Eligible | Action | Skip reason | Tip SHA |"
  echo "| --- | ---: | --- | --- | --- | --- | --- | --- |"
  branch_count="$(printf '%s\n' "${analysis_json}" | jq -r '.branches | length')"
  if [[ "${branch_count}" == "0" ]]; then
    echo "| _none_ | - | - | - | - | - | - | - |"
  else
    while IFS=$'\t' read -r branch age_days merged protected eligible action skip_reason tip_sha; do
      echo "| ${branch} | ${age_days} | ${merged} | ${protected} | ${eligible} | ${action} | ${skip_reason} | ${tip_sha} |"
    done < <(printf '%s\n' "${analysis_json}" | jq -r '.branches[] | [.branch, (.age_days|tostring), (.merged_into_base|tostring), (.protected|tostring), (.eligible|tostring), .action, (.skip_reason // "-"), .tip_sha] | @tsv')
  fi
} >"${OUTPUT_MD}"

delete_failed_count="$(printf '%s\n' "${analysis_json}" | jq -r '.summary.delete_failed_count')"
if [[ "${delete_failed_count}" != "0" ]]; then
  echo "error: one or more branch deletions failed" >&2
  exit 1
fi

log_info "wrote ${OUTPUT_JSON}"
log_info "wrote ${OUTPUT_MD}"
