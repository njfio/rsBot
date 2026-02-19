#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PRUNE_SCRIPT="${SCRIPT_DIR}/stale-merged-branch-prune.sh"

assert_equals() {
  local expected="$1"
  local actual="$2"
  local label="$3"
  if [[ "${expected}" != "${actual}" ]]; then
    echo "assertion failed (${label}): expected '${expected}' got '${actual}'" >&2
    exit 1
  fi
}

assert_non_empty() {
  local value="$1"
  local label="$2"
  if [[ -z "${value}" ]]; then
    echo "assertion failed (${label}): expected non-empty value" >&2
    exit 1
  fi
}

assert_file_exists() {
  local path="$1"
  local label="$2"
  if [[ ! -f "${path}" ]]; then
    echo "assertion failed (${label}): missing file ${path}" >&2
    exit 1
  fi
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

remote_repo="${tmp_dir}/remote.git"
work_repo="${tmp_dir}/work"
dry_run_json="${tmp_dir}/dry-run.json"
dry_run_md="${tmp_dir}/dry-run.md"
execute_json="${tmp_dir}/execute.json"
execute_md="${tmp_dir}/execute.md"

git init --bare "${remote_repo}" >/dev/null
git clone "${remote_repo}" "${work_repo}" >/dev/null

cd "${work_repo}"
git config user.name "Tau Bot"
git config user.email "tau@example.com"

echo "base" >README.md
GIT_AUTHOR_DATE="2026-01-01T00:00:00Z" \
GIT_COMMITTER_DATE="2026-01-01T00:00:00Z" \
  git add README.md
GIT_AUTHOR_DATE="2026-01-01T00:00:00Z" \
GIT_COMMITTER_DATE="2026-01-01T00:00:00Z" \
  git commit -m "base" >/dev/null
git push origin master >/dev/null

git checkout -b stale-merged >/dev/null
echo "stale merged candidate" >stale-merged.txt
GIT_AUTHOR_DATE="2026-01-02T00:00:00Z" \
GIT_COMMITTER_DATE="2026-01-02T00:00:00Z" \
  git add stale-merged.txt
GIT_AUTHOR_DATE="2026-01-02T00:00:00Z" \
GIT_COMMITTER_DATE="2026-01-02T00:00:00Z" \
  git commit -m "stale merged candidate" >/dev/null
git checkout master >/dev/null
GIT_AUTHOR_DATE="2026-01-03T00:00:00Z" \
GIT_COMMITTER_DATE="2026-01-03T00:00:00Z" \
  git merge --no-ff -m "merge stale-merged" stale-merged >/dev/null
git push origin master stale-merged >/dev/null

git checkout -b codex/protected-merged >/dev/null
echo "protected merged branch" >protected.txt
GIT_AUTHOR_DATE="2026-01-04T00:00:00Z" \
GIT_COMMITTER_DATE="2026-01-04T00:00:00Z" \
  git add protected.txt
GIT_AUTHOR_DATE="2026-01-04T00:00:00Z" \
GIT_COMMITTER_DATE="2026-01-04T00:00:00Z" \
  git commit -m "protected merged branch" >/dev/null
git checkout master >/dev/null
GIT_AUTHOR_DATE="2026-01-05T00:00:00Z" \
GIT_COMMITTER_DATE="2026-01-05T00:00:00Z" \
  git merge --no-ff -m "merge protected branch" codex/protected-merged >/dev/null
git push origin master codex/protected-merged >/dev/null

git checkout -b stale-unmerged >/dev/null
echo "stale unmerged branch" >stale-unmerged.txt
GIT_AUTHOR_DATE="2026-01-06T00:00:00Z" \
GIT_COMMITTER_DATE="2026-01-06T00:00:00Z" \
  git add stale-unmerged.txt
GIT_AUTHOR_DATE="2026-01-06T00:00:00Z" \
GIT_COMMITTER_DATE="2026-01-06T00:00:00Z" \
  git commit -m "stale unmerged branch" >/dev/null
git push origin stale-unmerged >/dev/null
git checkout master >/dev/null
git fetch origin --prune >/dev/null

candidate_tip_sha="$(git rev-parse origin/stale-merged)"
assert_non_empty "${candidate_tip_sha}" "fixture stale-merged sha"

# Functional C-01: dry-run produces deterministic inventory and never deletes.
"${PRUNE_SCRIPT}" \
  --repo-root "${work_repo}" \
  --remote origin \
  --base-branch master \
  --min-age-days 1 \
  --protect-pattern "codex/*" \
  --generated-at "2026-02-20T00:00:00Z" \
  --output-json "${dry_run_json}" \
  --output-md "${dry_run_md}" \
  --quiet

assert_file_exists "${dry_run_json}" "functional dry-run json output"
assert_file_exists "${dry_run_md}" "functional dry-run markdown output"
assert_equals "1" "$(jq -r '.summary.candidate_count' "${dry_run_json}")" "functional candidate count"
assert_equals "candidate" "$(jq -r '.branches[] | select(.branch == "stale-merged") | .action' "${dry_run_json}")" "functional stale-merged action"
assert_equals "protected" "$(jq -r '.branches[] | select(.branch == "codex/protected-merged") | .skip_reason' "${dry_run_json}")" "functional protected skip reason"
assert_equals "not_merged" "$(jq -r '.branches[] | select(.branch == "stale-unmerged") | .skip_reason' "${dry_run_json}")" "functional unmerged skip reason"
assert_non_empty "$(git ls-remote --heads origin stale-merged)" "functional dry-run remote branch retained"

# Regression C-02: execute without confirmation fails closed and keeps branch.
set +e
"${PRUNE_SCRIPT}" \
  --repo-root "${work_repo}" \
  --remote origin \
  --base-branch master \
  --min-age-days 1 \
  --protect-pattern "codex/*" \
  --generated-at "2026-02-20T00:00:00Z" \
  --execute \
  --output-json "${execute_json}" \
  --output-md "${execute_md}" \
  --quiet >/dev/null 2>&1
exit_code="$?"
set -e
if [[ "${exit_code}" -eq 0 ]]; then
  echo "assertion failed (regression delete guard): expected non-zero exit without confirmation" >&2
  exit 1
fi
assert_non_empty "$(git ls-remote --heads origin stale-merged)" "regression branch retained without confirmation"

# Integration C-03: explicit execute deletes only eligible branch and records rollback SHA.
"${PRUNE_SCRIPT}" \
  --repo-root "${work_repo}" \
  --remote origin \
  --base-branch master \
  --min-age-days 1 \
  --protect-pattern "codex/*" \
  --generated-at "2026-02-20T00:00:00Z" \
  --execute \
  --confirm-delete "DELETE_REMOTE_BRANCHES" \
  --output-json "${execute_json}" \
  --output-md "${execute_md}" \
  --quiet

assert_equals "1" "$(jq -r '.summary.deleted_count' "${execute_json}")" "integration deleted count"
assert_equals "deleted" "$(jq -r '.branches[] | select(.branch == "stale-merged") | .action' "${execute_json}")" "integration stale-merged deleted"
assert_equals "${candidate_tip_sha}" "$(jq -r '.branches[] | select(.branch == "stale-merged") | .tip_sha' "${execute_json}")" "integration rollback tip sha"

if [[ -n "$(git ls-remote --heads origin stale-merged)" ]]; then
  echo "assertion failed (integration stale-merged deletion): branch still present on remote" >&2
  exit 1
fi
assert_non_empty "$(git ls-remote --heads origin codex/protected-merged)" "integration protected branch retained"
assert_non_empty "$(git ls-remote --heads origin stale-unmerged)" "integration unmerged branch retained"

echo "stale-merged-branch-prune tests passed"
