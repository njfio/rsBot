#!/usr/bin/env python3

from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path


TRUE_VALUES = {"1", "true", "yes", "on"}


@dataclass(frozen=True)
class QualityModeDecision:
    mode: str
    reason: str
    heavy_changed: bool
    run_coverage: bool
    run_cross_platform: bool
    heavy_reason: str


def parse_bool(raw: str | None) -> bool:
    if raw is None:
        return False
    return raw.strip().lower() in TRUE_VALUES


def resolve_quality_mode(
    event_name: str,
    head_ref: str,
    heavy_changed: bool,
    run_coverage_requested: bool,
    run_cross_platform_requested: bool,
) -> QualityModeDecision:
    event = (event_name or "").strip().lower()
    head = (head_ref or "").strip()
    is_codex_branch = head.startswith("codex/")
    run_coverage = False
    run_cross_platform = False
    heavy_reason = "non-deferred-event"

    if event == "schedule":
        run_coverage = True
        run_cross_platform = True
        heavy_reason = "scheduled-deferred-heavy"
    elif event == "workflow_dispatch":
        run_coverage = run_coverage_requested
        run_cross_platform = run_cross_platform_requested
        if run_coverage or run_cross_platform:
            heavy_reason = "manual-dispatch-requested"
        else:
            heavy_reason = "manual-dispatch-default"
    elif event == "pull_request":
        heavy_reason = "pull-request-cost-governed"

    if event == "pull_request" and is_codex_branch and not heavy_changed:
        return QualityModeDecision(
            mode="codex-light",
            reason="codex-branch-non-heavy-pr",
            heavy_changed=heavy_changed,
            run_coverage=run_coverage,
            run_cross_platform=run_cross_platform,
            heavy_reason=heavy_reason,
        )
    if event == "pull_request" and is_codex_branch and heavy_changed:
        return QualityModeDecision(
            mode="full",
            reason="codex-branch-heavy-pr",
            heavy_changed=heavy_changed,
            run_coverage=run_coverage,
            run_cross_platform=run_cross_platform,
            heavy_reason=heavy_reason,
        )
    if event == "pull_request":
        return QualityModeDecision(
            mode="full",
            reason="pull-request-default",
            heavy_changed=heavy_changed,
            run_coverage=run_coverage,
            run_cross_platform=run_cross_platform,
            heavy_reason=heavy_reason,
        )
    return QualityModeDecision(
        mode="full",
        reason="non-pr-default",
        heavy_changed=heavy_changed,
        run_coverage=run_coverage,
        run_cross_platform=run_cross_platform,
        heavy_reason=heavy_reason,
    )


def render_summary(decision: QualityModeDecision) -> str:
    lane = "light (codex smoke)" if decision.mode == "codex-light" else "full (workspace)"
    heavy = "true" if decision.heavy_changed else "false"
    run_coverage = "true" if decision.run_coverage else "false"
    run_cross_platform = "true" if decision.run_cross_platform else "false"
    heavy_lane = (
        "enabled"
        if decision.run_coverage or decision.run_cross_platform
        else "disabled"
    )
    return "\n".join(
        [
            "### CI Cost Governance",
            f"- Mode: {decision.mode}",
            f"- Lane: {lane}",
            f"- Reason: {decision.reason}",
            f"- Heavy paths changed: {heavy}",
            f"- Heavy lane: {heavy_lane}",
            f"- Heavy reason: {decision.heavy_reason}",
            f"- Run coverage: {run_coverage}",
            f"- Run cross-platform smoke: {run_cross_platform}",
        ]
    )


def append_github_output(output_path: Path, decision: QualityModeDecision) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    heavy = "true" if decision.heavy_changed else "false"
    run_coverage = "true" if decision.run_coverage else "false"
    run_cross_platform = "true" if decision.run_cross_platform else "false"
    lines = [
        f"mode={decision.mode}",
        f"reason={decision.reason}",
        f"heavy_changed={heavy}",
        f"run_coverage={run_coverage}",
        f"run_cross_platform={run_cross_platform}",
        f"heavy_reason={decision.heavy_reason}",
    ]
    with output_path.open("a", encoding="utf-8") as handle:
        for line in lines:
            handle.write(f"{line}\n")


def append_summary(summary_path: Path, summary: str) -> None:
    summary_path.parent.mkdir(parents=True, exist_ok=True)
    with summary_path.open("a", encoding="utf-8") as handle:
        handle.write(f"{summary}\n")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Resolve CI quality mode and render cost-governance diagnostics."
    )
    parser.add_argument("--event-name", default="", help="GitHub event name")
    parser.add_argument("--head-ref", default="", help="GitHub head ref")
    parser.add_argument(
        "--heavy-changed",
        default="false",
        help="Whether heavy paths changed (true/false)",
    )
    parser.add_argument(
        "--run-coverage",
        default="false",
        help="Whether coverage is requested by workflow dispatch input",
    )
    parser.add_argument(
        "--run-cross-platform",
        default="false",
        help="Whether cross-platform smoke is requested by workflow dispatch input",
    )
    parser.add_argument(
        "--output",
        required=True,
        help="Path to GITHUB_OUTPUT-compatible file",
    )
    parser.add_argument(
        "--summary",
        required=True,
        help="Path to GITHUB_STEP_SUMMARY-compatible file",
    )
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    heavy_changed = parse_bool(args.heavy_changed)
    run_coverage_requested = parse_bool(args.run_coverage)
    run_cross_platform_requested = parse_bool(args.run_cross_platform)
    decision = resolve_quality_mode(
        args.event_name,
        args.head_ref,
        heavy_changed,
        run_coverage_requested,
        run_cross_platform_requested,
    )
    append_github_output(Path(args.output), decision)
    append_summary(Path(args.summary), render_summary(decision))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
