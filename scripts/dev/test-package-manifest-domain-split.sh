#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

manifest_file="crates/tau-skills/src/package_manifest.rs"
manifest_dir="crates/tau-skills/src/package_manifest"

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

manifest_line_count="$(wc -l < "${manifest_file}" | tr -d ' ')"
if (( manifest_line_count >= 3200 )); then
  echo "assertion failed (line budget): expected ${manifest_file} < 3200 lines, got ${manifest_line_count}" >&2
  exit 1
fi

if [[ ! -d "${manifest_dir}" ]]; then
  echo "assertion failed (module dir): missing ${manifest_dir}" >&2
  exit 1
fi

manifest_contents="$(cat "${manifest_file}")"

assert_contains "${manifest_contents}" "mod schema;" "schema module marker"
assert_contains "${manifest_contents}" "mod validation;" "validation module marker"
assert_contains "${manifest_contents}" "mod io;" "io module marker"

assert_not_contains "${manifest_contents}" "struct PackageManifest {" "schema extraction marker"
assert_not_contains "${manifest_contents}" "fn validate_component_set(" "validation extraction marker"
assert_not_contains "${manifest_contents}" "fn resolve_component_source_path(" "io extraction marker"

for extracted_file in "schema.rs" "validation.rs" "io.rs"; do
  if [[ ! -f "${manifest_dir}/${extracted_file}" ]]; then
    echo "assertion failed (domain extraction file): missing ${manifest_dir}/${extracted_file}" >&2
    exit 1
  fi
done

echo "package-manifest-domain-split tests passed"
