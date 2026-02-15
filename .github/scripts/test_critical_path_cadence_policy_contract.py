import json
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
CADENCE_SCRIPT = REPO_ROOT / "scripts" / "dev" / "critical-path-cadence-check.sh"
POLICY_PATH = REPO_ROOT / "tasks" / "policies" / "critical-path-update-cadence-policy.json"
CHECKLIST_PATH = REPO_ROOT / "tasks" / "templates" / "critical-path-cadence-checklist.md"
SYNC_GUIDE_PATH = REPO_ROOT / "docs" / "guides" / "roadmap-status-sync.md"
DOCS_INDEX_PATH = REPO_ROOT / "docs" / "README.md"


def make_comment(created_at: str, body: str) -> dict:
    return {
        "id": 1,
        "created_at": created_at,
        "updated_at": created_at,
        "body": body,
        "user": {"login": "tester"},
    }


class CriticalPathCadencePolicyContractTests(unittest.TestCase):
    def test_unit_policy_checklist_and_script_exist(self):
        self.assertTrue(CADENCE_SCRIPT.is_file())
        self.assertTrue(CADENCE_SCRIPT.stat().st_mode & 0o111)
        self.assertTrue(POLICY_PATH.is_file())
        self.assertTrue(CHECKLIST_PATH.is_file())

        policy = json.loads(POLICY_PATH.read_text(encoding="utf-8"))
        self.assertEqual(policy["schema_version"], 1)
        self.assertEqual(policy["policy_id"], "critical-path-update-cadence-policy")
        self.assertGreater(policy["cadence_hours"], 0)
        self.assertGreaterEqual(policy["grace_period_hours"], 0)

    def test_functional_in_window_update_passes_cadence_check(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            tmp = Path(temp_dir)
            fixture_path = tmp / "comments.json"
            fixture_path.write_text(
                json.dumps(
                    [
                        make_comment(
                            "2026-02-15T15:00:00Z",
                            "## Critical-Path Update\n\nstatus body",
                        )
                    ]
                ),
                encoding="utf-8",
            )

            completed = subprocess.run(
                [
                    "bash",
                    str(CADENCE_SCRIPT),
                    "--fixture-comments-json",
                    str(fixture_path),
                    "--now-utc",
                    "2026-02-15T16:00:00Z",
                    "--json",
                    "--quiet",
                ],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, msg=completed.stderr)
            payload = json.loads(completed.stdout)
            self.assertEqual(payload["status"], "ok")
            self.assertEqual(payload["reason_code"], "within_cadence")

    def test_functional_stale_update_triggers_escalation(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            tmp = Path(temp_dir)
            fixture_path = tmp / "comments.json"
            fixture_path.write_text(
                json.dumps(
                    [
                        make_comment(
                            "2026-02-10T00:00:00Z",
                            "## Critical-Path Update\n\nstale body",
                        )
                    ]
                ),
                encoding="utf-8",
            )

            completed = subprocess.run(
                [
                    "bash",
                    str(CADENCE_SCRIPT),
                    "--fixture-comments-json",
                    str(fixture_path),
                    "--now-utc",
                    "2026-02-15T16:00:00Z",
                    "--json",
                    "--quiet",
                ],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(completed.returncode, 0)
            payload = json.loads(completed.stdout)
            self.assertEqual(payload["status"], "critical")
            self.assertEqual(payload["reason_code"], "stale_update_escalation")

    def test_regression_missing_update_header_fails_closed(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            tmp = Path(temp_dir)
            fixture_path = tmp / "comments.json"
            fixture_path.write_text(
                json.dumps(
                    [
                        make_comment(
                            "2026-02-15T15:00:00Z",
                            "No matching cadence header in this comment",
                        )
                    ]
                ),
                encoding="utf-8",
            )

            completed = subprocess.run(
                [
                    "bash",
                    str(CADENCE_SCRIPT),
                    "--fixture-comments-json",
                    str(fixture_path),
                    "--now-utc",
                    "2026-02-15T16:00:00Z",
                    "--json",
                    "--quiet",
                ],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(completed.returncode, 0)
            payload = json.loads(completed.stdout)
            self.assertEqual(payload["reason_code"], "no_update_found")

    def test_integration_docs_reference_cadence_assets(self):
        sync_guide = SYNC_GUIDE_PATH.read_text(encoding="utf-8")
        docs_index = DOCS_INDEX_PATH.read_text(encoding="utf-8")

        self.assertIn("critical-path-update-cadence-policy.json", sync_guide)
        self.assertIn("critical-path-cadence-checklist.md", sync_guide)
        self.assertIn("critical-path-cadence-check.sh", sync_guide)
        self.assertIn("Critical-Path Cadence Enforcement", docs_index)


if __name__ == "__main__":
    unittest.main()
