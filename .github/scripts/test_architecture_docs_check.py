import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

import architecture_docs_check  # noqa: E402


REPO_ROOT = SCRIPT_DIR.parents[1]


class ArchitectureDocsCheckTests(unittest.TestCase):
    def test_unit_contains_fenced_block_detects_expected_languages(self):
        markdown = (
            "# Sample\n"
            "```mermaid\nflowchart TD\nA-->B\n```\n"
            "```bash\necho test\n```\n"
        )
        self.assertTrue(architecture_docs_check.contains_fenced_block(markdown, "mermaid"))
        self.assertTrue(architecture_docs_check.contains_fenced_block(markdown, "bash"))
        self.assertFalse(architecture_docs_check.contains_fenced_block(markdown, "rust"))

    def test_functional_cli_reports_success_for_repository(self):
        script_path = SCRIPT_DIR / "architecture_docs_check.py"
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
        self.assertIn("checked_docs=3", completed.stdout)
        self.assertIn("issues=0", completed.stdout)

    def test_integration_architecture_docs_and_navigation_links_are_current(self):
        issues = architecture_docs_check.collect_architecture_issues(REPO_ROOT)
        self.assertEqual(issues, [])

        docs_index = (REPO_ROOT / "docs" / "README.md").read_text(encoding="utf-8")
        self.assertIn("guides/startup-di-pipeline.md", docs_index)
        self.assertIn("guides/contract-pattern-lifecycle.md", docs_index)
        self.assertIn("guides/multi-channel-event-pipeline.md", docs_index)

        readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
        self.assertIn("docs/guides/startup-di-pipeline.md", readme)
        self.assertIn("docs/guides/contract-pattern-lifecycle.md", readme)
        self.assertIn("docs/guides/multi-channel-event-pipeline.md", readme)

    def test_regression_check_doc_spec_reports_marker_and_symbol_drift(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "docs" / "guides").mkdir(parents=True, exist_ok=True)
            (root / "src").mkdir(parents=True, exist_ok=True)

            (root / "docs" / "guides" / "sample.md").write_text(
                """
# Sample
## Stage
```mermaid
flowchart TD
A-->B
```
```bash
echo ok
```
""",
                encoding="utf-8",
            )
            (root / "src" / "runtime.rs").write_text(
                "fn other_symbol() {}\n",
                encoding="utf-8",
            )

            spec = architecture_docs_check.ArchitectureDocSpec(
                key="sample",
                path="docs/guides/sample.md",
                marker="<!-- architecture-doc:sample -->",
                required_headings=("## Stage",),
                required_symbols=(
                    architecture_docs_check.SymbolExpectation(
                        "`expected_symbol`", "expected_symbol"
                    ),
                ),
                source_files=("src/runtime.rs",),
            )

            issues = architecture_docs_check.check_doc_spec(root, spec)
            categories = {issue.category for issue in issues}
            self.assertIn("missing_marker", categories)
            self.assertIn("missing_symbol_in_doc", categories)
            self.assertIn("stale_symbol_source_missing", categories)


if __name__ == "__main__":
    unittest.main()
