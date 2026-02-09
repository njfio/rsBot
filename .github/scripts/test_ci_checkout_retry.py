import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

import ci_checkout_retry  # noqa: E402


REPO_ROOT = SCRIPT_DIR.parents[1]
WORKFLOW_PATH = REPO_ROOT / ".github" / "workflows" / "ci.yml"


class CheckoutRetryTests(unittest.TestCase):
    def test_unit_compute_retry_delays_applies_exponential_backoff_with_cap(self):
        policy = ci_checkout_retry.CheckoutRetryPolicy(
            max_attempts=4,
            base_delay_seconds=3,
            cap_delay_seconds=6,
            max_total_delay_seconds=30,
        )
        delays = ci_checkout_retry.compute_retry_delays(policy)
        self.assertEqual(delays, [3, 6, 6])

    def test_functional_cli_dry_run_reports_deterministic_policy_json(self):
        script_path = SCRIPT_DIR / "ci_checkout_retry.py"
        completed = subprocess.run(
            [
                sys.executable,
                str(script_path),
                "--dry-run",
                "--max-attempts",
                "3",
                "--base-delay-seconds",
                "3",
                "--cap-delay-seconds",
                "6",
                "--max-total-delay-seconds",
                "12",
                "--json",
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, msg=completed.stderr)
        payload = json.loads(completed.stdout)
        self.assertEqual(payload["retry_delays_seconds"], [3, 6])
        self.assertEqual(payload["planned_total_delay_seconds"], 9)
        self.assertEqual(payload["status"], "failure")

    def test_integration_ci_workflow_uses_retry_attempts_and_helper_script(self):
        raw = WORKFLOW_PATH.read_text(encoding="utf-8")
        self.assertIn("id: checkout_attempt_1", raw)
        self.assertIn("id: checkout_attempt_2", raw)
        self.assertIn("id: checkout_attempt_3", raw)
        self.assertIn(".github/scripts/ci_checkout_retry.py", raw)
        self.assertIn("Fail when checkout retries are exhausted", raw)

    def test_regression_cli_rejects_delay_budget_overrun(self):
        script_path = SCRIPT_DIR / "ci_checkout_retry.py"
        completed = subprocess.run(
            [
                sys.executable,
                str(script_path),
                "--dry-run",
                "--max-attempts",
                "4",
                "--base-delay-seconds",
                "10",
                "--cap-delay-seconds",
                "30",
                "--max-total-delay-seconds",
                "5",
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("planned total retry delay exceeds budget", completed.stderr)

    def test_regression_cli_outputs_failure_exit_code_when_outcomes_exhausted(self):
        script_path = SCRIPT_DIR / "ci_checkout_retry.py"
        with tempfile.TemporaryDirectory() as temp_dir:
            output_path = Path(temp_dir) / "output.txt"
            completed = subprocess.run(
                [
                    sys.executable,
                    str(script_path),
                    "--outcomes",
                    "failure,failure,failure",
                    "--output",
                    str(output_path),
                ],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 1)
            output = output_path.read_text(encoding="utf-8")
            self.assertIn("checkout_retry_status=failure", output)
            self.assertIn("checkout_retry_mode=checkout_retries_exhausted", output)


if __name__ == "__main__":
    unittest.main()
