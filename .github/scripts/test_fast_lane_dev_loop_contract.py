import json
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
SCRIPT_PATH = REPO_ROOT / "scripts" / "dev" / "fast-lane-dev-loop.sh"
TEST_SCRIPT_PATH = REPO_ROOT / "scripts" / "dev" / "test-fast-lane-dev-loop.sh"
GUIDE_PATH = REPO_ROOT / "docs" / "guides" / "fast-lane-dev-loop.md"
REPORT_JSON_PATH = REPO_ROOT / "tasks" / "reports" / "m25-fast-lane-loop-comparison.json"
REPORT_MD_PATH = REPO_ROOT / "tasks" / "reports" / "m25-fast-lane-loop-comparison.md"

REQUIRED_GUIDE_SNIPPETS = (
    "fast-lane-dev-loop.sh",
    "test-fast-lane-dev-loop.sh",
    "m25-fast-lane-loop-comparison.json",
)


def find_missing_snippets(text: str, required_snippets: tuple[str, ...]) -> list[str]:
    return [snippet for snippet in required_snippets if snippet not in text]


class FastLaneDevLoopContractTests(unittest.TestCase):
    def test_unit_required_paths_exist(self):
        self.assertTrue(SCRIPT_PATH.is_file(), msg=f"missing script: {SCRIPT_PATH}")
        self.assertTrue(SCRIPT_PATH.stat().st_mode & 0o111)
        self.assertTrue(TEST_SCRIPT_PATH.is_file(), msg=f"missing test script: {TEST_SCRIPT_PATH}")
        self.assertTrue(GUIDE_PATH.is_file(), msg=f"missing guide: {GUIDE_PATH}")
        self.assertTrue(REPORT_JSON_PATH.is_file(), msg=f"missing report json: {REPORT_JSON_PATH}")
        self.assertTrue(REPORT_MD_PATH.is_file(), msg=f"missing report md: {REPORT_MD_PATH}")

    def test_functional_report_shape(self):
        report = json.loads(REPORT_JSON_PATH.read_text(encoding="utf-8"))
        self.assertEqual(report["schema_version"], 1)
        self.assertIn("baseline_median_ms", report)
        self.assertIn("fast_lane_median_ms", report)
        self.assertIn("improvement_ms", report)
        self.assertIn("status", report)
        self.assertIn("wrappers", report)
        self.assertIsInstance(report["wrappers"], list)
        self.assertGreater(len(report["wrappers"]), 0)

    def test_integration_guide_references_wrapper_artifacts(self):
        guide_text = GUIDE_PATH.read_text(encoding="utf-8")
        missing = find_missing_snippets(guide_text, REQUIRED_GUIDE_SNIPPETS)
        self.assertEqual(missing, [], msg=f"missing guide snippets: {missing}")

    def test_regression_script_usage_contains_expected_subcommands(self):
        script_text = SCRIPT_PATH.read_text(encoding="utf-8")
        self.assertIn("list", script_text)
        self.assertIn("run", script_text)
        self.assertIn("benchmark", script_text)
        self.assertIn("--fixture-json", script_text)


if __name__ == "__main__":
    unittest.main()
