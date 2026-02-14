#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
import shlex
import shutil
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path


SURFACE_MANIFEST_SCHEMA_VERSION = 1
UNIFIED_RUN_SCHEMA_VERSION = 1


@dataclass(frozen=True)
class SurfaceSpec:
    surface_id: str
    script: str
    artifact_roots: list[str]


@dataclass(frozen=True)
class SurfaceRunResult:
    surface_id: str
    script: str
    command: list[str]
    status: str
    exit_code: int
    duration_ms: int
    stdout_log: Path
    stderr_log: Path
    artifacts_dir: Path
    artifacts: list[dict[str, object]]
    missing_artifact_roots: list[str]
    summary_line: str
    diagnostics: list[str]


def load_surface_manifest(path: Path) -> list[SurfaceSpec]:
    raw = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(raw, dict):
        raise ValueError("surface manifest must be a JSON object")

    schema_version = raw.get("schema_version")
    if schema_version != SURFACE_MANIFEST_SCHEMA_VERSION:
        raise ValueError(
            "unsupported live-run surface manifest schema_version: "
            f"expected {SURFACE_MANIFEST_SCHEMA_VERSION}, found {schema_version}"
        )

    surfaces = raw.get("surfaces")
    if not isinstance(surfaces, list) or not surfaces:
        raise ValueError("surface manifest 'surfaces' must be a non-empty array")

    parsed: list[SurfaceSpec] = []
    seen_ids: set[str] = set()
    for index, entry in enumerate(surfaces):
        if not isinstance(entry, dict):
            raise ValueError(f"surfaces[{index}] must be an object")

        surface_id = entry.get("id")
        if not isinstance(surface_id, str) or not surface_id.strip():
            raise ValueError(f"surfaces[{index}].id must be a non-empty string")
        normalized_id = surface_id.strip()
        if normalized_id in seen_ids:
            raise ValueError(f"surface manifest contains duplicate id '{normalized_id}'")
        seen_ids.add(normalized_id)

        script = entry.get("script")
        if not isinstance(script, str) or not script.strip():
            raise ValueError(f"surfaces[{index}].script must be a non-empty string")

        artifact_roots = entry.get("artifact_roots")
        if not isinstance(artifact_roots, list) or not artifact_roots:
            raise ValueError(
                f"surfaces[{index}].artifact_roots must be a non-empty array"
            )
        parsed_roots: list[str] = []
        for root_index, root in enumerate(artifact_roots):
            if not isinstance(root, str) or not root.strip():
                raise ValueError(
                    f"surfaces[{index}].artifact_roots[{root_index}] must be a non-empty string"
                )
            parsed_roots.append(root.strip())

        parsed.append(
            SurfaceSpec(
                surface_id=normalized_id,
                script=script.strip(),
                artifact_roots=parsed_roots,
            )
        )
    return parsed


