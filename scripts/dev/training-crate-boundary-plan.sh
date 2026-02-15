#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

OUTPUT_JSON="${REPO_ROOT}/tasks/reports/training-crate-boundary-plan.json"
OUTPUT_MD="${REPO_ROOT}/tasks/reports/training-crate-boundary-plan.md"
SCHEMA_PATH="${REPO_ROOT}/tasks/schemas/training-crate-boundary-plan.schema.json"
GENERATED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
FIXTURE_PLAN_JSON=""
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: training-crate-boundary-plan.sh [options]

Generate a machine-readable training crate boundary decision plan for M21.

Options:
  --repo-root <path>           Repository root (default: detected from script location).
  --output-json <path>         JSON plan output path.
  --output-md <path>           Markdown plan output path.
  --schema-path <path>         Schema path reference embedded in output JSON.
  --generated-at <iso>         Override generated timestamp.
  --fixture-plan-json <path>   Optional fixture plan JSON (for tests/regression checks).
  --quiet                      Suppress informational logs.
  --help                       Show this help text.
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
    --fixture-plan-json)
      FIXTURE_PLAN_JSON="$2"
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

if [[ -n "${FIXTURE_PLAN_JSON}" && ! -f "${FIXTURE_PLAN_JSON}" ]]; then
  echo "error: fixture plan JSON not found: ${FIXTURE_PLAN_JSON}" >&2
  exit 1
fi

mkdir -p "$(dirname "${OUTPUT_JSON}")" "$(dirname "${OUTPUT_MD}")"

python3 - \
  "${REPO_ROOT}" \
  "${OUTPUT_JSON}" \
  "${OUTPUT_MD}" \
  "${SCHEMA_PATH}" \
  "${GENERATED_AT}" \
  "${FIXTURE_PLAN_JSON}" <<'PY'
import json
import sys
from pathlib import Path

(
    repo_root_raw,
    output_json_raw,
    output_md_raw,
    schema_path_raw,
    generated_at,
    fixture_plan_json_raw,
) = sys.argv[1:]

repo_root = Path(repo_root_raw).resolve()
output_json = Path(output_json_raw)
if not output_json.is_absolute():
    output_json = (repo_root / output_json).resolve()
output_md = Path(output_md_raw)
if not output_md.is_absolute():
    output_md = (repo_root / output_md).resolve()

required_crates = [
    "tau-training-types",
    "tau-training-store",
    "tau-training-tracer",
    "tau-training-runner",
    "tau-training-proxy",
    "tau-trainer",
    "tau-algorithm",
]


def default_plan() -> dict:
    return {
        "crates": [
            {
                "crate": "tau-training-types",
                "decision": "retain",
                "merge_target": None,
                "owner_surface": "shared training domain types and serde contracts",
                "rationale": "Leaf types crate used by store/runner/tracer/algorithm/trainer; avoids cyclic dependencies.",
            },
            {
                "crate": "tau-training-store",
                "decision": "retain",
                "merge_target": None,
                "owner_surface": "rollout queue, persistence, and resource versioning",
                "rationale": "SQLite and in-memory store boundaries are stable and used by multiple runtime surfaces.",
            },
            {
                "crate": "tau-training-tracer",
                "decision": "retain",
                "merge_target": None,
                "owner_surface": "execution spans and reward emission contracts",
                "rationale": "Tracer integrates with agent events and store without owning runner orchestration.",
            },
            {
                "crate": "tau-training-runner",
                "decision": "retain",
                "merge_target": None,
                "owner_surface": "worker poll-execute-report loop",
                "rationale": "Runner behavior remains independently testable and can scale without coupling to trainer orchestration.",
            },
            {
                "crate": "tau-training-proxy",
                "decision": "retain",
                "merge_target": None,
                "owner_surface": "optional OpenAI-compatible attribution proxy",
                "rationale": "Operationally optional HTTP surface; should remain isolated from core prompt optimization runtime.",
            },
            {
                "crate": "tau-trainer",
                "decision": "retain",
                "merge_target": None,
                "owner_surface": "top-level fit orchestration and lifecycle coordination",
                "rationale": "Keeps orchestration boundary explicit above runner/store without forcing algorithm coupling.",
            },
            {
                "crate": "tau-algorithm",
                "decision": "retain",
                "merge_target": None,
                "owner_surface": "strategy layer (APO + adapters)",
                "rationale": "Algorithm surface evolves separately from runtime/store plumbing and keeps strategy polymorphism clean.",
            },
        ],
        "first_pr_sets": [
            {
                "id": "training-boundary-set-a",
                "title": "Boundary decision plan + docs ownership contract",
                "status": "completed",
                "issues": ["#1711"],
                "scope": [
                    "Publish crate-by-crate retain/merge decisions.",
                    "Wire decision-plan checks and docs references.",
                ],
                "test_matrix": ["unit", "functional", "integration", "regression"],
            },
            {
                "id": "training-boundary-set-b",
                "title": "Stale training flag/docs cleanup",
                "status": "completed",
                "issues": ["#1712"],
                "scope": [
                    "Remove stale training alias/docs paths after boundary confirmation.",
                    "Align CLI/help output with prompt-optimization naming.",
                ],
                "test_matrix": ["unit", "functional", "integration", "regression"],
            },
            {
                "id": "training-boundary-set-c",
                "title": "Consolidation execution follow-through",
                "status": "planned",
                "issues": ["#1628"],
                "scope": [
                    "Implement merges only where future ambiguity appears or duplication emerges.",
                    "Preserve compile/test stability across trainer/runner/store/algorithm surfaces.",
                ],
                "test_matrix": ["unit", "functional", "integration", "regression"],
            },
        ],
    }


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"error: {message}")


