import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

import docs_link_check  # noqa: E402


REPO_ROOT = SCRIPT_DIR.parents[1]


class DocsLinkCheckTests(unittest.TestCase):
    def test_unit_extract_links_handles_relative_external_and_anchor_targets(self):
        markdown = (
            "[README](README.md)\n"
            "[Quickstart](docs/guides/quickstart.md)\n"
            "[External](https://example.com/docs)\n"
            "[Anchor](#quickstart)\n"
        )
        links = docs_link_check.extract_links(markdown)
        self.assertEqual(
            links,
            ["README.md", "docs/guides/quickstart.md", "https://example.com/docs", "#quickstart"],
        )

    def test_functional_cli_reports_success_for_key_docs(self):
        script_path = SCRIPT_DIR / "docs_link_check.py"
        completed = subprocess.run(
            [
                sys.executable,
                str(script_path),
                "--repo-root",
                str(REPO_ROOT),
                "--file",
                "README.md",
                "--file",
                "docs/README.md",
                "--file",
                "docs/guides/quickstart.md",
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, msg=completed.stderr)
        self.assertIn("checked_files=3", completed.stdout)
        self.assertIn("issues=0", completed.stdout)

    def test_integration_docs_index_and_readme_links_stay_valid(self):
        markdown_files = docs_link_check.discover_markdown_files(
            REPO_ROOT,
            ["README.md", "docs/README.md"],
        )
        issues = docs_link_check.check_markdown_links(REPO_ROOT, markdown_files)
        self.assertEqual(issues, [])

        docs_index = (REPO_ROOT / "docs" / "README.md").read_text(encoding="utf-8")
        self.assertIn("guides/quickstart.md", docs_index)
        self.assertIn("guides/transports.md", docs_index)
        self.assertIn("guides/packages.md", docs_index)
        self.assertIn("guides/events.md", docs_index)

    def test_regression_cli_reports_missing_link_and_fails(self):
        script_path = SCRIPT_DIR / "docs_link_check.py"
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "docs").mkdir(parents=True, exist_ok=True)
            (root / "docs" / "broken.md").write_text(
                "[Missing](./does-not-exist.md)\n",
                encoding="utf-8",
            )
            completed = subprocess.run(
                [
                    sys.executable,
                    str(script_path),
                    "--repo-root",
                    str(root),
                    "--file",
                    "docs/broken.md",
                    "--json",
                ],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(completed.returncode, 0)
            payload = json.loads(completed.stdout)
            self.assertEqual(payload["checked_files"], 1)
            self.assertEqual(len(payload["issues"]), 1)
            issue = payload["issues"][0]
            self.assertEqual(issue["source"], "docs/broken.md")
            self.assertEqual(issue["link"], "./does-not-exist.md")
            self.assertEqual(issue["reason"], "missing_target")


if __name__ == "__main__":
    unittest.main()
