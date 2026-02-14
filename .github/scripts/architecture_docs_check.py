#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class SymbolExpectation:
    doc_token: str
    source_token: str


@dataclass(frozen=True)
class ArchitectureDocSpec:
    key: str
    path: str
    marker: str
    required_headings: tuple[str, ...]
    required_symbols: tuple[SymbolExpectation, ...]
    source_files: tuple[str, ...]


@dataclass(frozen=True)
class LinkExpectation:
    file_path: str
    expected_link: str


@dataclass(frozen=True)
class ArchitectureIssue:
    category: str
    subject: str
    detail: str


DOC_SPECS: tuple[ArchitectureDocSpec, ...] = (
    ArchitectureDocSpec(
        key="startup-di",
        path="docs/guides/startup-di-pipeline.md",
        marker="<!-- architecture-doc:startup-di -->",
        required_headings=(
            "## Stage 1: Preflight command gate",
            "## Stage 2: Dependency and context resolution",
            "## Stage 3: Runtime mode dispatch",
        ),
        required_symbols=(
            SymbolExpectation("`execute_startup_preflight`", "execute_startup_preflight"),
            SymbolExpectation(
                "`resolve_startup_model_runtime_from_cli`",
                "resolve_startup_model_runtime_from_cli",
            ),
            SymbolExpectation(
                "`resolve_startup_runtime_dispatch_context_from_cli`",
                "resolve_startup_runtime_dispatch_context_from_cli",
            ),
            SymbolExpectation(
                "`build_startup_runtime_dispatch_context`",
                "build_startup_runtime_dispatch_context",
            ),
            SymbolExpectation("`execute_startup_runtime_modes`", "execute_startup_runtime_modes"),
            SymbolExpectation("`run_transport_mode_if_requested`", "run_transport_mode_if_requested"),
        ),
        source_files=(
            "crates/tau-coding-agent/src/startup_dispatch.rs",
            "crates/tau-onboarding/src/startup_dispatch.rs",
            "crates/tau-onboarding/src/startup_preflight.rs",
            "crates/tau-startup/src/lib.rs",
        ),
    ),
    ArchitectureDocSpec(
        key="contract-pattern",
        path="docs/guides/contract-pattern-lifecycle.md",
        marker="<!-- architecture-doc:contract-pattern -->",
        required_headings=(
            "## Purpose",
            "## When to apply the pattern",
            "## Extension process checklist",
            "## Anti-patterns",
        ),
        required_symbols=(
            SymbolExpectation("`parse_fixture_with_validation`", "parse_fixture_with_validation"),
            SymbolExpectation("`load_fixture_from_path`", "load_fixture_from_path"),
            SymbolExpectation("`validate_fixture_header`", "validate_fixture_header"),
            SymbolExpectation("`ensure_unique_case_ids`", "ensure_unique_case_ids"),
        ),
        source_files=(
            "crates/tau-contract/src/lib.rs",
            "crates/tau-custom-command/src/custom_command_contract.rs",
            "crates/tau-dashboard/src/dashboard_contract.rs",
            "crates/tau-gateway/src/gateway_contract.rs",
            "crates/tau-multi-channel/src/multi_channel_contract.rs",
        ),
    ),
    ArchitectureDocSpec(
        key="multi-channel-event-pipeline",
        path="docs/guides/multi-channel-event-pipeline.md",
        marker="<!-- architecture-doc:multi-channel-event-pipeline -->",
        required_headings=(
            "## End-to-end stages",
            "## Pipeline diagram (with retry/failure paths)",
            "## Failure and retry semantics",
        ),
        required_symbols=(
            SymbolExpectation(
                "`parse_multi_channel_live_inbound_envelope`",
                "parse_multi_channel_live_inbound_envelope",
            ),
            SymbolExpectation(
                "`evaluate_multi_channel_channel_policy`",
                "evaluate_multi_channel_channel_policy",
            ),
            SymbolExpectation("`resolve_multi_channel_event_route`", "resolve_multi_channel_event_route"),
            SymbolExpectation("`persist_event`", "persist_event"),
            SymbolExpectation(
                "`MultiChannelOutboundDispatcher::deliver`",
                "pub async fn deliver",
            ),
        ),
        source_files=(
            "crates/tau-multi-channel/src/multi_channel_live_ingress.rs",
            "crates/tau-multi-channel/src/multi_channel_runtime.rs",
            "crates/tau-multi-channel/src/multi_channel_policy.rs",
            "crates/tau-multi-channel/src/multi_channel_routing.rs",
            "crates/tau-multi-channel/src/multi_channel_outbound.rs",
        ),
    ),
)


NAV_LINK_EXPECTATIONS: tuple[LinkExpectation, ...] = (
    LinkExpectation("README.md", "docs/guides/startup-di-pipeline.md"),
    LinkExpectation("README.md", "docs/guides/contract-pattern-lifecycle.md"),
    LinkExpectation("README.md", "docs/guides/multi-channel-event-pipeline.md"),
    LinkExpectation("docs/README.md", "guides/startup-di-pipeline.md"),
    LinkExpectation("docs/README.md", "guides/contract-pattern-lifecycle.md"),
    LinkExpectation("docs/README.md", "guides/multi-channel-event-pipeline.md"),
    LinkExpectation("crates/tau-startup/src/lib.rs", "docs/guides/startup-di-pipeline.md"),
    LinkExpectation(
        "crates/tau-contract/src/lib.rs",
        "docs/guides/contract-pattern-lifecycle.md",
    ),
    LinkExpectation(
        "crates/tau-multi-channel/src/lib.rs",
        "docs/guides/multi-channel-event-pipeline.md",
    ),
)


