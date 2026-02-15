#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import re
import subprocess
from dataclasses import dataclass
from pathlib import Path

PUB_ITEM_PATTERN = re.compile(
    r"^\s*pub\s+(?:async\s+)?(?P<kind>const|fn|struct|enum|trait|mod|type)\b"
)
LINE_DOC_PATTERN = re.compile(r"^\s*///")
FAILED_CRATE_PATTERN = re.compile(r"crate '([^']+)'")


@dataclass(frozen=True)
class UndocumentedItem:
    crate: str
    file_path: str
    line: int
    signature: str


def parse_failed_crates(issues: list[dict[str, str]]) -> set[str]:
    failed: set[str] = set()
    for issue in issues:
        if issue.get("kind") != "crate_min_failed":
            continue
        detail = issue.get("detail", "")
        match = FAILED_CRATE_PATTERN.search(detail)
        if match:
            failed.add(match.group(1))
    return failed


def parse_changed_files_from_file(path: Path) -> list[str]:
    lines = [line.strip() for line in path.read_text(encoding="utf-8").splitlines()]
    return sorted({line for line in lines if line})


def git_changed_files(repo_root: Path, base_ref: str | None) -> list[str]:
    def run_git(*args: str) -> str:
        return subprocess.check_output(["git", *args], cwd=repo_root, text=True)

    if base_ref:
        try:
            subprocess.run(
                ["git", "fetch", "origin", base_ref, "--depth=1"],
                cwd=repo_root,
                check=False,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                text=True,
            )
        except OSError:
            pass
        try:
            raw = run_git("diff", "--name-only", f"origin/{base_ref}...HEAD")
            files = [line.strip() for line in raw.splitlines() if line.strip()]
            if files:
                return sorted(set(files))
        except subprocess.CalledProcessError:
            pass

    for fallback in (("diff", "--name-only", "HEAD~1..HEAD"), ("diff", "--name-only")):
        try:
            raw = run_git(*fallback)
            files = [line.strip() for line in raw.splitlines() if line.strip()]
            if files:
                return sorted(set(files))
        except subprocess.CalledProcessError:
            continue
    return []


def file_has_preceding_doc(lines: list[str], index: int) -> bool:
    cursor = index - 1
    while cursor >= 0 and lines[cursor].strip() == "":
        cursor -= 1
    if cursor < 0:
        return False
    return bool(LINE_DOC_PATTERN.match(lines[cursor]))


def scan_undocumented_public_items(
    path: Path, crate: str, display_path: str, limit: int
) -> list[UndocumentedItem]:
    lines = path.read_text(encoding="utf-8").splitlines()
    items: list[UndocumentedItem] = []
    for index, line in enumerate(lines):
        if len(items) >= limit:
            break
        if not PUB_ITEM_PATTERN.match(line):
            continue
        if file_has_preceding_doc(lines, index):
            continue
        items.append(
            UndocumentedItem(
                crate=crate,
                file_path=display_path,
                line=index + 1,
                signature=line.strip(),
            )
        )
    return items


def collect_annotations(
    repo_root: Path,
    failed_crates: set[str],
    changed_files: list[str],
    max_hints: int,
) -> list[UndocumentedItem]:
    if max_hints <= 0:
        return []

    annotations: list[UndocumentedItem] = []
    for rel_path in changed_files:
        rel = rel_path.replace("\\", "/")
        if not rel.startswith("crates/") or not rel.endswith(".rs"):
            continue
        parts = rel.split("/")
        if len(parts) < 3:
            continue
        crate = parts[1]
        if crate not in failed_crates:
            continue
        path = repo_root / rel
        if not path.is_file():
            continue
        for item in scan_undocumented_public_items(path, crate, rel, limit=5):
            annotations.append(item)
            if len(annotations) >= max_hints:
                return annotations
    return annotations


def escape_annotation_message(message: str) -> str:
    return (
        message.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
    )


def emit_github_warnings(annotations: list[UndocumentedItem]) -> None:
    for item in annotations:
        message = (
            f"Doc density hint for crate '{item.crate}': add /// docs for public API "
            f"`{item.signature}`."
        )
        escaped_message = escape_annotation_message(message)
        print(
            f"::warning file={item.file_path},line={item.line},title=doc-density::{item.crate}::{item.line}::{escaped_message}"
        )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Emit file-level GitHub annotations for failed rust doc density crates."
    )
    parser.add_argument("--repo-root", default=".")
    parser.add_argument(
        "--density-json",
        default="ci-artifacts/rust-doc-density.json",
        help="Path to rust doc density JSON artifact.",
    )
    parser.add_argument(
        "--changed-files-file",
        help="Optional text file containing changed file paths (one per line).",
    )
    parser.add_argument(
        "--base-ref",
        help="Base branch ref used for git diff fallback (e.g., master).",
    )
    parser.add_argument("--max-hints", type=int, default=25)
    parser.add_argument("--json-output-file")
    parser.add_argument("--quiet", action="store_true")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    repo_root = Path(args.repo_root).resolve()
    density_path = Path(args.density_json)
    if not density_path.is_absolute():
        density_path = repo_root / density_path

    payload = {
        "schema_version": 1,
        "failed_crates": [],
        "changed_files": [],
        "annotations": [],
    }

    if not density_path.is_file():
        if not args.quiet:
            print(f"doc-density-annotations: density artifact missing at {density_path}")
        if args.json_output_file:
            out = Path(args.json_output_file)
            if not out.is_absolute():
                out = repo_root / out
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
        return 0

    density = json.loads(density_path.read_text(encoding="utf-8"))
    failed_crates = parse_failed_crates(density.get("issues", []))
    if args.changed_files_file:
        changed_files = parse_changed_files_from_file(Path(args.changed_files_file))
    else:
        changed_files = git_changed_files(repo_root, args.base_ref)
    annotations = collect_annotations(repo_root, failed_crates, changed_files, args.max_hints)

    emit_github_warnings(annotations)
    if not args.quiet:
        print(
            f"doc-density-annotations: failed_crates={len(failed_crates)} "
            f"changed_files={len(changed_files)} hints={len(annotations)}"
        )

    payload = {
        "schema_version": 1,
        "failed_crates": sorted(failed_crates),
        "changed_files": changed_files,
        "annotations": [
            {
                "crate": item.crate,
                "file_path": item.file_path,
                "line": item.line,
                "signature": item.signature,
            }
            for item in annotations
        ],
    }

    if args.json_output_file:
        out = Path(args.json_output_file)
        if not out.is_absolute():
            out = repo_root / out
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
