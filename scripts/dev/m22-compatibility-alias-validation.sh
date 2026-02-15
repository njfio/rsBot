#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
OUTPUT_JSON="${REPO_ROOT}/tasks/reports/m22-compatibility-alias-validation.json"
OUTPUT_MD="${REPO_ROOT}/tasks/reports/m22-compatibility-alias-validation.md"
GENERATED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
FIXTURE_RESULTS_JSON=""
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: m22-compatibility-alias-validation.sh [options]

Run M22 legacy-alias compatibility validation checks and emit gate artifacts.

Options:
  --repo-root <path>            Repository root (default: auto-detected).
  --output-json <path>          Output JSON report path.
  --output-md <path>            Output Markdown report path.
  --generated-at <iso>          Override generated-at timestamp.
  --fixture-results-json <path> Use fixture command results JSON (skip live commands).
  --quiet                       Suppress informational output.
  --help                        Show this help text.
EOF
}

log_info() {
  if [[ "${QUIET_MODE}" != "true" ]]; then
    echo "$@"
  fi
}

fail() {
  echo "error: $*" >&2
  exit 1
}

require_cmd() {
  local name="$1"
  if ! command -v "${name}" >/dev/null 2>&1; then
    fail "required command '${name}' not found"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo-root)
      REPO_ROOT="$2"
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
    --fixture-results-json)
      FIXTURE_RESULTS_JSON="$2"
      shift 2
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
      fail "unknown option '$1'"
      ;;
  esac
done

require_cmd python3

if [[ ! -d "${REPO_ROOT}" ]]; then
  fail "repo root not found: ${REPO_ROOT}"
fi

mkdir -p "$(dirname "${OUTPUT_JSON}")" "$(dirname "${OUTPUT_MD}")"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

results_json="${tmp_dir}/results.json"

if [[ -n "${FIXTURE_RESULTS_JSON}" ]]; then
  if [[ ! -f "${FIXTURE_RESULTS_JSON}" ]]; then
    fail "fixture results JSON not found: ${FIXTURE_RESULTS_JSON}"
  fi
  cp "${FIXTURE_RESULTS_JSON}" "${results_json}"
else
  require_cmd cargo
  require_cmd jq

  commands_file="${tmp_dir}/commands.json"
  cat >"${commands_file}" <<'EOF'
[
  {
    "name": "legacy_train_alias",
    "cmd": "cargo test -p tau-coding-agent legacy_train_aliases_with_warning_snapshot"
  },
  {
    "name": "legacy_proxy_alias",
    "cmd": "cargo test -p tau-coding-agent legacy_training_aliases_with_warning_snapshot"
  },
  {
    "name": "unknown_flag_fail_closed",
    "cmd": "cargo test -p tau-coding-agent prompt_optimization_alias_normalization_keeps_unknown_flags_fail_closed"
  },
  {
    "name": "docs_policy_discoverability",
    "cmd": "python3 -m unittest discover -s .github/scripts -p 'test_docs_link_check.py'"
  }
]
EOF

  python3 - "${commands_file}" "${results_json}" "${REPO_ROOT}" <<'PY'
import json
import pathlib
import subprocess
import sys

commands_path, output_path, repo_root = sys.argv[1:]
commands = json.loads(pathlib.Path(commands_path).read_text(encoding="utf-8"))
results = {"commands": []}

for entry in commands:
    cmd = entry["cmd"]
    completed = subprocess.run(
        ["bash", "-lc", cmd],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=False,
    )
    excerpt = (completed.stdout + "\n" + completed.stderr).strip()
    if len(excerpt) > 600:
        excerpt = excerpt[:600] + " ...[truncated]"
    results["commands"].append(
        {
            "name": entry["name"],
            "cmd": cmd,
            "status": "pass" if completed.returncode == 0 else "fail",
            "stdout_excerpt": excerpt,
        }
    )

pathlib.Path(output_path).write_text(
    json.dumps(results, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY
fi

python3 - \
  "${results_json}" \
  "${OUTPUT_JSON}" \
  "${OUTPUT_MD}" \
  "${GENERATED_AT}" \
  "${REPO_ROOT}" <<'PY'
import json
import pathlib
import sys

results_path, output_json_path, output_md_path, generated_at, repo_root = sys.argv[1:]
raw = json.loads(pathlib.Path(results_path).read_text(encoding="utf-8"))
commands = raw.get("commands", [])

total = len(commands)
passed = sum(1 for entry in commands if entry.get("status") == "pass")
failed = total - passed

payload = {
    "schema_version": 1,
    "generated_at": generated_at,
    "repo_root": repo_root,
    "commands": commands,
    "summary": {
        "total": total,
        "passed": passed,
        "failed": failed,
    },
}

output_json = pathlib.Path(output_json_path)
output_json.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")

lines = [
    "# M22 Compatibility Alias Validation",
    "",
    f"- Generated at: `{generated_at}`",
    f"- Repo root: `{repo_root}`",
    "",
    "## Summary",
    "",
    f"- Total checks: `{total}`",
    f"- Passed: `{passed}`",
    f"- Failed: `{failed}`",
    "",
    "## Command Results",
    "",
    "| Name | Status | Command |",
    "| --- | --- | --- |",
]
for entry in commands:
    lines.append(
        f"| {entry.get('name', '')} | {entry.get('status', '')} | `{entry.get('cmd', '')}` |"
    )

lines.extend(
    [
        "",
        "## Migration Policy",
        "",
        "Use canonical `--prompt-optimization-*` flags for all new automation.",
        "Legacy `--train-*` and `--training-proxy-*` aliases remain temporary compatibility paths and emit deprecation warnings.",
    ]
)

output_md = pathlib.Path(output_md_path)
output_md.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY

summary_failed="$(python3 - "${OUTPUT_JSON}" <<'PY'
import json
import sys
payload = json.loads(open(sys.argv[1], encoding="utf-8").read())
print(payload["summary"]["failed"])
PY
)"

log_info "wrote JSON report: ${OUTPUT_JSON}"
log_info "wrote Markdown report: ${OUTPUT_MD}"

if [[ "${summary_failed}" != "0" ]]; then
  fail "validation reported failing commands (failed=${summary_failed})"
fi
