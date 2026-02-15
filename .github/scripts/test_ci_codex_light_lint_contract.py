import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
WORKFLOW_PATH = REPO_ROOT / ".github" / "workflows" / "ci.yml"


class CiCodexLightLintContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.workflow = WORKFLOW_PATH.read_text(encoding="utf-8")

    def test_integration_codex_light_lint_uses_fast_validate_check_only(self):
        self.assertIn("name: Lint (codex-light lane)", self.workflow)
        self.assertIn(
            "./scripts/dev/fast-validate.sh --check-only --direct-packages-only --base",
            self.workflow,
        )
        self.assertIn(
            "./scripts/dev/fast-validate.sh --check-only --direct-packages-only --full",
            self.workflow,
        )

    def test_regression_codex_light_lint_no_longer_uses_direct_tau_coding_agent_check(self):
        self.assertNotIn(
            "cargo check -p tau-coding-agent --all-targets",
            self.workflow,
        )


if __name__ == "__main__":
    unittest.main()
