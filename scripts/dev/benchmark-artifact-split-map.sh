#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

SOURCE_FILE="crates/tau-trainer/src/benchmark_artifact.rs"
TARGET_LINES=3000
OUTPUT_JSON="${REPO_ROOT}/tasks/reports/m25-benchmark-artifact-split-map.json"
OUTPUT_MD="${REPO_ROOT}/tasks/reports/m25-benchmark-artifact-split-map.md"
GENERATED_AT=""
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: benchmark-artifact-split-map.sh [options]

Generate M25 split-map artifacts for crates/tau-trainer/src/benchmark_artifact.rs.

Options:
  --source-file <path>      Source file to analyze (default: crates/tau-trainer/src/benchmark_artifact.rs)
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
        "id": "phase-1-schema-types",
        "title": "Benchmark schema/types and serde payload structures",
        "owner": "trainer-core",
        "line_reduction_estimate": 220,
        "modules": [
            "benchmark_artifact/schema.rs",
            "benchmark_artifact/types.rs",
        ],
        "depends_on": [],
        "notes": "Preserve externally consumed artifact JSON schema and field names.",
    },
    {
        "id": "phase-2-io-persistence",
        "title": "Artifact IO, path handling, and persistence utilities",
        "owner": "trainer-runtime",
        "line_reduction_estimate": 260,
        "modules": [
            "benchmark_artifact/io.rs",
            "benchmark_artifact/persistence.rs",
        ],
        "depends_on": ["phase-1-schema-types"],
        "notes": "Keep filesystem contracts and error surface stable for callers.",
    },
    {
        "id": "phase-3-report-rendering",
        "title": "Markdown/JSON reporting and presentation helpers",
        "owner": "trainer-observability",
        "line_reduction_estimate": 230,
        "modules": [
            "benchmark_artifact/reporting.rs",
            "benchmark_artifact/render.rs",
        ],
        "depends_on": ["phase-2-io-persistence"],
        "notes": "Separate formatting logic from core benchmark artifact state.",
    },
    {
        "id": "phase-4-validation-compare",
        "title": "Validation, comparison, and regression guard logic",
        "owner": "trainer-quality",
        "line_reduction_estimate": 240,
        "modules": [
            "benchmark_artifact/validation.rs",
            "benchmark_artifact/compare.rs",
        ],
        "depends_on": ["phase-3-report-rendering"],
        "notes": "Retain benchmark conformance semantics and regression reason-code behavior.",
    },
]

estimated_lines_to_extract = sum(entry["line_reduction_estimate"] for entry in extraction_phases)
post_split_estimated_line_count = max(line_count - estimated_lines_to_extract, 0)

public_api_impact = [
    "Retain current benchmark artifact struct names and serialized field contracts.",
    "Keep public constructor/loader entrypoints stable for trainer and CLI callers.",
    "Confine extraction changes behind module boundaries without changing call signatures.",
]

import_impact = [
    "Introduce benchmark_artifact module tree under crates/tau-trainer/src/benchmark_artifact/.",
    "Move domain-specific helpers into phased modules while preserving root re-exports.",
    "Minimize cross-module imports by grouping schema/IO/report/validation concerns.",
]

test_migration_plan = [
    {
        "order": 1,
        "id": "benchmark-conformance-suite",
        "description": "Run benchmark artifact conformance tests after each extraction phase.",
        "command": "cargo test -p tau-trainer benchmark_artifact",
        "expected_signal": "benchmark artifact conformance tests remain green",
    },
    {
        "order": 2,
        "id": "trainer-integration-suite",
        "description": "Run trainer integration tests that persist/load benchmark artifacts.",
        "command": "cargo test -p tau-trainer",
        "expected_signal": "trainer integration flows preserve artifact behavior",
    },
    {
        "order": 3,
        "id": "workspace-regression-suite",
        "description": "Run workspace-level governance/contract checks for generated artifacts.",
        "command": "python3 -m unittest discover -s .github/scripts -p test_*.py",
        "expected_signal": "contract suite remains green after module extraction",
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
    "# Benchmark Artifact Split Map (M25)",
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

lines.extend(["", "## Public API Impact", ""])
for item in public_api_impact:
    lines.append(f"- {item}")

lines.extend(["", "## Import Impact", ""])
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
    "[benchmark-artifact-split-map] "
    f"source={source_file_raw} current_lines={line_count} target={target_lines} gap={line_gap}"
)
PY

log_info "wrote benchmark-artifact split-map artifacts:"
log_info "  - ${OUTPUT_JSON}"
log_info "  - ${OUTPUT_MD}"
