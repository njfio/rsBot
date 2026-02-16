import json
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
SCRIPT_PATH = REPO_ROOT / "scripts" / "dev" / "roadmap-status-artifact.sh"
SCHEMA_PATH = REPO_ROOT / "tasks" / "schemas" / "roadmap-status-artifact.schema.json"
GUIDE_PATH = REPO_ROOT / "docs" / "guides" / "roadmap-status-sync.md"
WORKFLOW_PATH = REPO_ROOT / ".github" / "workflows" / "roadmap-status-artifacts.yml"

REQUIRED_GUIDE_SNIPPETS = (
    "roadmap-status-artifact.sh",
    "roadmap-status-artifact.schema.json",
    "roadmap-status-artifacts.yml",
)

REQUIRED_WORKFLOW_SNIPPETS = (
    "schedule:",
    "workflow_dispatch:",
    "issues: read",
    "scripts/dev/roadmap-status-artifact.sh",
    "actions/upload-artifact@v6",
)


def find_missing_snippets(text: str, required_snippets: tuple[str, ...]) -> list[str]:
    return [snippet for snippet in required_snippets if snippet not in text]


class RoadmapStatusArtifactContractTests(unittest.TestCase):
    def test_unit_script_and_schema_exist(self):
        self.assertTrue(SCRIPT_PATH.is_file(), msg=f"missing script: {SCRIPT_PATH}")
        self.assertTrue(SCRIPT_PATH.stat().st_mode & 0o111)
        self.assertTrue(SCHEMA_PATH.is_file(), msg=f"missing schema: {SCHEMA_PATH}")

    def test_functional_schema_has_required_contract_shape(self):
        schema = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
        self.assertEqual(schema["$schema"], "https://json-schema.org/draft/2020-12/schema")
        self.assertEqual(schema["type"], "object")

        required = set(schema["required"])
        self.assertIn("schema_version", required)
        self.assertIn("generated_at", required)
        self.assertIn("repository", required)
        self.assertIn("source_mode", required)
        self.assertIn("summary", required)
        self.assertIn("todo_groups", required)
        self.assertIn("epics", required)
        self.assertIn("gap", required)
        self.assertIn("issue_states", required)

    def test_integration_docs_reference_script_schema_and_workflow(self):
        guide_text = GUIDE_PATH.read_text(encoding="utf-8")
        missing = find_missing_snippets(guide_text, REQUIRED_GUIDE_SNIPPETS)
        self.assertEqual(missing, [], msg=f"missing guide snippets: {missing}")

    def test_regression_workflow_contract_for_scheduled_manual_artifacts(self):
        workflow_text = WORKFLOW_PATH.read_text(encoding="utf-8")
        missing = find_missing_snippets(workflow_text, REQUIRED_WORKFLOW_SNIPPETS)
        self.assertEqual(missing, [], msg=f"missing workflow snippets: {missing}")
        self.assertEqual(workflow_text.count("workflow_dispatch:"), 1)
        self.assertGreaterEqual(workflow_text.count("cron:"), 1)


if __name__ == "__main__":
    unittest.main()
