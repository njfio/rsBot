import json
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
RULES_PATH = REPO_ROOT / "tasks" / "policies" / "issue-hierarchy-drift-rules.json"
GUIDE_PATH = REPO_ROOT / "docs" / "guides" / "issue-hierarchy-drift-rules.md"
SYNC_GUIDE_PATH = REPO_ROOT / "docs" / "guides" / "roadmap-status-sync.md"


def load_rules() -> dict:
    return json.loads(RULES_PATH.read_text(encoding="utf-8"))


class IssueHierarchyDriftRulesTests(unittest.TestCase):
    def test_functional_rules_file_has_required_contract_sections(self):
        rules = load_rules()
        self.assertEqual(rules["schema_version"], 1)
        self.assertIn("required_metadata", rules)
        self.assertIn("orphan_conditions", rules)
        self.assertIn("drift_conditions", rules)
        self.assertIn("remediation", rules)

        required_labels = set(rules["required_metadata"]["required_labels"])
        self.assertIn("roadmap", required_labels)
        self.assertIn("testing-matrix", required_labels)
        self.assertIn("epic", rules["required_metadata"]["hierarchy_labels"])
        self.assertIn("story", rules["required_metadata"]["hierarchy_labels"])
        self.assertIn("task", rules["required_metadata"]["hierarchy_labels"])

    def test_regression_condition_ids_are_unique_and_fully_remediated(self):
        rules = load_rules()
        orphan_ids = [entry["id"] for entry in rules["orphan_conditions"]]
        drift_ids = [entry["id"] for entry in rules["drift_conditions"]]
        all_condition_ids = orphan_ids + drift_ids

        self.assertEqual(len(all_condition_ids), len(set(all_condition_ids)))
        remediation_ids = {entry["condition_id"] for entry in rules["remediation"]}
        self.assertEqual(set(all_condition_ids), remediation_ids)

    def test_regression_docs_reference_policy_and_condition_ids(self):
        rules = load_rules()
        guide_text = GUIDE_PATH.read_text(encoding="utf-8")
        sync_guide_text = SYNC_GUIDE_PATH.read_text(encoding="utf-8")

        self.assertIn("issue-hierarchy-drift-rules.json", guide_text)
        self.assertIn("issue-hierarchy-drift-rules.json", sync_guide_text)
        for entry in rules["orphan_conditions"] + rules["drift_conditions"]:
            self.assertIn(entry["id"], guide_text)


if __name__ == "__main__":
    unittest.main()
