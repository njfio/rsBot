import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

import rust_doc_density  # noqa: E402


REPO_ROOT = SCRIPT_DIR.parents[1]


class RustDocDensityTests(unittest.TestCase):
    def test_unit_extract_public_items_reports_documented_and_missing(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            file_path = Path(temp_dir) / "lib.rs"
            file_path.write_text(
                """
/// documented struct
pub struct Documented;

pub enum Undocumented {
    A,
}
""",
                encoding="utf-8",
            )
            items = rust_doc_density.extract_public_items(file_path)
            self.assertEqual(len(items), 2)
            self.assertTrue(items[0].documented)
            self.assertFalse(items[1].documented)

    def test_functional_cli_reports_success_for_repository_targets(self):
        script_path = SCRIPT_DIR / "rust_doc_density.py"
        completed = subprocess.run(
            [
                sys.executable,
                str(script_path),
                "--repo-root",
                str(REPO_ROOT),
                "--targets-file",
                "docs/guides/doc-density-targets.json",
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, msg=completed.stdout + completed.stderr)
        self.assertIn("rust doc density check", completed.stdout)
        self.assertIn("issues=0", completed.stdout)

    def test_functional_cli_json_includes_schema_version(self):
        script_path = SCRIPT_DIR / "rust_doc_density.py"
        completed = subprocess.run(
            [
                sys.executable,
                str(script_path),
                "--repo-root",
                str(REPO_ROOT),
                "--targets-file",
                "docs/guides/doc-density-targets.json",
                "--json",
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, msg=completed.stdout + completed.stderr)
        payload = json.loads(completed.stdout)
        self.assertEqual(payload["schema_version"], 1)
        self.assertIn("reports", payload)
        self.assertIsInstance(payload["reports"], list)

    def test_integration_density_reports_include_anchor_crates(self):
        reports, items = rust_doc_density.compute_density_reports(REPO_ROOT, "crates")
        by_crate = {report.crate: report for report in reports}

        self.assertIn("tau-core", by_crate)
        self.assertIn("tau-startup", by_crate)
        self.assertGreater(by_crate["tau-core"].total_public_items, 0)
        self.assertGreater(len(items), 0)

    def test_regression_cli_fails_when_crate_target_not_met(self):
        script_path = SCRIPT_DIR / "rust_doc_density.py"
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            src_dir = root / "crates" / "demo-crate" / "src"
            src_dir.mkdir(parents=True, exist_ok=True)
            (root / "crates" / "demo-crate" / "Cargo.toml").write_text(
                "[package]\nname = \"demo-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                encoding="utf-8",
            )
            (src_dir / "lib.rs").write_text("pub fn undoc() {}\n", encoding="utf-8")
            targets_path = root / "targets.json"
            targets_path.write_text(
                json.dumps(
                    {
                        "crate_min_percent": {
                            "demo-crate": 100.0,
                        }
                    }
                ),
                encoding="utf-8",
            )

            completed = subprocess.run(
                [
                    sys.executable,
                    str(script_path),
                    "--repo-root",
                    str(root),
                    "--targets-file",
                    str(targets_path.relative_to(root)),
                ],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("crate_min_failed", completed.stdout)

    def test_regression_json_output_file_writes_per_crate_artifact(self):
        script_path = SCRIPT_DIR / "rust_doc_density.py"
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            src_dir = root / "crates" / "demo-crate" / "src"
            src_dir.mkdir(parents=True, exist_ok=True)
            (root / "crates" / "demo-crate" / "Cargo.toml").write_text(
                "[package]\nname = \"demo-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                encoding="utf-8",
            )
            (src_dir / "lib.rs").write_text("/// documented\npub fn documented() {}\n", encoding="utf-8")

            artifact_rel = Path("ci-artifacts/rust-doc-density.json")
            artifact_path = root / artifact_rel
            completed = subprocess.run(
                [
                    sys.executable,
                    str(script_path),
                    "--repo-root",
                    str(root),
                    "--json-output-file",
                    str(artifact_rel),
                ],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, msg=completed.stdout + completed.stderr)
            self.assertTrue(artifact_path.is_file())

            payload = json.loads(artifact_path.read_text(encoding="utf-8"))
            self.assertEqual(payload["schema_version"], 1)
            self.assertEqual(payload["crate_count"], 1)
            self.assertEqual(payload["reports"][0]["crate"], "demo-crate")


if __name__ == "__main__":
    unittest.main()
