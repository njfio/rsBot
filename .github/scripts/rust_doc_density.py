#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass
from pathlib import Path


PUB_ITEM_PATTERN = re.compile(
    r"^\s*pub\s+(?:async\s+)?(?P<kind>const|fn|struct|enum|trait|mod|type)\b"
)
LINE_DOC_PATTERN = re.compile(r"^\s*///")


@dataclass(frozen=True)
class PublicItem:
    file_path: Path
    line: int
    kind: str
    signature: str
    documented: bool


@dataclass(frozen=True)
class CrateDensityReport:
    crate: str
    total_public_items: int
    documented_public_items: int
    percent: float


@dataclass(frozen=True)
class ThresholdIssue:
    kind: str
    detail: str


def is_rust_source_file(path: Path) -> bool:
    return path.is_file() and path.suffix == ".rs"


def should_skip_file(path: Path) -> bool:
    as_posix = path.as_posix()
    return "/tests/" in as_posix or path.name == "tests.rs"


def has_preceding_line_doc(lines: list[str], index: int) -> bool:
    cursor = index - 1
    while cursor >= 0 and lines[cursor].strip() == "":
        cursor -= 1

    if cursor < 0:
        return False
    return bool(LINE_DOC_PATTERN.match(lines[cursor]))


def extract_public_items(path: Path) -> list[PublicItem]:
    lines = path.read_text(encoding="utf-8").splitlines()
    items: list[PublicItem] = []
    for index, line in enumerate(lines):
        match = PUB_ITEM_PATTERN.match(line)
        if not match:
            continue
        kind = match.group("kind")
        items.append(
            PublicItem(
                file_path=path,
                line=index + 1,
                kind=kind,
                signature=line.strip(),
                documented=has_preceding_line_doc(lines, index),
            )
        )
    return items


def discover_crate_dirs(crates_root: Path) -> list[Path]:
    crate_dirs: list[Path] = []
    if not crates_root.exists():
        return crate_dirs

    for candidate in sorted(crates_root.iterdir()):
        if not candidate.is_dir():
            continue
        if not (candidate / "Cargo.toml").is_file():
            continue
        if not (candidate / "src").is_dir():
            continue
        crate_dirs.append(candidate)
    return crate_dirs


def compute_density_reports(repo_root: Path, crates_dir: str) -> tuple[list[CrateDensityReport], list[PublicItem]]:
    crates_root = (repo_root / crates_dir).resolve()
    reports: list[CrateDensityReport] = []
    all_items: list[PublicItem] = []

    for crate_dir in discover_crate_dirs(crates_root):
        items: list[PublicItem] = []
        for rust_file in sorted((crate_dir / "src").rglob("*.rs")):
            if not is_rust_source_file(rust_file):
                continue
            if should_skip_file(rust_file):
                continue
            items.extend(extract_public_items(rust_file))

        total = len(items)
        documented = sum(1 for item in items if item.documented)
        percent = (documented / total * 100.0) if total else 100.0

        reports.append(
            CrateDensityReport(
                crate=crate_dir.name,
                total_public_items=total,
                documented_public_items=documented,
                percent=percent,
            )
        )
        all_items.extend(items)

    reports.sort(key=lambda report: (report.percent, report.crate))
    return reports, all_items


def parse_crate_target_arg(raw: str) -> tuple[str, float]:
    if "=" not in raw:
        raise ValueError(f"invalid --crate-min '{raw}' (expected crate=percent)")
    crate, raw_percent = raw.split("=", 1)
    crate = crate.strip()
    if not crate:
        raise ValueError(f"invalid --crate-min '{raw}' (crate name cannot be empty)")
    try:
        percent = float(raw_percent.strip())
    except ValueError as error:
        raise ValueError(f"invalid --crate-min '{raw}' (percent must be numeric)") from error
    if percent < 0.0 or percent > 100.0:
        raise ValueError(f"invalid --crate-min '{raw}' (percent must be between 0 and 100)")
    return crate, percent


def load_targets_file(path: Path) -> tuple[float | None, dict[str, float]]:
    if not path.is_file():
        raise FileNotFoundError(f"targets file not found: {path}")

    payload = json.loads(path.read_text(encoding="utf-8"))
    global_min = payload.get("global_min_percent")
    crate_targets_raw = payload.get("crate_min_percent", {})

    if global_min is not None:
        global_min = float(global_min)
        if global_min < 0.0 or global_min > 100.0:
            raise ValueError("global_min_percent must be between 0 and 100")

    crate_targets: dict[str, float] = {}
    if not isinstance(crate_targets_raw, dict):
        raise ValueError("crate_min_percent must be an object mapping crate to percent")
    for crate, value in crate_targets_raw.items():
        crate_name = str(crate).strip()
        percent = float(value)
        if not crate_name:
            raise ValueError("crate_min_percent contains an empty crate name")
        if percent < 0.0 or percent > 100.0:
            raise ValueError(f"crate target for '{crate_name}' must be between 0 and 100")
        crate_targets[crate_name] = percent

    return global_min, crate_targets


