#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

ROUTER_PATH="${REPO_ROOT}/crates/tau-gateway/src/gateway_openresponses.rs"
API_DOC_PATH="${REPO_ROOT}/docs/guides/gateway-api-reference.md"
OUTPUT_JSON="${REPO_ROOT}/tasks/reports/gateway-api-route-inventory.json"
OUTPUT_MD="${REPO_ROOT}/tasks/reports/gateway-api-route-inventory.md"
GENERATED_AT=""
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: gateway-api-route-inventory.sh [options]

Generate deterministic gateway route inventory report and enforce API docs marker drift checks.

Options:
  --router <path>        Router source path to scan for `.route(...)` calls.
  --api-doc <path>       API reference markdown path to validate.
  --output-json <path>   Output JSON artifact path.
  --output-md <path>     Output Markdown artifact path.
  --generated-at <iso>   Deterministic ISO-8601 UTC timestamp override.
  --quiet                Suppress informational output.
  --help                 Show this message.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --router)
      ROUTER_PATH="$2"
      shift 2
      ;;
    --api-doc)
      API_DOC_PATH="$2"
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

if [[ ! -f "${ROUTER_PATH}" ]]; then
  echo "error: router path not found: ${ROUTER_PATH}" >&2
  exit 1
fi

if [[ ! -f "${API_DOC_PATH}" ]]; then
  echo "error: api doc path not found: ${API_DOC_PATH}" >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "error: python3 is required for gateway-api-route-inventory.sh" >&2
  exit 1
fi

mkdir -p "$(dirname "${OUTPUT_JSON}")"
mkdir -p "$(dirname "${OUTPUT_MD}")"

python3 - "${REPO_ROOT}" "${ROUTER_PATH}" "${API_DOC_PATH}" "${OUTPUT_JSON}" "${OUTPUT_MD}" "${GENERATED_AT}" "${QUIET_MODE}" <<'PY'
from __future__ import annotations

import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

repo_root = Path(sys.argv[1])
router_path = Path(sys.argv[2])
api_doc_path = Path(sys.argv[3])
output_json = Path(sys.argv[4])
output_md = Path(sys.argv[5])
generated_at_raw = sys.argv[6].strip()
quiet_mode = sys.argv[7] == "true"


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


def display_path(path: Path) -> str:
    try:
        return path.relative_to(repo_root).as_posix()
    except ValueError:
        return path.as_posix()


router_text = router_path.read_text(encoding="utf-8")
api_doc_text = api_doc_path.read_text(encoding="utf-8")

route_calls = len(re.findall(r"\.route\(", router_text))
method_path_rows = len(re.findall(r"^\| (GET|POST|PUT|PATCH|DELETE) \| ", api_doc_text, flags=re.MULTILINE))

route_marker_match = re.search(
    r"\*\*Route inventory:\*\*\s*(\d+)\s+router route calls",
    api_doc_text,
)
if route_marker_match is None:
    fail("missing route inventory marker in API doc")
route_marker = int(route_marker_match.group(1))

method_marker_match = re.search(
    r"^## Endpoint Inventory \((\d+) method-path entries\)",
    api_doc_text,
    flags=re.MULTILINE,
)
if method_marker_match is None:
    fail("missing endpoint inventory marker in API doc")
method_marker = int(method_marker_match.group(1))

route_calls_match = route_calls == route_marker
method_rows_match = method_path_rows == method_marker
ok = route_calls_match and method_rows_match

payload = {
    "schema_version": 1,
    "generated_at": parse_generated_at(generated_at_raw),
    "inputs": {
        "router_path": display_path(router_path),
        "api_doc_path": display_path(api_doc_path),
    },
    "actual_counts": {
        "route_calls": route_calls,
        "method_path_rows": method_path_rows,
    },
    "doc_markers": {
        "route_inventory_marker": route_marker,
        "method_path_inventory_marker": method_marker,
    },
    "drift": {
        "route_calls_match": route_calls_match,
        "method_path_rows_match": method_rows_match,
        "ok": ok,
    },
}

output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

md_lines = [
    "# Gateway API Route Inventory Report",
    "",
    f"- generated_at: `{payload['generated_at']}`",
    f"- router_path: `{payload['inputs']['router_path']}`",
    f"- api_doc_path: `{payload['inputs']['api_doc_path']}`",
    "",
    "## Inventory",
    "",
    "| Metric | Value |",
    "|---|---:|",
    f"| route_calls | {route_calls} |",
    f"| method_path_rows | {method_path_rows} |",
    f"| route_inventory_marker | {route_marker} |",
    f"| method_path_inventory_marker | {method_marker} |",
    "",
    "## Drift Verdict",
    "",
    "| Check | Pass |",
    "|---|---|",
    f"| route_calls_match | {'yes' if route_calls_match else 'no'} |",
    f"| method_path_rows_match | {'yes' if method_rows_match else 'no'} |",
    f"| overall_ok | {'yes' if ok else 'no'} |",
]
output_md.write_text("\n".join(md_lines) + "\n", encoding="utf-8")

if not route_calls_match:
    print(
        "route inventory marker mismatch: "
        f"marker={route_marker} actual={route_calls}",
        file=sys.stderr,
    )
if not method_rows_match:
    print(
        "method-path inventory marker mismatch: "
        f"marker={method_marker} actual={method_path_rows}",
        file=sys.stderr,
    )

if not quiet_mode:
    print(
        "gateway-api-route-inventory: "
        f"route_calls={route_calls} method_path_rows={method_path_rows} "
        f"route_marker={route_marker} method_marker={method_marker} "
        f"ok={'true' if ok else 'false'} "
        f"output_json={output_json} output_md={output_md}"
    )

if not ok:
    raise SystemExit(1)
PY
