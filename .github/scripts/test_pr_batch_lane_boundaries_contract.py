import json
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
POLICY_PATH = REPO_ROOT / "tasks" / "policies" / "pr-batch-lane-boundaries.json"
EXCEPTIONS_PATH = REPO_ROOT / "tasks" / "policies" / "pr-batch-exceptions.json"
GUIDE_PATH = REPO_ROOT / "docs" / "guides" / "pr-batch-lane-boundaries.md"
SYNC_GUIDE_PATH = REPO_ROOT / "docs" / "guides" / "roadmap-status-sync.md"
PR_TEMPLATE_PATH = REPO_ROOT / ".github" / "pull_request_template.md"


def load_policy() -> dict:
    return json.loads(POLICY_PATH.read_text(encoding="utf-8"))


def load_exceptions() -> dict:
    return json.loads(EXCEPTIONS_PATH.read_text(encoding="utf-8"))


def find_missing_snippets(text: str, required_snippets: tuple[str, ...]) -> list[str]:
    return [snippet for snippet in required_snippets if snippet not in text]


class PrBatchLaneBoundariesContractTests(unittest.TestCase):
    def test_functional_policy_has_required_lane_boundary_contract(self):
        policy = load_policy()

        self.assertEqual(policy["schema_version"], 1)
        self.assertIn("lanes", policy)
        self.assertIn("high_conflict_hotspots", policy)
        self.assertIn("batch_size_review_sla_matrix", policy)
        self.assertIn("exception_workflow", policy)
        self.assertIn("pr_reference_contract", policy)

        lane_ids = {lane["id"] for lane in policy["lanes"]}
        self.assertEqual(lane_ids, {"structural", "docs", "rl"})
        for lane in policy["lanes"]:
            self.assertGreater(len(lane["owned_path_prefixes"]), 0)
            self.assertGreater(len(lane["shared_paths"]), 0)

        matrix_lane_ids = {entry["lane"] for entry in policy["batch_size_review_sla_matrix"]}
        self.assertEqual(matrix_lane_ids, lane_ids)
        for entry in policy["batch_size_review_sla_matrix"]:
            self.assertGreater(entry["max_open_prs_per_batch"], 0)
            self.assertGreater(entry["max_hotspot_paths_per_pr"], 0)
            self.assertGreater(entry["target_first_review_hours"], 0)
            self.assertGreater(entry["target_merge_hours"], 0)
            self.assertLessEqual(
                entry["target_first_review_hours"],
                entry["target_merge_hours"],
            )
            self.assertGreater(entry["required_reviewers"], 0)

    def test_regression_hotspot_ids_and_lane_ownership_are_consistent(self):
        policy = load_policy()
        lane_ids = {lane["id"] for lane in policy["lanes"]}
        hotspots = policy["high_conflict_hotspots"]
        hotspot_ids = [entry["id"] for entry in hotspots]

        self.assertEqual(len(hotspot_ids), len(set(hotspot_ids)))
        self.assertGreaterEqual(len(hotspots), 4)
        for entry in hotspots:
            self.assertIn(entry["preferred_owner_lane"], lane_ids)
            self.assertGreater(len(entry["mitigation_steps"]), 0)

    def test_integration_docs_and_template_reference_policy_and_ids(self):
        policy = load_policy()
        exceptions = load_exceptions()
        guide_text = GUIDE_PATH.read_text(encoding="utf-8")
        sync_guide_text = SYNC_GUIDE_PATH.read_text(encoding="utf-8")
        template_text = PR_TEMPLATE_PATH.read_text(encoding="utf-8")

        self.assertIn("pr-batch-lane-boundaries.json", guide_text)
        self.assertIn("pr-batch-lane-boundaries.json", sync_guide_text)
        self.assertIn("pr-batch-exceptions.json", guide_text)
        self.assertIn("pr-batch-exceptions.json", sync_guide_text)

        required_template_fields = tuple(
            policy["pr_reference_contract"]["required_pr_template_fields"]
        )
        missing_template_fields = find_missing_snippets(template_text, required_template_fields)
        self.assertEqual(
            missing_template_fields,
            [],
            msg=f"missing PR template lane fields: {missing_template_fields}",
        )

        for lane in policy["lanes"]:
            self.assertIn(lane["id"], guide_text)
            self.assertIn(lane["id"], template_text)

        for hotspot in policy["high_conflict_hotspots"]:
            self.assertIn(hotspot["id"], guide_text)

        required_exception_fields = tuple(policy["exception_workflow"]["required_fields"])
        missing_exception_fields = find_missing_snippets(guide_text, required_exception_fields)
        self.assertEqual(
            missing_exception_fields,
            [],
            msg=f"missing exception workflow fields in guide: {missing_exception_fields}",
        )
        self.assertEqual(exceptions["policy_id"], "pr-batch-exceptions")

    def test_regression_pr_template_references_boundary_map_file(self):
        policy = load_policy()
        template_text = PR_TEMPLATE_PATH.read_text(encoding="utf-8")
        boundary_map = policy["pr_reference_contract"]["boundary_map_reference"]
        self.assertIn(boundary_map, template_text)

    def test_regression_exception_tracking_contract_requires_rationale_fields(self):
        policy = load_policy()
        exceptions = load_exceptions()

        self.assertEqual(exceptions["schema_version"], 1)
        self.assertIn("exceptions", exceptions)
        required_fields = set(policy["exception_workflow"]["required_fields"])
        self.assertIn("rationale", required_fields)

        for entry in exceptions["exceptions"]:
            self.assertTrue(required_fields.issubset(entry.keys()))


if __name__ == "__main__":
    unittest.main()
