import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPTS_DIR = REPO_ROOT / "scripts" / "demo"


def write_mock_memory_harness(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        """#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path

args = sys.argv[1:]

def value(flag: str, default: str) -> str:
    if flag not in args:
        return default
    return args[args.index(flag) + 1]

output_dir = Path(value("--output-dir", ".tau/demo-memory-live"))
state_dir = Path(value("--state-dir", str(output_dir / "state")))
summary_path = Path(value("--summary-json-out", str(output_dir / "memory-live-summary.json")))
quality_path = Path(
    value("--quality-report-json-out", str(output_dir / "memory-live-quality-report.json"))
)
manifest_path = Path(
    value("--artifact-manifest-json-out", str(output_dir / "memory-live-artifact-manifest.json"))
)
workspace_id = value("--workspace-id", "demo-workspace")

output_dir.mkdir(parents=True, exist_ok=True)
state_dir.mkdir(parents=True, exist_ok=True)
(state_dir / "live-backend").mkdir(parents=True, exist_ok=True)
backend_path = state_dir / "live-backend" / f"{workspace_id}.jsonl"
backend_path.write_text('{"entry":"ok"}\\n', encoding="utf-8")

quality_gate_passed = os.environ.get("TAU_MOCK_MEMORY_LIVE_FAIL_QUALITY") != "1"

summary_path.write_text(
    json.dumps(
        {
            "schema_version": 1,
            "workspace_id": workspace_id,
            "total_cases": 3,
            "persisted_entry_count": 6,
            "top1_hits": 3 if quality_gate_passed else 1,
            "topk_hits": 3,
            "top1_relevance_rate": 1.0 if quality_gate_passed else 0.33,
            "topk_relevance_rate": 1.0,
            "quality_gate_passed": quality_gate_passed,
            "request_captures_path": str(output_dir / "memory-live-request-captures.json"),
        }
    ),
    encoding="utf-8",
)
quality_path.write_text(
    json.dumps(
        {
            "schema_version": 1,
            "workspace_id": workspace_id,
            "thresholds": {
                "top1_relevance_min": 0.66,
                "topk_relevance_min": 1.0,
            },
            "metrics": {
                "total_cases": 3,
                "top1_hits": 3 if quality_gate_passed else 1,
                "topk_hits": 3,
                "top1_relevance_rate": 1.0 if quality_gate_passed else 0.33,
                "topk_relevance_rate": 1.0,
                "quality_gate_passed": quality_gate_passed,
            },
            "cases": [],
        }
    ),
    encoding="utf-8",
)
manifest_path.write_text(
    json.dumps(
        {
            "schema_version": 1,
            "artifacts": [
                {"label": "summary", "path": str(summary_path), "bytes": summary_path.stat().st_size},
                {"label": "quality_report", "path": str(quality_path), "bytes": quality_path.stat().st_size},
                {"label": "backend_state", "path": str(backend_path), "bytes": backend_path.stat().st_size},
            ],
            "missing_artifacts": [],
        }
    ),
    encoding="utf-8",
)

print("mock-memory-live-harness-ok")
""",
        encoding="utf-8",
    )
    path.chmod(0o755)


def run_memory_live_script(
    repo_root: Path, harness_path: Path, env_overrides: dict[str, str] | None = None
) -> subprocess.CompletedProcess[str]:
    env = dict(os.environ)
    if env_overrides:
        env.update(env_overrides)
    return subprocess.run(
        [
            str(SCRIPTS_DIR / "memory-live.sh"),
            "--skip-build",
            "--repo-root",
            str(repo_root),
            "--harness-bin",
            str(harness_path),
            "--timeout-seconds",
            "30",
        ],
        text=True,
        capture_output=True,
        env=env,
        check=False,
    )


class MemoryLiveDemoTests(unittest.TestCase):
    def test_unit_memory_live_rejects_unknown_argument(self) -> None:
        completed = subprocess.run(
            [str(SCRIPTS_DIR / "memory-live.sh"), "--definitely-unknown"],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 2)
        self.assertIn("unknown argument: --definitely-unknown", completed.stderr)

    def test_functional_memory_live_runs_with_mock_harness(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            harness_path = root / "bin" / "memory_live_harness"
            write_mock_memory_harness(harness_path)

            completed = run_memory_live_script(root, harness_path)
            self.assertEqual(completed.returncode, 0, msg=completed.stderr)
            self.assertIn("[demo:memory-live] summary: total=", completed.stdout)
            self.assertIn("failed=0", completed.stdout)

            summary_path = root / ".tau" / "demo-memory-live" / "memory-live-summary.json"
            report_path = root / ".tau" / "demo-memory-live" / "memory-live-report.json"
            self.assertTrue(summary_path.exists())
            self.assertTrue(report_path.exists())

    def test_integration_memory_live_report_contains_quality_metrics(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            harness_path = root / "bin" / "memory_live_harness"
            write_mock_memory_harness(harness_path)

            completed = run_memory_live_script(root, harness_path)
            self.assertEqual(completed.returncode, 0, msg=completed.stderr)

            report_path = root / ".tau" / "demo-memory-live" / "memory-live-report.json"
            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertTrue(report["quality_gate_passed"])
            self.assertEqual(report["total_cases"], 3)
            self.assertGreaterEqual(report["artifact_manifest_entries"], 3)
            self.assertEqual(report["top1_relevance_rate"], 1.0)
            self.assertEqual(report["topk_relevance_rate"], 1.0)

    def test_regression_memory_live_fails_closed_when_quality_gate_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            harness_path = root / "bin" / "memory_live_harness"
            write_mock_memory_harness(harness_path)

            completed = run_memory_live_script(
                root,
                harness_path,
                env_overrides={"TAU_MOCK_MEMORY_LIVE_FAIL_QUALITY": "1"},
            )
            self.assertNotEqual(completed.returncode, 0)
            combined = completed.stdout + "\n" + completed.stderr
            self.assertIn("memory live quality gate failed", combined)


if __name__ == "__main__":
    unittest.main()