def load_plan(path_raw: str) -> dict:
    if not path_raw:
        return default_plan()
    path = Path(path_raw).resolve()
    with path.open(encoding="utf-8") as handle:
        payload = json.load(handle)
    require(isinstance(payload, dict), "fixture plan must decode to a JSON object")
    return payload


plan = load_plan(fixture_plan_json_raw)
crates = plan.get("crates")
require(isinstance(crates, list) and crates, "plan crates[] must be a non-empty array")

seen = set()
retain_count = 0
merge_count = 0
normalized_crates: list[dict] = []
for index, entry in enumerate(crates):
    require(isinstance(entry, dict), f"crates[{index}] must be an object")
    crate = entry.get("crate")
    decision = entry.get("decision")
    merge_target = entry.get("merge_target")
    owner_surface = entry.get("owner_surface")
    rationale = entry.get("rationale")

    require(isinstance(crate, str) and crate.strip(), f"crates[{index}].crate must be non-empty")
    require(crate not in seen, f"duplicate crate decision for '{crate}'")
    seen.add(crate)
    require(
        isinstance(decision, str) and decision in {"retain", "merge"},
        f"crates[{index}].decision must be 'retain' or 'merge'",
    )
    require(
        isinstance(owner_surface, str) and owner_surface.strip(),
        f"crates[{index}].owner_surface must be non-empty",
    )
    require(
        isinstance(rationale, str) and rationale.strip(),
        f"crates[{index}].rationale must be non-empty",
    )

    crate_dir = repo_root / "crates" / crate
    require(crate_dir.is_dir(), f"crate path not found for decision entry: {crate_dir}")
    require((crate_dir / "Cargo.toml").is_file(), f"crate manifest missing: {crate_dir / 'Cargo.toml'}")

    if decision == "retain":
        retain_count += 1
        require(merge_target in {None, ""}, f"retain decision for '{crate}' cannot set merge_target")
        normalized_merge_target = None
    else:
        merge_count += 1
        require(
            isinstance(merge_target, str) and merge_target.strip(),
            f"merge decision for '{crate}' requires merge_target",
        )
        normalized_merge_target = merge_target

    normalized_crates.append(
        {
            "crate": crate,
            "decision": decision,
            "merge_target": normalized_merge_target,
            "owner_surface": owner_surface,
            "rationale": rationale,
        }
    )

required_set = set(required_crates)
missing = sorted(required_set - seen)
extras = sorted(seen - required_set)
require(not missing, f"missing required crates in plan: {', '.join(missing)}")
require(not extras, f"plan includes crates outside required boundary list: {', '.join(extras)}")

