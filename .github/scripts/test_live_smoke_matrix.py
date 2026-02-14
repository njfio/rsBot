import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
RUNNER_SCRIPT = SCRIPT_DIR / "live_smoke_matrix.py"
sys.path.insert(0, str(SCRIPT_DIR))

import live_smoke_matrix  # noqa: E402


class LiveSmokeMatrixTests(unittest.TestCase):
    def test_unit_resolve_surface_plan_returns_expected_browser_fallback(self) -> None:
        plan = live_smoke_matrix.resolve_surface_plan("browser")
        self.assertEqual(plan.surface, "browser")
        self.assertEqual(plan.primary_wrapper, "scripts/demo/browser-automation-live.sh")
        self.assertEqual(plan.fallback_wrapper, "scripts/demo/browser-automation.sh")
        self.assertIn(".tau/demo-browser-automation-live", plan.artifact_dirs)

    def test_unit_resolve_surface_plan_returns_expected_dashboard_primary_and_fallback(self) -> None:
        plan = live_smoke_matrix.resolve_surface_plan("dashboard")
        self.assertEqual(plan.surface, "dashboard")
        self.assertEqual(plan.primary_wrapper, "scripts/demo/dashboard-live.sh")
        self.assertEqual(plan.fallback_wrapper, "scripts/demo/dashboard.sh")
        self.assertIn(".tau/demo-dashboard-live", plan.artifact_dirs)
        self.assertIn(".tau/demo-dashboard", plan.artifact_dirs)

    def test_unit_resolve_surface_plan_returns_expected_custom_command_primary_and_fallback(self) -> None:
        plan = live_smoke_matrix.resolve_surface_plan("custom-command")
        self.assertEqual(plan.surface, "custom-command")
        self.assertEqual(plan.primary_wrapper, "scripts/demo/custom-command-live.sh")
        self.assertEqual(plan.fallback_wrapper, "scripts/demo/custom-command.sh")
        self.assertIn(".tau/demo-custom-command-live", plan.artifact_dirs)
        self.assertIn(".tau/demo-custom-command", plan.artifact_dirs)

    def test_functional_cli_json_output_includes_surface_plan_fields(self) -> None:
        completed = subprocess.run(
            [sys.executable, str(RUNNER_SCRIPT), "--surface", "voice", "--json"],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, msg=completed.stderr)
        payload = json.loads(completed.stdout)
        self.assertEqual(payload["surface"], "voice")
        self.assertEqual(payload["primary_wrapper"], "scripts/demo/voice.sh")
        self.assertEqual(payload["fallback_wrapper"], "")
        self.assertEqual(payload["timeout_seconds"], 180)

    def test_integration_cli_writes_output_and_summary_files(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            output_path = root / "gh-output.txt"
            summary_path = root / "summary.md"
            completed = subprocess.run(
                [
                    sys.executable,
                    str(RUNNER_SCRIPT),
                    "--surface",
                    "memory",
                    "--output",
                    str(output_path),
                    "--summary",
                    str(summary_path),
                ],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, msg=completed.stderr)
            output_text = output_path.read_text(encoding="utf-8")
            self.assertIn("surface=memory", output_text)
            self.assertIn("primary_wrapper=scripts/demo/memory.sh", output_text)
            self.assertIn("artifact_dirs_json=[\".tau/demo-memory\"]", output_text)
            summary_text = summary_path.read_text(encoding="utf-8")
            self.assertIn("### Live Smoke Surface Plan", summary_text)
            self.assertIn("Surface: memory", summary_text)

    def test_regression_cli_rejects_unknown_surface(self) -> None:
        completed = subprocess.run(
            [sys.executable, str(RUNNER_SCRIPT), "--surface", "unknown"],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("unsupported live-smoke surface", completed.stderr)


if __name__ == "__main__":
    unittest.main()
