#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

REPO_ROOT="${DEFAULT_REPO_ROOT}"
TARGETS_FILE="docs/guides/doc-density-targets.json"
DENSITY_SCRIPT=".github/scripts/rust_doc_density.py"
OUTPUT_JSON="${DEFAULT_REPO_ROOT}/tasks/reports/m23-doc-density-gate-artifact.json"
OUTPUT_MD="${DEFAULT_REPO_ROOT}/tasks/reports/m23-doc-density-gate-artifact.md"
GENERATED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: doc-density-gate-artifact.sh [options]

Generate reproducible M23 doc-density gate artifacts (JSON + Markdown) with
captured command/version/context metadata.

Options:
  --repo-root <path>      Repository root (default: auto-detected).
  --targets-file <path>   Targets JSON path relative to repo root (default: docs/guides/doc-density-targets.json).
  --density-script <path> rust_doc_density.py path relative to repo root (default: .github/scripts/rust_doc_density.py).
  --output-json <path>    Output JSON artifact path.
  --output-md <path>      Output Markdown artifact path.
  --generated-at <iso>    Override generated-at timestamp (UTC ISO-8601).
  --quiet                 Suppress informational output.
  --help                  Show this help text.
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

resolve_path() {
  local base="$1"
  local path="$2"
  if [[ "${path}" = /* ]]; then
    printf '%s\n' "${path}"
  else
    printf '%s\n' "${base}/${path}"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo-root)
      REPO_ROOT="$2"
      shift 2
      ;;
    --targets-file)
      TARGETS_FILE="$2"
      shift 2
      ;;
    --density-script)
      DENSITY_SCRIPT="$2"
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
require_cmd git

if [[ ! -d "${REPO_ROOT}" ]]; then
  fail "repo root not found: ${REPO_ROOT}"
fi

TARGETS_FILE_ABS="$(resolve_path "${REPO_ROOT}" "${TARGETS_FILE}")"
DENSITY_SCRIPT_ABS="$(resolve_path "${REPO_ROOT}" "${DENSITY_SCRIPT}")"

if [[ ! -f "${TARGETS_FILE_ABS}" ]]; then
  fail "targets file not found: ${TARGETS_FILE_ABS}"
fi
if [[ ! -f "${DENSITY_SCRIPT_ABS}" ]]; then
  fail "density script not found: ${DENSITY_SCRIPT_ABS}"
fi

mkdir -p "$(dirname "${OUTPUT_JSON}")"
mkdir -p "$(dirname "${OUTPUT_MD}")"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

density_json_path="${tmp_dir}/density.json"
density_stderr_path="${tmp_dir}/density.stderr"

rendered_command="python3 ${DENSITY_SCRIPT} --repo-root ${REPO_ROOT} --targets-file ${TARGETS_FILE} --json"
log_info "running: ${rendered_command}"

set +e
python3 "${DENSITY_SCRIPT_ABS}" \
  --repo-root "${REPO_ROOT}" \
  --targets-file "${TARGETS_FILE}" \
  --json >"${density_json_path}" 2>"${density_stderr_path}"
density_exit=$?
set -e

if [[ "${density_exit}" -ne 0 ]]; then
  cat "${density_stderr_path}" >&2
  fail "rust doc density command failed"
fi

python3 - "${density_json_path}" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
with path.open(encoding="utf-8") as handle:
    json.load(handle)
PY

python3_version="$(python3 --version 2>&1 | head -n1)"
if command -v rustc >/dev/null 2>&1; then
  rustc_version="$(rustc --version 2>&1 | head -n1)"
else
  rustc_version="unavailable"
fi
if command -v cargo >/dev/null 2>&1; then
  cargo_version="$(cargo --version 2>&1 | head -n1)"
else
  cargo_version="unavailable"
fi
if command -v gh >/dev/null 2>&1; then
  gh_version="$(gh --version 2>&1 | head -n1)"
else
  gh_version="unavailable"
fi
if command -v jq >/dev/null 2>&1; then
  jq_version="$(jq --version 2>&1 | head -n1)"
else
  jq_version="unavailable"
fi

os_context="$(uname -srm 2>/dev/null || true)"
if [[ -z "${os_context}" ]]; then
  os_context="unknown"
fi

git_commit="$(git -C "${REPO_ROOT}" rev-parse HEAD 2>/dev/null || true)"
if [[ -z "${git_commit}" ]]; then
  git_commit="unknown"
fi
git_branch="$(git -C "${REPO_ROOT}" rev-parse --abbrev-ref HEAD 2>/dev/null || true)"
if [[ -z "${git_branch}" ]]; then
  git_branch="unknown"
fi
git_dirty_count="$(git -C "${REPO_ROOT}" status --porcelain 2>/dev/null | wc -l | tr -d '[:space:]' || true)"
if [[ -z "${git_dirty_count}" ]]; then
  git_dirty_count="0"
fi
if [[ "${git_dirty_count}" == "0" ]]; then
  git_dirty="false"
else
  git_dirty="true"
fi

python3 - \
  "${density_json_path}" \
  "${OUTPUT_JSON}" \
  "${OUTPUT_MD}" \
  "${GENERATED_AT}" \
  "${REPO_ROOT}" \
  "${DENSITY_SCRIPT}" \
  "${TARGETS_FILE}" \
  "${rendered_command}" \
  "${python3_version}" \
  "${rustc_version}" \
  "${cargo_version}" \
  "${gh_version}" \
  "${jq_version}" \
  "${os_context}" \
  "${git_commit}" \
  "${git_branch}" \
  "${git_dirty}" <<'PY'
import json
import pathlib
import sys

(
    density_json_path,
    output_json_path,
    output_md_path,
    generated_at,
    repo_root,
    density_script,
    targets_file,
    rendered_command,
    python3_version,
    rustc_version,
    cargo_version,
    gh_version,
    jq_version,
    os_context,
    git_commit,
    git_branch,
    git_dirty,
) = sys.argv[1:]

with open(density_json_path, encoding="utf-8") as handle:
    density_report = json.load(handle)

payload = {
    "schema_version": 1,
    "generated_at": generated_at,
    "repo_root": repo_root,
    "command": {
        "script": density_script,
        "targets_file": targets_file,
        "rendered": rendered_command,
    },
    "versions": {
        "python3": python3_version,
        "rustc": rustc_version,
        "cargo": cargo_version,
        "gh": gh_version,
        "jq": jq_version,
    },
    "context": {
        "os": os_context,
        "git_commit": git_commit,
        "git_branch": git_branch,
        "git_dirty": git_dirty == "true",
    },
    "density_report": density_report,
    "troubleshooting": [
        {
            "id": "targets-mismatch",
            "symptom": "density output differs from CI artifact",
            "action": "verify targets file path and compare rendered command with CI logs",
        },
        {
            "id": "tool-version-drift",
            "symptom": "unexpected parser or formatting behavior",
            "action": "compare python/rustc/cargo versions in artifact context and rerun with aligned toolchain",
        },
        {
            "id": "workspace-drift",
            "symptom": "local run differs from gate branch output",
            "action": "confirm git commit and dirty state, then rerun on the gate commit",
        },
    ],
}

output_json = pathlib.Path(output_json_path)
output_json.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")

overall_public = density_report.get("overall_public_items", 0)
overall_documented = density_report.get("overall_documented_items", 0)
overall_percent = density_report.get("overall_percent", 0.0)
reports = density_report.get("reports", [])

lines = [
    "# M23 Doc Density Gate Artifact",
    "",
    f"- Generated at: `{generated_at}`",
    f"- Repo root: `{repo_root}`",
    "",
    "## Command",
    "",
    "```bash",
    rendered_command,
    "```",
    "",
    "## Versions",
    "",
    "| Tool | Version |",
    "| --- | --- |",
    f"| python3 | `{python3_version}` |",
    f"| rustc | `{rustc_version}` |",
    f"| cargo | `{cargo_version}` |",
    f"| gh | `{gh_version}` |",
    f"| jq | `{jq_version}` |",
    "",
    "## Context",
    "",
    "| Field | Value |",
    "| --- | --- |",
    f"| OS | `{os_context}` |",
    f"| Git commit | `{git_commit}` |",
    f"| Git branch | `{git_branch}` |",
    f"| Git dirty | `{git_dirty}` |",
    "",
    "## Summary",
    "",
    f"- Overall documented/public: `{overall_documented}/{overall_public}`",
    f"- Overall percent: `{overall_percent}%`",
    f"- Issues reported by density checker: `{len(density_report.get('issues', []))}`",
    "",
    "## Crate Breakdown",
    "",
    "| Crate | Documented | Public | Percent |",
    "| --- | ---: | ---: | ---: |",
]

for row in reports:
    lines.append(
        "| {crate} | {documented} | {total} | {percent}% |".format(
            crate=row.get("crate", ""),
            documented=row.get("documented_public_items", 0),
            total=row.get("total_public_items", 0),
            percent=row.get("percent", 0.0),
        )
    )

lines.extend(
    [
        "",
        "## Troubleshooting",
        "",
        "1. Compare the rendered command and `targets_file` field with CI to rule out mismatched thresholds.",
        "2. Compare tool versions (`python3`, `rustc`, `cargo`) when count output changes unexpectedly.",
        "3. Re-run on a clean worktree at the same commit when local dirty state is `true`.",
        "",
        "## Reproduction Command",
        "",
        "```bash",
        rendered_command,
        "```",
    ]
)

output_md = pathlib.Path(output_md_path)
output_md.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY

log_info "wrote JSON artifact: ${OUTPUT_JSON}"
log_info "wrote Markdown artifact: ${OUTPUT_MD}"