def resolve_path(root: Path, raw: str) -> Path:
    candidate = Path(raw)
    if candidate.is_absolute():
        return candidate.resolve()
    return (root / candidate).resolve()


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while True:
            chunk = handle.read(1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest()


def clean_relative_label(raw: str) -> str:
    label = raw.strip().replace("\\", "/")
    if label.startswith("./"):
        label = label[2:]
    label = label.strip("/")
    return label or "root"


def copy_surface_artifacts(
    repo_root: Path,
    surface_output_dir: Path,
    artifact_roots: list[str],
) -> tuple[list[dict[str, object]], list[str]]:
    artifacts_dir = surface_output_dir / "artifacts"
    artifacts_dir.mkdir(parents=True, exist_ok=True)

    missing_roots: list[str] = []
    for root in artifact_roots:
        source = resolve_path(repo_root, root)
        if not source.exists():
            missing_roots.append(root)
            continue

        destination = artifacts_dir / clean_relative_label(root)
        if source.is_file():
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
            continue

        if destination.exists():
            shutil.rmtree(destination)
        shutil.copytree(source, destination)

    artifacts: list[dict[str, object]] = []
    if artifacts_dir.exists():
        for path in sorted(artifacts_dir.rglob("*")):
            if not path.is_file():
                continue
            relative = path.relative_to(surface_output_dir).as_posix()
            artifacts.append(
                {
                    "relative_path": relative,
                    "bytes": path.stat().st_size,
                    "sha256": file_sha256(path),
                }
            )
    return artifacts, missing_roots


def extract_summary_line(stdout: str) -> str:
    for line in reversed(stdout.splitlines()):
        if "summary:" in line:
            return line.strip()
    return ""


def run_surface(
    spec: SurfaceSpec,
    repo_root: Path,
    output_dir: Path,
    binary: Path,
    skip_build: bool,
    timeout_seconds: int | None,
) -> SurfaceRunResult:
    script_path = resolve_path(repo_root, spec.script)
    if not script_path.exists():
        raise FileNotFoundError(f"surface script does not exist: {script_path}")

    surface_dir = output_dir / "surfaces" / spec.surface_id
    surface_dir.mkdir(parents=True, exist_ok=True)
    stdout_log = surface_dir / "stdout.log"
    stderr_log = surface_dir / "stderr.log"

    command = [
        str(script_path),
        "--repo-root",
        str(repo_root),
        "--binary",
        str(binary),
    ]
    if skip_build:
        command.append("--skip-build")
    if timeout_seconds is not None:
        command.extend(["--timeout-seconds", str(timeout_seconds)])

    started = time.perf_counter()
    completed = subprocess.run(
        command,
        cwd=repo_root,
        text=True,
        capture_output=True,
        check=False,
    )
    duration_ms = int((time.perf_counter() - started) * 1000)

    stdout_log.write_text(completed.stdout, encoding="utf-8")
    stderr_log.write_text(completed.stderr, encoding="utf-8")

    artifacts, missing_roots = copy_surface_artifacts(
        repo_root=repo_root,
        surface_output_dir=surface_dir,
        artifact_roots=spec.artifact_roots,
    )

    diagnostics: list[str] = []
    if completed.returncode != 0:
        diagnostics.append(f"surface script exited with code {completed.returncode}")
    if missing_roots:
        diagnostics.append(
            "missing expected artifact roots: " + ", ".join(sorted(missing_roots))
        )
    if completed.returncode == 0 and not artifacts:
        diagnostics.append("surface run produced no copied artifacts")

    status = "passed" if completed.returncode == 0 else "failed"
    return SurfaceRunResult(
        surface_id=spec.surface_id,
        script=spec.script,
        command=command,
        status=status,
        exit_code=completed.returncode,
        duration_ms=duration_ms,
        stdout_log=stdout_log,
        stderr_log=stderr_log,
        artifacts_dir=surface_dir / "artifacts",
        artifacts=artifacts,
        missing_artifact_roots=missing_roots,
        summary_line=extract_summary_line(completed.stdout),
        diagnostics=diagnostics,
    )


def build_binaries(repo_root: Path) -> None:
    subprocess.run(
        ["cargo", "build", "-p", "tau-coding-agent"],
        cwd=repo_root,
        check=True,
    )
    subprocess.run(
        [
            "cargo",
            "build",
            "-p",
            "tau-browser-automation",
            "--bin",
            "browser_automation_live_harness",
        ],
        cwd=repo_root,
        check=True,
    )


def print_surface_list(specs: list[SurfaceSpec], as_json: bool) -> None:
    if as_json:
        print(
            json.dumps(
                {
                    "schema_version": SURFACE_MANIFEST_SCHEMA_VERSION,
                    "surfaces": [
                        {
                            "id": spec.surface_id,
                            "script": spec.script,
                            "artifact_roots": spec.artifact_roots,
                        }
                        for spec in specs
                    ],
                },
                indent=2,
            )
        )
        return

    for spec in specs:
        print(spec.surface_id)
        print(f"  script: {spec.script}")
        for root in spec.artifact_roots:
            print(f"  artifact_root: {root}")


def write_json(path: Path, payload: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Run unified live validation harness across voice/browser/dashboard/custom-command/memory."
    )
    parser.add_argument(
        "--repo-root",
        default=".",
        help="Repository root used to resolve scripts and artifact roots",
    )
    parser.add_argument(
        "--surfaces-manifest",
        default=".github/live-run-unified-manifest.json",
        help="Path to unified live-run surfaces manifest JSON",
    )
    parser.add_argument(
        "--output-dir",
        default=".tau/live-run-unified",
        help="Directory where unified logs/manifests are written",
    )
    parser.add_argument(
        "--binary",
        default="target/debug/tau-coding-agent",
        help="tau-coding-agent binary path forwarded to demo wrappers",
    )
    parser.add_argument(
        "--skip-build",
        action="store_true",
        help="Skip cargo build and require binaries to already exist",
    )
    parser.add_argument(
        "--keep-going",
        action="store_true",
        help="Continue running remaining surfaces after one fails",
    )
    parser.add_argument(
        "--timeout-seconds",
        type=int,
        default=0,
        help="Optional timeout in seconds forwarded to wrappers (0 disables timeout forwarding)",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="Print available surfaces from the surfaces manifest and exit",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit JSON output for --list mode",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)

    repo_root = Path(args.repo_root).resolve()
    manifest_path = resolve_path(repo_root, args.surfaces_manifest)
    output_dir = resolve_path(repo_root, args.output_dir)
    binary_path = resolve_path(repo_root, args.binary)

    specs = load_surface_manifest(manifest_path)
    if args.list:
        print_surface_list(specs, as_json=args.json)
        return 0

    timeout_seconds: int | None = None
    if args.timeout_seconds < 0:
        raise ValueError("--timeout-seconds must be >= 0")
    if args.timeout_seconds > 0:
        timeout_seconds = args.timeout_seconds

    if output_dir.exists():
        shutil.rmtree(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    if not args.skip_build:
        print("[live-run-unified] building tau-coding-agent and browser live harness binaries")
        build_binaries(repo_root)
    elif not binary_path.exists():
        raise FileNotFoundError(
            f"--skip-build requested but binary does not exist: {binary_path}"
        )

    started_unix_ms = int(time.time() * 1000)
    started_monotonic = time.perf_counter()

    results: list[SurfaceRunResult] = []
    for index, spec in enumerate(specs, start=1):
        print(f"[live-run-unified] [{index}/{len(specs)}] {spec.surface_id}")
        print(
            "[live-run-unified] command: "
            + shlex.join(
                [
                    spec.script,
                    "--repo-root",
                    str(repo_root),
                    "--binary",
                    str(binary_path),
                    "--skip-build" if args.skip_build else "",
                ]
            ).replace(" ''", "")
        )
        result = run_surface(
            spec=spec,
            repo_root=repo_root,
            output_dir=output_dir,
            binary=binary_path,
            skip_build=args.skip_build,
            timeout_seconds=timeout_seconds,
        )
        results.append(result)

        print(
            f"[live-run-unified] {result.status.upper()} {result.surface_id} "
            f"exit={result.exit_code} duration_ms={result.duration_ms} "
            f"artifacts={len(result.artifacts)}"
        )
        if result.summary_line:
            print(f"[live-run-unified] summary-line: {result.summary_line}")
        if result.status == "failed" and not args.keep_going:
            print("[live-run-unified] stopping after first failure (use --keep-going to continue)")
            break

    duration_ms = int((time.perf_counter() - started_monotonic) * 1000)
    passed = sum(1 for result in results if result.status == "passed")
    failed = len(results) - passed
    overall_status = "passed" if failed == 0 else "failed"

    manifest = {
        "schema_version": UNIFIED_RUN_SCHEMA_VERSION,
        "started_unix_ms": started_unix_ms,
        "duration_ms": duration_ms,
        "repo_root": str(repo_root),
        "binary": str(binary_path),
        "surfaces_manifest": str(manifest_path),
        "output_dir": str(output_dir),
        "overall": {
            "status": overall_status,
            "total_surfaces": len(results),
            "passed_surfaces": passed,
            "failed_surfaces": failed,
        },
        "surfaces": [
            {
                "surface_id": result.surface_id,
                "script": result.script,
                "command": result.command,
                "status": result.status,
                "exit_code": result.exit_code,
                "duration_ms": result.duration_ms,
                "stdout_log": result.stdout_log.relative_to(output_dir).as_posix(),
                "stderr_log": result.stderr_log.relative_to(output_dir).as_posix(),
                "artifacts_dir": result.artifacts_dir.relative_to(output_dir).as_posix(),
                "artifact_count": len(result.artifacts),
                "artifacts": result.artifacts,
                "missing_artifact_roots": result.missing_artifact_roots,
                "summary_line": result.summary_line,
                "diagnostics": result.diagnostics,
            }
            for result in results
        ],
    }

    report = {
        "schema_version": UNIFIED_RUN_SCHEMA_VERSION,
        "overall": manifest["overall"],
        "duration_ms": duration_ms,
        "surface_status": {
            result.surface_id: {
                "status": result.status,
                "exit_code": result.exit_code,
                "artifact_count": len(result.artifacts),
                "summary_line": result.summary_line,
            }
            for result in results
        },
        "failed_surfaces": [result.surface_id for result in results if result.status == "failed"],
    }

    manifest_path_out = output_dir / "manifest.json"
    report_path_out = output_dir / "report.json"
    write_json(manifest_path_out, manifest)
    write_json(report_path_out, report)

    print(
        f"[live-run-unified] summary: total={len(results)} passed={passed} failed={failed} "
        f"duration_ms={duration_ms}"
    )
    print(f"[live-run-unified] manifest={manifest_path_out}")
    print(f"[live-run-unified] report={report_path_out}")
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
