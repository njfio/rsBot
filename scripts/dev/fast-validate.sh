#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
cd "${REPO_ROOT}"

FULL_MODE="false"
BASE_REF=""
PRINT_PACKAGES_FROM_STDIN="false"

usage() {
  cat <<'EOF'
Usage: fast-validate.sh [--full] [--base <git-ref>] [--print-packages-from-stdin]

Fast validation defaults to impacted-package scope:
  - cargo fmt --all -- --check
  - cargo clippy -p <impacted crates> --all-targets --all-features -- -D warnings
  - cargo test -p <impacted crates>

Options:
  --full                        Run full workspace clippy + tests.
  --base <git-ref>              Compare changes against this base ref.
  --print-packages-from-stdin   Internal/test mode: read newline-delimited paths from stdin
                                and print derived impacted package scope.
  --help                        Show this message.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --full)
      FULL_MODE="true"
      ;;
    --base)
      shift
      if [[ $# -eq 0 ]]; then
        echo "error: --base requires a git ref" >&2
        exit 1
      fi
      BASE_REF="$1"
      ;;
    --print-packages-from-stdin)
      PRINT_PACKAGES_FROM_STDIN="true"
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

resolve_default_base_ref() {
  if [[ -n "${BASE_REF}" ]]; then
    echo "${BASE_REF}"
    return
  fi

  if git rev-parse --verify origin/main >/dev/null 2>&1; then
    git merge-base HEAD origin/main
    return
  fi

  if git rev-parse --verify origin/master >/dev/null 2>&1; then
    git merge-base HEAD origin/master
    return
  fi

  if git rev-parse --verify HEAD~1 >/dev/null 2>&1; then
    echo "HEAD~1"
    return
  fi

  echo "HEAD"
}

collect_changed_files() {
  local base_ref="$1"
  local base_available="true"

  if ! git rev-parse --verify "${base_ref}^{commit}" >/dev/null 2>&1; then
    base_available="false"
    echo "warning: base ref '${base_ref}' not available; forcing full workspace scope" >&2
  fi

  {
    if [[ "${base_available}" == "true" ]]; then
      git diff --name-only "${base_ref}...HEAD" || true
    else
      echo "Cargo.toml"
    fi
    git diff --name-only || true
    git diff --name-only --cached || true
    git ls-files --others --exclude-standard || true
  } | awk 'NF' | sort -u
}

derive_scope_from_files() {
  local full_workspace="0"
  declare -A package_map=()

  for file in "$@"; do
    case "${file}" in
      Cargo.toml|Cargo.lock|rust-toolchain|rust-toolchain.toml|.github/workflows/*)
        full_workspace="1"
        ;;
    esac

    if [[ "${file}" =~ ^crates/([^/]+)/ ]]; then
      local crate_dir="${BASH_REMATCH[1]}"
      local crate_manifest="crates/${crate_dir}/Cargo.toml"
      local package_name=""
      if [[ -f "${crate_manifest}" ]]; then
        package_name="$(sed -n 's/^name = "\(.*\)"/\1/p' "${crate_manifest}" | head -n 1)"
      fi
      if [[ -z "${package_name}" ]]; then
        package_name="${crate_dir//_/-}"
      fi
      package_map["${package_name}"]="1"
    fi
  done

  echo "${full_workspace}"
  if [[ ${#package_map[@]} -gt 0 ]]; then
    printf '%s\n' "${!package_map[@]}" | sort
  fi
}

expand_impacted_packages() {
  if [[ $# -eq 0 ]]; then
    return 0
  fi

  local seed_csv
  seed_csv="$(IFS=,; echo "$*")"

  python3 - "${seed_csv}" <<'PY'
import json
import subprocess
import sys
from collections import deque

seeds = [seed for seed in sys.argv[1].split(",") if seed]
if not seeds:
    raise SystemExit(0)

try:
    metadata = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        check=True,
        capture_output=True,
        text=True,
    )
except Exception:
    for package in sorted(set(seeds)):
        print(package)
    raise SystemExit(0)

parsed = json.loads(metadata.stdout)
workspace_ids = set(parsed.get("workspace_members", []))
id_to_package = {package["id"]: package for package in parsed.get("packages", [])}
workspace_names = set()

for package_id in workspace_ids:
    package = id_to_package.get(package_id)
    if package:
        workspace_names.add(package["name"])

reverse_dependencies = {name: set() for name in workspace_names}

for package_id in workspace_ids:
    package = id_to_package.get(package_id)
    if not package:
        continue
    package_name = package["name"]
    for dependency in package.get("dependencies", []):
        dep_name = dependency.get("name")
        if dep_name in workspace_names:
            reverse_dependencies.setdefault(dep_name, set()).add(package_name)

visited = {seed for seed in seeds if seed in workspace_names}
queue = deque(visited)

while queue:
    current = queue.popleft()
    for dependent in reverse_dependencies.get(current, set()):
        if dependent not in visited:
            visited.add(dependent)
            queue.append(dependent)

if not visited:
    visited = set(seeds)

for package in sorted(visited):
    print(package)
PY
}

print_scope() {
  local full_workspace="$1"
  shift
  echo "full_workspace=${full_workspace}"
  for pkg in "$@"; do
    echo "package=${pkg}"
  done
}

run_step() {
  local label="$1"
  shift
  local started_at
  started_at="$(date +%s)"
  echo "==> ${label}: $*"
  "$@"
  local finished_at
  finished_at="$(date +%s)"
  echo "<== ${label}: $((finished_at - started_at))s"
}

if [[ "${PRINT_PACKAGES_FROM_STDIN}" == "true" ]]; then
  mapfile -t stdin_files
  mapfile -t scope < <(derive_scope_from_files "${stdin_files[@]}")
  full_workspace="${scope[0]:-0}"
  packages=("${scope[@]:1}")
  if [[ "${full_workspace}" != "1" && ${#packages[@]} -gt 0 ]]; then
    mapfile -t packages < <(expand_impacted_packages "${packages[@]}")
  fi
  print_scope "${full_workspace}" "${packages[@]}"
  exit 0
fi

BASE="$(resolve_default_base_ref)"
mapfile -t CHANGED_FILES < <(collect_changed_files "${BASE}")
mapfile -t SCOPE < <(derive_scope_from_files "${CHANGED_FILES[@]}")
FULL_WORKSPACE_FROM_SCOPE="${SCOPE[0]:-0}"
PACKAGES=("${SCOPE[@]:1}")
if [[ "${FULL_MODE}" != "true" && "${FULL_WORKSPACE_FROM_SCOPE}" != "1" && ${#PACKAGES[@]} -gt 0 ]]; then
  mapfile -t PACKAGES < <(expand_impacted_packages "${PACKAGES[@]}")
fi

echo "fast-validate base=${BASE} changed_files=${#CHANGED_FILES[@]} impacted_packages=${#PACKAGES[@]}"

run_step "fmt-check" cargo fmt --all -- --check

if [[ "${FULL_MODE}" == "true" || "${FULL_WORKSPACE_FROM_SCOPE}" == "1" ]]; then
  echo "running full workspace validation"
  run_step "clippy-workspace" cargo clippy --workspace --all-targets --all-features -- -D warnings
  run_step "test-workspace" cargo test --workspace
  exit 0
fi

if [[ ${#PACKAGES[@]} -eq 0 ]]; then
  echo "no crate changes detected; fmt check completed"
  exit 0
fi

PACKAGE_ARGS=()
for pkg in "${PACKAGES[@]}"; do
  PACKAGE_ARGS+=("-p" "${pkg}")
done

echo "running package-scoped validation for: ${PACKAGES[*]}"
run_step "clippy-packages" cargo clippy "${PACKAGE_ARGS[@]}" --all-targets --all-features -- -D warnings
run_step "test-packages" cargo test "${PACKAGE_ARGS[@]}"
