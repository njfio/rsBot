#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

CANDIDATES_JSON="${REPO_ROOT}/tasks/reports/m21-scaffold-merge-remove-decision-matrix.json"
OUTPUT_JSON="${REPO_ROOT}/tasks/reports/m21-scaffold-inventory.json"
OUTPUT_MD="${REPO_ROOT}/tasks/reports/m21-scaffold-inventory.md"
SCHEMA_PATH="${REPO_ROOT}/tasks/schemas/m21-scaffold-inventory.schema.json"
GENERATED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
FIXTURE_CANDIDATES_JSON=""
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: scaffold-inventory.sh [options]

Generate deterministic scaffold-risk inventory and ownership-map artifacts.

Options:
  --repo-root <path>                Repository root (default: detected from script location).
  --candidates-json <path>          Candidate source JSON path (default: tasks/reports/m21-scaffold-merge-remove-decision-matrix.json).
  --output-json <path>              JSON inventory output path.
  --output-md <path>                Markdown inventory output path.
  --schema-path <path>              Schema path reference embedded in output JSON.
  --generated-at <iso>              Override generated timestamp.
  --fixture-candidates-json <path>  Optional fixture candidate JSON (for tests/regression checks).
  --quiet                           Suppress informational logs.
  --help                            Show this help text.
EOF
}

log_info() {
  if [[ "${QUIET_MODE}" != "true" ]]; then
    echo "$@"
  fi
}

