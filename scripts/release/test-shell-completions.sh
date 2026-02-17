#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
cd "${REPO_ROOT}"

GENERATOR_SCRIPT="scripts/release/generate-shell-completions.sh"

assert_file_exists() {
  local file_path="$1"
  if [[ ! -f "${file_path}" ]]; then
    echo "assertion failed: expected file to exist: ${file_path}" >&2
    exit 1
  fi
}

assert_contains_file() {
  local file_path="$1"
  local needle="$2"
  local label="$3"
  if ! grep -Fq "${needle}" "${file_path}"; then
    echo "assertion failed (${label}): '${needle}' not found in ${file_path}" >&2
    exit 1
  fi
}

temp_dir="$(mktemp -d)"
trap 'rm -rf "${temp_dir}"' EXIT

mock_binary="${temp_dir}/mock-tau.sh"
output_dir="${temp_dir}/completions"

cat > "${mock_binary}" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ $# -ne 2 || "$1" != "--shell-completion" ]]; then
  echo "unexpected args: $*" >&2
  exit 1
fi
printf 'completion-%s\n' "$2"
EOF

chmod +x "${mock_binary}"

"${GENERATOR_SCRIPT}" "${mock_binary}" "${output_dir}"

assert_file_exists "${output_dir}/tau-coding-agent.bash"
assert_file_exists "${output_dir}/tau-coding-agent.zsh"
assert_file_exists "${output_dir}/tau-coding-agent.fish"

assert_contains_file "${output_dir}/tau-coding-agent.bash" "completion-bash" "bash output"
assert_contains_file "${output_dir}/tau-coding-agent.zsh" "completion-zsh" "zsh output"
assert_contains_file "${output_dir}/tau-coding-agent.fish" "completion-fish" "fish output"

echo "shell completion generation contract tests passed"
