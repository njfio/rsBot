import json
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
POLICY_PATH = REPO_ROOT / "tasks" / "policies" / "rl-terms-allowlist.json"
GUIDE_PATH = REPO_ROOT / "docs" / "guides" / "rl-terminology-allowlist.md"
SCRIPT_PATH = REPO_ROOT / "scripts" / "dev" / "rl-terminology-scan.sh"
DOCS_INDEX_PATH = REPO_ROOT / "docs" / "README.md"


class RlTerminologyAllowlistContractTests(unittest.TestCase):
    def test_unit_policy_schema_has_required_fields(self):
        self.assertTrue(POLICY_PATH.is_file())
        policy = json.loads(POLICY_PATH.read_text(encoding="utf-8"))

        self.assertEqual(policy["schema_version"], 1)
        self.assertEqual(policy["policy_id"], "rl-terms-allowlist")
        self.assertIn("approved_terms", policy)
        self.assertIn("disallowed_defaults", policy)
        self.assertGreater(len(policy["approved_terms"]), 0)

        for entry in policy["approved_terms"]:
            self.assertIn("term", entry)
            self.assertIn("allowed_paths", entry)
            self.assertIn("required_context", entry)
            self.assertIn("rationale", entry)

    def test_functional_guide_has_examples_and_non_examples(self):
        self.assertTrue(GUIDE_PATH.is_file())
        guide = GUIDE_PATH.read_text(encoding="utf-8")

        self.assertIn("## Approved Examples", guide)
        self.assertIn("## Non-Examples", guide)
        self.assertIn("future true-RL roadmap", guide)
        self.assertIn("stale wording", guide)

    def test_integration_docs_index_and_script_discoverability(self):
        self.assertTrue(SCRIPT_PATH.is_file())
        self.assertTrue(SCRIPT_PATH.stat().st_mode & 0o111)

        docs_index = DOCS_INDEX_PATH.read_text(encoding="utf-8")
        self.assertIn("RL Terminology Allowlist", docs_index)
        self.assertIn("guides/rl-terminology-allowlist.md", docs_index)

    def test_regression_policy_examples_align_with_guide(self):
        policy = json.loads(POLICY_PATH.read_text(encoding="utf-8"))
        guide = GUIDE_PATH.read_text(encoding="utf-8")

        for entry in policy["approved_terms"]:
            self.assertIn(entry["term"], guide)
            for path in entry["allowed_paths"]:
                self.assertIn(path, guide)


if __name__ == "__main__":
    unittest.main()
