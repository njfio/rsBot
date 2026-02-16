#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

lib_file="crates/tau-skills/src/lib.rs"
load_file="crates/tau-skills/src/load_registry.rs"
trust_file="crates/tau-skills/src/trust_policy.rs"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" != *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected to find '${needle}'" >&2
    exit 1
  fi
}

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "${haystack}" == *"${needle}"* ]]; then
    echo "assertion failed (${label}): expected to NOT find '${needle}'" >&2
    exit 1
  fi
}

line_count="$(wc -l < "${lib_file}" | tr -d ' ')"
if (( line_count >= 1800 )); then
  echo "assertion failed (line budget): expected ${lib_file} < 1800 lines, got ${line_count}" >&2
  exit 1
fi

for file in "${load_file}" "${trust_file}"; do
  if [[ ! -f "${file}" ]]; then
    echo "assertion failed (module file): missing ${file}" >&2
    exit 1
  fi
done

lib_contents="$(cat "${lib_file}")"

assert_contains "${lib_contents}" "mod load_registry;" "module marker: load registry"
assert_contains "${lib_contents}" "mod trust_policy;" "module marker: trust policy"
assert_contains "${lib_contents}" "pub fn load_catalog(" "public api: load_catalog"
assert_contains "${lib_contents}" "pub fn resolve_registry_skill_sources(" "public api: resolve_registry_skill_sources"

assert_not_contains "${lib_contents}" "fn fetch_remote_skill_bytes(" "moved helper: remote fetch"
assert_not_contains "${lib_contents}" "fn validate_skills_lockfile(" "moved helper: lockfile validation"
assert_not_contains "${lib_contents}" "fn build_trusted_key_map(" "moved helper: trust map"
assert_not_contains "${lib_contents}" "fn verify_ed25519_signature(" "moved helper: signature verify"

echo "skills-lib-domain-split tests passed"
