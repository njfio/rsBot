#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

SOURCE_FILE="crates/tau-tools/src/tools.rs"
TARGET_LINES=3000
OUTPUT_JSON="${REPO_ROOT}/tasks/reports/m25-tools-split-map.json"
OUTPUT_MD="${REPO_ROOT}/tasks/reports/m25-tools-split-map.md"
GENERATED_AT=""
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: tools-split-map.sh [options]

Generate M25 split-map artifacts for crates/tau-tools/src/tools.rs.

Options:
  --source-file <path>      Source file to analyze (default: crates/tau-tools/src/tools.rs)
  --target-lines <n>        Target post-split line budget (default: 3000)
  --output-json <path>      JSON artifact output path
  --output-md <path>        Markdown artifact output path
  --generated-at <iso>      Deterministic generated timestamp (ISO-8601 UTC)
  --quiet                   Suppress informational output
  --help                    Show this help text
EOF
}

log_info() {
  if [[ "${QUIET_MODE}" != "true" ]]; then
    echo "$@"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source-file)
      SOURCE_FILE="$2"
      shift 2
      ;;
    --target-lines)
      TARGET_LINES="$2"
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
      echo "error: unknown argument '$1'" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if ! [[ "${TARGET_LINES}" =~ ^[0-9]+$ ]] || [[ "${TARGET_LINES}" -lt 1 ]]; then
  echo "error: --target-lines must be an integer >= 1" >&2
  exit 1
fi

if [[ ! -f "${SOURCE_FILE}" ]]; then
  echo "error: source file not found: ${SOURCE_FILE}" >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "error: required command 'python3' not found" >&2
  exit 1
fi

mkdir -p "$(dirname "${OUTPUT_JSON}")" "$(dirname "${OUTPUT_MD}")"

python3 - \
  "${SOURCE_FILE}" \
  "${TARGET_LINES}" \
  "${OUTPUT_JSON}" \
  "${OUTPUT_MD}" \
  "${GENERATED_AT}" \
  "${QUIET_MODE}" <<'PY'
from __future__ import annotations

import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

(
    source_file_raw,
    target_lines_raw,
    output_json_raw,
    output_md_raw,
    generated_at_raw,
    quiet_mode_raw,
) = sys.argv[1:]

source_file = Path(source_file_raw)
output_json = Path(output_json_raw)
output_md = Path(output_md_raw)
target_lines = int(target_lines_raw)
quiet_mode = quiet_mode_raw == "true"


def log(message: str) -> None:
    if not quiet_mode:
        print(message)


def fail(message: str) -> None:
    raise SystemExit(f"error: {message}")


def parse_iso8601_utc(value: str) -> datetime:
    candidate = value.strip()
    if not candidate:
        fail("generated-at value must not be empty")
    if candidate.endswith("Z"):
        candidate = candidate[:-1] + "+00:00"
    try:
        parsed = datetime.fromisoformat(candidate)
    except ValueError as exc:
        fail(f"invalid --generated-at timestamp: {value} ({exc})")
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc).replace(microsecond=0)


