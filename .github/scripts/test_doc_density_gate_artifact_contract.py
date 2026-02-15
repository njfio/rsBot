import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
SCRIPT_PATH = REPO_ROOT / "scripts" / "dev" / "doc-density-gate-artifact.sh"
SCORECARD_PATH = REPO_ROOT / "docs" / "guides" / "doc-density-scorecard.md"
DOCS_INDEX_PATH = REPO_ROOT / "docs" / "README.md"


class DocDensityGateArtifactContractTests(unittest.TestCase):
    def test_unit_script_exists_and_is_executable(self):
        self.assertTrue(SCRIPT_PATH.is_file())
        self.assertTrue(SCRIPT_PATH.stat().st_mode & 0o111)

    def test_functional_script_supports_help(self):
        completed = subprocess.run(
            ["bash", str(SCRIPT_PATH), "--help"],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, msg=completed.stderr)
        self.assertIn("doc-density-gate-artifact.sh", completed.stdout)
        self.assertIn("--output-json", completed.stdout)
        self.assertIn("--output-md", completed.stdout)

    def test_integration_scorecard_includes_template_and_troubleshooting(self):
        scorecard = SCORECARD_PATH.read_text(encoding="utf-8")
        self.assertIn("## Gate Reproducibility Artifact (M23)", scorecard)
        self.assertIn("## Artifact Template", scorecard)
        self.assertIn("## Troubleshooting", scorecard)
        self.assertIn("doc-density-gate-artifact.sh", scorecard)

    def test_regression_script_writes_expected_schema_keys(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            tmp = Path(temp_dir)
            output_json = tmp / "artifact.json"
            output_md = tmp / "artifact.md"

            completed = subprocess.run(
                [
                    "bash",
                    str(SCRIPT_PATH),
                    "--repo-root",
                    str(REPO_ROOT),
                    "--targets-file",
                    "docs/guides/doc-density-targets.json",
                    "--output-json",
                    str(output_json),
                    "--output-md",
                    str(output_md),
                    "--generated-at",
                    "2026-02-15T13:00:00Z",
                    "--quiet",
                ],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, msg=completed.stdout + completed.stderr)
            self.assertTrue(output_json.is_file())
            self.assertTrue(output_md.is_file())

            payload = output_json.read_text(encoding="utf-8")
            self.assertIn('"schema_version"', payload)
            self.assertIn('"command"', payload)
            self.assertIn('"versions"', payload)
            self.assertIn('"context"', payload)
            self.assertIn('"density_report"', payload)

    def test_regression_docs_index_references_scorecard(self):
        docs_index = DOCS_INDEX_PATH.read_text(encoding="utf-8")
        self.assertIn("Doc Density Scorecard", docs_index)
        self.assertIn("doc-density-scorecard.md", docs_index)


if __name__ == "__main__":
    unittest.main()
