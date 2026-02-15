from __future__ import annotations

import json
from pathlib import Path
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]
PLAN_PATH = REPO_ROOT / "tasks" / "policies" / "m23-doc-allocation-plan.json"
TARGETS_PATH = REPO_ROOT / "docs" / "guides" / "doc-density-targets.json"
SCORECARD_PATH = REPO_ROOT / "docs" / "guides" / "doc-density-scorecard.md"
GUIDE_PATH = REPO_ROOT / "docs" / "guides" / "doc-density-allocation-plan.md"
DOCS_INDEX_PATH = REPO_ROOT / "docs" / "README.md"


class DocAllocationPlanContractTests(unittest.TestCase):
    def test_unit_plan_totals_match_delta_math_and_gate_floor(self) -> None:
        plan = json.loads(PLAN_PATH.read_text(encoding="utf-8"))
        self.assertEqual(plan["schema_version"], 1)
        self.assertGreaterEqual(plan["target_floor_markers"], 3000)

        current_total = int(plan["current_total_markers"])
        target_total = int(plan["target_total_markers"])
        delta_sum = sum(int(entry["delta_required"]) for entry in plan["quota_allocations"])
        self.assertEqual(current_total + delta_sum, target_total)
        self.assertGreaterEqual(target_total, plan["target_floor_markers"])

    def test_functional_quota_allocations_have_owner_domain_and_cadence(self) -> None:
        plan = json.loads(PLAN_PATH.read_text(encoding="utf-8"))
        cadences = plan["owner_domain_cadence"]
        allocations = plan["quota_allocations"]
        self.assertGreaterEqual(len(allocations), 10)

        for row in allocations:
            self.assertIn("crate", row)
            self.assertIn("current_markers", row)
            self.assertIn("target_markers", row)
            self.assertIn("delta_required", row)
            self.assertIn("owner_domain", row)
            self.assertIn("review_cadence_days", row)
            self.assertGreaterEqual(int(row["delta_required"]), 1)
            self.assertGreaterEqual(int(row["target_markers"]), int(row["current_markers"]))

            domain = row["owner_domain"]
            self.assertIn(domain, cadences)
            self.assertEqual(
                int(row["review_cadence_days"]),
                int(cadences[domain]["review_cadence_days"]),
            )

    def test_conformance_checkpoint_schedule_is_ordered_and_hits_gate(self) -> None:
        plan = json.loads(PLAN_PATH.read_text(encoding="utf-8"))
        checkpoints = plan["checkpoint_schedule"]
        self.assertGreaterEqual(len(checkpoints), 3)

        dates = [entry["date"] for entry in checkpoints]
        self.assertEqual(dates, sorted(dates))

        mins = [int(entry["min_total_markers"]) for entry in checkpoints]
        self.assertEqual(mins, sorted(mins))
        self.assertGreaterEqual(mins[-1], 3000)

    def test_integration_docs_reference_allocation_plan_contract(self) -> None:
        targets = json.loads(TARGETS_PATH.read_text(encoding="utf-8"))
        scorecard = SCORECARD_PATH.read_text(encoding="utf-8")
        guide = GUIDE_PATH.read_text(encoding="utf-8")
        docs_index = DOCS_INDEX_PATH.read_text(encoding="utf-8")

        self.assertEqual(
            targets["allocation_plan_file"],
            "tasks/policies/m23-doc-allocation-plan.json",
        )
        self.assertIn("owner_domain_review_cadence_days", targets)
        self.assertIn("m23-doc-allocation-plan.json", scorecard)
        self.assertIn("m23-doc-allocation-plan.md", scorecard)
        self.assertIn("M23 gate floor", guide)
        self.assertIn("guides/doc-density-allocation-plan.md", docs_index)


if __name__ == "__main__":
    unittest.main()
