import subprocess
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
GITIGNORE_PATH = REPO_ROOT / ".gitignore"
DEMO_SMOKE_SHELL = REPO_ROOT / "scripts" / "demo-smoke.sh"
DEMO_SMOKE_RUNNER = REPO_ROOT / ".github" / "scripts" / "demo_smoke_runner.py"
CLEANUP_SCRIPT = REPO_ROOT / "scripts" / "dev" / "clean-local-artifacts.sh"


def run_cleanup() -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(CLEANUP_SCRIPT)],
        cwd=REPO_ROOT,
        text=True,
        capture_output=True,
        check=False,
    )


class RepoHygieneTests(unittest.TestCase):
    def setUp(self) -> None:
        run_cleanup()

    def tearDown(self) -> None:
        run_cleanup()

    def test_unit_gitignore_covers_generated_artifacts(self):
        gitignore = GITIGNORE_PATH.read_text(encoding="utf-8")
        self.assertIn("ci-artifacts/", gitignore)
        self.assertIn(".github/scripts/__pycache__/", gitignore)
        self.assertIn("*.pyc", gitignore)
        self.assertIn("/=", gitignore)
        self.assertIn("/]", gitignore)

    def test_functional_generated_paths_are_ignored_by_git(self):
        generated_files = [
            REPO_ROOT / "ci-artifacts" / "demo-smoke" / "example.log",
            REPO_ROOT / ".github" / "scripts" / "__pycache__" / "test.cpython-311.pyc",
            REPO_ROOT / "=",
            REPO_ROOT / "]",
        ]
        for generated in generated_files:
            generated.parent.mkdir(parents=True, exist_ok=True)
            generated.write_text("generated\n", encoding="utf-8")
            relative = str(generated.relative_to(REPO_ROOT))
            ignored = subprocess.run(
                ["git", "check-ignore", "-q", relative],
                cwd=REPO_ROOT,
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(
                ignored.returncode,
                0,
                msg=f"expected ignored path: {relative}",
            )

    def test_integration_demo_smoke_contract_uses_shared_artifact_path(self):
        shell_contents = DEMO_SMOKE_SHELL.read_text(encoding="utf-8")
        runner_contents = DEMO_SMOKE_RUNNER.read_text(encoding="utf-8")
        self.assertIn("ci-artifacts/demo-smoke", shell_contents)
        self.assertIn("ci-artifacts/demo-smoke", runner_contents)

    def test_regression_cleanup_script_removes_noise_outputs(self):
        (REPO_ROOT / "ci-artifacts" / "demo-smoke").mkdir(parents=True, exist_ok=True)
        (REPO_ROOT / "ci-artifacts" / "demo-smoke" / "leftover.log").write_text(
            "leftover\n",
            encoding="utf-8",
        )
        (REPO_ROOT / ".github" / "scripts" / "__pycache__").mkdir(parents=True, exist_ok=True)
        (REPO_ROOT / ".github" / "scripts" / "__pycache__" / "leftover.pyc").write_text(
            "leftover\n",
            encoding="utf-8",
        )
        (REPO_ROOT / "=").write_text("leftover\n", encoding="utf-8")
        (REPO_ROOT / "]").write_text("leftover\n", encoding="utf-8")

        result = run_cleanup()
        self.assertEqual(result.returncode, 0, msg=result.stderr)
        self.assertFalse((REPO_ROOT / "ci-artifacts").exists())
        self.assertFalse((REPO_ROOT / ".github" / "scripts" / "__pycache__").exists())
        self.assertFalse((REPO_ROOT / "=").exists())
        self.assertFalse((REPO_ROOT / "]").exists())
        self.assertIn("cleanup complete", result.stdout)


if __name__ == "__main__":
    unittest.main()
