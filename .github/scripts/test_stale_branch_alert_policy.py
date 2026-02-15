import json
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
POLICY_PATH = REPO_ROOT / "tasks" / "policies" / "stale-branch-alert-policy.json"
GUIDE_PATH = REPO_ROOT / "docs" / "guides" / "stale-branch-response-playbook.md"
SYNC_GUIDE_PATH = REPO_ROOT / "docs" / "guides" / "roadmap-status-sync.md"
PR_TEMPLATE_PATH = REPO_ROOT / ".github" / "pull_request_template.md"


def load_policy() -> dict:
    return json.loads(POLICY_PATH.read_text(encoding="utf-8"))


class StaleBranchAlertPolicyTests(unittest.TestCase):
    def test_functional_policy_has_required_threshold_and_alert_sections(self):
        policy = load_policy()

        self.assertEqual(policy["schema_version"], 1)
        self.assertIn("thresholds", policy)
        self.assertIn("alert_conditions", policy)
        self.assertIn("acknowledge_resolve_workflow", policy)
        self.assertIn("conflict_response", policy)
        self.assertIn("rollback_triggers", policy)
        self.assertIn("pr_reference_fields", policy)

        warning = policy["thresholds"]["warning"]
        critical = policy["thresholds"]["critical"]
        self.assertLess(warning["age_days"], critical["age_days"])
        self.assertLess(warning["behind_commits"], critical["behind_commits"])
        self.assertGreater(policy["thresholds"]["unresolved_conflict_warning_hours"], 0)
        self.assertGreater(len(policy["conflict_response"]["triage_flow"]), 0)
        self.assertIn("merge", policy["conflict_response"]["decision_criteria"])
        self.assertIn("rebase", policy["conflict_response"]["decision_criteria"])
        self.assertIn("abandon", policy["conflict_response"]["decision_criteria"])

    def test_regression_alert_condition_ids_are_unique_and_actionable(self):
        policy = load_policy()
        conditions = policy["alert_conditions"]
        condition_ids = [entry["id"] for entry in conditions]

        self.assertEqual(len(condition_ids), len(set(condition_ids)))
        self.assertGreaterEqual(len(conditions), 3)
        for entry in conditions:
            self.assertIn(entry["severity"], {"warning", "error"})
            self.assertGreater(len(entry["channels"]), 0)

    def test_integration_docs_and_template_reference_policy_contract(self):
        policy = load_policy()
        guide_text = GUIDE_PATH.read_text(encoding="utf-8")
        sync_text = SYNC_GUIDE_PATH.read_text(encoding="utf-8")
        template_text = PR_TEMPLATE_PATH.read_text(encoding="utf-8")

        self.assertIn("stale-branch-alert-policy.json", guide_text)
        self.assertIn("stale-branch-alert-policy.json", sync_text)

        for condition in policy["alert_conditions"]:
            self.assertIn(condition["id"], guide_text)

        for field in policy["acknowledge_resolve_workflow"]["required_ack_fields"]:
            self.assertIn(field, guide_text)

        for field in policy["pr_reference_fields"]:
            self.assertIn(field, guide_text)
            self.assertIn(field, template_text)

        normalized_guide = guide_text.replace("`", "").lower()
        self.assertIn("conflict triage flow", normalized_guide)
        self.assertIn("merge", normalized_guide)
        self.assertIn("rebase", normalized_guide)
        self.assertIn("abandon", normalized_guide)

        for trigger in policy["rollback_triggers"]:
            self.assertIn(trigger["id"], guide_text)

    def test_regression_resolve_states_are_documented_in_playbook(self):
        policy = load_policy()
        guide_text = GUIDE_PATH.read_text(encoding="utf-8")
        for state in policy["acknowledge_resolve_workflow"]["resolve_states"]:
            self.assertIn(state, guide_text)

    def test_regression_rollback_trigger_ids_are_unique_with_actions(self):
        policy = load_policy()
        triggers = policy["rollback_triggers"]
        trigger_ids = [entry["id"] for entry in triggers]

        self.assertEqual(len(trigger_ids), len(set(trigger_ids)))
        self.assertGreaterEqual(len(triggers), 2)
        for entry in triggers:
            self.assertGreater(len(entry["required_actions"]), 0)


if __name__ == "__main__":
    unittest.main()
