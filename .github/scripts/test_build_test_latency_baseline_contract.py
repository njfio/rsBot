import json
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
SCRIPT_PATH = REPO_ROOT / "scripts" / "dev" / "build-test-latency-baseline.sh"
SCHEMA_PATH = REPO_ROOT / "tasks" / "schemas" / "m25-build-test-latency-baseline.schema.json"
GUIDE_PATH = REPO_ROOT / "docs" / "guides" / "build-test-latency-baseline.md"

REQUIRED_GUIDE_SNIPPETS = (
    "build-test-latency-baseline.sh",
    "m25-build-test-latency-baseline.schema.json",
    "test-build-test-latency-baseline.sh",
)


def find_missing_snippets(text: str, required_snippets: tuple[str, ...]) -> list[str]:
    return [snippet for snippet in required_snippets if snippet not in text]


class BuildTestLatencyBaselineContractTests(unittest.TestCase):
    def test_unit_required_paths_exist(self):
        self.assertTrue(SCRIPT_PATH.is_file(), msg=f"missing script: {SCRIPT_PATH}")
        self.assertTrue(SCRIPT_PATH.stat().st_mode & 0o111)
        self.assertTrue(SCHEMA_PATH.is_file(), msg=f"missing schema: {SCHEMA_PATH}")
        self.assertTrue(GUIDE_PATH.is_file(), msg=f"missing guide: {GUIDE_PATH}")

    def test_functional_schema_shape(self):
        schema = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
        self.assertEqual(schema["$schema"], "https://json-schema.org/draft/2020-12/schema")
        self.assertEqual(schema["type"], "object")

        required = set(schema["required"])
        self.assertIn("schema_version", required)
        self.assertIn("generated_at", required)
        self.assertIn("repository", required)
        self.assertIn("source_mode", required)
        self.assertIn("environment", required)
        self.assertIn("summary", required)
        self.assertIn("commands", required)
        self.assertIn("hotspots", required)

    def test_integration_guide_references_contract_assets(self):
        guide_text = GUIDE_PATH.read_text(encoding="utf-8")
        missing = find_missing_snippets(guide_text, REQUIRED_GUIDE_SNIPPETS)
        self.assertEqual(missing, [], msg=f"missing guide snippets: {missing}")

    def test_regression_script_usage_includes_fixture_and_command_modes(self):
        script_text = SCRIPT_PATH.read_text(encoding="utf-8")
        self.assertIn("--fixture-json", script_text)
        self.assertIn("--command", script_text)
        self.assertIn("--iterations", script_text)
        self.assertIn("source_mode", script_text)


if __name__ == "__main__":
    unittest.main()