first_pr_sets = plan.get("first_pr_sets")
require(
    isinstance(first_pr_sets, list) and first_pr_sets,
    "plan first_pr_sets[] must be a non-empty array",
)
normalized_sets: list[dict] = []
for index, item in enumerate(first_pr_sets):
    require(isinstance(item, dict), f"first_pr_sets[{index}] must be an object")
    set_id = item.get("id")
    title = item.get("title")
    status = item.get("status")
    issues = item.get("issues")
    scope = item.get("scope")
    test_matrix = item.get("test_matrix")
    require(isinstance(set_id, str) and set_id.strip(), f"first_pr_sets[{index}].id must be non-empty")
    require(isinstance(title, str) and title.strip(), f"first_pr_sets[{index}].title must be non-empty")
    require(
        isinstance(status, str) and status in {"planned", "in_progress", "completed"},
        f"first_pr_sets[{index}].status must be planned|in_progress|completed",
    )
    require(
        isinstance(issues, list) and all(isinstance(issue, str) and issue.strip() for issue in issues),
        f"first_pr_sets[{index}].issues must be a non-empty string array",
    )
    require(
        isinstance(scope, list) and all(isinstance(line, str) and line.strip() for line in scope),
        f"first_pr_sets[{index}].scope must be a non-empty string array",
    )
    require(
        isinstance(test_matrix, list)
        and all(isinstance(entry, str) and entry.strip() for entry in test_matrix),
        f"first_pr_sets[{index}].test_matrix must be a non-empty string array",
    )
    normalized_sets.append(
        {
            "id": set_id,
            "title": title,
            "status": status,
            "issues": issues,
            "scope": scope,
            "test_matrix": test_matrix,
        }
    )

report = {
    "schema_version": 1,
    "generated_at": generated_at,
    "repository_root": ".",
    "schema_path": str(Path(schema_path_raw).resolve().relative_to(repo_root))
    if Path(schema_path_raw).resolve().is_relative_to(repo_root)
    else schema_path_raw,
    "required_crates": required_crates,
    "summary": {
        "total_crates": len(normalized_crates),
        "retain_count": retain_count,
        "merge_count": merge_count,
        "ambiguous_count": 0,
    },
    "crates": sorted(normalized_crates, key=lambda item: item["crate"]),
    "first_pr_sets": normalized_sets,
}

output_json.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

lines = []
lines.append("# Training Crate Boundary Plan")
lines.append("")
lines.append(f"- Generated: {generated_at}")
lines.append("- Scope: tau-training-* crates + tau-trainer + tau-algorithm")
lines.append("")
lines.append("## Summary")
lines.append("")
lines.append("| Metric | Value |")
lines.append("| --- | ---: |")
lines.append(f"| Total crates | {report['summary']['total_crates']} |")
lines.append(f"| Retain decisions | {report['summary']['retain_count']} |")
lines.append(f"| Merge decisions | {report['summary']['merge_count']} |")
lines.append(f"| Ambiguous decisions | {report['summary']['ambiguous_count']} |")
lines.append("")
lines.append("## Decision Matrix")
lines.append("")
lines.append("| Crate | Decision | Merge Target | Owner Surface | Rationale |")
lines.append("| --- | --- | --- | --- | --- |")
for entry in report["crates"]:
    merge_target = entry["merge_target"] if entry["merge_target"] is not None else "-"
    lines.append(
        f"| `{entry['crate']}` | {entry['decision']} | {merge_target} | "
        f"{entry['owner_surface']} | {entry['rationale']} |"
    )
lines.append("")
lines.append("## First Consolidation PR Sets")
lines.append("")
lines.append("| Set | Status | Issues | Scope | Test Matrix |")
lines.append("| --- | --- | --- | --- | --- |")
for item in report["first_pr_sets"]:
    issue_text = ", ".join(item["issues"])
    scope_text = "; ".join(item["scope"])
    test_matrix_text = ", ".join(item["test_matrix"])
    lines.append(
        f"| `{item['id']}` | {item['status']} | {issue_text} | "
        f"{scope_text} | {test_matrix_text} |"
    )
lines.append("")

output_md.write_text("\n".join(lines), encoding="utf-8")
PY

log_info "wrote training boundary plan artifacts:"
log_info "- ${OUTPUT_JSON}"
log_info "- ${OUTPUT_MD}"