def contains_fenced_block(markdown: str, language: str) -> bool:
    pattern = re.compile(rf"```{re.escape(language)}\b[\s\S]*?```", re.MULTILINE)
    return bool(pattern.search(markdown))


def read_text_if_exists(path: Path) -> str | None:
    if not path.is_file():
        return None
    return path.read_text(encoding="utf-8")


def check_doc_spec(repo_root: Path, spec: ArchitectureDocSpec) -> list[ArchitectureIssue]:
    issues: list[ArchitectureIssue] = []
    doc_path = repo_root / spec.path
    markdown = read_text_if_exists(doc_path)
    if markdown is None:
        issues.append(
            ArchitectureIssue(
                category="missing_doc",
                subject=spec.path,
                detail="architecture document is missing",
            )
        )
        return issues

    if spec.marker not in markdown:
        issues.append(
            ArchitectureIssue(
                category="missing_marker",
                subject=spec.path,
                detail=f"missing marker '{spec.marker}'",
            )
        )

    for heading in spec.required_headings:
        if heading not in markdown:
            issues.append(
                ArchitectureIssue(
                    category="missing_heading",
                    subject=spec.path,
                    detail=f"missing heading '{heading}'",
                )
            )

    if not contains_fenced_block(markdown, "mermaid"):
        issues.append(
            ArchitectureIssue(
                category="missing_mermaid",
                subject=spec.path,
                detail="missing mermaid architecture diagram",
            )
        )

    if not contains_fenced_block(markdown, "bash"):
        issues.append(
            ArchitectureIssue(
                category="missing_validation_snippet",
                subject=spec.path,
                detail="missing bash validation snippet",
            )
        )

    source_texts: list[tuple[str, str]] = []
    for source_file in spec.source_files:
        source_path = repo_root / source_file
        source_text = read_text_if_exists(source_path)
        if source_text is None:
            issues.append(
                ArchitectureIssue(
                    category="missing_source_file",
                    subject=source_file,
                    detail="source file referenced by architecture guard is missing",
                )
            )
            continue
        source_texts.append((source_file, source_text))

    for symbol in spec.required_symbols:
        if symbol.doc_token not in markdown:
            issues.append(
                ArchitectureIssue(
                    category="missing_symbol_in_doc",
                    subject=spec.path,
                    detail=f"missing symbol token '{symbol.doc_token}'",
                )
            )

        if not any(symbol.source_token in source_text for _, source_text in source_texts):
            issues.append(
                ArchitectureIssue(
                    category="stale_symbol_source_missing",
                    subject=spec.path,
                    detail=(
                        "symbol token "
                        f"'{symbol.source_token}' no longer found in referenced source files"
                    ),
                )
            )

    return issues


def check_navigation_expectations(repo_root: Path) -> list[ArchitectureIssue]:
    issues: list[ArchitectureIssue] = []
    for expectation in NAV_LINK_EXPECTATIONS:
        path = repo_root / expectation.file_path
        content = read_text_if_exists(path)
        if content is None:
            issues.append(
                ArchitectureIssue(
                    category="missing_navigation_file",
                    subject=expectation.file_path,
                    detail="navigation file is missing",
                )
            )
            continue
        if expectation.expected_link not in content:
            issues.append(
                ArchitectureIssue(
                    category="missing_navigation_link",
                    subject=expectation.file_path,
                    detail=f"missing link token '{expectation.expected_link}'",
                )
            )
    return issues


def collect_architecture_issues(repo_root: Path) -> list[ArchitectureIssue]:
    issues: list[ArchitectureIssue] = []
    for spec in DOC_SPECS:
        issues.extend(check_doc_spec(repo_root, spec))
    issues.extend(check_navigation_expectations(repo_root))
    return issues


def render_human_report(repo_root: Path, issues: list[ArchitectureIssue]) -> str:
    lines = [
        "architecture docs check",
        f"repo_root={repo_root}",
        f"checked_docs={len(DOC_SPECS)}",
        f"issues={len(issues)}",
    ]
    for issue in issues:
        lines.append(f"- {issue.category}: {issue.subject} ({issue.detail})")
    return "\n".join(lines)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Validate architecture docs presence, required diagrams/snippets, "
            "symbol freshness, and repository navigation links."
        )
    )
    parser.add_argument(
        "--repo-root",
        default=".",
        help="Repository root used to resolve docs and source files",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit JSON report",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    repo_root = Path(args.repo_root).resolve()
    issues = collect_architecture_issues(repo_root)

    if args.json:
        print(
            json.dumps(
                {
                    "repo_root": str(repo_root),
                    "checked_docs": len(DOC_SPECS),
                    "issues": [
                        {
                            "category": issue.category,
                            "subject": issue.subject,
                            "detail": issue.detail,
                        }
                        for issue in issues
                    ],
                },
                indent=2,
            )
        )
    else:
        print(render_human_report(repo_root, issues))

    return 0 if not issues else 1


if __name__ == "__main__":
    raise SystemExit(main())
