import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

import runbook_ownership_docs_check  # noqa: E402


REPO_ROOT = SCRIPT_DIR.parents[1]


class RunbookOwnershipDocsCheckTests(unittest.TestCase):
    def test_functional_cli_reports_success_for_repository(self):
        script_path = SCRIPT_DIR / "runbook_ownership_docs_check.py"
        completed = subprocess.run(
            [
                sys.executable,
                str(script_path),
                "--repo-root",
                str(REPO_ROOT),
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, msg=completed.stdout + completed.stderr)
        self.assertIn("checked_docs=8", completed.stdout)
        self.assertIn("issues=0", completed.stdout)

    def test_integration_collect_ownership_issues_returns_empty_for_repository(self):
        issues = runbook_ownership_docs_check.collect_ownership_issues(REPO_ROOT)
        self.assertEqual(issues, [])

    def test_regression_collect_ownership_issues_reports_missing_tokens(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "docs" / "guides").mkdir(parents=True, exist_ok=True)
            (root / "docs" / "README.md").write_text(
                "# Documentation Index\n",
                encoding="utf-8",
            )

            for spec in runbook_ownership_docs_check.OWNERSHIP_SPECS:
                path = root / spec.path
                path.parent.mkdir(parents=True, exist_ok=True)
                # Deliberately omit required ownership content.
                path.write_text("# Stub\n", encoding="utf-8")

            issues = runbook_ownership_docs_check.collect_ownership_issues(root)
            categories = {issue.category for issue in issues}
            self.assertIn("missing_readme_link", categories)
            self.assertIn("missing_token", categories)
            self.assertGreater(len(issues), 3)


if __name__ == "__main__":
    unittest.main()
