import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

import oversized_file_guard  # noqa: E402


class OversizedFileGuardTests(unittest.TestCase):
    def test_unit_escape_annotation_encodes_special_chars(self):
        raw = "line:1\n100% complete"
        escaped = oversized_file_guard.escape_annotation(raw)
        self.assertEqual(escaped, "line%3A1%0A100%25 complete")

    def test_functional_cli_passes_with_exemption_and_writes_json(self):
        script_path = SCRIPT_DIR / "oversized_file_guard.py"
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            src_dir = root / "crates" / "demo" / "src"
            src_dir.mkdir(parents=True, exist_ok=True)
            (src_dir / "large.rs").write_text(
                "\n".join(["pub fn item() {}" for _ in range(12)]) + "\n",
                encoding="utf-8",
            )
            exemptions = {
                "schema_version": 1,
                "exemptions": [
                    {
                        "path": "crates/demo/src/large.rs",
                        "threshold_lines": 20,
                        "owner_issue": 1754,
                        "rationale": "fixture exemption",
                        "approved_by": "ci-test",
                        "approved_at": "2026-02-15",
                        "expires_on": "2026-03-15",
                    }
                ],
            }
            (root / "tasks" / "policies").mkdir(parents=True, exist_ok=True)
            (root / "tasks" / "policies" / "oversized-file-exemptions.json").write_text(
                json.dumps(exemptions),
                encoding="utf-8",
            )
            report_rel = Path("ci-artifacts/oversized-file-guard.json")
            completed = subprocess.run(
                [
                    sys.executable,
                    str(script_path),
                    "--repo-root",
                    str(root),
                    "--default-threshold",
                    "10",
                    "--json-output-file",
                    str(report_rel),
                ],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, msg=completed.stdout + completed.stderr)
            self.assertIn("issues=0", completed.stdout)
            report = json.loads((root / report_rel).read_text(encoding="utf-8"))
            self.assertEqual(report["schema_version"], 1)
            self.assertEqual(report["issue_count"], 0)

    def test_regression_cli_emits_annotation_with_path_size_threshold_and_hint(self):
        script_path = SCRIPT_DIR / "oversized_file_guard.py"
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            src_dir = root / "crates" / "demo" / "src"
            src_dir.mkdir(parents=True, exist_ok=True)
            (src_dir / "oversized.rs").write_text(
                "\n".join(["pub fn item() {}" for _ in range(11)]) + "\n",
                encoding="utf-8",
            )
            (root / "tasks" / "policies").mkdir(parents=True, exist_ok=True)
            (root / "tasks" / "policies" / "oversized-file-exemptions.json").write_text(
                json.dumps({"schema_version": 1, "exemptions": []}),
                encoding="utf-8",
            )
            completed = subprocess.run(
                [
                    sys.executable,
                    str(script_path),
                    "--repo-root",
                    str(root),
                    "--default-threshold",
                    "10",
                ],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("issues=1", completed.stdout)
            self.assertIn(
                "::error file=crates/demo/src/oversized.rs,line=1,title=Oversized file threshold exceeded::",
                completed.stdout,
            )
            self.assertIn("Split file modules or update auditable exemption metadata", completed.stdout)

    def test_regression_cli_reports_metadata_error_for_invalid_exemption_contract(self):
        script_path = SCRIPT_DIR / "oversized_file_guard.py"
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "tasks" / "policies").mkdir(parents=True, exist_ok=True)
            (root / "tasks" / "policies" / "oversized-file-exemptions.json").write_text(
                json.dumps({"schema_version": 1, "exemptions": [{"path": "x.rs"}]}),
                encoding="utf-8",
            )
            completed = subprocess.run(
                [
                    sys.executable,
                    str(script_path),
                    "--repo-root",
                    str(root),
                ],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("exemption_metadata_error", completed.stdout)
            self.assertIn(
                "::error file=tasks/policies/oversized-file-exemptions.json,line=1,title=Oversized file policy metadata error::",
                completed.stdout,
            )


if __name__ == "__main__":
    unittest.main()
