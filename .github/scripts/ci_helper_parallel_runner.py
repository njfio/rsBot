#!/usr/bin/env python3
"""Run CI helper unittest modules in parallel worker processes."""

from __future__ import annotations

import argparse
import concurrent.futures
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


@dataclass(frozen=True)
class ModuleResult:
    path: str
    return_code: int
    duration_ms: int
    stdout: str
    stderr: str


def discover_modules(start_dir: Path, pattern: str) -> list[Path]:
    if not start_dir.is_dir():
        raise ValueError(f"start directory does not exist: {start_dir}")
    return sorted(path for path in start_dir.glob(pattern) if path.is_file())


def run_module(module_path: Path) -> ModuleResult:
    start = time.perf_counter()
    completed = subprocess.run(
        [sys.executable, str(module_path)],
        text=True,
        capture_output=True,
        check=False,
    )
    duration_ms = int(round((time.perf_counter() - start) * 1000))
    return ModuleResult(
        path=str(module_path),
        return_code=completed.returncode,
        duration_ms=duration_ms,
        stdout=completed.stdout,
        stderr=completed.stderr,
    )


def run_modules(modules: Iterable[Path], workers: int) -> list[ModuleResult]:
    module_list = list(modules)
    if workers == 1:
        return [run_module(module_path) for module_path in module_list]

    results: list[ModuleResult] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as executor:
        futures = [executor.submit(run_module, module_path) for module_path in module_list]
        for future in concurrent.futures.as_completed(futures):
            results.append(future.result())
    return sorted(results, key=lambda item: item.path)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run helper unittest modules in parallel while preserving discovery scope/pattern.",
    )
    parser.add_argument("--workers", type=int, default=4, help="parallel worker count (default: 4)")
    parser.add_argument("--start-dir", default=".github/scripts", help="module discovery directory")
    parser.add_argument("--pattern", default="test_*.py", help="module file glob pattern")
    parser.add_argument("--quiet", action="store_true", help="suppress per-module summary output")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.workers <= 0:
        print("error: --workers must be greater than zero", file=sys.stderr)
        return 2

    try:
        modules = discover_modules(Path(args.start_dir), args.pattern)
    except ValueError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    if not modules:
        print(
            f"error: no helper tests discovered in {args.start_dir} with pattern {args.pattern}",
            file=sys.stderr,
        )
        return 2

    results = run_modules(modules, args.workers)
    failures = [result for result in results if result.return_code != 0]

    if not args.quiet:
        for result in results:
            print(
                f"[helper] module={result.path} duration_ms={result.duration_ms} rc={result.return_code}",
            )
        print(
            f"[helper] completed modules={len(results)} failures={len(failures)} workers={args.workers}",
        )

    if failures:
        for failure in failures:
            print(f"--- failure: {failure.path} ---", file=sys.stderr)
            if failure.stdout.strip():
                print(failure.stdout.rstrip(), file=sys.stderr)
            if failure.stderr.strip():
                print(failure.stderr.rstrip(), file=sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
