import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
DEFAULT_MANIFEST = REPO_ROOT / ".github" / "demo-smoke-manifest.json"
sys.path.insert(0, str(SCRIPT_DIR))

import demo_smoke_runner  # noqa: E402


def write_file(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def write_mock_binary(path: Path) -> None:
    write_file(
        path,
        """#!/usr/bin/env python3
import sys

if "--fail" in sys.argv:
    print("forced-failure", file=sys.stderr)
    raise SystemExit(7)

if "--gateway-remote-plan" in sys.argv and "tailscale-funnel" in sys.argv:
    if "--gateway-openresponses-auth-password" not in sys.argv:
        print(
            "gateway remote plan rejected: profile=tailscale-funnel gate=hold reason_codes=tailscale_funnel_missing_password",
            file=sys.stderr,
        )
        raise SystemExit(1)

print("mock-ok " + " ".join(sys.argv[1:]))
""",
    )
    path.chmod(0o755)


class DemoSmokeRunnerTests(unittest.TestCase):
    def test_unit_repository_manifest_includes_live_mode_and_gateway_diagnostics(self):
        commands = demo_smoke_runner.load_manifest(DEFAULT_MANIFEST)
        names = [command.name for command in commands]
        self.assertIn("multi-channel-live-ingest-telegram", names)
        self.assertIn("multi-channel-live-ingest-discord", names)
        self.assertIn("multi-channel-live-ingest-whatsapp", names)
        self.assertIn("multi-channel-live-runner", names)
        self.assertIn("multi-channel-transport-health", names)
        self.assertIn("multi-channel-status-inspect", names)
        self.assertIn("gateway-contract-runner", names)
        self.assertIn("gateway-transport-health", names)
        self.assertIn("gateway-status-inspect", names)
        self.assertIn("gateway-service-start", names)
        self.assertIn("gateway-service-status", names)
        self.assertIn("gateway-service-stop", names)
        self.assertIn("gateway-remote-plan-tailscale-serve", names)
        self.assertIn(
            "gateway-remote-plan-fails-closed-missing-funnel-password",
            names,
        )

    def test_unit_load_manifest_accepts_valid_schema_and_commands(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            manifest_path = Path(temp_dir) / "manifest.json"
            write_file(
                manifest_path,
                """{
  "schema_version": 1,
  "commands": [
    {"name": "validate", "args": ["--rpc-capabilities"]},
    {"name": "show", "args": ["--package-show", "./examples/starter/package.json"]}
  ]
}
""",
            )
            commands = demo_smoke_runner.load_manifest(manifest_path)
            self.assertEqual(len(commands), 2)
            self.assertEqual(commands[0].name, "validate")
            self.assertEqual(commands[0].args, ["--rpc-capabilities"])

    def test_functional_run_commands_executes_manifest_with_mock_binary(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            manifest_path = root / "manifest.json"
            binary_path = root / "bin" / "tau-coding-agent"
            log_dir = root / "logs"
            write_mock_binary(binary_path)
            write_file(
                manifest_path,
                """{
  "schema_version": 1,
  "commands": [
    {"name": "first", "args": ["--rpc-capabilities"]},
    {"name": "second", "args": ["--package-validate", "./examples/starter/package.json"]}
  ]
}
""",
            )
            commands = demo_smoke_runner.load_manifest(manifest_path)
            report = demo_smoke_runner.run_commands(
                commands=commands,
                binary=binary_path,
                repo_root=root,
                log_dir=log_dir,
                keep_going=False,
            )
            self.assertEqual(report.total, 2)
            self.assertEqual(report.failed, 0)
            self.assertEqual(report.passed, 2)
            self.assertTrue((log_dir / "01-first.stdout.log").exists())
            self.assertTrue((log_dir / "02-second.stdout.log").exists())

    def test_functional_run_commands_executes_repository_manifest_with_mock_binary(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            binary_path = root / "bin" / "tau-coding-agent"
            log_dir = root / "logs"
            write_mock_binary(binary_path)
            commands = demo_smoke_runner.load_manifest(DEFAULT_MANIFEST)
            report = demo_smoke_runner.run_commands(
                commands=commands,
                binary=binary_path,
                repo_root=REPO_ROOT,
                log_dir=log_dir,
                keep_going=False,
            )
            self.assertEqual(report.failed, 0)
            self.assertGreaterEqual(report.passed, 10)

    def test_functional_run_commands_supports_expected_non_zero_exit_and_stderr_check(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            manifest_path = root / "manifest.json"
            binary_path = root / "bin" / "tau-coding-agent"
            log_dir = root / "logs"
            write_mock_binary(binary_path)
            write_file(
                manifest_path,
                """{
  "schema_version": 1,
  "commands": [
    {
      "name": "expected-failure-contract",
      "expected_exit_code": 7,
      "stderr_contains": "forced-failure",
      "args": ["--fail"]
    }
  ]
}
""",
            )
            commands = demo_smoke_runner.load_manifest(manifest_path)
            report = demo_smoke_runner.run_commands(
                commands=commands,
                binary=binary_path,
                repo_root=root,
                log_dir=log_dir,
                keep_going=False,
            )
            self.assertEqual(report.total, 1)
            self.assertEqual(report.passed, 1)
            self.assertEqual(report.failed, 0)
            stderr_log = (log_dir / "01-expected-failure-contract.stderr.log").read_text(
                encoding="utf-8"
            )
            self.assertIn("forced-failure", stderr_log)

    def test_integration_cli_runs_manifest_and_writes_summary(self):
        script_path = SCRIPT_DIR / "demo_smoke_runner.py"
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            manifest_path = root / "manifest.json"
            binary_path = root / "bin" / "tau-coding-agent"
            summary_path = root / "summary.md"
            log_dir = root / "logs"
            write_mock_binary(binary_path)
            write_file(
                manifest_path,
                """{
  "schema_version": 1,
  "commands": [
    {"name": "single", "args": ["--rpc-capabilities"]}
  ]
}
""",
            )
            subprocess.run(
                [
                    sys.executable,
                    str(script_path),
                    "--repo-root",
                    str(root),
                    "--manifest",
                    str(manifest_path),
                    "--binary",
                    str(binary_path),
                    "--log-dir",
                    str(log_dir),
                    "--summary",
                    str(summary_path),
                ],
                check=True,
            )
            summary = summary_path.read_text(encoding="utf-8")
            self.assertIn("### Demo Smoke", summary)
            self.assertIn("- Status: pass", summary)
            self.assertIn("- Failed: 0", summary)
            self.assertTrue((log_dir / "01-single.stdout.log").exists())
            self.assertTrue((log_dir / "01-single.stderr.log").exists())

    def test_regression_cli_reports_failing_command_name_and_exit_code(self):
        script_path = SCRIPT_DIR / "demo_smoke_runner.py"
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            manifest_path = root / "manifest.json"
            binary_path = root / "bin" / "tau-coding-agent"
            log_dir = root / "logs"
            write_mock_binary(binary_path)
            write_file(
                manifest_path,
                """{
  "schema_version": 1,
  "commands": [
    {"name": "pass-command", "args": ["--rpc-capabilities"]},
    {"name": "failing-command", "args": ["--fail"]}
  ]
}
""",
            )
            completed = subprocess.run(
                [
                    sys.executable,
                    str(script_path),
                    "--repo-root",
                    str(root),
                    "--manifest",
                    str(manifest_path),
                    "--binary",
                    str(binary_path),
                    "--log-dir",
                    str(log_dir),
                ],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("[demo-smoke] FAIL failing-command", completed.stdout)
            self.assertTrue((log_dir / "02-failing-command.stderr.log").exists())

    def test_regression_repository_manifest_command_names_are_unique(self):
        commands = demo_smoke_runner.load_manifest(DEFAULT_MANIFEST)
        names = [command.name for command in commands]
        self.assertEqual(len(names), len(set(names)))

    def test_regression_expected_substring_mismatch_fails_contract(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            manifest_path = root / "manifest.json"
            binary_path = root / "bin" / "tau-coding-agent"
            log_dir = root / "logs"
            write_mock_binary(binary_path)
            write_file(
                manifest_path,
                """{
  "schema_version": 1,
  "commands": [
    {
      "name": "bad-contract",
      "expected_exit_code": 7,
      "stderr_contains": "missing-substring",
      "args": ["--fail"]
    }
  ]
}
""",
            )
            commands = demo_smoke_runner.load_manifest(manifest_path)
            report = demo_smoke_runner.run_commands(
                commands=commands,
                binary=binary_path,
                repo_root=root,
                log_dir=log_dir,
                keep_going=False,
            )
            self.assertEqual(report.passed, 0)
            self.assertEqual(report.failed, 1)


if __name__ == "__main__":
    unittest.main()
