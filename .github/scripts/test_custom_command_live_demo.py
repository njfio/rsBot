import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPTS_DIR = REPO_ROOT / "scripts" / "demo"


def write_mock_custom_command_binary(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        """#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path

args = sys.argv[1:]

if "--custom-command-contract-runner" in args:
    state_dir = Path(args[args.index("--custom-command-state-dir") + 1])
    state_dir.mkdir(parents=True, exist_ok=True)
    (state_dir / "state.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "processed_case_keys": ["a", "b", "c", "d", "e", "f"],
                "commands": [],
                "health": {
                    "health_state": "degraded",
                    "failure_streak": 1,
                    "queue_depth": 0,
                },
            }
        ),
        encoding="utf-8",
    )
    (state_dir / "runtime-events.jsonl").write_text(
        '{"reason_codes":["command_runs_recorded","case_processing_failed"],"health_reason":"retry pending"}\\n',
        encoding="utf-8",
    )

    base = state_dir / "channel-store" / "channels" / "custom-command"
    deploy = base / "deploy_release"
    deploy.mkdir(parents=True, exist_ok=True)
    deploy_event = {
        "payload": {
            "outcome": "success",
            "operation": "run",
            "command_name": "deploy_release",
            "status_code": 200,
            "error_code": "",
        }
    }
    (deploy / "log.jsonl").write_text(json.dumps(deploy_event) + "\\n", encoding="utf-8")

    if os.environ.get("TAU_MOCK_SKIP_POLICY_DENY") != "1":
        admin = base / "admin_shutdown"
        admin.mkdir(parents=True, exist_ok=True)
        admin_event = {
            "payload": {
                "outcome": "malformed_input",
                "operation": "run",
                "command_name": "admin_shutdown",
                "status_code": 403,
                "error_code": "custom_command_policy_denied",
            }
        }
        (admin / "log.jsonl").write_text(json.dumps(admin_event) + "\\n", encoding="utf-8")

    triage = base / "triage_alerts"
    triage.mkdir(parents=True, exist_ok=True)
    triage_event = {
        "payload": {
            "outcome": "retryable_failure",
            "operation": "run",
            "command_name": "triage_alerts",
            "status_code": 503,
            "error_code": "custom_command_backend_unavailable",
        }
    }
    (triage / "log.jsonl").write_text(json.dumps(triage_event) + "\\n", encoding="utf-8")
    print("custom-command-runner-ok")
    raise SystemExit(0)

if "--transport-health-inspect" in args:
    print(json.dumps({"health_state": "degraded", "failure_streak": 1, "queue_depth": 0}))
    raise SystemExit(0)

if "--custom-command-status-inspect" in args:
    print(json.dumps({"health_state": "degraded", "rollout_gate": "hold"}))
    raise SystemExit(0)

if "--channel-store-inspect" in args:
    print(json.dumps({"status": "ok", "channel": args[args.index("--channel-store-inspect") + 1]}))
    raise SystemExit(0)

print("mock-ok " + " ".join(args))
""",
        encoding="utf-8",
    )
    path.chmod(0o755)


def prepare_fixture_tree(repo_root: Path) -> None:
    fixture = (
        repo_root
        / "crates"
        / "tau-coding-agent"
        / "testdata"
        / "custom-command-contract"
        / "live-execution-matrix.json"
    )
    fixture.parent.mkdir(parents=True, exist_ok=True)
    fixture.write_text('{"schema_version":1,"cases":[]}', encoding="utf-8")


def run_custom_command_live_script(
    repo_root: Path,
    binary_path: Path,
    env_overrides: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    env = dict(os.environ)
    if env_overrides:
        env.update(env_overrides)
    return subprocess.run(
        [
            str(SCRIPTS_DIR / "custom-command-live.sh"),
            "--skip-build",
            "--repo-root",
            str(repo_root),
            "--binary",
            str(binary_path),
            "--timeout-seconds",
            "30",
        ],
        text=True,
        capture_output=True,
        env=env,
        check=False,
    )


class CustomCommandLiveDemoTests(unittest.TestCase):
    def test_unit_custom_command_live_rejects_unknown_argument(self) -> None:
        completed = subprocess.run(
            [str(SCRIPTS_DIR / "custom-command-live.sh"), "--definitely-unknown"],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 2)
        self.assertIn("unknown argument: --definitely-unknown", completed.stderr)

    def test_functional_custom_command_live_runs_with_mock_runtime(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            prepare_fixture_tree(root)
            binary_path = root / "bin" / "tau-coding-agent"
            write_mock_custom_command_binary(binary_path)

            completed = run_custom_command_live_script(root, binary_path)
            self.assertEqual(completed.returncode, 0, msg=completed.stderr)
            self.assertIn("[demo:custom-command-live] summary: total=", completed.stdout)
            self.assertIn("failed=0", completed.stdout)

            summary_path = root / ".tau" / "demo-custom-command-live" / "custom-command-live-summary.json"
            report_path = root / ".tau" / "demo-custom-command-live" / "custom-command-live-report.json"
            self.assertTrue(summary_path.exists())
            self.assertTrue(report_path.exists())

    def test_integration_custom_command_live_report_includes_policy_and_retry_counts(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            prepare_fixture_tree(root)
            binary_path = root / "bin" / "tau-coding-agent"
            write_mock_custom_command_binary(binary_path)

            completed = run_custom_command_live_script(root, binary_path)
            self.assertEqual(completed.returncode, 0, msg=completed.stderr)

            summary_path = root / ".tau" / "demo-custom-command-live" / "custom-command-live-summary.json"
            summary = json.loads(summary_path.read_text(encoding="utf-8"))
            self.assertEqual(summary["health_state"], "degraded")
            self.assertEqual(summary["rollout_gate"], "hold")
            self.assertEqual(summary["deploy_event_count"], 1)
            self.assertEqual(summary["policy_deny_event_count"], 1)
            self.assertEqual(summary["retryable_failure_event_count"], 1)

    def test_regression_custom_command_live_fails_closed_when_policy_deny_events_are_missing(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            prepare_fixture_tree(root)
            binary_path = root / "bin" / "tau-coding-agent"
            write_mock_custom_command_binary(binary_path)

            completed = run_custom_command_live_script(
                root,
                binary_path,
                env_overrides={"TAU_MOCK_SKIP_POLICY_DENY": "1"},
            )
            self.assertNotEqual(completed.returncode, 0)
            combined = completed.stdout + "\n" + completed.stderr
            self.assertIn("policy deny events were not captured", combined)


if __name__ == "__main__":
    unittest.main()
