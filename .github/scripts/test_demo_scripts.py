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
            ["local.sh", "rpc.sh", "events.sh", "package.sh"],
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

    def test_functional_demo_scripts_run_expected_command_chains(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            binary_path = root / "bin" / "tau-coding-agent"
            trace_path = root / "trace.ndjson"
            write_mock_binary(binary_path)

            for script_name in ("local.sh", "rpc.sh", "events.sh", "package.sh"):
                completed = run_demo_script(script_name, binary_path, trace_path)
                self.assertEqual(
                    completed.returncode,
                    0,
                    msg=f"{script_name} failed\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
                )
                self.assertIn("summary: total=", completed.stdout)
                self.assertIn("failed=0", completed.stdout)

            rows = [json.loads(line) for line in trace_path.read_text(encoding="utf-8").splitlines()]
            self.assertGreaterEqual(len(rows), 12)

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
            self.assertIn("[demo:all] summary: total=4 passed=4 failed=0", completed.stdout)

            rows = [json.loads(line) for line in trace_path.read_text(encoding="utf-8").splitlines()]
            self.assertGreaterEqual(len(rows), 12)

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
            self.assertEqual(payload["demos"], [{"name": "local.sh", "status": "passed", "exit_code": 0}])
            self.assertIn("[demo:all] [1] local.sh", completed.stderr)

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

    def test_integration_all_script_list_json_reports_canonical_order(self) -> None:
        completed = subprocess.run(
            [str(SCRIPTS_DIR / "all.sh"), "--list", "--json"],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0)
        payload = json.loads(completed.stdout)
        self.assertEqual(payload["demos"], ["local.sh", "rpc.sh", "events.sh", "package.sh"])

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


if __name__ == "__main__":
    unittest.main()
