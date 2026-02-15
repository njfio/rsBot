import json
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
POLICY_PATH = REPO_ROOT / "tasks" / "policies" / "doc-quality-remediation-policy.json"
TEMPLATE_PATH = REPO_ROOT / "tasks" / "templates" / "doc-quality-remediation-tracker.md"
GUIDE_PATH = REPO_ROOT / "docs" / "guides" / "doc-quality-remediation.md"
DOCS_INDEX_PATH = REPO_ROOT / "docs" / "README.md"


class DocQualityRemediationContractTests(unittest.TestCase):
    def test_unit_policy_contract_shape(self):
        self.assertTrue(POLICY_PATH.is_file())
        policy = json.loads(POLICY_PATH.read_text(encoding="utf-8"))

        self.assertEqual(policy["schema_version"], 1)
        self.assertEqual(policy["policy_id"], "doc-quality-remediation-policy")
        self.assertIn("severity_classes", policy)
        self.assertIn("closure_proof_fields", policy)
        self.assertIn("checklist_items", policy)

        severities = policy["severity_classes"]
        self.assertIsInstance(severities, list)
        self.assertGreaterEqual(len(severities), 3)
        for severity in severities:
            self.assertIn("id", severity)
            self.assertIn("name", severity)
            self.assertIn("definition", severity)
            self.assertIn("target_sla_hours", severity)
            self.assertGreater(int(severity["target_sla_hours"]), 0)

    def test_functional_template_contains_required_sections(self):
        self.assertTrue(TEMPLATE_PATH.is_file())
        template = TEMPLATE_PATH.read_text(encoding="utf-8")

        self.assertIn("# Doc Quality Remediation Tracker", template)
        self.assertIn("## Finding Metadata", template)
        self.assertIn("## Severity And SLA", template)
        self.assertIn("## Remediation Checklist", template)
        self.assertIn("## Closure Proof", template)

    def test_integration_guide_references_policy_and_template(self):
        self.assertTrue(GUIDE_PATH.is_file())
        guide = GUIDE_PATH.read_text(encoding="utf-8")
        docs_index = DOCS_INDEX_PATH.read_text(encoding="utf-8")

        self.assertIn("doc-quality-remediation-policy.json", guide)
        self.assertIn("doc-quality-remediation-tracker.md", guide)
        self.assertIn("Doc Quality Remediation Workflow", docs_index)
        self.assertIn("guides/doc-quality-remediation.md", docs_index)

    def test_regression_severity_ids_stay_aligned(self):
        policy = json.loads(POLICY_PATH.read_text(encoding="utf-8"))
        template = TEMPLATE_PATH.read_text(encoding="utf-8")

        severity_names = [entry["name"] for entry in policy["severity_classes"]]
        for name in severity_names:
            self.assertIn(name, template)

    def test_regression_closure_fields_are_referenced_in_template_and_guide(self):
        policy = json.loads(POLICY_PATH.read_text(encoding="utf-8"))
        template = TEMPLATE_PATH.read_text(encoding="utf-8")
        guide = GUIDE_PATH.read_text(encoding="utf-8")

        closure_fields = policy["closure_proof_fields"]
        for field in closure_fields:
            self.assertIn(field, template)
            self.assertIn(field, guide)


if __name__ == "__main__":
    unittest.main()
