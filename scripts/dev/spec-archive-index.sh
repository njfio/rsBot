#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

SPEC_ROOT="${REPO_ROOT}/specs"
OUTPUT_JSON="${REPO_ROOT}/tasks/reports/spec-archive-index.json"
OUTPUT_MD="${REPO_ROOT}/tasks/reports/spec-archive-index.md"
GENERATED_AT=""
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: spec-archive-index.sh [options]

Generate implemented-spec archive index artifacts from `specs/*/spec.md`.

Options:
  --spec-root <path>     Root spec directory to scan (default: specs/)
  --output-json <path>   Output JSON artifact path
  --output-md <path>     Output Markdown artifact path
  --generated-at <iso>   Deterministic ISO-8601 UTC timestamp override
  --quiet                Suppress informational output
  --help                 Show this message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --spec-root)
      SPEC_ROOT="$2"
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
      echo "error: unknown option '$1'" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ ! -d "${SPEC_ROOT}" ]]; then
  echo "error: spec root not found: ${SPEC_ROOT}" >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "error: python3 is required for spec-archive-index.sh" >&2
  exit 1
fi

mkdir -p "$(dirname "${OUTPUT_JSON}")"
mkdir -p "$(dirname "${OUTPUT_MD}")"

python3 - "${SPEC_ROOT}" "${OUTPUT_JSON}" "${OUTPUT_MD}" "${GENERATED_AT}" "${QUIET_MODE}" <<'PY'
from __future__ import annotations

import json
import sys
from datetime import datetime, timezone
from pathlib import Path

spec_root = Path(sys.argv[1])
output_json = Path(sys.argv[2])
output_md = Path(sys.argv[3])
generated_at_raw = sys.argv[4].strip()
quiet_mode = sys.argv[5] == "true"


def fail(message: str) -> None:
    raise SystemExit(f"error: {message}")


def iso_utc(dt: datetime) -> str:
    return dt.astimezone(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def parse_generated_at(raw: str) -> str:
    if not raw:
        return iso_utc(datetime.now(timezone.utc))
    candidate = raw
    if candidate.endswith("Z"):
        candidate = candidate[:-1] + "+00:00"
    try:
        parsed = datetime.fromisoformat(candidate)
    except ValueError as exc:
        fail(f"invalid --generated-at timestamp: {raw} ({exc})")
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return iso_utc(parsed)


def parse_status(spec_path: Path) -> str:
    for line in spec_path.read_text(encoding="utf-8").splitlines():
        if line.lower().startswith("status:"):
            value = line.split(":", 1)[1].strip()
            return value or "Unknown"
    return "Unknown"


entries: list[dict[str, str]] = []
path_root = spec_root.parent

def display_path(spec_path: Path) -> str:
    try:
        rel = spec_path.relative_to(path_root)
        return rel.as_posix()
    except ValueError:
        return spec_path.as_posix()

for spec_path in spec_root.glob("*/spec.md"):
    issue_id = spec_path.parent.name
    status = parse_status(spec_path)
    entries.append(
        {
            "issue_id": issue_id,
            "status": status,
            "spec_path": display_path(spec_path),
        }
    )


def sort_key(entry: dict[str, str]) -> tuple[int, str]:
    issue = entry["issue_id"]
    if issue.isdigit():
        return (0, f"{int(issue):09d}")
    return (1, issue)


entries.sort(key=sort_key)
implemented_entries = [entry for entry in entries if entry["status"].lower() == "implemented"]

status_counts: dict[str, int] = {}
for entry in entries:
    status_counts[entry["status"]] = status_counts.get(entry["status"], 0) + 1

payload = {
    "schema_version": 1,
    "generated_at": parse_generated_at(generated_at_raw),
    "summary": {
        "total_specs": len(entries),
        "implemented_specs": len(implemented_entries),
        "status_counts": status_counts,
    },
    "implemented_specs": implemented_entries,
    "specs": entries,
}

output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

md_lines = [
    "# Implemented Spec Archive Index",
    "",
    f"- generated_at: `{payload['generated_at']}`",
    f"- total_specs: `{payload['summary']['total_specs']}`",
    f"- implemented_specs: `{payload['summary']['implemented_specs']}`",
    "",
    "## Spec Status Inventory",
    "",
    "| Issue | Status | Spec Path |",
    "|---|---|---|",
]
for entry in entries:
    md_lines.append(f"| {entry['issue_id']} | {entry['status']} | `{entry['spec_path']}` |")
output_md.write_text("\n".join(md_lines) + "\n", encoding="utf-8")

if not quiet_mode:
    print(
        "spec-archive-index: "
        f"total_specs={payload['summary']['total_specs']} "
        f"implemented_specs={payload['summary']['implemented_specs']} "
        f"output_json={output_json} output_md={output_md}"
    )
PY
