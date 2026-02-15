import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
WORKFLOW_PATH = REPO_ROOT / ".github" / "workflows" / "ci.yml"


class CiDemoSmokeScopeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.workflow = WORKFLOW_PATH.read_text(encoding="utf-8")

    def test_integration_paths_filter_defines_demo_smoke_scope(self):
        self.assertIn("demo_smoke:", self.workflow)
        self.assertIn('- "crates/tau-coding-agent/**"', self.workflow)
        self.assertIn('- ".github/demo-smoke-manifest.json"', self.workflow)
        self.assertIn('- ".github/scripts/demo_smoke_runner.py"', self.workflow)

    def test_integration_demo_smoke_scope_step_is_present(self):
        self.assertIn("name: Determine codex demo smoke scope", self.workflow)
        self.assertIn("id: demo_smoke_scope", self.workflow)
        self.assertIn(
            'demo_smoke_needed="${{ steps.change_scope.outputs.demo_smoke }}"',
            self.workflow,
        )

    def test_regression_codex_demo_smoke_run_is_gated_by_scope_output(self):
        self.assertIn("name: Run codex light demo smoke", self.workflow)
        self.assertIn(
            "steps.demo_smoke_scope.outputs.demo_smoke_needed == 'true'",
            self.workflow,
        )

    def test_regression_codex_demo_smoke_upload_is_gated_by_scope_output(self):
        self.assertIn("name: Upload codex light demo smoke logs", self.workflow)
        self.assertIn(
            "steps.demo_smoke_scope.outputs.demo_smoke_needed == 'true'",
            self.workflow,
        )


if __name__ == "__main__":
    unittest.main()