def iso_utc(value: datetime) -> str:
    return value.astimezone(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


generated_at = (
    parse_iso8601_utc(generated_at_raw)
    if generated_at_raw.strip()
    else datetime.now(timezone.utc).replace(microsecond=0)
)
generated_at_iso = iso_utc(generated_at)

line_count = len(source_file.read_text(encoding="utf-8").splitlines())
line_gap = max(line_count - target_lines, 0)

extraction_phases: list[dict[str, Any]] = [
    {
        "id": "phase-1-fs-edit-memory",
        "title": "Filesystem/edit and memory domain tools",
        "owner": "tools-runtime",
        "line_reduction_estimate": 410,
        "modules": [
            "tools/fs_tools.rs",
            "tools/edit_tools.rs",
            "tools/memory_tools.rs",
        ],
        "depends_on": [],
        "notes": "Keep read/write/edit/memory tool JSON contracts and policy hooks stable.",
    },
    {
        "id": "phase-2-jobs-history-http",
        "title": "Jobs/history/http command tool surfaces",
        "owner": "tools-orchestration",
        "line_reduction_estimate": 320,
        "modules": [
            "tools/jobs_tools.rs",
            "tools/history_tools.rs",
            "tools/http_tools.rs",
        ],
        "depends_on": ["phase-1-fs-edit-memory"],
        "notes": "Preserve queue/status/cancel behavior and HTTP safety options across call sites.",
    },
    {
        "id": "phase-3-bash-policy-gates",
        "title": "Bash execution and policy/approval gate logic",
        "owner": "tools-safety",
        "line_reduction_estimate": 260,
        "modules": [
            "tools/bash_tool.rs",
            "tools/policy_gates.rs",
        ],
        "depends_on": ["phase-2-jobs-history-http"],
        "notes": "Keep approval, RBAC, protected-path, and rate-limit gates behaviorally identical.",
    },
]

estimated_lines_to_extract = sum(entry["line_reduction_estimate"] for entry in extraction_phases)
post_split_estimated_line_count = max(line_count - estimated_lines_to_extract, 0)

public_api_impact = [
    "Keep exported tool type names and trait implementations stable for runtime callers.",
    "Preserve JSON argument/return contracts for all moved tools.",
    "Maintain existing policy gate result semantics and error envelopes.",
]

import_impact = [
    "Introduce module declarations under crates/tau-tools/src/tools/ with selective re-exports.",
    "Move domain-specific tool implementations from tools.rs into phased modules.",
    "Keep shared helper functions centralized to reduce import fan-out during phased extraction.",
]

test_migration_plan = [
    {
        "order": 1,
        "id": "guardrail-threshold-enforcement",
        "description": "Introduce and enforce tools.rs split guardrail ending at <3000.",
        "command": "scripts/dev/test-tools-domain-split.sh",
        "expected_signal": "tools.rs threshold checks fail closed until split target is reached",
    },
    {
        "order": 2,
        "id": "tools-crate-coverage",
        "description": "Run crate-scoped tau-tools tests after each extraction phase.",
        "command": "cargo test -p tau-tools",
        "expected_signal": "tool behavior, safety checks, and serialization tests stay green",
    },
    {
        "order": 3,
        "id": "runtime-integration",
        "description": "Run cross-crate runtime integration suites that consume tau-tools surfaces.",
        "command": "cargo test -p tau-coding-agent",
        "expected_signal": "no regressions in tool wiring and end-to-end command flows",
    },
]

payload = {
    "schema_version": 1,
    "generated_at": generated_at_iso,
    "source_file": source_file_raw,
    "target_line_budget": target_lines,
    "current_line_count": line_count,
    "line_gap_to_target": line_gap,
    "estimated_lines_to_extract": estimated_lines_to_extract,
    "post_split_estimated_line_count": post_split_estimated_line_count,
    "extraction_phases": extraction_phases,
    "public_api_impact": public_api_impact,
    "import_impact": import_impact,
    "test_migration_plan": test_migration_plan,
}

output_json.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")

lines: list[str] = [
    "# Tools Runtime Split Map (M25)",
    "",
    f"- Generated at (UTC): `{generated_at_iso}`",
    f"- Source file: `{source_file_raw}`",
    f"- Target line budget: `{target_lines}`",
    f"- Current line count: `{line_count}`",
    f"- Current gap to target: `{line_gap}`",
    f"- Estimated lines to extract: `{estimated_lines_to_extract}`",
    f"- Estimated post-split line count: `{post_split_estimated_line_count}`",
    "",
    "## Extraction Phases",
    "",
    "| Phase | Owner | Est. Reduction | Depends On | Modules | Notes |",
    "| --- | --- | ---: | --- | --- | --- |",
]

for phase in extraction_phases:
    depends_on = ", ".join(phase["depends_on"]) if phase["depends_on"] else "-"
    modules = ", ".join(phase["modules"])
    lines.append(
        "| "
        f"{phase['id']} ({phase['title']}) | "
        f"{phase['owner']} | "
        f"{phase['line_reduction_estimate']} | "
        f"{depends_on} | "
        f"{modules} | "
        f"{phase['notes']} |"
    )

lines.extend(
    [
        "",
        "## Public API Impact",
        "",
    ]
)
for item in public_api_impact:
    lines.append(f"- {item}")

lines.extend(
    [
        "",
        "## Import Impact",
        "",
    ]
)
for item in import_impact:
    lines.append(f"- {item}")

lines.extend(
    [
        "",
        "## Test Migration Plan",
        "",
        "| Order | Step | Command | Expected Signal |",
        "| ---: | --- | --- | --- |",
    ]
)
for entry in test_migration_plan:
    lines.append(
        "| "
        f"{entry['order']} | "
        f"{entry['id']}: {entry['description']} | "
        f"{entry['command']} | "
        f"{entry['expected_signal']} |"
    )

output_md.write_text("\n".join(lines) + "\n", encoding="utf-8")
log(
    "[tools-split-map] "
    f"source={source_file_raw} current_lines={line_count} target={target_lines} gap={line_gap}"
)
PY

log_info "wrote tools split-map artifacts:"
log_info "  - ${OUTPUT_JSON}"
log_info "  - ${OUTPUT_MD}"
