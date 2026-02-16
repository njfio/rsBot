import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
TEMPLATE_DIR = REPO_ROOT / ".github" / "ISSUE_TEMPLATE"

REQUIRED_TEMPLATES = {
    "epic.md": {
        "type_label": "type:epic",
        "must_contain": (
            "## Milestone",
            "## Dependencies",
            "## Risk",
            "## Required Labels",
            "## Definition of Ready",
        ),
    },
    "story.md": {
        "type_label": "type:story",
        "must_contain": (
            "## Parent",
            "## Milestone",
            "## Dependencies",
            "## Risk",
            "## Required Labels",
            "## Definition of Ready",
        ),
    },
    "task.md": {
        "type_label": "type:task",
        "must_contain": (
            "## Parent",
            "## Milestone",
            "## Dependencies",
            "## Risk",
            "## Required Labels",
            "## Definition of Ready",
        ),
    },
    "subtask.md": {
        "type_label": "type:subtask",
        "must_contain": (
            "## Parent",
            "## Milestone",
            "## Dependencies",
            "## Risk",
            "## Required Labels",
            "## Definition of Ready",
        ),
    },
}

NAMESPACE_TOKENS = ("type:", "area:", "process:", "priority:", "status:")


def template_text(name: str) -> str:
    return (TEMPLATE_DIR / name).read_text(encoding="utf-8")


class IssueTemplateContractTests(unittest.TestCase):
    def test_unit_required_issue_template_files_exist(self):
        self.assertTrue(TEMPLATE_DIR.is_dir(), msg=f"missing template directory: {TEMPLATE_DIR}")
        for template_name in REQUIRED_TEMPLATES:
            path = TEMPLATE_DIR / template_name
            self.assertTrue(path.is_file(), msg=f"missing template file: {path}")
            self.assertGreater(path.stat().st_size, 0, msg=f"empty template file: {path}")

    def test_functional_templates_include_required_metadata_sections(self):
        for template_name, contract in REQUIRED_TEMPLATES.items():
            text = template_text(template_name)
            for section in contract["must_contain"]:
                self.assertIn(
                    section,
                    text,
                    msg=f"template {template_name} missing section: {section}",
                )

    def test_integration_templates_encode_required_label_namespaces(self):
        for template_name, contract in REQUIRED_TEMPLATES.items():
            text = template_text(template_name)
            self.assertIn(
                contract["type_label"],
                text,
                msg=f"template {template_name} missing type label: {contract['type_label']}",
            )
            for token in NAMESPACE_TOKENS:
                self.assertIn(
                    token,
                    text,
                    msg=f"template {template_name} missing namespace token: {token}",
                )

    def test_regression_non_epic_templates_require_parent_metadata(self):
        for template_name in ("story.md", "task.md", "subtask.md"):
            text = template_text(template_name)
            self.assertIn("Parent:", text, msg=f"template {template_name} missing parent guidance")
            self.assertIn(
                "exactly one parent",
                text.lower(),
                msg=f"template {template_name} missing single-parent rule",
            )


if __name__ == "__main__":
    unittest.main()
