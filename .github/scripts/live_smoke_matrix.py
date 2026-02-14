#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class SurfacePlan:
    surface: str
    primary_wrapper: str
    fallback_wrapper: str
    artifact_dirs: tuple[str, ...]
    timeout_seconds: int


SURFACE_PLANS: dict[str, SurfacePlan] = {
    "voice": SurfacePlan(
        surface="voice",
        primary_wrapper="scripts/demo/voice.sh",
        fallback_wrapper="",
        artifact_dirs=(".tau/demo-voice",),
        timeout_seconds=180,
    ),
    "browser": SurfacePlan(
        surface="browser",
        primary_wrapper="scripts/demo/browser-automation-live.sh",
        fallback_wrapper="scripts/demo/browser-automation.sh",
        artifact_dirs=(".tau/demo-browser-automation-live", ".tau/demo-browser-automation"),
        timeout_seconds=180,
    ),
    "dashboard": SurfacePlan(
        surface="dashboard",
        primary_wrapper="scripts/demo/dashboard.sh",
        fallback_wrapper="",
        artifact_dirs=(".tau/demo-dashboard",),
        timeout_seconds=180,
    ),
    "custom-command": SurfacePlan(
        surface="custom-command",
        primary_wrapper="scripts/demo/custom-command.sh",
        fallback_wrapper="",
        artifact_dirs=(".tau/demo-custom-command",),
        timeout_seconds=180,
    ),
    "memory": SurfacePlan(
        surface="memory",
        primary_wrapper="scripts/demo/memory.sh",
        fallback_wrapper="",
        artifact_dirs=(".tau/demo-memory",),
        timeout_seconds=180,
    ),
}


def resolve_surface_plan(surface: str) -> SurfacePlan:
    key = surface.strip().lower()
    if key not in SURFACE_PLANS:
        supported = ", ".join(sorted(SURFACE_PLANS.keys()))
        raise ValueError(f"unsupported live-smoke surface '{surface}' (supported: {supported})")
    return SURFACE_PLANS[key]


def emit_outputs(path: Path, plan: SurfacePlan) -> None:
    payload = {
        "surface": plan.surface,
        "primary_wrapper": plan.primary_wrapper,
        "fallback_wrapper": plan.fallback_wrapper,
        "artifact_dirs": list(plan.artifact_dirs),
        "timeout_seconds": plan.timeout_seconds,
    }
    lines = [
        f"surface={plan.surface}",
        f"primary_wrapper={plan.primary_wrapper}",
        f"fallback_wrapper={plan.fallback_wrapper}",
        f"artifact_dirs_json={json.dumps(payload['artifact_dirs'])}",
        f"timeout_seconds={plan.timeout_seconds}",
        f"plan_json={json.dumps(payload)}",
    ]
    with path.open("a", encoding="utf-8") as handle:
        for line in lines:
            handle.write(f"{line}\n")


def append_summary(path: Path, plan: SurfacePlan) -> None:
    lines = [
        "### Live Smoke Surface Plan",
        f"- Surface: {plan.surface}",
        f"- Primary wrapper: `{plan.primary_wrapper}`",
        (
            f"- Fallback wrapper: `{plan.fallback_wrapper}`"
            if plan.fallback_wrapper
            else "- Fallback wrapper: none"
        ),
        f"- Timeout seconds: {plan.timeout_seconds}",
        "- Artifact roots:",
    ]
    lines.extend([f"  - `{root}`" for root in plan.artifact_dirs])
    with path.open("a", encoding="utf-8") as handle:
        handle.write("\n".join(lines))
        handle.write("\n")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Resolve CI live-smoke matrix surface plans.")
    parser.add_argument("--surface", required=True, help="Surface key to resolve")
    parser.add_argument(
        "--output",
        default="",
        help="Optional GitHub output file path to write key=value fields",
    )
    parser.add_argument(
        "--summary",
        default="",
        help="Optional markdown summary file path append target",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Print resolved plan JSON to stdout",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    plan = resolve_surface_plan(args.surface)

    if args.output.strip():
        emit_outputs(Path(args.output), plan)
    if args.summary.strip():
        append_summary(Path(args.summary), plan)
    if args.json or not args.output.strip():
        print(
            json.dumps(
                {
                    "surface": plan.surface,
                    "primary_wrapper": plan.primary_wrapper,
                    "fallback_wrapper": plan.fallback_wrapper,
                    "artifact_dirs": list(plan.artifact_dirs),
                    "timeout_seconds": plan.timeout_seconds,
                }
            )
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
