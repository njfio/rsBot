from __future__ import annotations

import importlib.util
import json
import io
from pathlib import Path
import sys
import tempfile
import unittest
from contextlib import redirect_stdout

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = REPO_ROOT / ".github" / "scripts" / "doc_density_annotations.py"

spec = importlib.util.spec_from_file_location("doc_density_annotations", SCRIPT_PATH)
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)


class DocDensityAnnotationsTests(unittest.TestCase):
    def test_unit_parse_failed_crates_extracts_crate_names(self) -> None:
        issues = [
            {
                "kind": "crate_min_failed",
                "detail": "crate 'tau-provider' density 22.0% is below configured minimum 27.0%",
            },
            {"kind": "global_min_failed", "detail": "overall density below threshold"},
            {
                "kind": "crate_min_failed",
                "detail": "crate 'tau-gateway' density 10.0% is below configured minimum 15.0%",
            },
        ]
        crates = module.parse_failed_crates(issues)
        self.assertEqual(crates, {"tau-provider", "tau-gateway"})

    def test_functional_parse_changed_files_from_file_trims_and_dedupes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "changed.txt"
            path.write_text(
                "crates/tau-provider/src/auth.rs\n\ncrates/tau-provider/src/auth.rs\ncrates/tau-gateway/src/lib.rs\n",
                encoding="utf-8",
            )
            changed = module.parse_changed_files_from_file(path)
            self.assertEqual(
                changed,
                [
                    "crates/tau-gateway/src/lib.rs",
                    "crates/tau-provider/src/auth.rs",
                ],
            )

    def test_regression_collect_annotations_emits_file_line_hints(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            target = repo / "crates" / "tau-provider" / "src"
            target.mkdir(parents=True, exist_ok=True)
            rust_file = target / "auth.rs"
            rust_file.write_text(
                "\n".join(
                    [
                        "pub struct MissingDocStruct {",
                        "    pub value: u32,",
                        "}",
                        "",
                        "/// Existing docs",
                        "pub fn documented_fn() {}",
                        "",
                        "pub fn missing_doc_fn() {}",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )

            annotations = module.collect_annotations(
                repo_root=repo,
                failed_crates={"tau-provider"},
                changed_files=["crates/tau-provider/src/auth.rs"],
                max_hints=10,
            )
            self.assertGreaterEqual(len(annotations), 2)
            self.assertEqual(annotations[0].file_path, "crates/tau-provider/src/auth.rs")
            self.assertEqual(annotations[0].line, 1)
            self.assertIn("MissingDocStruct", annotations[0].signature)
            self.assertTrue(any("missing_doc_fn" in item.signature for item in annotations))

    def test_integration_main_writes_json_payload(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            density_path = repo / "density.json"
            changed_path = repo / "changed.txt"
            output_path = repo / "out.json"

            src_dir = repo / "crates" / "tau-provider" / "src"
            src_dir.mkdir(parents=True, exist_ok=True)
            (src_dir / "auth.rs").write_text("pub fn missing_docs() {}\n", encoding="utf-8")

            density_payload = {
                "issues": [
                    {
                        "kind": "crate_min_failed",
                        "detail": "crate 'tau-provider' density 20.0% is below configured minimum 27.0%",
                    }
                ]
            }
            density_path.write_text(json.dumps(density_payload), encoding="utf-8")
            changed_path.write_text("crates/tau-provider/src/auth.rs\n", encoding="utf-8")

            with redirect_stdout(io.StringIO()):
                code = module.main(
                    [
                        "--repo-root",
                        str(repo),
                        "--density-json",
                        str(density_path),
                        "--changed-files-file",
                        str(changed_path),
                        "--json-output-file",
                        str(output_path),
                        "--quiet",
                    ]
                )
            self.assertEqual(code, 0)
            payload = json.loads(output_path.read_text(encoding="utf-8"))
            self.assertEqual(payload["failed_crates"], ["tau-provider"])
            self.assertEqual(payload["changed_files"], ["crates/tau-provider/src/auth.rs"])
            self.assertEqual(len(payload["annotations"]), 1)
            self.assertEqual(payload["annotations"][0]["line"], 1)


if __name__ == "__main__":
    unittest.main()
