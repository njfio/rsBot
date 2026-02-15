from __future__ import annotations

import json
from pathlib import Path
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]
REPORT_JSON_PATH = REPO_ROOT / "tasks" / "reports" / "m23-doc-quality-spot-audit.json"
REPORT_MD_PATH = REPO_ROOT / "tasks" / "reports" / "m23-doc-quality-spot-audit.md"
HELPER_JSON_PATH = REPO_ROOT / "tasks" / "reports" / "m23-doc-quality-audit-helper.json"
POLICY_PATH = REPO_ROOT / "tasks" / "policies" / "doc-quality-anti-patterns.json"
GUIDE_PATH = REPO_ROOT / "docs" / "guides" / "doc-quality-remediation.md"


class DocQualitySpotAuditContractTests(unittest.TestCase):
    def test_unit_spot_audit_schema_and_score_threshold(self) -> None:
        report = json.loads(REPORT_JSON_PATH.read_text(encoding="utf-8"))
        self.assertEqual(report["schema_version"], 1)
        self.assertEqual(report["issue"], 1656)
        self.assertGreaterEqual(report["summary"]["sample_count"], 5)
        self.assertGreaterEqual(
            float(report["summary"]["average_score"]),
            float(report["summary"]["pass_threshold_average"]),
        )
        self.assertTrue(report["summary"]["audit_passed"])

    def test_functional_helper_findings_reduced_after_calibration(self) -> None:
        report = json.loads(REPORT_JSON_PATH.read_text(encoding="utf-8"))
        baseline_findings = int(report["summary"]["baseline_findings_count"])
        post_findings = int(report["summary"]["post_remediation_findings_count"])
        self.assertGreaterEqual(baseline_findings, post_findings)
        self.assertEqual(post_findings, 0)

        helper = json.loads(HELPER_JSON_PATH.read_text(encoding="utf-8"))
        self.assertEqual(helper["summary"]["findings_count"], post_findings)

    def test_conformance_policy_heuristic_calibration_present(self) -> None:
        policy = json.loads(POLICY_PATH.read_text(encoding="utf-8"))
        pattern = None
        for entry in policy["patterns"]:
            if entry["id"] == "generic_sets_gets_returns":
                pattern = entry
                break
        self.assertIsNotNone(pattern)
        self.assertIn("single-token payload", pattern["description"])

    def test_integration_report_and_guide_references_present(self) -> None:
        report_md = REPORT_MD_PATH.read_text(encoding="utf-8")
        guide = GUIDE_PATH.read_text(encoding="utf-8")
        self.assertIn("## Checklist", report_md)
        self.assertIn("m23-doc-quality-spot-audit.json", guide)
        self.assertIn("m23-doc-quality-spot-audit.md", guide)


if __name__ == "__main__":
    unittest.main()
