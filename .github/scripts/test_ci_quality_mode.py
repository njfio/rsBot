import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

import ci_quality_mode  # noqa: E402


class QualityModeTests(unittest.TestCase):
    def test_unit_resolve_quality_mode_codex_non_heavy_uses_light_lane(self):
        decision = ci_quality_mode.resolve_quality_mode(
            event_name="pull_request",
            head_ref="codex/issue-123",
            heavy_changed=False,
            run_coverage_requested=False,
            run_cross_platform_requested=False,
        )
        self.assertEqual(decision.mode, "codex-light")
        self.assertEqual(decision.reason, "codex-branch-non-heavy-pr")
        self.assertFalse(decision.heavy_changed)
        self.assertFalse(decision.run_coverage)
        self.assertFalse(decision.run_cross_platform)
        self.assertEqual(decision.heavy_reason, "pull-request-cost-governed")

    def test_functional_render_summary_includes_cost_governance_fields(self):
        decision = ci_quality_mode.QualityModeDecision(
            mode="full",
            reason="codex-branch-heavy-pr",
            heavy_changed=True,
            run_coverage=True,
            run_cross_platform=False,
            heavy_reason="manual-dispatch-requested",
        )
        summary = ci_quality_mode.render_summary(decision)
        self.assertIn("### CI Cost Governance", summary)
        self.assertIn("- Mode: full", summary)
        self.assertIn("- Reason: codex-branch-heavy-pr", summary)
        self.assertIn("- Heavy paths changed: true", summary)
        self.assertIn("- Heavy lane: enabled", summary)
        self.assertIn("- Heavy reason: manual-dispatch-requested", summary)
        self.assertIn("- Run coverage: true", summary)
        self.assertIn("- Run cross-platform smoke: false", summary)

    def test_integration_cli_writes_workflow_output_and_summary(self):
        script_path = SCRIPT_DIR / "ci_quality_mode.py"
        with tempfile.TemporaryDirectory() as temp_dir:
            output_path = Path(temp_dir) / "github_output.txt"
            summary_path = Path(temp_dir) / "summary.md"
            subprocess.run(
                [
                    sys.executable,
                    str(script_path),
                    "--event-name",
                    "schedule",
                    "--head-ref",
                    "codex/issue-456",
                    "--heavy-changed",
                    "false",
                    "--run-coverage",
                    "false",
                    "--run-cross-platform",
                    "false",
                    "--output",
                    str(output_path),
                    "--summary",
                    str(summary_path),
                ],
                check=True,
            )
            output_raw = output_path.read_text(encoding="utf-8")
            summary_raw = summary_path.read_text(encoding="utf-8")
            self.assertIn("mode=full", output_raw)
            self.assertIn("reason=non-pr-default", output_raw)
            self.assertIn("heavy_changed=false", output_raw)
            self.assertIn("run_coverage=true", output_raw)
            self.assertIn("run_cross_platform=true", output_raw)
            self.assertIn("heavy_reason=scheduled-deferred-heavy", output_raw)
            self.assertIn("Heavy lane: enabled", summary_raw)

    def test_regression_pull_request_never_enables_heavy_lane_from_manual_inputs(self):
        non_codex = ci_quality_mode.resolve_quality_mode(
            event_name="pull_request",
            head_ref="feature/new-lane",
            heavy_changed=False,
            run_coverage_requested=True,
            run_cross_platform_requested=True,
        )
        self.assertEqual(non_codex.mode, "full")
        self.assertEqual(non_codex.reason, "pull-request-default")
        self.assertFalse(non_codex.run_coverage)
        self.assertFalse(non_codex.run_cross_platform)
        self.assertEqual(non_codex.heavy_reason, "pull-request-cost-governed")

        heavy_codex = ci_quality_mode.resolve_quality_mode(
            event_name="pull_request",
            head_ref="codex/issue-789",
            heavy_changed=True,
            run_coverage_requested=True,
            run_cross_platform_requested=True,
        )
        self.assertEqual(heavy_codex.mode, "full")
        self.assertEqual(heavy_codex.reason, "codex-branch-heavy-pr")
        self.assertFalse(heavy_codex.run_coverage)
        self.assertFalse(heavy_codex.run_cross_platform)
        self.assertEqual(heavy_codex.heavy_reason, "pull-request-cost-governed")


if __name__ == "__main__":
    unittest.main()