resolve_path() {
  local repo_root="$1"
  local raw="$2"
  if [[ "${raw}" = /* ]]; then
    printf '%s\n' "${raw}"
  else
    printf '%s\n' "${repo_root}/${raw}"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo-root)
      REPO_ROOT="$2"
      shift 2
      ;;
    --candidates-json)
      CANDIDATES_JSON="$2"
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
    --schema-path)
      SCHEMA_PATH="$2"
      shift 2
      ;;
    --generated-at)
      GENERATED_AT="$2"
      shift 2
      ;;
    --fixture-candidates-json)
      FIXTURE_CANDIDATES_JSON="$2"
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

if [[ -n "${FIXTURE_CANDIDATES_JSON}" && ! -f "${FIXTURE_CANDIDATES_JSON}" ]]; then
  echo "error: fixture candidates JSON not found: ${FIXTURE_CANDIDATES_JSON}" >&2
  exit 1
fi

CANDIDATES_JSON_ABS="$(resolve_path "${REPO_ROOT}" "${CANDIDATES_JSON}")"
OUTPUT_JSON_ABS="$(resolve_path "${REPO_ROOT}" "${OUTPUT_JSON}")"
OUTPUT_MD_ABS="$(resolve_path "${REPO_ROOT}" "${OUTPUT_MD}")"
SCHEMA_PATH_ABS="$(resolve_path "${REPO_ROOT}" "${SCHEMA_PATH}")"

if [[ ! -f "${CANDIDATES_JSON_ABS}" && -z "${FIXTURE_CANDIDATES_JSON}" ]]; then
  echo "error: candidates JSON not found: ${CANDIDATES_JSON_ABS}" >&2
  exit 1
fi

mkdir -p "$(dirname "${OUTPUT_JSON_ABS}")" "$(dirname "${OUTPUT_MD_ABS}")"

python3 - \
  "${REPO_ROOT}" \
  "${CANDIDATES_JSON_ABS}" \
  "${OUTPUT_JSON_ABS}" \
  "${OUTPUT_MD_ABS}" \
  "${SCHEMA_PATH_ABS}" \
  "${GENERATED_AT}" \
  "${FIXTURE_CANDIDATES_JSON}" <<'PY'
import json
import re
import sys
from pathlib import Path
from typing import Any

(
    repo_root_raw,
    candidates_json_raw,
    output_json_raw,
    output_md_raw,
    schema_path_raw,
    generated_at,
    fixture_candidates_json_raw,
) = sys.argv[1:]

repo_root = Path(repo_root_raw).resolve()
candidates_json = Path(candidates_json_raw).resolve()
output_json = Path(output_json_raw).resolve()
output_md = Path(output_md_raw).resolve()
schema_path = Path(schema_path_raw).resolve()
fixture_candidates_json = (
    Path(fixture_candidates_json_raw).resolve() if fixture_candidates_json_raw else None
)

runtime_roots = [
    repo_root / "crates/tau-runtime/src",
    repo_root / "crates/tau-agent-core/src",
    repo_root / "crates/tau-tools/src",
    repo_root / "crates/tau-gateway/src",
    repo_root / "crates/tau-cli/src",
    repo_root / "crates/tau-multi-channel/src",
]
test_roots = list((repo_root / "crates").glob("*/tests"))


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"error: {message}")


def rel_to_repo(path: Path | None) -> str | None:
    if path is None:
        return None
    if path.is_relative_to(repo_root):
        return str(path.relative_to(repo_root))
    return str(path)


def load_json(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as handle:
        payload = json.load(handle)
    require(isinstance(payload, dict), f"{path} must decode to JSON object")
    return payload


def load_candidates() -> tuple[list[dict[str, Any]], Path]:
    source = fixture_candidates_json or candidates_json
    payload = load_json(source)
    candidates = payload.get("candidates")
    require(isinstance(candidates, list) and candidates, "candidates[] must be non-empty array")
    return candidates, source


def parse_crate_name(surface: str) -> str | None:
    if not surface.startswith("crate:"):
        return None
    remainder = surface.split("crate:", 1)[1].strip()
    if not remainder:
        return None
    return re.split(r"[ (\t]", remainder, maxsplit=1)[0].strip() or None


def gather_rs_files(root: Path) -> list[Path]:
    if not root.exists():
        return []
    return sorted(path for path in root.rglob("*.rs") if path.is_file())


def count_hits(files: list[Path], tokens: list[str]) -> int:
    if not tokens:
        return 0
    total = 0
    for file_path in files:
        text = file_path.read_text(encoding="utf-8", errors="ignore")
        for token in tokens:
            total += text.count(token)
    return total


def normalize_candidate(entry: dict[str, Any], index: int) -> dict[str, Any]:
    require(isinstance(entry, dict), f"candidates[{index}] must be object")
    candidate_id = entry.get("candidate_id")
    surface = entry.get("surface")
    owner = entry.get("owner")
    action = entry.get("action", "unknown")

    require(isinstance(candidate_id, str) and candidate_id.strip(), f"candidates[{index}].candidate_id must be non-empty")
    require(isinstance(surface, str) and surface.strip(), f"candidates[{index}].surface must be non-empty")
    require(isinstance(owner, str) and owner.strip(), f"candidates[{index}].owner must be non-empty")
    require(isinstance(action, str) and action.strip(), f"candidates[{index}].action must be non-empty")

    crate_name = parse_crate_name(surface.strip())
    crate_path = (repo_root / "crates" / crate_name) if crate_name else None
    crate_exists = bool(crate_path and crate_path.is_dir())
    src_files = gather_rs_files(crate_path / "src") if crate_exists and crate_path else []
    src_loc = sum(len(path.read_text(encoding="utf-8", errors="ignore").splitlines()) for path in src_files)

    search_tokens = {candidate_id.strip()}
    if crate_name:
        search_tokens.add(crate_name)
        search_tokens.add(crate_name.replace("-", "_"))
    tokens = sorted(token for token in search_tokens if token)

    runtime_files: list[Path] = []
    for root in runtime_roots:
        runtime_files.extend(gather_rs_files(root))
    runtime_reference_hits = count_hits(runtime_files, tokens)

    test_files: list[Path] = []
    for root in test_roots:
        test_files.extend(gather_rs_files(root))
    test_touchpoint_hits = count_hits(test_files, tokens)

    return {
        "candidate_id": candidate_id.strip(),
        "surface": surface.strip(),
        "owner": owner.strip(),
        "action": action.strip(),
        "crate_name": crate_name,
        "crate_path": rel_to_repo(crate_path) if crate_path else None,
        "crate_exists": crate_exists,
        "src_rust_files": len(src_files),
        "src_rust_loc": src_loc,
        "runtime_reference_hits": runtime_reference_hits,
        "test_touchpoint_hits": test_touchpoint_hits,
    }


candidates_raw, source_path = load_candidates()
normalized = [normalize_candidate(candidate, index) for index, candidate in enumerate(candidates_raw)]
normalized.sort(key=lambda item: item["candidate_id"])

seen = set()
for candidate in normalized:
    candidate_id = candidate["candidate_id"]
    require(candidate_id not in seen, f"duplicate candidate_id '{candidate_id}'")
    seen.add(candidate_id)

missing_owner_count = sum(1 for candidate in normalized if not candidate["owner"])
existing_crate_count = sum(1 for candidate in normalized if candidate["crate_exists"])
runtime_hits_total = sum(candidate["runtime_reference_hits"] for candidate in normalized)
test_hits_total = sum(candidate["test_touchpoint_hits"] for candidate in normalized)

payload = {
    "schema_version": 1,
    "generated_at": generated_at,
    "repository_root": ".",
    "schema_path": rel_to_repo(schema_path),
    "source_candidates_path": rel_to_repo(source_path),
    "summary": {
        "total_candidates": len(normalized),
        "existing_crate_count": existing_crate_count,
        "missing_owner_count": missing_owner_count,
        "total_runtime_reference_hits": runtime_hits_total,
        "total_test_touchpoint_hits": test_hits_total,
    },
    "candidates": normalized,
}

output_json.parent.mkdir(parents=True, exist_ok=True)
output_md.parent.mkdir(parents=True, exist_ok=True)
output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

lines = [
    "# M21 Scaffold Inventory and Ownership Map",
    "",
    f"- Generated: {generated_at}",
    f"- Source candidates: `{payload['source_candidates_path']}`",
    f"- Schema: `{payload['schema_path']}`",
    "",
    "## Summary",
    "",
    "| Metric | Value |",
    "| --- | ---: |",
    f"| Total candidates | {payload['summary']['total_candidates']} |",
    f"| Existing crate paths | {payload['summary']['existing_crate_count']} |",
    f"| Missing owners | {payload['summary']['missing_owner_count']} |",
    f"| Runtime reference hits | {payload['summary']['total_runtime_reference_hits']} |",
    f"| Test touchpoint hits | {payload['summary']['total_test_touchpoint_hits']} |",
    "",
    "## Inventory",
    "",
    "| Candidate | Action | Owner | Crate Path | Exists | Rust Files | Rust LOC | Runtime Hits | Test Hits |",
    "| --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: |",
]

for candidate in normalized:
    crate_path = candidate["crate_path"] if candidate["crate_path"] else "-"
    crate_exists = "yes" if candidate["crate_exists"] else "no"
    lines.append(
        "| "
        f"`{candidate['candidate_id']}` | {candidate['action']} | `{candidate['owner']}` | "
        f"`{crate_path}` | {crate_exists} | {candidate['src_rust_files']} | "
        f"{candidate['src_rust_loc']} | {candidate['runtime_reference_hits']} | "
        f"{candidate['test_touchpoint_hits']} |"
    )

lines.extend(
    [
        "",
        "## Update Instructions",
        "",
        "Regenerate inventory artifacts with:",
        "",
        "```bash",
        "scripts/dev/scaffold-inventory.sh",
        "```",
    ]
)

output_md.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY

log_info "wrote scaffold inventory artifacts:"
log_info "  JSON: ${OUTPUT_JSON_ABS}"
log_info "  Markdown: ${OUTPUT_MD_ABS}"
