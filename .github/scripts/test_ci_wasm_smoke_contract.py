import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
WORKFLOW_PATH = REPO_ROOT / ".github" / "workflows" / "ci.yml"


def extract_section(raw: str, start_marker: str, end_marker: str) -> str:
    start = raw.find(start_marker)
    if start == -1:
        return ""
    end = raw.find(end_marker, start)
    if end == -1:
        return raw[start:]
    return raw[start:end]


class CiWasmSmokeContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.workflow = WORKFLOW_PATH.read_text(encoding="utf-8")
        cls.wasm_scope_section = extract_section(
            cls.workflow,
            "wasm_smoke:",
            "demo_smoke:",
        )

    def test_integration_wasm_scope_lists_wasm_relevant_paths(self):
        self.assertIn("wasm_smoke:", self.wasm_scope_section)
        self.assertIn('- "scripts/dev/wasm-smoke.sh"', self.wasm_scope_section)
        self.assertIn('- "scripts/dev/test-wasm-smoke.sh"', self.wasm_scope_section)
        self.assertIn('- "crates/kamn-core/**"', self.wasm_scope_section)

    def test_regression_wasm_scope_not_broadly_triggered_by_ci_workflow_edits(self):
        self.assertNotIn('.github/workflows/ci.yml', self.wasm_scope_section)

    def test_integration_wasm_job_still_executes_harness(self):
        self.assertIn("name: WASM compile smoke", self.workflow)
        self.assertIn("run: ./scripts/dev/wasm-smoke.sh", self.workflow)
        self.assertIn("name: Determine wasm smoke scope", self.workflow)
        self.assertIn("id: wasm_scope", self.workflow)


if __name__ == "__main__":
    unittest.main()
