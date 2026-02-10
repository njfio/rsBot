import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPTS_DIR = REPO_ROOT / "scripts" / "demo"


def write_mock_binary(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        """#!/usr/bin/env python3
import json
import os
import sys

trace_path = os.environ.get("TAU_DEMO_MOCK_TRACE")
if trace_path:
    with open(trace_path, "a", encoding="utf-8") as handle:
        handle.write(json.dumps(sys.argv[1:]))
        handle.write("\\n")

print("mock-ok " + " ".join(sys.argv[1:]))
""",
        encoding="utf-8",
    )
    path.chmod(0o755)


def write_failing_binary(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        """#!/usr/bin/env bash
set -euo pipefail
echo "mock-fail $*" >&2
exit 1
""",
        encoding="utf-8",
    )
    path.chmod(0o755)


def write_sleeping_binary(path: Path, sleep_seconds: int = 5) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        f"""#!/usr/bin/env bash
set -euo pipefail
sleep {sleep_seconds}
echo \"mock-slow-ok $*\"
""",
        encoding="utf-8",
    )
    path.chmod(0o755)


def write_mock_cargo(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        """#!/usr/bin/env bash
set -euo pipefail
: "${TAU_DEMO_CARGO_TRACE:?}"
: "${TAU_DEMO_BUILT_BINARY:?}"
: "${TAU_DEMO_BINARY_TEMPLATE:?}"
echo "$*" >> "${TAU_DEMO_CARGO_TRACE}"
mkdir -p "$(dirname "${TAU_DEMO_BUILT_BINARY}")"
cp "${TAU_DEMO_BINARY_TEMPLATE}" "${TAU_DEMO_BUILT_BINARY}"
chmod +x "${TAU_DEMO_BUILT_BINARY}"
""",
        encoding="utf-8",
    )
    path.chmod(0o755)


def run_demo_script(
    script_name: str,
    binary_path: Path,
    trace_path: Path,
    extra_args: list[str] | None = None,
) -> subprocess.CompletedProcess[str]:
    script_path = SCRIPTS_DIR / script_name
    env = dict(os.environ)
    env["TAU_DEMO_MOCK_TRACE"] = str(trace_path)
    command = [
        str(script_path),
        "--skip-build",
        "--repo-root",
        str(REPO_ROOT),
        "--binary",
        str(binary_path),
    ]
    if extra_args:
        command.extend(extra_args)
    return subprocess.run(
        command,
        env=env,
        text=True,
        capture_output=True,
        check=False,
    )


def assert_duration_ms_field(test_case: unittest.TestCase, entry: dict[str, object]) -> None:
    test_case.assertIn("duration_ms", entry)
    duration = entry["duration_ms"]
    test_case.assertIsInstance(duration, int)
    test_case.assertGreaterEqual(duration, 0)