def evaluate_thresholds(
    reports: list[CrateDensityReport],
    global_min_percent: float | None,
    crate_min_percent: dict[str, float],
) -> list[ThresholdIssue]:
    issues: list[ThresholdIssue] = []
    total_public = sum(report.total_public_items for report in reports)
    total_documented = sum(report.documented_public_items for report in reports)
    overall_percent = (total_documented / total_public * 100.0) if total_public else 100.0

    if global_min_percent is not None and overall_percent < global_min_percent:
        issues.append(
            ThresholdIssue(
                kind="global_min_failed",
                detail=(
                    f"overall density {overall_percent:.2f}% is below global minimum "
                    f"{global_min_percent:.2f}%"
                ),
            )
        )

    by_crate = {report.crate: report for report in reports}
    for crate, target in sorted(crate_min_percent.items()):
        report = by_crate.get(crate)
        if report is None:
            issues.append(
                ThresholdIssue(
                    kind="missing_crate",
                    detail=f"crate target configured for '{crate}' but crate was not discovered",
                )
            )
            continue
        if report.percent < target:
            issues.append(
                ThresholdIssue(
                    kind="crate_min_failed",
                    detail=(
                        f"crate '{crate}' density {report.percent:.2f}% is below "
                        f"configured minimum {target:.2f}%"
                    ),
                )
            )

    return issues


def render_human_report(
    repo_root: Path,
    reports: list[CrateDensityReport],
    global_min_percent: float | None,
    crate_min_percent: dict[str, float],
    issues: list[ThresholdIssue],
) -> str:
    total_public = sum(report.total_public_items for report in reports)
    total_documented = sum(report.documented_public_items for report in reports)
    overall_percent = (total_documented / total_public * 100.0) if total_public else 100.0

    lines = [
        "rust doc density check",
        f"repo_root={repo_root}",
        f"crate_count={len(reports)}",
        f"overall_public_items={total_public}",
        f"overall_documented_items={total_documented}",
        f"overall_percent={overall_percent:.2f}",
        f"global_min_percent={global_min_percent if global_min_percent is not None else 'none'}",
        f"crate_targets={len(crate_min_percent)}",
        f"issues={len(issues)}",
    ]

    for report in reports:
        target = crate_min_percent.get(report.crate)
        target_render = f"{target:.2f}" if target is not None else "-"
        lines.append(
            f"- {report.crate}: total={report.total_public_items} documented={report.documented_public_items} "
            f"percent={report.percent:.2f} target={target_render}"
        )

    for issue in issues:
        lines.append(f"! {issue.kind}: {issue.detail}")

    return "\n".join(lines)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Measure rust public API doc density and validate configured thresholds."
    )
    parser.add_argument(
        "--repo-root",
        default=".",
        help="Repository root containing crates/",
    )
    parser.add_argument(
        "--crates-dir",
        default="crates",
        help="Relative directory containing crate folders (default: crates)",
    )
    parser.add_argument(
        "--targets-file",
        help="Optional JSON file with global_min_percent and crate_min_percent targets",
    )
    parser.add_argument(
        "--global-min-percent",
        type=float,
        help="Optional global minimum percent override",
    )
    parser.add_argument(
        "--crate-min",
        action="append",
        default=[],
        help="Per-crate minimum threshold in crate=percent format (repeatable)",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit JSON payload instead of human-readable text",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    repo_root = Path(args.repo_root).resolve()
    reports, _ = compute_density_reports(repo_root, args.crates_dir)

    global_min_percent: float | None = None
    crate_min_percent: dict[str, float] = {}

    if args.targets_file:
        file_global_min, file_crate_targets = load_targets_file(
            (repo_root / args.targets_file).resolve()
        )
        global_min_percent = file_global_min
        crate_min_percent.update(file_crate_targets)

    if args.global_min_percent is not None:
        if args.global_min_percent < 0.0 or args.global_min_percent > 100.0:
            raise ValueError("--global-min-percent must be between 0 and 100")
        global_min_percent = float(args.global_min_percent)

    for raw_target in args.crate_min:
        crate, percent = parse_crate_target_arg(raw_target)
        crate_min_percent[crate] = percent

    issues = evaluate_thresholds(reports, global_min_percent, crate_min_percent)

    if args.json:
        total_public = sum(report.total_public_items for report in reports)
        total_documented = sum(report.documented_public_items for report in reports)
        overall_percent = (total_documented / total_public * 100.0) if total_public else 100.0
        print(
            json.dumps(
                {
                    "repo_root": str(repo_root),
                    "crate_count": len(reports),
                    "overall_public_items": total_public,
                    "overall_documented_items": total_documented,
                    "overall_percent": round(overall_percent, 2),
                    "global_min_percent": global_min_percent,
                    "crate_targets": crate_min_percent,
                    "reports": [
                        {
                            "crate": report.crate,
                            "total_public_items": report.total_public_items,
                            "documented_public_items": report.documented_public_items,
                            "percent": round(report.percent, 2),
                        }
                        for report in reports
                    ],
                    "issues": [
                        {
                            "kind": issue.kind,
                            "detail": issue.detail,
                        }
                        for issue in issues
                    ],
                },
                indent=2,
            )
        )
    else:
        print(
            render_human_report(
                repo_root,
                reports,
                global_min_percent,
                crate_min_percent,
                issues,
            )
        )

    return 0 if not issues else 1


if __name__ == "__main__":
    raise SystemExit(main())
