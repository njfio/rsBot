import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
RUNNER_SCRIPT = SCRIPT_DIR / "live_run_unified_runner.py"
WRAPPER_SCRIPT = REPO_ROOT / "scripts" / "demo" / "live-run-unified.sh"
sys.path.insert(0, str(SCRIPT_DIR))

import live_run_unified_runner  # noqa: E402


def write_file(path: Path, content: str, executable: bool = False) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    if executable:
        path.chmod(0o755)


def write_pass_surface_script(path: Path, state_dir: str, summary_tag: str) -> None:
    write_file(
        path,
        f"""#!/usr/bin/env bash
set -euo pipefail
mkdir -p "{state_dir}"
echo "{{\\"surface\\":\\"ok\\"}}" > "{state_dir}/state.json"
echo "[demo:{summary_tag}] summary: total=1 passed=1 failed=0"
""",
        executable=True,
    )


def write_fail_surface_script(path: Path, state_dir: str, summary_tag: str) -> None:
    write_file(
        path,
        f"""#!/usr/bin/env bash
set -euo pipefail
mkdir -p "{state_dir}"
echo "{{\\"surface\\":\\"failed\\"}}" > "{state_dir}/state.json"
echo "[demo:{summary_tag}] summary: total=1 passed=0 failed=1" >&2
exit 7
""",
        executable=True,
    )


class LiveRunUnifiedRunnerTests(unittest.TestCase):
    def test_unit_load_surface_manifest_rejects_duplicate_surface_ids(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            manifest_path = Path(temp_dir) / "manifest.json"
            write_file(
                manifest_path,
                """{
  "schema_version": 1,
  "surfaces": [
    {"id":"voice","script":"scripts/demo/voice.sh","artifact_roots":[".tau/demo-voice"]},
    {"id":"voice","script":"scripts/demo/voice-2.sh","artifact_roots":[".tau/demo-voice-2"]}
  ]
}
""",
            )
            with self.assertRaisesRegex(ValueError, "duplicate id"):
                live_run_unified_runner.load_surface_manifest(manifest_path)

    def test_functional_run_writes_manifest_report_and_surface_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            repo_root = Path(temp_dir)
            binary_path = repo_root / "target" / "debug" / "tau-coding-agent"
            write_file(binary_path, "#!/usr/bin/env bash\nexit 0\n", executable=True)
            voice_script = repo_root / "scripts" / "demo" / "voice.sh"
            browser_script = repo_root / "scripts" / "demo" / "browser-automation-live.sh"
            write_pass_surface_script(voice_script, ".tau/demo-voice", "voice")
            write_pass_surface_script(
                browser_script, ".tau/demo-browser-automation-live", "browser-automation-live"
            )
            manifest_path = repo_root / ".github" / "live-manifest.json"
            write_file(
                manifest_path,
                """{
  "schema_version": 1,
  "surfaces": [
    {"id":"voice","script":"scripts/demo/voice.sh","artifact_roots":[".tau/demo-voice"]},
    {"id":"browser","script":"scripts/demo/browser-automation-live.sh","artifact_roots":[".tau/demo-browser-automation-live"]}
  ]
}
""",
            )
            output_dir = repo_root / ".tau" / "live-run-unified"
            exit_code = live_run_unified_runner.main(
                [
                    "--repo-root",
                    str(repo_root),
                    "--surfaces-manifest",
                    str(manifest_path),
                    "--output-dir",
                    str(output_dir),
                    "--binary",
                    str(binary_path),
                    "--skip-build",
                ]
            )
            self.assertEqual(exit_code, 0)
            manifest_out = json.loads((output_dir / "manifest.json").read_text(encoding="utf-8"))
            report_out = json.loads((output_dir / "report.json").read_text(encoding="utf-8"))
            self.assertEqual(manifest_out["overall"]["status"], "passed")
            self.assertEqual(manifest_out["overall"]["total_surfaces"], 2)
            self.assertEqual(report_out["overall"]["failed_surfaces"], 0)
            self.assertEqual(len(manifest_out["surfaces"]), 2)
            voice_surface = manifest_out["surfaces"][0]
            self.assertEqual(voice_surface["surface_id"], "voice")
            self.assertGreaterEqual(voice_surface["artifact_count"], 1)
            copied_artifact = (
                output_dir
                / "surfaces"
                / "voice"
                / "artifacts"
                / ".tau"
                / "demo-voice"
                / "state.json"
            )
            self.assertTrue(copied_artifact.exists())

    def test_integration_wrapper_lists_default_surfaces_with_json(self) -> None:
        completed = subprocess.run(
            [str(WRAPPER_SCRIPT), "--list", "--json"],
            cwd=REPO_ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, msg=completed.stderr)
        payload = json.loads(completed.stdout)
        surface_ids = [entry["id"] for entry in payload["surfaces"]]
        self.assertEqual(
            surface_ids,
            ["voice", "browser", "dashboard", "custom-command", "memory"],
        )

    def test_regression_failure_surface_sets_nonzero_exit_and_diagnostics(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            repo_root = Path(temp_dir)
            binary_path = repo_root / "target" / "debug" / "tau-coding-agent"
            write_file(binary_path, "#!/usr/bin/env bash\nexit 0\n", executable=True)
            pass_script = repo_root / "scripts" / "demo" / "voice.sh"
            fail_script = repo_root / "scripts" / "demo" / "dashboard.sh"
            write_pass_surface_script(pass_script, ".tau/demo-voice", "voice")
            write_fail_surface_script(fail_script, ".tau/demo-dashboard", "dashboard")
            manifest_path = repo_root / ".github" / "live-manifest.json"
            write_file(
                manifest_path,
                """{
  "schema_version": 1,
  "surfaces": [
    {"id":"voice","script":"scripts/demo/voice.sh","artifact_roots":[".tau/demo-voice"]},
    {"id":"dashboard","script":"scripts/demo/dashboard.sh","artifact_roots":[".tau/demo-dashboard"]}
  ]
}
""",
            )
            output_dir = repo_root / ".tau" / "live-run-unified"
            completed = subprocess.run(
                [
                    sys.executable,
                    str(RUNNER_SCRIPT),
                    "--repo-root",
                    str(repo_root),
                    "--surfaces-manifest",
                    str(manifest_path),
                    "--output-dir",
                    str(output_dir),
                    "--binary",
                    str(binary_path),
                    "--skip-build",
                    "--keep-going",
                ],
                cwd=repo_root,
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 1)
            self.assertIn("[live-run-unified] FAILED dashboard", completed.stdout)
            manifest_out = json.loads((output_dir / "manifest.json").read_text(encoding="utf-8"))
            self.assertEqual(manifest_out["overall"]["status"], "failed")
            self.assertEqual(manifest_out["overall"]["failed_surfaces"], 1)
            dashboard = manifest_out["surfaces"][1]
            self.assertEqual(dashboard["surface_id"], "dashboard")
            self.assertEqual(dashboard["status"], "failed")
            self.assertIn("surface script exited with code 7", dashboard["diagnostics"][0])


if __name__ == "__main__":
    unittest.main()
