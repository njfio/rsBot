#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

paths=(
  "$repo_root/ci-artifacts"
  "$repo_root/.github/scripts/__pycache__"
  "$repo_root/="
  "$repo_root/]"
)

for path in "${paths[@]}"; do
  label="${path#"$repo_root/"}"
  if [[ -e "$path" ]]; then
    rm -rf "$path"
    echo "removed: $label"
  else
    echo "absent: $label"
  fi
done

while IFS= read -r -d '' pyc_file; do
  rm -f "$pyc_file"
  echo "removed: ${pyc_file#"$repo_root/"}"
done < <(find "$repo_root/.github/scripts" -type f -name '*.pyc' -print0 2>/dev/null || true)

echo "cleanup complete"
