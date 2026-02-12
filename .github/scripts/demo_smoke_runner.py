#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import shlex
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path


DEMO_SMOKE_SCHEMA_VERSION = 1


@dataclass(frozen=True)
class SmokeCommand:
    name: str
    args: list[str]
    expected_exit_code: int
    stdout_contains: str | None
    stderr_contains: str | None


@dataclass(frozen=True)
class SmokeCommandResult:
    name: str
    args: list[str]
    returncode: int
    duration_ms: int
    stdout_path: Path
    stderr_path: Path
    expected_exit_code: int
    stdout_contains: str | None
    stderr_contains: str | None
    expectation_failures: list[str]

    @property
    def succeeded(self) -> bool:
        return not self.expectation_failures


@dataclass(frozen=True)
class SmokeRunReport:
    total: int
    passed: int
    failed: int
    results: list[SmokeCommandResult]

    @property
    def failed_result(self) -> SmokeCommandResult | None:
        for result in self.results:
            if not result.succeeded:
                return result
        return None


def load_manifest(path: Path) -> list[SmokeCommand]:
    raw = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(raw, dict):
        raise ValueError("manifest must be a JSON object")
    schema_version = raw.get("schema_version")
    if schema_version != DEMO_SMOKE_SCHEMA_VERSION:
        raise ValueError(
            f"unsupported demo smoke manifest schema_version: expected {DEMO_SMOKE_SCHEMA_VERSION}, found {schema_version}"
        )
    commands = raw.get("commands")
    if not isinstance(commands, list) or not commands:
        raise ValueError("manifest commands must be a non-empty array")

    parsed: list[SmokeCommand] = []
    for index, command in enumerate(commands):
        if not isinstance(command, dict):
            raise ValueError(f"commands[{index}] must be an object")
        name = command.get("name")
        if not isinstance(name, str) or not name.strip():
            raise ValueError(f"commands[{index}].name must be a non-empty string")
        args = command.get("args")
        if not isinstance(args, list) or not args:
            raise ValueError(f"commands[{index}].args must be a non-empty array")

        expected_exit_code = command.get("expected_exit_code", 0)
        if not isinstance(expected_exit_code, int) or expected_exit_code < 0:
            raise ValueError(
                f"commands[{index}].expected_exit_code must be a non-negative integer"
            )
        stdout_contains = command.get("stdout_contains")
        if stdout_contains is not None and (
            not isinstance(stdout_contains, str) or not stdout_contains.strip()
        ):
            raise ValueError(
                f"commands[{index}].stdout_contains must be a non-empty string when set"
            )
        stderr_contains = command.get("stderr_contains")
        if stderr_contains is not None and (
            not isinstance(stderr_contains, str) or not stderr_contains.strip()
        ):
            raise ValueError(
                f"commands[{index}].stderr_contains must be a non-empty string when set"
            )

        parsed_args: list[str] = []
        for arg_index, arg in enumerate(args):
            if not isinstance(arg, str) or not arg.strip():
                raise ValueError(
                    f"commands[{index}].args[{arg_index}] must be a non-empty string"
                )
            parsed_args.append(arg)
        parsed.append(
            SmokeCommand(
                name=name.strip(),
                args=parsed_args,
                expected_exit_code=expected_exit_code,
                stdout_contains=stdout_contains.strip() if stdout_contains else None,
                stderr_contains=stderr_contains.strip() if stderr_contains else None,
            )
        )
    return parsed


def sanitize_name(raw: str) -> str:
    sanitized = "".join(ch if ch.isalnum() or ch in {"-", "_"} else "-" for ch in raw)
    sanitized = sanitized.strip("-")
    return sanitized or "command"


def format_command(binary: Path, args: list[str]) -> str:
    return shlex.join([str(binary), *args])


def append_summary(path: Path | None, report: SmokeRunReport, manifest_path: Path) -> None:
    if path is None:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    failed = report.failed_result
    status = "pass" if failed is None else "fail"
    lines = [
        "### Demo Smoke",
        f"- Status: {status}",
        f"- Manifest: {manifest_path}",
        f"- Commands: {report.total}",
        f"- Passed: {report.passed}",
        f"- Failed: {report.failed}",
    ]
    if failed is not None:
        lines.append(f"- Failed command: {failed.name}")
        lines.append(f"- Failed command line: {shlex.join(failed.args)}")
        lines.append(f"- Failed stdout log: {failed.stdout_path}")
        lines.append(f"- Failed stderr log: {failed.stderr_path}")
    with path.open("a", encoding="utf-8") as handle:
        handle.write("\n".join(lines))
        handle.write("\n")


