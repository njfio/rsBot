import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
CI_WORKFLOW = REPO_ROOT / ".github" / "workflows" / "ci.yml"
DOCS_QUALITY_WORKFLOW = REPO_ROOT / ".github" / "workflows" / "docs-quality.yml"

CI_REQUIRED_SNIPPETS = (
    "issues: read",
    "- name: Check roadmap status sync blocks",
    "GH_TOKEN: ${{ github.token }}",
    "run: scripts/dev/roadmap-status-sync.sh --check",
)

DOCS_REQUIRED_SNIPPETS = (
    "issues: read",
    '- "tasks/**"',
    '- "scripts/dev/roadmap-status-sync.sh"',
    '- "scripts/dev/test-roadmap-status-sync.sh"',
    "- name: Validate roadmap status sync script",
    "run: scripts/dev/test-roadmap-status-sync.sh",
    "- name: Check roadmap status sync blocks",
    "GH_TOKEN: ${{ github.token }}",
    "run: scripts/dev/roadmap-status-sync.sh --check",
)


def find_missing_snippets(text: str, required_snippets: tuple[str, ...]) -> list[str]:
    return [snippet for snippet in required_snippets if snippet not in text]


class RoadmapStatusWorkflowContractTests(unittest.TestCase):
    def test_unit_find_missing_snippets_detects_absent_requirements(self):
        missing = find_missing_snippets(
            "permissions:\n  contents: read\n",
            ("contents: read", "issues: read"),
        )
        self.assertEqual(missing, ["issues: read"])

    def test_functional_ci_workflow_enforces_roadmap_status_check(self):
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        missing = find_missing_snippets(workflow, CI_REQUIRED_SNIPPETS)
        self.assertEqual(missing, [], msg=f"missing CI workflow requirements: {missing}")

    def test_integration_docs_quality_workflow_covers_roadmap_sync_contract(self):
        workflow = DOCS_QUALITY_WORKFLOW.read_text(encoding="utf-8")
        missing = find_missing_snippets(workflow, DOCS_REQUIRED_SNIPPETS)
        self.assertEqual(
            missing,
            [],
            msg=f"missing docs-quality workflow requirements: {missing}",
        )

        self.assertEqual(workflow.count("scripts/dev/roadmap-status-sync.sh --check"), 1)
        self.assertEqual(workflow.count("scripts/dev/test-roadmap-status-sync.sh"), 2)

    def test_regression_contract_reports_missing_permissions_and_check_step(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            broken_workflow = Path(temp_dir) / "ci.yml"
            broken_workflow.write_text(
                "permissions:\n"
                "  contents: read\n"
                "jobs:\n"
                "  quality-linux:\n"
                "    steps:\n"
                "      - name: Validate CI helper scripts\n"
                "        run: python3 -m unittest discover -s .github/scripts -p \"test_*.py\"\n",
                encoding="utf-8",
            )
            missing = find_missing_snippets(
                broken_workflow.read_text(encoding="utf-8"),
                CI_REQUIRED_SNIPPETS,
            )
            self.assertIn("issues: read", missing)
            self.assertIn("- name: Check roadmap status sync blocks", missing)
            self.assertIn("run: scripts/dev/roadmap-status-sync.sh --check", missing)


if __name__ == "__main__":
    unittest.main()
