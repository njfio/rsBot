#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

METADATA_PATH=""
OUTPUT_JSON="${REPO_ROOT}/tasks/reports/crate-dependency-graph.json"
OUTPUT_MD="${REPO_ROOT}/tasks/reports/crate-dependency-graph.md"
GENERATED_AT=""
QUIET_MODE="false"

usage() {
  cat <<'EOF'
Usage: crate-dependency-graph.sh [options]

Generate deterministic workspace crate dependency graph artifacts from cargo metadata.

Options:
  --metadata <path>      Optional cargo metadata JSON input path.
  --output-json <path>   Output JSON artifact path.
  --output-md <path>     Output Markdown artifact path.
  --generated-at <iso>   Deterministic ISO-8601 UTC timestamp override.
  --quiet                Suppress informational output.
  --help                 Show this message.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --metadata)
      METADATA_PATH="$2"
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

if ! command -v python3 >/dev/null 2>&1; then
  echo "error: python3 is required for crate-dependency-graph.sh" >&2
  exit 1
fi

if [[ -n "${METADATA_PATH}" && ! -f "${METADATA_PATH}" ]]; then
  echo "error: metadata path not found: ${METADATA_PATH}" >&2
  exit 1
fi

mkdir -p "$(dirname "${OUTPUT_JSON}")"
mkdir -p "$(dirname "${OUTPUT_MD}")"

python3 - "${REPO_ROOT}" "${METADATA_PATH}" "${OUTPUT_JSON}" "${OUTPUT_MD}" "${GENERATED_AT}" "${QUIET_MODE}" <<'PY'
from __future__ import annotations

import json
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

repo_root = Path(sys.argv[1])
metadata_path_raw = sys.argv[2].strip()
output_json = Path(sys.argv[3])
output_md = Path(sys.argv[4])
generated_at_raw = sys.argv[5].strip()
quiet_mode = sys.argv[6] == "true"


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


def node_id(crate_name: str) -> str:
    return re.sub(r"[^a-zA-Z0-9_]", "_", crate_name)


if metadata_path_raw:
    metadata_source = Path(metadata_path_raw)
    try:
        metadata_payload = json.loads(metadata_source.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        fail(f"failed to parse metadata JSON {metadata_source}: {exc}")
    metadata_source_display = display_path(metadata_source)
else:
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=repo_root,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        fail(f"cargo metadata failed: {result.stderr.strip()}")
    try:
        metadata_payload = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        fail(f"failed to parse cargo metadata output: {exc}")
    metadata_source_display = "cargo metadata --no-deps --format-version 1"

packages = metadata_payload.get("packages")
workspace_members = metadata_payload.get("workspace_members")
if not isinstance(packages, list) or not isinstance(workspace_members, list):
    fail("metadata payload missing `packages` or `workspace_members` list")

packages_by_id = {
    package.get("id"): package
    for package in packages
    if isinstance(package, dict) and isinstance(package.get("id"), str)
}

workspace_packages = []
for member_id in workspace_members:
    package = packages_by_id.get(member_id)
    if package is not None:
        workspace_packages.append(package)

workspace_crates = []
for package in workspace_packages:
    name = package.get("name")
    manifest_path = package.get("manifest_path")
    if not isinstance(name, str) or not isinstance(manifest_path, str):
        continue
    workspace_crates.append(
        {
            "name": name,
            "manifest_path": display_path(Path(manifest_path)),
        }
    )

workspace_crates.sort(key=lambda item: item["name"])
workspace_names = {item["name"] for item in workspace_crates}

edges_set: set[tuple[str, str]] = set()
for package in workspace_packages:
    source_name = package.get("name")
    if not isinstance(source_name, str) or source_name not in workspace_names:
        continue
    dependencies = package.get("dependencies") or []
    for dependency in dependencies:
        if not isinstance(dependency, dict):
            continue
        dep_name = dependency.get("name")
        if isinstance(dep_name, str) and dep_name in workspace_names:
            edges_set.add((source_name, dep_name))

edges = [{"from": src, "to": dst} for src, dst in sorted(edges_set)]

payload = {
    "schema_version": 1,
    "generated_at": parse_generated_at(generated_at_raw),
    "inputs": {
        "metadata_source": metadata_source_display,
    },
    "summary": {
        "workspace_crates": len(workspace_crates),
        "workspace_edges": len(edges),
    },
    "crates": workspace_crates,
    "edges": edges,
}

output_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

md_lines = [
    "# Workspace Crate Dependency Graph",
    "",
    f"- generated_at: `{payload['generated_at']}`",
    f"- metadata_source: `{payload['inputs']['metadata_source']}`",
    "",
    "## Summary",
    "",
    "| Metric | Value |",
    "|---|---:|",
    f"| workspace_crates | {payload['summary']['workspace_crates']} |",
    f"| workspace_edges | {payload['summary']['workspace_edges']} |",
    "",
    "## Mermaid",
    "",
    "```mermaid",
    "graph TD",
]
for crate in workspace_crates:
    nid = node_id(crate["name"])
    md_lines.append(f"  {nid}[\"{crate['name']}\"]")
for edge in edges:
    md_lines.append(f"  {node_id(edge['from'])} --> {node_id(edge['to'])}")
md_lines.extend(
    [
        "```",
        "",
        "## Workspace Crates",
        "",
        "| Crate | Manifest Path |",
        "|---|---|",
    ]
)
for crate in workspace_crates:
    md_lines.append(f"| {crate['name']} | `{crate['manifest_path']}` |")
md_lines.extend(
    [
        "",
        "## Workspace Edges",
        "",
        "| From | To |",
        "|---|---|",
    ]
)
for edge in edges:
    md_lines.append(f"| {edge['from']} | {edge['to']} |")

output_md.write_text("\n".join(md_lines) + "\n", encoding="utf-8")

if not quiet_mode:
    print(
        "crate-dependency-graph: "
        f"workspace_crates={payload['summary']['workspace_crates']} "
        f"workspace_edges={payload['summary']['workspace_edges']} "
        f"output_json={output_json} output_md={output_md}"
    )
PY
