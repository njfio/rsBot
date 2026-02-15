#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
MATRIX_JSON="${REPO_ROOT}/scripts/demo/m21-retained-capability-proof-matrix.json"
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: validate-m21-retained-capability-proof-matrix.sh [options]

Validate retained-capability live-proof matrix/checklist contracts.

Options:
  --repo-root <path>    Repository root (default: detected from script location).
  --matrix-json <path>  Matrix JSON path.
  --quiet               Suppress informational output.
  --help                Show this help text.
EOF
}

log_info() {
  if [[ "${QUIET_MODE}" != "true" ]]; then
    echo "$@"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo-root)
      REPO_ROOT="$2"
      shift 2
      ;;
    --matrix-json)
      MATRIX_JSON="$2"
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
      echo "error: unknown argument '$1'" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if ! command -v python3 >/dev/null 2>&1; then
  echo "error: required command 'python3' not found" >&2
  exit 1
fi

if [[ ! -f "${MATRIX_JSON}" ]]; then
  echo "error: matrix JSON not found: ${MATRIX_JSON}" >&2
  exit 1
fi

python3 - "${REPO_ROOT}" "${MATRIX_JSON}" "${QUIET_MODE}" <<'PY'
import json
import sys
from pathlib import Path

repo_root = Path(sys.argv[1]).resolve()
matrix_path = Path(sys.argv[2]).resolve()
quiet_mode = sys.argv[3] == "true"


def log_info(message: str) -> None:
    if not quiet_mode:
        print(message)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"error: {message}")


with matrix_path.open(encoding="utf-8") as handle:
    matrix = json.load(handle)

require(isinstance(matrix, dict), "matrix JSON must decode to an object")
require(matrix.get("schema_version") == 1, "schema_version must be 1")

required_top_level = {"artifact_checklist", "capabilities", "runs"}
missing = sorted(key for key in required_top_level if key not in matrix)
require(not missing, f"missing top-level keys: {', '.join(missing)}")

artifact_checklist = matrix["artifact_checklist"]
require(isinstance(artifact_checklist, dict), "artifact_checklist must be an object")
required_fields = artifact_checklist.get("required_fields")
require(
    required_fields == ["name", "path", "required", "status"],
    "artifact_checklist.required_fields must equal ['name', 'path', 'required', 'status']",
)
required_artifacts = artifact_checklist.get("required_artifacts")
require(
    isinstance(required_artifacts, list) and len(required_artifacts) > 0,
    "artifact_checklist.required_artifacts must be a non-empty array",
)
artifact_names = set()
for index, artifact in enumerate(required_artifacts):
    require(isinstance(artifact, dict), f"required_artifacts[{index}] must be an object")
    name = artifact.get("name")
    path_token = artifact.get("path_token")
    producer = artifact.get("producer")
    require(isinstance(name, str) and name.strip(), f"required_artifacts[{index}].name must be non-empty")
    require(
        isinstance(path_token, str) and path_token.strip(),
        f"required_artifacts[{index}].path_token must be non-empty",
    )
    require(
        isinstance(producer, str) and producer.strip(),
        f"required_artifacts[{index}].producer must be non-empty",
    )
    artifact_names.add(name.strip())

capabilities = matrix["capabilities"]
require(isinstance(capabilities, list) and len(capabilities) > 0, "capabilities must be a non-empty array")
runs = matrix["runs"]
require(isinstance(runs, list) and len(runs) > 0, "runs must be a non-empty array")

run_names = set()
for index, run in enumerate(runs):
    require(isinstance(run, dict), f"runs[{index}] must be an object")
    run_name = run.get("name")
    require(isinstance(run_name, str) and run_name.strip(), f"runs[{index}].name must be non-empty")
    require(run_name not in run_names, f"runs[{index}].name must be unique")
    run_names.add(run_name)
    command = run.get("command")
    require(isinstance(command, list) and len(command) > 0, f"runs[{index}].command must be a non-empty array")

for index, capability in enumerate(capabilities):
    require(isinstance(capability, dict), f"capabilities[{index}] must be an object")
    cap_id = capability.get("id")
    wrapper = capability.get("wrapper")
    proof_step = capability.get("proof_step")
    expected_markers = capability.get("expected_markers")
    required = capability.get("required_artifacts")
    require(isinstance(cap_id, str) and cap_id.strip(), f"capabilities[{index}].id must be non-empty")
    require(isinstance(wrapper, str) and wrapper.strip(), f"capabilities[{index}].wrapper must be non-empty")
    require(
        isinstance(proof_step, str) and proof_step.strip(),
        f"capabilities[{index}].proof_step must be non-empty",
    )
    require(
        isinstance(expected_markers, list) and len(expected_markers) > 0,
        f"capabilities[{index}].expected_markers must be a non-empty array",
    )
    require(
        isinstance(required, list) and len(required) > 0,
        f"capabilities[{index}].required_artifacts must be a non-empty array",
    )

    wrapper_path = repo_root / wrapper
    require(wrapper_path.is_file(), f"capabilities[{index}].wrapper does not exist: {wrapper_path}")
    require(
        proof_step in run_names,
        f"capabilities[{index}].proof_step '{proof_step}' does not map to any run name",
    )
    for artifact_name in required:
        require(
            isinstance(artifact_name, str) and artifact_name in artifact_names,
            f"capabilities[{index}] references unknown artifact '{artifact_name}'",
        )

log_info(
    "[retained-proof-matrix] validation passed: "
    f"capabilities={len(capabilities)} runs={len(runs)} required_artifacts={len(required_artifacts)}"
)
PY

log_info "retained-capability proof matrix contract is valid"
