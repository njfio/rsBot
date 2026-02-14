import re
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]

RELEASE_CHANNEL_OPS = REPO_ROOT / "docs" / "guides" / "release-channel-ops.md"
SIGNOFF_CHECKLIST = REPO_ROOT / "docs" / "guides" / "release-signoff-checklist.md"
SURFACE_RUNBOOKS = [
    REPO_ROOT / "docs" / "guides" / "voice-ops.md",
    REPO_ROOT / "docs" / "guides" / "browser-automation-live-ops.md",
    REPO_ROOT / "docs" / "guides" / "dashboard-ops.md",
    REPO_ROOT / "docs" / "guides" / "custom-command-ops.md",
    REPO_ROOT / "docs" / "guides" / "memory-ops.md",
]


class ReleaseRolloutDocsTests(unittest.TestCase):
    def test_unit_release_channel_runbook_defines_expected_canary_percentages(self):
        content = RELEASE_CHANNEL_OPS.read_text(encoding="utf-8")
        phase_percentages = re.findall(r"\|\s*canary-\d+\s*\|\s*(\d+)%\s*\|", content)
        self.assertEqual(phase_percentages, ["5", "25", "50"])
        self.assertIn("| general-availability | 100% |", content)

    def test_functional_surface_runbooks_include_canary_profile_and_global_contract_link(self):
        for runbook in SURFACE_RUNBOOKS:
            content = runbook.read_text(encoding="utf-8")
            self.assertIn("## Canary rollout profile", content, msg=str(runbook))
            self.assertIn(
                "release-channel-ops.md#cross-surface-rollout-contract",
                content,
                msg=str(runbook),
            )

    def test_integration_signoff_checklist_covers_all_surfaces_and_evidence_contract(self):
        content = SIGNOFF_CHECKLIST.read_text(encoding="utf-8")
        required_links = [
            "release-channel-ops.md",
            "live-run-unified-ops.md",
            "voice-ops.md",
            "browser-automation-live-ops.md",
            "dashboard-ops.md",
            "custom-command-ops.md",
            "memory-ops.md",
        ]
        for link in required_links:
            self.assertIn(link, content)
        self.assertIn("Mandatory Evidence Contract", content)
        self.assertIn("CI URL", content)
        self.assertIn("repository artifact path", content)

    def test_regression_release_channel_runbook_includes_rollback_trigger_matrix(self):
        content = RELEASE_CHANNEL_OPS.read_text(encoding="utf-8")
        self.assertIn("## Rollback Trigger Matrix", content)
        self.assertIn("failure_streak>=3", content)
        self.assertIn("case_processing_failed", content)
        self.assertIn("malformed_inputs_observed", content)
        self.assertIn("live-smoke-matrix", content)
        self.assertIn("## Rollback Execution Steps", content)


if __name__ == "__main__":
    unittest.main()
