#!/usr/bin/env python3

from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class OwnershipSpec:
    path: str
    required_tokens: tuple[str, ...]


@dataclass(frozen=True)
class OwnershipIssue:
    category: str
    subject: str
    detail: str


OWNERSHIP_SPECS: tuple[OwnershipSpec, ...] = (
    OwnershipSpec(
        path="docs/guides/demo-index.md",
        required_tokens=(
            "## Ownership",
            "`crates/tau-coding-agent`",
            "`scripts/demo/`",
            "docs/guides/runbook-ownership-map.md",
        ),
    ),
    OwnershipSpec(
        path="docs/guides/training-ops.md",
        required_tokens=(
            "## Ownership",
            "`crates/tau-trainer`",
            "`crates/tau-training-runner`",
            "docs/guides/runbook-ownership-map.md",
        ),
    ),
    OwnershipSpec(
        path="docs/guides/training-proxy-ops.md",
        required_tokens=(
            "## Ownership",
            "`crates/tau-training-proxy`",
            "`crates/tau-gateway`",
            "docs/guides/runbook-ownership-map.md",
        ),
    ),
    OwnershipSpec(
        path="docs/guides/training-crate-boundary-plan.md",
        required_tokens=(
            "## Ownership",
            "`scripts/dev/training-crate-boundary-plan.sh`",
            "`crates/tau-trainer`",
            "docs/guides/runbook-ownership-map.md",
        ),
    ),
    OwnershipSpec(
        path="docs/guides/transports.md",
        required_tokens=(
            "## Ownership",
            "`crates/tau-github-issues-runtime`",
            "`crates/tau-slack-runtime`",
            "docs/guides/runbook-ownership-map.md",
        ),
    ),
    OwnershipSpec(
        path="docs/guides/memory-ops.md",
        required_tokens=(
            "## Ownership",
            "`crates/tau-agent-core`",
            "`crates/tau-memory`",
            "docs/guides/runbook-ownership-map.md",
        ),
    ),
    OwnershipSpec(
        path="docs/guides/dashboard-ops.md",
        required_tokens=(
            "## Ownership",
            "`crates/tau-dashboard`",
            "`crates/tau-gateway`",
            "docs/guides/runbook-ownership-map.md",
        ),
    ),
    OwnershipSpec(
        path="docs/guides/custom-command-ops.md",
        required_tokens=(
            "## Ownership",
            "`crates/tau-custom-command`",
            "`crates/tau-coding-agent`",
            "docs/guides/runbook-ownership-map.md",
        ),
    ),
    OwnershipSpec(
        path="docs/guides/consolidated-runtime-rollback-drill.md",
        required_tokens=(
            "## Ownership",
            "`scripts/demo/rollback-drill-checklist.sh`",
            "`scripts/dev/m21-retained-capability-proof-summary.sh`",
            "docs/guides/runbook-ownership-map.md",
        ),
    ),
    OwnershipSpec(
        path="docs/guides/runbook-ownership-map.md",
        required_tokens=(
            "# Runbook Ownership Map",
            "docs/guides/demo-index.md",
            "docs/guides/training-ops.md",
            "docs/guides/training-proxy-ops.md",
            "docs/guides/training-crate-boundary-plan.md",
            "docs/guides/transports.md",
            "docs/guides/memory-ops.md",
            "docs/guides/dashboard-ops.md",
            "docs/guides/custom-command-ops.md",
            "docs/guides/consolidated-runtime-rollback-drill.md",
        ),
    ),
)


README_EXPECTED_LINKS: tuple[str, ...] = (
    "guides/runbook-ownership-map.md",
    "guides/training-crate-boundary-plan.md",
    "guides/dashboard-ops.md",
    "guides/custom-command-ops.md",
)


def read_text_if_exists(path: Path) -> str | None:
    if not path.is_file():
        return None
    return path.read_text(encoding="utf-8")


def collect_ownership_issues(repo_root: Path) -> list[OwnershipIssue]:
    issues: list[OwnershipIssue] = []

    readme_path = repo_root / "docs" / "README.md"
    readme = read_text_if_exists(readme_path)
    if readme is None:
        issues.append(
            OwnershipIssue(
                category="missing_doc",
                subject="docs/README.md",
                detail="documentation index file is missing",
            )
        )
    else:
        for link in README_EXPECTED_LINKS:
            if link not in readme:
                issues.append(
                    OwnershipIssue(
                        category="missing_readme_link",
                        subject="docs/README.md",
                        detail=f"missing docs index link '{link}'",
                    )
                )

    for spec in OWNERSHIP_SPECS:
        path = repo_root / spec.path
        content = read_text_if_exists(path)
        if content is None:
            issues.append(
                OwnershipIssue(
                    category="missing_doc",
                    subject=spec.path,
                    detail="runbook ownership file is missing",
                )
            )
            continue

        for token in spec.required_tokens:
            if token not in content:
                issues.append(
                    OwnershipIssue(
                        category="missing_token",
                        subject=spec.path,
                        detail=f"missing ownership token '{token}'",
                    )
                )

    return issues


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Validate runbook ownership sections and ownership map links."
    )
    parser.add_argument(
        "--repo-root",
        default=".",
        help="Repository root path",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    repo_root = Path(args.repo_root).resolve()
    issues = collect_ownership_issues(repo_root)
    checked = len(OWNERSHIP_SPECS) + 1

    print(f"checked_docs={checked} issues={len(issues)}")
    for issue in issues:
        print(f"[{issue.category}] {issue.subject}: {issue.detail}")

    return 0 if not issues else 1


if __name__ == "__main__":
    raise SystemExit(main())
