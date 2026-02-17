#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <tau-binary-path> <output-dir>" >&2
  exit 1
fi

tau_binary="$1"
output_dir="$2"

if [[ ! -x "${tau_binary}" ]]; then
  echo "error: tau binary is not executable: ${tau_binary}" >&2
  exit 1
fi

mkdir -p "${output_dir}"

"${tau_binary}" --shell-completion bash > "${output_dir}/tau-coding-agent.bash"
"${tau_binary}" --shell-completion zsh > "${output_dir}/tau-coding-agent.zsh"
"${tau_binary}" --shell-completion fish > "${output_dir}/tau-coding-agent.fish"

for completion_file in \
  "${output_dir}/tau-coding-agent.bash" \
  "${output_dir}/tau-coding-agent.zsh" \
  "${output_dir}/tau-coding-agent.fish"; do
  if [[ ! -s "${completion_file}" ]]; then
    echo "error: generated completion file is empty: ${completion_file}" >&2
    exit 1
  fi
done

echo "generated shell completions in ${output_dir}"
