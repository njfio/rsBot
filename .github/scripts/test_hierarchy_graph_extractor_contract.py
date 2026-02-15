import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
EXTRACTOR_SCRIPT = REPO_ROOT / "scripts" / "dev" / "hierarchy-graph-extractor.sh"
ROADMAP_SYNC_GUIDE = REPO_ROOT / "docs" / "guides" / "roadmap-status-sync.md"


def make_issue(
    number: int,
    title: str,
    state: str = "open",
    labels: list[str] | None = None,
    parent_issue_url: str | None = None,
) -> dict:
    payload = {
        "number": number,
        "title": title,
        "state": state,
        "html_url": f"https://github.com/njfio/Tau/issues/{number}",
        "url": f"https://api.github.com/repos/njfio/Tau/issues/{number}",
        "labels": [{"name": name} for name in (labels or [])],
    }
    if parent_issue_url is not None:
        payload["parent_issue_url"] = parent_issue_url
    return payload


class HierarchyGraphExtractorContractTests(unittest.TestCase):
    def test_unit_extractor_script_exists_and_is_executable(self):
        self.assertTrue(EXTRACTOR_SCRIPT.is_file())
        self.assertTrue(EXTRACTOR_SCRIPT.stat().st_mode & 0o111)

    def test_functional_fixture_run_emits_json_and_markdown_graph(self):
        fixture = [
            make_issue(1678, "M21 Root", labels=["epic", "roadmap"]),
            make_issue(
                1761,
                "Dependency Graph Task",
                labels=["task", "roadmap"],
                parent_issue_url="https://api.github.com/repos/njfio/Tau/issues/1678",
            ),
            make_issue(
                1767,
                "Extractor Subtask",
                labels=["task", "roadmap"],
                parent_issue_url="https://api.github.com/repos/njfio/Tau/issues/1761",
            ),
            make_issue(1999, "Orphan Node", labels=["task", "roadmap"]),
            make_issue(
                2000,
                "Missing Parent",
                labels=["task", "roadmap"],
                parent_issue_url="https://api.github.com/repos/njfio/Tau/issues/2999",
            ),
        ]

        with tempfile.TemporaryDirectory() as temp_dir:
            tmp = Path(temp_dir)
            fixture_path = tmp / "fixture.json"
            output_json = tmp / "graph.json"
            output_md = tmp / "graph.md"
            fixture_path.write_text(json.dumps(fixture), encoding="utf-8")

            completed = subprocess.run(
                [
                    "bash",
                    str(EXTRACTOR_SCRIPT),
                    "--root-issue",
                    "1678",
                    "--repo",
                    "fixture/repository",
                    "--fixture-issues-json",
                    str(fixture_path),
                    "--output-json",
                    str(output_json),
                    "--output-md",
                    str(output_md),
                    "--quiet",
                ],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, msg=completed.stderr)
            self.assertTrue(output_json.is_file())
            self.assertTrue(output_md.is_file())

            payload = json.loads(output_json.read_text(encoding="utf-8"))
            self.assertEqual(payload["root_issue_number"], 1678)
            self.assertEqual(payload["summary"]["in_scope_nodes"], 3)
            self.assertEqual(payload["summary"]["in_scope_edges"], 2)
            self.assertEqual(payload["summary"]["missing_links"], 2)
            self.assertEqual(payload["summary"]["orphan_nodes"], 2)

            markdown = output_md.read_text(encoding="utf-8")
            self.assertIn("Issue Hierarchy Graph", markdown)
            self.assertIn("#1678", markdown)
            self.assertIn("Missing Links", markdown)
            self.assertIn("Orphan Nodes", markdown)

    def test_functional_live_mode_retries_on_transient_gh_failure(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            tmp = Path(temp_dir)
            bin_dir = tmp / "bin"
            bin_dir.mkdir(parents=True, exist_ok=True)
            state_file = tmp / "gh-attempt-count"
            state_file.write_text("0", encoding="utf-8")

            fake_gh = bin_dir / "gh"
            fake_gh.write_text(
                r"""#!/usr/bin/env bash
set -euo pipefail
state_file="${FAKE_GH_STATE_FILE:?}"
count="$(cat "${state_file}")"
count="$((count + 1))"
echo "${count}" >"${state_file}"

if [[ "$1" != "api" ]]; then
  echo "unsupported fake-gh command: $1" >&2
  exit 2
fi

if [[ "${count}" -eq 1 ]]; then
  echo "transient error" >&2
  exit 1
fi

endpoint="$2"
case "${endpoint}" in
  repos/fixture/repository/issues\?state=all\&labels=roadmap\&per_page=100\&page=1)
    cat <<'JSON'
[
  {
    "number": 1761,
    "title": "Dependency Graph Task",
    "state": "open",
    "html_url": "https://github.com/njfio/Tau/issues/1761",
    "url": "https://api.github.com/repos/njfio/Tau/issues/1761",
    "parent_issue_url": "https://api.github.com/repos/njfio/Tau/issues/1678",
    "labels": [{"name":"task"},{"name":"roadmap"}]
  },
  {
    "number": 1767,
    "title": "Extractor Subtask",
    "state": "open",
    "html_url": "https://github.com/njfio/Tau/issues/1767",
    "url": "https://api.github.com/repos/njfio/Tau/issues/1767",
    "parent_issue_url": "https://api.github.com/repos/njfio/Tau/issues/1761",
    "labels": [{"name":"task"},{"name":"roadmap"}]
  }
]
JSON
    ;;
  repos/fixture/repository/issues\?state=all\&labels=roadmap\&per_page=100\&page=2)
    echo '[]'
    ;;
  repos/fixture/repository/issues/1678)
    cat <<'JSON'
{
  "number": 1678,
  "title": "M21 Root",
  "state": "open",
  "html_url": "https://github.com/njfio/Tau/issues/1678",
  "url": "https://api.github.com/repos/njfio/Tau/issues/1678",
  "labels": [{"name":"epic"},{"name":"roadmap"}]
}
JSON
    ;;
  *)
    echo "unexpected endpoint: ${endpoint}" >&2
    exit 3
    ;;
esac
""",
                encoding="utf-8",
            )
            fake_gh.chmod(0o755)

            output_json = tmp / "graph.json"
            output_md = tmp / "graph.md"

            env = os.environ.copy()
            env["PATH"] = f"{bin_dir}:{env.get('PATH', '')}"
            env["FAKE_GH_STATE_FILE"] = str(state_file)
            completed = subprocess.run(
                [
                    "bash",
                    str(EXTRACTOR_SCRIPT),
                    "--root-issue",
                    "1678",
                    "--repo",
                    "fixture/repository",
                    "--output-json",
                    str(output_json),
                    "--output-md",
                    str(output_md),
                    "--max-retries",
                    "2",
                    "--quiet",
                ],
                text=True,
                capture_output=True,
                check=False,
                env=env,
            )
            self.assertEqual(completed.returncode, 0, msg=completed.stderr)
            self.assertTrue(output_json.is_file())
            self.assertTrue(output_md.is_file())

            payload = json.loads(output_json.read_text(encoding="utf-8"))
            self.assertEqual(payload["summary"]["in_scope_nodes"], 3)
            self.assertEqual(payload["summary"]["in_scope_edges"], 2)
            self.assertEqual(payload["source_mode"], "live")
            self.assertGreaterEqual(int(state_file.read_text(encoding="utf-8")), 3)

    def test_integration_roadmap_sync_guide_references_extractor(self):
        guide_text = ROADMAP_SYNC_GUIDE.read_text(encoding="utf-8")
        self.assertIn("hierarchy-graph-extractor.sh", guide_text)

    def test_regression_malformed_fixture_fails_with_deterministic_error(self):
        malformed_fixture = {"issues": {"not": "an-array"}}
        with tempfile.TemporaryDirectory() as temp_dir:
            tmp = Path(temp_dir)
            fixture_path = tmp / "bad-fixture.json"
            fixture_path.write_text(json.dumps(malformed_fixture), encoding="utf-8")

            completed = subprocess.run(
                [
                    "bash",
                    str(EXTRACTOR_SCRIPT),
                    "--root-issue",
                    "1678",
                    "--repo",
                    "fixture/repository",
                    "--fixture-issues-json",
                    str(fixture_path),
                    "--output-json",
                    str(tmp / "graph.json"),
                    "--output-md",
                    str(tmp / "graph.md"),
                ],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("must decode to a JSON array", completed.stderr)


if __name__ == "__main__":
    unittest.main()
