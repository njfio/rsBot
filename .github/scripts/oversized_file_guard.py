#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from pathlib import Path


JSON_SCHEMA_VERSION = 1


@dataclass(frozen=True)
class Exemption:
    path: str
    threshold_lines: int
    owner_issue: int
    expires_on: str


@dataclass(frozen=True)
class OversizedIssue:
    path: str
    line_count: int
    threshold: int
    threshold_source: str
    owner_issue: int | None
    expires_on: str | None


def should_skip_file(path: Path) -> bool:
    as_posix = path.as_posix()
    return "/tests/" in as_posix or path.name == "tests.rs"


def discover_production_rust_files(repo_root: Path, crates_dir: str) -> list[Path]:
    crates_root = (repo_root / crates_dir).resolve()
    if not crates_root.is_dir():
        return []

    files: list[Path] = []
    for rust_file in sorted(crates_root.rglob("*.rs")):
        if should_skip_file(rust_file):
            continue
        files.append(rust_file)
    return files


def load_exemptions(repo_root: Path, exemptions_file: str) -> dict[str, Exemption]:
    file_path = (repo_root / exemptions_file).resolve()
    if not file_path.is_file():
        raise ValueError(f"exemptions file not found: {exemptions_file}")

    payload = json.loads(file_path.read_text(encoding="utf-8"))
    if payload.get("schema_version") != 1:
        raise ValueError("exemptions schema_version must be 1")

    exemptions_raw = payload.get("exemptions", [])
    if not isinstance(exemptions_raw, list):
        raise ValueError("exemptions must be a list")

    exemptions: dict[str, Exemption] = {}
    for index, entry in enumerate(exemptions_raw):
        if not isinstance(entry, dict):
            raise ValueError(f"exemptions[{index}] must be an object")

        path = str(entry.get("path", "")).strip()
        threshold_lines = entry.get("threshold_lines")
        owner_issue = entry.get("owner_issue")
        expires_on = str(entry.get("expires_on", "")).strip()

        if not path:
            raise ValueError(f"exemptions[{index}].path is required")
        if not isinstance(threshold_lines, int) or threshold_lines < 1:
            raise ValueError(f"exemptions[{index}].threshold_lines must be a positive integer")
        if not isinstance(owner_issue, int) or owner_issue < 1:
            raise ValueError(f"exemptions[{index}].owner_issue must be a positive integer")
        if not expires_on:
            raise ValueError(f"exemptions[{index}].expires_on is required")
        if path in exemptions:
            raise ValueError(f"duplicate exemption path: {path}")

        exemptions[path] = Exemption(
            path=path,
            threshold_lines=threshold_lines,
            owner_issue=owner_issue,
            expires_on=expires_on,
        )
    return exemptions


def line_count(path: Path) -> int:
    return len(path.read_text(encoding="utf-8").splitlines())


def find_oversized_issues(
    repo_root: Path,
    rust_files: list[Path],
    exemptions: dict[str, Exemption],
    default_threshold: int,
) -> list[OversizedIssue]:
    issues: list[OversizedIssue] = []
    for rust_file in rust_files:
        rel_path = rust_file.resolve().relative_to(repo_root.resolve()).as_posix()
        exemption = exemptions.get(rel_path)
        threshold = exemption.threshold_lines if exemption is not None else default_threshold
        threshold_source = "exemption" if exemption is not None else "default"
        file_lines = line_count(rust_file)
        if file_lines <= threshold:
            continue
        issues.append(
            OversizedIssue(
                path=rel_path,
                line_count=file_lines,
                threshold=threshold,
                threshold_source=threshold_source,
                owner_issue=exemption.owner_issue if exemption is not None else None,
                expires_on=exemption.expires_on if exemption is not None else None,
            )
        )
    return issues


def build_json_payload(
    repo_root: Path,
    default_threshold: int,
    exemptions_file: str,
    policy_guide: str,
    checked_files: int,
    issues: list[OversizedIssue],
) -> dict[str, object]:
    return {
        "schema_version": JSON_SCHEMA_VERSION,
        "repo_root": str(repo_root),
        "default_threshold": default_threshold,
        "policy_guide": policy_guide,
        "exemptions_file": exemptions_file,
        "checked_file_count": checked_files,
        "issue_count": len(issues),
        "issues": [
            {
                "path": issue.path,
                "line_count": issue.line_count,
                "threshold": issue.threshold,
                "threshold_source": issue.threshold_source,
                "owner_issue": issue.owner_issue,
                "expires_on": issue.expires_on,
            }
            for issue in issues
        ],
    }


