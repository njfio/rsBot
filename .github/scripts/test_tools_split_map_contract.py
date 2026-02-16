import json
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
SPLIT_MAP_SCRIPT = REPO_ROOT / "scripts" / "dev" / "tools-split-map.sh"
SCHEMA_PATH = REPO_ROOT / "tasks" / "schemas" / "m25-tools-split-map.schema.json"
GUIDE_PATH = REPO_ROOT / "docs" / "guides" / "tools-split-map.md"
REPORT_JSON_PATH = REPO_ROOT / "tasks" / "reports" / "m25-tools-split-map.json"
REPORT_MD_PATH = REPO_ROOT / "tasks" / "reports" / "m25-tools-split-map.md"

REQUIRED_GUIDE_SNIPPETS = (
    "tools-split-map.sh",
    "m25-tools-split-map.schema.json",
    "m25-tools-split-map.json",
    "m25-tools-split-map.md",
    "Public API Impact",
    "Test Migration Plan",
)


class ToolsSplitMapContractTests(unittest.TestCase):
    def test_unit_required_files_exist(self):
        self.assertTrue(SPLIT_MAP_SCRIPT.is_file(), msg=f"missing script: {SPLIT_MAP_SCRIPT}")
        self.assertTrue(SPLIT_MAP_SCRIPT.stat().st_mode & 0o111)
        self.assertTrue(SCHEMA_PATH.is_file(), msg=f"missing schema: {SCHEMA_PATH}")
        self.assertTrue(GUIDE_PATH.is_file(), msg=f"missing guide: {GUIDE_PATH}")
        self.assertTrue(REPORT_JSON_PATH.is_file(), msg=f"missing report json: {REPORT_JSON_PATH}")
        self.assertTrue(REPORT_MD_PATH.is_file(), msg=f"missing report md: {REPORT_MD_PATH}")

    def test_functional_schema_contract_contains_required_fields(self):
        schema = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
        self.assertEqual(schema["$schema"], "https://json-schema.org/draft/2020-12/schema")
        required = set(schema["required"])
        self.assertIn("schema_version", required)
        self.assertIn("generated_at", required)
        self.assertIn("source_file", required)
        self.assertIn("target_line_budget", required)
        self.assertIn("current_line_count", required)
        self.assertIn("line_gap_to_target", required)
        self.assertIn("extraction_phases", required)
        self.assertIn("public_api_impact", required)
        self.assertIn("test_migration_plan", required)

    def test_integration_guide_references_split_map_contract_artifacts(self):
        guide_text = GUIDE_PATH.read_text(encoding="utf-8")
        missing = [snippet for snippet in REQUIRED_GUIDE_SNIPPETS if snippet not in guide_text]
        self.assertEqual(missing, [], msg=f"missing guide snippets: {missing}")

    def test_regression_report_matches_schema_version_and_target_budget(self):
        payload = json.loads(REPORT_JSON_PATH.read_text(encoding="utf-8"))
        self.assertEqual(payload["schema_version"], 1)
        self.assertEqual(payload["source_file"], "crates/tau-tools/src/tools.rs")
        self.assertEqual(payload["target_line_budget"], 3000)
        self.assertGreater(len(payload["extraction_phases"]), 0)
        self.assertGreater(len(payload["public_api_impact"]), 0)
        self.assertGreater(len(payload["test_migration_plan"]), 0)


if __name__ == "__main__":
    unittest.main()
