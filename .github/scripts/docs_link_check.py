#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass
from pathlib import Path


LINK_PATTERN = re.compile(r"\[[^\]]+\]\(([^)]+)\)")
SKIP_PREFIXES = ("http://", "https://", "mailto:", "tel:")


@dataclass(frozen=True)
class LinkIssue:
    source: Path
    link: str
    resolved_target: Path | None
    reason: str


def extract_links(markdown: str) -> list[str]:
    return [match.group(1).strip() for match in LINK_PATTERN.finditer(markdown)]


def normalize_link_target(source: Path, link: str, repo_root: Path) -> Path | None:
    target = link.strip()
    if not target:
        return None
    if target.startswith(SKIP_PREFIXES):
        return None
    if target.startswith("#"):
        return None

    link_without_anchor = target.split("#", 1)[0].strip()
    if not link_without_anchor:
        return None

    if link_without_anchor.startswith("/"):
        candidate = (repo_root / link_without_anchor.lstrip("/")).resolve()
    else:
        candidate = (source.parent / link_without_anchor).resolve()
    return candidate


def discover_markdown_files(repo_root: Path, explicit_files: list[str]) -> list[Path]:
    if explicit_files:
        files: list[Path] = []
        for raw in explicit_files:
            candidate = (repo_root / raw).resolve()
            if not candidate.is_file():
                raise FileNotFoundError(f"markdown file not found: {candidate}")
            files.append(candidate)
        return sorted(files)
    return sorted(path for path in repo_root.rglob("*.md") if path.is_file())


def check_markdown_links(repo_root: Path, markdown_files: list[Path]) -> list[LinkIssue]:
    issues: list[LinkIssue] = []
    for markdown_file in markdown_files:
        source_text = markdown_file.read_text(encoding="utf-8")
        for link in extract_links(source_text):
            target = normalize_link_target(markdown_file, link, repo_root)
            if target is None:
                continue
            if not target.exists():
                issues.append(
                    LinkIssue(
                        source=markdown_file,
                        link=link,
                        resolved_target=target,
                        reason="missing_target",
                    )
                )
    return issues


def render_human_report(
    repo_root: Path, markdown_files: list[Path], issues: list[LinkIssue]
) -> str:
    lines = [
        "docs link check",
        f"repo_root={repo_root}",
        f"checked_files={len(markdown_files)}",
        f"issues={len(issues)}",
    ]
    for issue in issues:
        source_display = issue.source.relative_to(repo_root)
        target_display = (
            issue.resolved_target.relative_to(repo_root)
            if issue.resolved_target is not None
            else "n/a"
        )
        lines.append(
            f"- {source_display}: link='{issue.link}' reason={issue.reason} resolved={target_display}"
        )
    return "\n".join(lines)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Validate internal markdown links for Tau docs and README files."
    )
    parser.add_argument(
        "--repo-root",
        default=".",
        help="Repository root used for resolving markdown files and relative links",
    )
    parser.add_argument(
        "--file",
        action="append",
        default=[],
        help="Specific markdown file relative to repo root (repeatable). If omitted, checks all *.md files.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit JSON report instead of human text",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    repo_root = Path(args.repo_root).resolve()
    markdown_files = discover_markdown_files(repo_root, args.file)
    issues = check_markdown_links(repo_root, markdown_files)

    if args.json:
        print(
            json.dumps(
                {
                    "repo_root": str(repo_root),
                    "checked_files": len(markdown_files),
                    "issues": [
                        {
                            "source": str(issue.source.relative_to(repo_root)),
                            "link": issue.link,
                            "reason": issue.reason,
                            "resolved_target": str(issue.resolved_target)
                            if issue.resolved_target is not None
                            else None,
                        }
                        for issue in issues
                    ],
                },
                indent=2,
            )
        )
    else:
        print(render_human_report(repo_root, markdown_files, issues))

    return 0 if not issues else 1


if __name__ == "__main__":
    raise SystemExit(main())
