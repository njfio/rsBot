#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
binary="${repo_root}/target/debug/tau-coding-agent"
skip_build="false"

print_usage() {
  cat <<EOF
Usage: all.sh [--repo-root PATH] [--binary PATH] [--skip-build] [--help]

Run all checked-in Tau demo wrappers (local/rpc/events/package) with aggregate summary output.

Options:
  --repo-root PATH  Repository root (defaults to caller-derived root)
  --binary PATH     tau-coding-agent binary path (default: <repo-root>/target/debug/tau-coding-agent)
  --skip-build      Skip cargo build and require --binary to exist
  --help            Show this usage message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo-root)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --repo-root" >&2
        print_usage >&2
        exit 2
      fi
      repo_root="$2"
      shift 2
      ;;
    --binary)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --binary" >&2
        print_usage >&2
        exit 2
      fi
      binary="$2"
      shift 2
      ;;
    --skip-build)
      skip_build="true"
      shift
      ;;
    --help)
      print_usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      print_usage >&2
      exit 2
      ;;
  esac
done

if [[ ! -d "${repo_root}" ]]; then
  echo "invalid --repo-root path (directory not found): ${repo_root}" >&2
  exit 2
fi
repo_root="$(cd "${repo_root}" && pwd)"

if [[ "${binary}" != /* ]]; then
  binary="${repo_root}/${binary}"
fi

if [[ "${skip_build}" == "true" && ! -f "${binary}" ]]; then
  echo "missing tau-coding-agent binary (use --binary or remove --skip-build): ${binary}" >&2
  exit 1
fi

demo_scripts=(
  "local.sh"
  "rpc.sh"
  "events.sh"
  "package.sh"
)

total=0
passed=0
failed=0

for demo_script in "${demo_scripts[@]}"; do
  total=$((total + 1))
  echo "[demo:all] [${total}] ${demo_script}"
  args=("${script_dir}/${demo_script}" "--repo-root" "${repo_root}" "--binary" "${binary}")
  if [[ "${skip_build}" == "true" ]]; then
    args+=("--skip-build")
  fi

  if "${args[@]}"; then
    passed=$((passed + 1))
    echo "[demo:all] PASS ${demo_script}"
  else
    failed=$((failed + 1))
    echo "[demo:all] FAIL ${demo_script}" >&2
  fi
done

echo "[demo:all] summary: total=${total} passed=${passed} failed=${failed}"
if [[ ${failed} -gt 0 ]]; then
  exit 1
fi
