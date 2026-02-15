import json
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
DEMO_INDEX_SCRIPT = REPO_ROOT / "scripts" / "demo" / "index.sh"
DEMO_ALL_SCRIPT = REPO_ROOT / "scripts" / "demo" / "all.sh"
DEMO_SMOKE_MANIFEST = REPO_ROOT / ".github" / "demo-smoke-manifest.json"


def run_command(args: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args,
        cwd=REPO_ROOT,
        text=True,
        capture_output=True,
        check=False,
    )


class DemoIndexTests(unittest.TestCase):
    def test_unit_demo_index_list_json_has_expected_scenarios(self):
        result = run_command(["bash", str(DEMO_INDEX_SCRIPT), "--list", "--json"])
        self.assertEqual(result.returncode, 0, msg=result.stderr)
        payload = json.loads(result.stdout)
        scenario_ids = [entry["id"] for entry in payload["scenarios"]]
        self.assertEqual(
            scenario_ids,
            [
                "onboarding",
                "gateway-auth",
                "gateway-remote-access",
                "multi-channel-live",
                "safety-smoke",
                "deployment-wasm",
            ],
        )
        wrappers = {entry["id"]: entry["wrapper"] for entry in payload["scenarios"]}
        self.assertEqual(wrappers["onboarding"], "local.sh")
        self.assertEqual(wrappers["gateway-auth"], "gateway-auth.sh")
        self.assertEqual(wrappers["gateway-remote-access"], "gateway-remote-access.sh")
        self.assertEqual(wrappers["multi-channel-live"], "multi-channel.sh")
        self.assertEqual(wrappers["safety-smoke"], "safety-smoke.sh")
        self.assertEqual(wrappers["deployment-wasm"], "deployment.sh")

    def test_functional_demo_index_list_text_contains_markers_and_hints(self):
        result = run_command(["bash", str(DEMO_INDEX_SCRIPT), "--list"])
        self.assertEqual(result.returncode, 0, msg=result.stderr)
        self.assertIn("onboarding", result.stdout)
        self.assertIn("gateway-auth", result.stdout)
        self.assertIn("gateway-remote-access", result.stdout)
        self.assertIn("multi-channel-live", result.stdout)
        self.assertIn("safety-smoke", result.stdout)
        self.assertIn("deployment-wasm", result.stdout)
        self.assertIn("expected_marker:", result.stdout)
        self.assertIn("troubleshooting:", result.stdout)

    def test_integration_demo_index_report_file_matches_json_payload(self):
        with tempfile.TemporaryDirectory(prefix="tau-demo-index-test-") as temp_dir:
            report_file = Path(temp_dir) / "report.json"
            manifest_file = Path(temp_dir) / "report.manifest.json"
            result = run_command(
                [
                    "bash",
                    str(DEMO_INDEX_SCRIPT),
                    "--list",
                    "--json",
                    "--report-file",
                    str(report_file),
                ]
            )
            self.assertEqual(result.returncode, 0, msg=result.stderr)
            self.assertTrue(report_file.exists())
            self.assertTrue(manifest_file.exists())
            stdout_payload = json.loads(result.stdout)
            file_payload = json.loads(report_file.read_text(encoding="utf-8"))
            self.assertEqual(stdout_payload, file_payload)
            manifest_payload = json.loads(manifest_file.read_text(encoding="utf-8"))
            self.assertEqual(manifest_payload["schema_version"], 1)
            self.assertEqual(manifest_payload["pack_name"], "demo-index-live-proof-pack")
            self.assertEqual(manifest_payload["producer"]["script"], "scripts/demo/index.sh")
            self.assertEqual(manifest_payload["producer"]["mode"], "list")
            self.assertEqual(manifest_payload["summary"]["status"], "unknown")
            self.assertEqual(manifest_payload["summary"]["total"], None)
            self.assertEqual(manifest_payload["artifacts"][0]["name"], "report")
            self.assertEqual(manifest_payload["artifacts"][0]["path"], str(report_file))
            self.assertEqual(manifest_payload["artifacts"][0]["status"], "present")

            all_list_result = run_command(
                [
                    "bash",
                    str(DEMO_ALL_SCRIPT),
                    "--list",
                    "--only",
                    "gateway-remote-access",
                ]
            )
            self.assertEqual(all_list_result.returncode, 0, msg=all_list_result.stderr)
            self.assertIn("gateway-remote-access.sh", all_list_result.stdout)

    def test_regression_demo_index_alias_filter_and_unknown_filter_handling(self):
        alias_result = run_command(
            [
                "bash",
                str(DEMO_INDEX_SCRIPT),
                "--list",
                "--json",
                "--only",
                "local,gatewayauth,gatewayremoteaccess,multi-channel,safety,deployment",
            ]
        )
        self.assertEqual(alias_result.returncode, 0, msg=alias_result.stderr)
        alias_payload = json.loads(alias_result.stdout)
        alias_ids = [entry["id"] for entry in alias_payload["scenarios"]]
        self.assertEqual(
            alias_ids,
            [
                "onboarding",
                "gateway-auth",
                "gateway-remote-access",
                "multi-channel-live",
                "safety-smoke",
                "deployment-wasm",
            ],
        )

        unknown_result = run_command(
            ["bash", str(DEMO_INDEX_SCRIPT), "--list", "--only", "unknown-scenario"]
        )
        self.assertNotEqual(unknown_result.returncode, 0)
        self.assertIn("unknown scenario names", unknown_result.stderr)

    def test_regression_demo_smoke_manifest_includes_story_889_core_commands(self):
        manifest = json.loads(DEMO_SMOKE_MANIFEST.read_text(encoding="utf-8"))
        command_names = [entry["name"] for entry in manifest["commands"]]
        self.assertIn("onboard-non-interactive", command_names)
        self.assertIn("gateway-remote-profile-token-auth", command_names)
        self.assertIn("gateway-remote-plan-tailscale-serve", command_names)
        self.assertIn("safety-prompt-injection-block", command_names)
        self.assertIn(
            "gateway-remote-plan-fails-closed-missing-funnel-password",
            command_names,
        )
        self.assertIn("deployment-wasm-package", command_names)
        self.assertIn("deployment-channel-store-inspect-edge-wasm", command_names)

    def test_regression_demo_index_manifest_file_requires_report_file(self):
        result = run_command(
            [
                "bash",
                str(DEMO_INDEX_SCRIPT),
                "--list",
                "--manifest-file",
                "/tmp/m21-index-manifest.json",
            ]
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("--manifest-file requires --report-file", result.stderr)


if __name__ == "__main__":
    unittest.main()
