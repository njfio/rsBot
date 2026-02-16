import json
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
SCRIPT_PATH = REPO_ROOT / "scripts" / "dev" / "latency-budget-gate.sh"
POLICY_PATH = REPO_ROOT / "tasks" / "policies" / "m25-latency-budget-policy.json"
GUIDE_PATH = REPO_ROOT / "docs" / "guides" / "latency-budget-gate.md"
REPORT_JSON_PATH = REPO_ROOT / "tasks" / "reports" / "m25-latency-budget-gate.json"
REPORT_MD_PATH = REPO_ROOT / "tasks" / "reports" / "m25-latency-budget-gate.md"

REQUIRED_GUIDE_SNIPPETS = (
    "latency-budget-gate.sh",
    "m25-latency-budget-policy.json",
    "test-latency-budget-gate.sh",
)


def find_missing_snippets(text: str, required_snippets: tuple[str, ...]) -> list[str]:
    return [snippet for snippet in required_snippets if snippet not in text]


class LatencyBudgetGateContractTests(unittest.TestCase):
    def test_unit_required_paths_exist(self):
        self.assertTrue(SCRIPT_PATH.is_file(), msg=f"missing script: {SCRIPT_PATH}")
        self.assertTrue(SCRIPT_PATH.stat().st_mode & 0o111)
        self.assertTrue(POLICY_PATH.is_file(), msg=f"missing policy: {POLICY_PATH}")
        self.assertTrue(GUIDE_PATH.is_file(), msg=f"missing guide: {GUIDE_PATH}")
        self.assertTrue(REPORT_JSON_PATH.is_file(), msg=f"missing report json: {REPORT_JSON_PATH}")
        self.assertTrue(REPORT_MD_PATH.is_file(), msg=f"missing report md: {REPORT_MD_PATH}")

    def test_functional_policy_shape(self):
        policy = json.loads(POLICY_PATH.read_text(encoding="utf-8"))
        self.assertEqual(policy["schema_version"], 1)
        self.assertIn("max_fast_lane_median_ms", policy)
        self.assertIn("min_improvement_percent", policy)
        self.assertIn("max_regression_percent", policy)
        self.assertIn("enforcement_mode", policy)
        self.assertIn("remediation", policy)

    def test_integration_guide_references_gate_assets(self):
        guide_text = GUIDE_PATH.read_text(encoding="utf-8")
        missing = find_missing_snippets(guide_text, REQUIRED_GUIDE_SNIPPETS)
        self.assertEqual(missing, [], msg=f"missing guide snippets: {missing}")

    def test_regression_report_shape(self):
        report = json.loads(REPORT_JSON_PATH.read_text(encoding="utf-8"))
        self.assertEqual(report["schema_version"], 1)
        self.assertIn("status", report)
        self.assertIn("violations", report)
        self.assertIn("checks", report)
        self.assertIn("report_summary", report)


if __name__ == "__main__":
    unittest.main()