class DemoScriptsTests(unittest.TestCase):
    def test_unit_script_argument_parser_rejects_unknown_argument(self) -> None:
        completed = subprocess.run(
            [str(SCRIPTS_DIR / "local.sh"), "--definitely-unknown"],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 2)
        self.assertIn("unknown argument: --definitely-unknown", completed.stderr)

    def test_unit_all_script_argument_parser_rejects_unknown_argument(self) -> None:
        completed = subprocess.run(
            [str(SCRIPTS_DIR / "all.sh"), "--definitely-unknown"],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 2)
        self.assertIn("unknown argument: --definitely-unknown", completed.stderr)

    def test_unit_all_script_list_prints_deterministic_inventory(self) -> None:
        completed = subprocess.run(
            [str(SCRIPTS_DIR / "all.sh"), "--list"],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0)
        self.assertEqual(
            completed.stdout.strip().splitlines(),
            [
                "local.sh",
                "rpc.sh",
                "events.sh",
                "package.sh",
                "multi-channel.sh",
                "memory.sh",
                "dashboard.sh",
            ],
        )

    def test_unit_all_script_only_rejects_unknown_demo_names(self) -> None:
        completed = subprocess.run(
            [str(SCRIPTS_DIR / "all.sh"), "--only", "rpc,unknown-demo"],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 2)
        self.assertIn("unknown demo names in --only", completed.stderr)
        self.assertIn("unknown-demo", completed.stderr)

    def test_unit_all_script_report_file_requires_value(self) -> None:
        completed = subprocess.run(
            [str(SCRIPTS_DIR / "all.sh"), "--report-file"],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 2)
        self.assertIn("missing value for --report-file", completed.stderr)

    def test_unit_all_script_fail_fast_flag_is_accepted(self) -> None:
        completed = subprocess.run(
            [str(SCRIPTS_DIR / "all.sh"), "--list", "--fail-fast"],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0)
        self.assertIn("local.sh", completed.stdout)

    def test_unit_all_script_timeout_flag_is_accepted(self) -> None:
        completed = subprocess.run(
            [str(SCRIPTS_DIR / "all.sh"), "--list", "--timeout-seconds", "5"],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0)
        self.assertIn("local.sh", completed.stdout)

    def test_functional_demo_scripts_run_expected_command_chains(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            binary_path = root / "bin" / "tau-coding-agent"
            trace_path = root / "trace.ndjson"
            write_mock_binary(binary_path)

            for script_name in (
                "local.sh",
                "rpc.sh",
                "events.sh",
                "package.sh",
                "multi-channel.sh",
                "memory.sh",
                "dashboard.sh",
            ):
                completed = run_demo_script(script_name, binary_path, trace_path)
                self.assertEqual(
                    completed.returncode,
                    0,
                    msg=f"{script_name} failed\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
                )
                self.assertIn("summary: total=", completed.stdout)
                self.assertIn("failed=0", completed.stdout)

            rows = [json.loads(line) for line in trace_path.read_text(encoding="utf-8").splitlines()]
            self.assertGreaterEqual(len(rows), 22)

    def test_functional_all_script_builds_once_when_skip_build_is_disabled(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            binary_template = root / "template" / "tau-coding-agent"
            binary_path = root / "built" / "tau-coding-agent"
            cargo_script = root / "mock-bin" / "cargo"
            cargo_trace = root / "cargo.trace"
            demo_trace = root / "demo.trace"
            write_mock_binary(binary_template)
            write_mock_cargo(cargo_script)

            env = dict(os.environ)
            env["PATH"] = f"{cargo_script.parent}:{env.get('PATH', '')}"
            env["TAU_DEMO_MOCK_TRACE"] = str(demo_trace)
            env["TAU_DEMO_CARGO_TRACE"] = str(cargo_trace)
            env["TAU_DEMO_BUILT_BINARY"] = str(binary_path)
            env["TAU_DEMO_BINARY_TEMPLATE"] = str(binary_template)

            completed = subprocess.run(
                [
                    str(SCRIPTS_DIR / "all.sh"),
                    "--repo-root",
                    str(REPO_ROOT),
                    "--binary",
                    str(binary_path),
                    "--only",
                    "local,rpc",
                ],
                env=env,
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, msg=completed.stderr)
            self.assertIn("[demo:all] summary: total=2 passed=2 failed=0", completed.stdout)

            cargo_calls = cargo_trace.read_text(encoding="utf-8").splitlines()
            self.assertEqual(len(cargo_calls), 1)
            self.assertIn("build -p tau-coding-agent", cargo_calls[0])
            self.assertTrue(binary_path.exists())

    def test_functional_all_script_runs_all_demo_wrappers(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            binary_path = root / "bin" / "tau-coding-agent"
            trace_path = root / "trace.ndjson"
            write_mock_binary(binary_path)

            completed = run_demo_script("all.sh", binary_path, trace_path)
            self.assertEqual(
                completed.returncode,
                0,
                msg=f"all.sh failed\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
            )
            self.assertIn("[demo:all] summary: total=7 passed=7 failed=0", completed.stdout)

            rows = [json.loads(line) for line in trace_path.read_text(encoding="utf-8").splitlines()]
            self.assertGreaterEqual(len(rows), 22)

    def test_functional_all_script_only_runs_selected_demo_wrappers(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            binary_path = root / "bin" / "tau-coding-agent"
            trace_path = root / "trace.ndjson"
            write_mock_binary(binary_path)

            completed = run_demo_script("all.sh", binary_path, trace_path, extra_args=["--only", "rpc,events"])
            self.assertEqual(completed.returncode, 0, msg=completed.stderr)
            self.assertIn("[demo:all] [1] rpc.sh", completed.stdout)
            self.assertIn("[demo:all] [2] events.sh", completed.stdout)
            self.assertIn("[demo:all] summary: total=2 passed=2 failed=0", completed.stdout)
            self.assertNotIn("local.sh", completed.stdout)
            self.assertNotIn("package.sh", completed.stdout)

            rows = [json.loads(line) for line in trace_path.read_text(encoding="utf-8").splitlines()]
            self.assertGreaterEqual(len(rows), 5)

    def test_functional_all_script_json_summary_reports_selected_demo_results(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            binary_path = root / "bin" / "tau-coding-agent"
            trace_path = root / "trace.ndjson"
            write_mock_binary(binary_path)

            completed = run_demo_script("all.sh", binary_path, trace_path, extra_args=["--only", "local", "--json"])
            self.assertEqual(completed.returncode, 0, msg=completed.stderr)
            payload = json.loads(completed.stdout)
            self.assertEqual(payload["summary"], {"total": 1, "passed": 1, "failed": 0})
            self.assertEqual(len(payload["demos"]), 1)
            entry = payload["demos"][0]
            self.assertEqual(entry["name"], "local.sh")
            self.assertEqual(entry["status"], "passed")
            self.assertEqual(entry["exit_code"], 0)
            assert_duration_ms_field(self, entry)
            self.assertIn("[demo:all] [1] local.sh", completed.stderr)

    def test_functional_all_script_report_file_writes_summary_payload(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            binary_path = root / "bin" / "tau-coding-agent"
            trace_path = root / "trace.ndjson"
            report_path = root / "reports" / "all.json"
            write_mock_binary(binary_path)

            completed = run_demo_script("all.sh", binary_path, trace_path, extra_args=["--report-file", str(report_path)])
            self.assertEqual(completed.returncode, 0, msg=completed.stderr)
            self.assertIn("[demo:all] summary: total=7 passed=7 failed=0", completed.stdout)
            self.assertTrue(report_path.exists())

            payload = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(payload["summary"], {"total": 7, "passed": 7, "failed": 0})
            self.assertEqual(
                [entry["name"] for entry in payload["demos"]],
                [
                    "local.sh",
                    "rpc.sh",
                    "events.sh",
                    "package.sh",
                    "multi-channel.sh",
                    "memory.sh",
                    "dashboard.sh",
                ],
            )
            for entry in payload["demos"]:
                assert_duration_ms_field(self, entry)

    def test_functional_all_script_fail_fast_stops_after_first_failure(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            binary_path = root / "bin" / "tau-coding-agent"
            trace_path = root / "trace.ndjson"
            write_failing_binary(binary_path)

            completed = run_demo_script(
                "all.sh",
                binary_path,
                trace_path,
                extra_args=["--only", "local,rpc", "--fail-fast"],
            )
            self.assertEqual(completed.returncode, 1)
            self.assertIn("[demo:all] [1] local.sh", completed.stdout)
            self.assertNotIn("[demo:all] [2] rpc.sh", completed.stdout)
            self.assertIn("[demo:all] summary: total=1 passed=0 failed=1", completed.stdout)
            self.assertIn("fail-fast triggered", completed.stderr)

    def test_functional_local_script_timeout_fails_with_timeout_code(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            binary_path = root / "bin" / "tau-coding-agent"
            trace_path = root / "trace.ndjson"
            write_sleeping_binary(binary_path)

            completed = run_demo_script(
                "local.sh",
                binary_path,
                trace_path,
                extra_args=["--timeout-seconds", "1"],
            )
            self.assertEqual(completed.returncode, 124)
            self.assertIn("TIMEOUT onboard-non-interactive after 1s", completed.stderr)
            self.assertIn("[demo:local] [1] onboard-non-interactive", completed.stdout)

    def test_integration_demo_scripts_use_checked_in_example_paths(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            binary_path = root / "bin" / "tau-coding-agent"
            trace_path = root / "trace.ndjson"
            write_mock_binary(binary_path)

            events_run = run_demo_script("events.sh", binary_path, trace_path)
            self.assertEqual(events_run.returncode, 0, msg=events_run.stderr)
            package_run = run_demo_script("package.sh", binary_path, trace_path)
            self.assertEqual(package_run.returncode, 0, msg=package_run.stderr)

            recorded = trace_path.read_text(encoding="utf-8")
            self.assertIn("./examples/events", recorded)
            self.assertIn("./examples/events-state.json", recorded)
            self.assertIn("./examples/starter/package.json", recorded)

    def test_integration_all_script_runs_demos_in_expected_order(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            binary_path = root / "bin" / "tau-coding-agent"
            trace_path = root / "trace.ndjson"
            write_mock_binary(binary_path)

            completed = run_demo_script("all.sh", binary_path, trace_path)
            self.assertEqual(completed.returncode, 0, msg=completed.stderr)
            self.assertIn("[demo:all] [1] local.sh", completed.stdout)
            self.assertIn("[demo:all] [2] rpc.sh", completed.stdout)
            self.assertIn("[demo:all] [3] events.sh", completed.stdout)
            self.assertIn("[demo:all] [4] package.sh", completed.stdout)
            self.assertIn("[demo:all] [5] multi-channel.sh", completed.stdout)
            self.assertIn("[demo:all] [6] memory.sh", completed.stdout)
            self.assertIn("[demo:all] [7] dashboard.sh", completed.stdout)

    def test_integration_all_script_list_json_reports_canonical_order(self) -> None:
        completed = subprocess.run(
            [str(SCRIPTS_DIR / "all.sh"), "--list", "--json"],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0)
        payload = json.loads(completed.stdout)
        self.assertEqual(
            payload["demos"],
            [
                "local.sh",
                "rpc.sh",
                "events.sh",
                "package.sh",
                "multi-channel.sh",
                "memory.sh",
                "dashboard.sh",
            ],
        )

    def test_integration_all_script_report_file_tracks_selected_subset_order(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            binary_path = root / "bin" / "tau-coding-agent"
            trace_path = root / "trace.ndjson"
            report_path = root / "report.json"
            write_mock_binary(binary_path)

            completed = run_demo_script(
                "all.sh",
                binary_path,
                trace_path,
                extra_args=["--only", "events,rpc", "--report-file", str(report_path)],
            )
            self.assertEqual(completed.returncode, 0, msg=completed.stderr)
            payload = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(
                [entry["name"] for entry in payload["demos"]],
                ["rpc.sh", "events.sh"],
            )
            self.assertEqual(payload["summary"], {"total": 2, "passed": 2, "failed": 0})

    def test_integration_all_script_fail_fast_json_summary_reflects_executed_subset(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            binary_path = root / "bin" / "tau-coding-agent"
            trace_path = root / "trace.ndjson"
            write_failing_binary(binary_path)

            completed = run_demo_script(
                "all.sh",
                binary_path,
                trace_path,
                extra_args=["--only", "rpc,events", "--fail-fast", "--json"],
            )
            self.assertEqual(completed.returncode, 1)
            payload = json.loads(completed.stdout)
            self.assertEqual(payload["summary"], {"total": 1, "passed": 0, "failed": 1})
            self.assertEqual(len(payload["demos"]), 1)
            entry = payload["demos"][0]
            self.assertEqual(entry["name"], "rpc.sh")
            self.assertEqual(entry["status"], "failed")
            self.assertEqual(entry["exit_code"], 1)
            assert_duration_ms_field(self, entry)
            self.assertIn("fail-fast triggered", completed.stderr)

    def test_integration_all_script_timeout_summary_marks_wrapper_failed_with_timeout_exit(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            binary_path = root / "bin" / "tau-coding-agent"
            trace_path = root / "trace.ndjson"
            report_path = root / "timeouts" / "report.json"
            write_sleeping_binary(binary_path)

            completed = run_demo_script(
                "all.sh",
                binary_path,
                trace_path,
                extra_args=[
                    "--only",
                    "local,rpc",
                    "--timeout-seconds",
                    "1",
                    "--fail-fast",
                    "--report-file",
                    str(report_path),
                    "--json",
                ],
            )
            self.assertEqual(completed.returncode, 1)
            payload = json.loads(completed.stdout)
            self.assertEqual(payload["summary"], {"total": 1, "passed": 0, "failed": 1})
            self.assertEqual(len(payload["demos"]), 1)
            entry = payload["demos"][0]
            self.assertEqual(entry["name"], "local.sh")
            self.assertEqual(entry["status"], "failed")
            self.assertEqual(entry["exit_code"], 124)
            assert_duration_ms_field(self, entry)
            self.assertIn("TIMEOUT onboard-non-interactive after 1s", completed.stderr)
            self.assertIn("fail-fast triggered", completed.stderr)

            report_payload = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(report_payload, payload)

    def test_regression_scripts_fail_closed_when_binary_missing_in_skip_build_mode(self) -> None:
        completed = subprocess.run(
            [
                str(SCRIPTS_DIR / "rpc.sh"),
                "--skip-build",
                "--repo-root",
                str(REPO_ROOT),
                "--binary",
                "/tmp/tau-missing-binary",
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("missing tau-coding-agent binary", completed.stderr)

    def test_regression_all_script_fail_closed_when_binary_missing_in_skip_build_mode(self) -> None:
        completed = subprocess.run(
            [
                str(SCRIPTS_DIR / "all.sh"),
                "--skip-build",
                "--repo-root",
                str(REPO_ROOT),
                "--binary",
                "/tmp/tau-missing-binary",
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("missing tau-coding-agent binary", completed.stderr)

    def test_regression_all_script_unknown_only_filter_fails_before_execution(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            binary_path = root / "bin" / "tau-coding-agent"
            trace_path = root / "trace.ndjson"
            write_mock_binary(binary_path)

            completed = run_demo_script("all.sh", binary_path, trace_path, extra_args=["--only", "unknown"])
            self.assertEqual(completed.returncode, 2)
            self.assertIn("unknown demo names in --only", completed.stderr)
            self.assertFalse(trace_path.exists())

    def test_regression_all_script_failure_still_writes_report_file(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            binary_path = root / "bin" / "tau-coding-agent"
            trace_path = root / "trace.ndjson"
            report_path = root / "failed" / "report.json"
            write_failing_binary(binary_path)

            completed = run_demo_script("all.sh", binary_path, trace_path, extra_args=["--report-file", str(report_path)])
            self.assertEqual(completed.returncode, 1)
            self.assertTrue(report_path.exists())

            payload = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(payload["summary"]["total"], 7)
            self.assertEqual(payload["summary"]["failed"], 7)
            self.assertEqual(payload["summary"]["passed"], 0)
            for entry in payload["demos"]:
                assert_duration_ms_field(self, entry)

    def test_regression_all_script_timeout_rejects_non_positive_values(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            binary_path = root / "bin" / "tau-coding-agent"
            trace_path = root / "trace.ndjson"
            write_mock_binary(binary_path)

            completed = run_demo_script("all.sh", binary_path, trace_path, extra_args=["--timeout-seconds", "0"])
            self.assertEqual(completed.returncode, 2)
            self.assertIn("invalid value for --timeout-seconds", completed.stderr)
            self.assertFalse(trace_path.exists())


if __name__ == "__main__":
    unittest.main()