def escape_annotation(value: str) -> str:
    return (
        value.replace("%", "%25")
        .replace("\r", "%0D")
        .replace("\n", "%0A")
        .replace(":", "%3A")
    )


def render_issue_message(issue: OversizedIssue, policy_guide: str, exemptions_file: str) -> str:
    owner_detail = (
        f", owner_issue=#{issue.owner_issue}, expires_on={issue.expires_on}"
        if issue.owner_issue is not None
        else ""
    )
    return (
        f"{issue.path} has {issue.line_count} lines; threshold is {issue.threshold} "
        f"(source={issue.threshold_source}{owner_detail}). "
        f"Split file modules or update auditable exemption metadata in {exemptions_file}. "
        f"Policy: {policy_guide}"
    )


def emit_annotations(issues: list[OversizedIssue], policy_guide: str, exemptions_file: str) -> None:
    for issue in issues:
        message = render_issue_message(issue, policy_guide, exemptions_file)
        print(
            f"::error file={issue.path},line=1,title=Oversized file threshold exceeded::"
            f"{escape_annotation(message)}"
        )


def render_human_report(
    checked_files: int,
    default_threshold: int,
    policy_guide: str,
    exemptions_file: str,
    issues: list[OversizedIssue],
) -> str:
    lines = [
        "oversized-file guard",
        f"checked_files={checked_files}",
        f"default_threshold={default_threshold}",
        f"policy_guide={policy_guide}",
        f"exemptions_file={exemptions_file}",
        f"issues={len(issues)}",
    ]
    for issue in issues:
        owner_detail = (
            f" owner_issue=#{issue.owner_issue} expires_on={issue.expires_on}"
            if issue.owner_issue is not None
            else ""
        )
        lines.append(
            f"! {issue.path}: lines={issue.line_count} threshold={issue.threshold} "
            f"source={issue.threshold_source}{owner_detail}"
        )
    return "\n".join(lines)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Validate production Rust source file line budgets with actionable CI annotations."
    )
    parser.add_argument("--repo-root", default=".", help="Repository root path")
    parser.add_argument("--crates-dir", default="crates", help="Relative crates directory")
    parser.add_argument(
        "--default-threshold",
        type=int,
        default=4000,
        help="Default line threshold for production Rust files",
    )
    parser.add_argument(
        "--exemptions-file",
        default="tasks/policies/oversized-file-exemptions.json",
        help="Relative path to exemption metadata JSON file",
    )
    parser.add_argument(
        "--policy-guide",
        default="docs/guides/oversized-file-policy.md",
        help="Relative policy guide path printed in remediation output",
    )
    parser.add_argument(
        "--json-output-file",
        default=None,
        help="Optional relative path for machine-readable JSON report",
    )
    parser.add_argument(
        "--no-annotations",
        action="store_true",
        help="Disable GitHub Actions ::error annotation output",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo_root = Path(args.repo_root).resolve()

    if args.default_threshold < 1:
        raise SystemExit("default threshold must be positive")

    try:
        exemptions = load_exemptions(repo_root, args.exemptions_file)
    except ValueError as error:
        print("oversized-file guard")
        print(f"policy_guide={args.policy_guide}")
        print(f"exemptions_file={args.exemptions_file}")
        print(f"! exemption_metadata_error: {error}")
        print(
            "::error file="
            f"{args.exemptions_file},line=1,title=Oversized file policy metadata error::"
            f"{escape_annotation(str(error))}"
        )
        return 1

    rust_files = discover_production_rust_files(repo_root, args.crates_dir)
    issues = find_oversized_issues(
        repo_root=repo_root,
        rust_files=rust_files,
        exemptions=exemptions,
        default_threshold=args.default_threshold,
    )

    print(
        render_human_report(
            checked_files=len(rust_files),
            default_threshold=args.default_threshold,
            policy_guide=args.policy_guide,
            exemptions_file=args.exemptions_file,
            issues=issues,
        )
    )

    if args.json_output_file:
        report_path = (repo_root / args.json_output_file).resolve()
        report_path.parent.mkdir(parents=True, exist_ok=True)
        payload = build_json_payload(
            repo_root=repo_root,
            default_threshold=args.default_threshold,
            exemptions_file=args.exemptions_file,
            policy_guide=args.policy_guide,
            checked_files=len(rust_files),
            issues=issues,
        )
        report_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

    if issues and not args.no_annotations:
        emit_annotations(issues, args.policy_guide, args.exemptions_file)

    return 1 if issues else 0


if __name__ == "__main__":
    raise SystemExit(main())
