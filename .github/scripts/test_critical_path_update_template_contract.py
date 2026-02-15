import json
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
TEMPLATE_PATH = REPO_ROOT / "tasks" / "templates" / "critical-path-update-template.md"
RUBRIC_PATH = REPO_ROOT / "tasks" / "policies" / "critical-path-risk-rubric.json"
SYNC_GUIDE_PATH = REPO_ROOT / "docs" / "guides" / "roadmap-status-sync.md"
DOCS_INDEX_PATH = REPO_ROOT / "docs" / "README.md"


class CriticalPathTemplateContractTests(unittest.TestCase):
    def test_unit_template_and_rubric_files_exist(self):
        self.assertTrue(TEMPLATE_PATH.is_file())
        self.assertTrue(RUBRIC_PATH.is_file())

        rubric = json.loads(RUBRIC_PATH.read_text(encoding="utf-8"))
        self.assertEqual(rubric["schema_version"], 1)
        self.assertEqual(rubric["policy_id"], "critical-path-risk-rubric")

    def test_functional_template_contains_required_fields_and_allowed_values(self):
        template = TEMPLATE_PATH.read_text(encoding="utf-8")

        required_snippets = [
            "Update Date (UTC)",
            "Wave/Milestone",
            "Critical Path Item",
            "Status",
            "Blockers",
            "Owner",
            "Risk Score",
            "Risk Rationale",
            "Next Action",
            "Target Date",
            "Allowed Status Values",
            "blocked|at-risk|on-track|done",
            "Allowed Risk Values",
            "low|med|high",
        ]
        for snippet in required_snippets:
            self.assertIn(snippet, template)

    def test_functional_rubric_defines_risk_levels_and_rationale_requirements(self):
        rubric = json.loads(RUBRIC_PATH.read_text(encoding="utf-8"))
        self.assertIn("status_values", rubric)
        self.assertEqual(
            set(rubric["status_values"]),
            {"blocked", "at-risk", "on-track", "done"},
        )

        levels = rubric.get("risk_levels", {})
        self.assertEqual(set(levels.keys()), {"low", "med", "high"})
        for level_name, definition in levels.items():
            self.assertIn("criteria", definition)
            self.assertIn("rationale_required", definition)
            self.assertTrue(definition["criteria"].strip())
            self.assertTrue(definition["rationale_required"])

    def test_regression_docs_reference_template_and_rubric(self):
        sync_guide = SYNC_GUIDE_PATH.read_text(encoding="utf-8")
        docs_index = DOCS_INDEX_PATH.read_text(encoding="utf-8")

        self.assertIn("critical-path-update-template.md", sync_guide)
        self.assertIn("critical-path-risk-rubric.json", sync_guide)
        self.assertIn("Critical-Path Update Template", docs_index)


if __name__ == "__main__":
    unittest.main()