def run_commands(
    commands: list[SmokeCommand],
    binary: Path,
    repo_root: Path,
    log_dir: Path,
    keep_going: bool,
) -> SmokeRunReport:
    log_dir.mkdir(parents=True, exist_ok=True)
    results: list[SmokeCommandResult] = []

    total = len(commands)
    for index, command in enumerate(commands, start=1):
        safe_name = sanitize_name(command.name)
        stdout_path = log_dir / f"{index:02d}-{safe_name}.stdout.log"
        stderr_path = log_dir / f"{index:02d}-{safe_name}.stderr.log"

        command_line = format_command(binary, command.args)
        print(f"[demo-smoke] [{index}/{total}] {command.name}")
        print(f"[demo-smoke] command: {command_line}")
        print(f"[demo-smoke] expected_exit_code: {command.expected_exit_code}")
        if command.stdout_contains is not None:
            print(f"[demo-smoke] expected_stdout_contains: {command.stdout_contains}")
        if command.stderr_contains is not None:
            print(f"[demo-smoke] expected_stderr_contains: {command.stderr_contains}")

        started = time.perf_counter()
        completed = subprocess.run(
            [str(binary), *command.args],
            cwd=repo_root,
            text=True,
            capture_output=True,
            check=False,
        )
        duration_ms = int((time.perf_counter() - started) * 1000)

        stdout_path.write_text(completed.stdout, encoding="utf-8")
        stderr_path.write_text(completed.stderr, encoding="utf-8")

        expectation_failures: list[str] = []
        if completed.returncode != command.expected_exit_code:
            expectation_failures.append(
                f"exit-code-mismatch expected={command.expected_exit_code} actual={completed.returncode}"
            )
        if command.stdout_contains is not None and command.stdout_contains not in completed.stdout:
            expectation_failures.append(
                f"stdout-missing-substring {command.stdout_contains!r}"
            )
        if command.stderr_contains is not None and command.stderr_contains not in completed.stderr:
            expectation_failures.append(
                f"stderr-missing-substring {command.stderr_contains!r}"
            )

        result = SmokeCommandResult(
            name=command.name,
            args=command.args,
            returncode=completed.returncode,
            duration_ms=duration_ms,
            stdout_path=stdout_path,
            stderr_path=stderr_path,
            expected_exit_code=command.expected_exit_code,
            stdout_contains=command.stdout_contains,
            stderr_contains=command.stderr_contains,
            expectation_failures=expectation_failures,
        )
        results.append(result)
        if result.succeeded:
            print(f"[demo-smoke] PASS {command.name} ({duration_ms}ms)")
        else:
            print(f"[demo-smoke] FAIL {command.name} ({duration_ms}ms)")
            print(f"[demo-smoke] expectation failures: {', '.join(result.expectation_failures)}")
            print(f"[demo-smoke] stdout log: {stdout_path}")
            print(f"[demo-smoke] stderr log: {stderr_path}")
            if not keep_going:
                break

    passed = sum(1 for result in results if result.succeeded)
    failed = len(results) - passed
    print(
        f"[demo-smoke] summary: total={len(results)} passed={passed} failed={failed} log_dir={log_dir}"
    )
    return SmokeRunReport(total=len(results), passed=passed, failed=failed, results=results)


def resolve_binary_path(repo_root: Path, binary: str) -> Path:
    path = Path(binary)
    if not path.is_absolute():
        path = repo_root / path
    return path.resolve()


def build_binary(repo_root: Path) -> None:
    subprocess.run(
        ["cargo", "build", "-p", "tau-coding-agent"],
        cwd=repo_root,
        check=True,
    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Run deterministic offline demo smoke commands."
    )
    parser.add_argument(
        "--repo-root",
        default=".",
        help="Repository root used for relative paths and command cwd",
    )
    parser.add_argument(
        "--manifest",
        default=".github/demo-smoke-manifest.json",
        help="Path to demo smoke manifest JSON",
    )
    parser.add_argument(
        "--binary",
        default="target/debug/tau-coding-agent",
        help="tau-coding-agent binary path",
    )
    parser.add_argument(
        "--log-dir",
        default="ci-artifacts/demo-smoke",
        help="Directory where stdout/stderr logs are written",
    )
    parser.add_argument(
        "--summary",
        default="",
        help="Optional path to append markdown summary output",
    )
    parser.add_argument(
        "--build",
        action="store_true",
        help="Build tau-coding-agent before running smoke commands",
    )
    parser.add_argument(
        "--keep-going",
        action="store_true",
        help="Continue running remaining commands after a failure",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    repo_root = Path(args.repo_root).resolve()
    manifest_path = Path(args.manifest)
    if not manifest_path.is_absolute():
        manifest_path = repo_root / manifest_path
    manifest_path = manifest_path.resolve()

    log_dir = Path(args.log_dir)
    if not log_dir.is_absolute():
        log_dir = repo_root / log_dir
    log_dir = log_dir.resolve()

    summary_path: Path | None = None
    if args.summary.strip():
        summary_path = Path(args.summary)
        if not summary_path.is_absolute():
            summary_path = repo_root / summary_path
        summary_path = summary_path.resolve()

    commands = load_manifest(manifest_path)
    if args.build:
        print("[demo-smoke] building tau-coding-agent binary")
        build_binary(repo_root)

    binary_path = resolve_binary_path(repo_root, args.binary)
    if not binary_path.is_file():
        raise FileNotFoundError(
            f"tau-coding-agent binary does not exist: {binary_path} (use --build)"
        )

    report = run_commands(
        commands=commands,
        binary=binary_path,
        repo_root=repo_root,
        log_dir=log_dir,
        keep_going=args.keep_going,
    )
    append_summary(summary_path, report, manifest_path)
    return 0 if report.failed == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
