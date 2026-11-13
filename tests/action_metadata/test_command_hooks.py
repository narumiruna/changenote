"""Check the root command-hook interface without executing publication."""

from pathlib import Path
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]


class CommandHookMetadata(unittest.TestCase):
    def setUp(self):
        self.action = yaml.load((ROOT / "action.yml").read_text(), Loader=yaml.BaseLoader)

    def test_only_two_optional_commands_extend_the_existing_inputs(self):
        self.assertEqual(set(self.action["inputs"]), {
            "github-token", "base-branch", "commit-message", "pr-title",
            "publish-command", "release-command",
        })
        for key in ("publish-command", "release-command"):
            self.assertEqual(self.action["inputs"][key]["default"], "")
        self.assertEqual(set(self.action["outputs"]), {
            "has-notes", "pr-number", "pr-operation", "release-commit", "releases",
        })
        self.assertNotIn("published", self.action["outputs"])

    def test_command_and_token_inputs_are_environment_data(self):
        step = next(s for s in self.action["runs"]["steps"] if s.get("id") == "version")
        self.assertNotIn("${{", step["run"])
        self.assertNotIn("GH_TOKEN", step["env"])
        for key in ("publish-command", "release-command", "github-token"):
            env = "INPUT_" + key.upper().replace("-", "_")
            self.assertEqual(step["env"][env], "${{ inputs." + key + " }}")
        self.assertIn('"$ACTION_PATH/scripts/action/lifecycle.sh"', step["run"])

    def test_router_tool_setup_is_pinned_and_does_not_build_a_consumer_cli(self):
        steps = self.action["runs"]["steps"]
        self.assertEqual(len(steps), 2)
        self.assertRegex(steps[0]["uses"], r"^astral-sh/setup-uv@[0-9a-f]{40}$")
        self.assertEqual(steps[0]["with"]["enable-cache"], "false")
        text = (ROOT / "action.yml").read_text()
        self.assertNotIn("cargo install", text)
        self.assertNotIn("npm install", text)
        self.assertNotIn("python-version", text)

    def test_hook_example_uses_public_root_and_safe_defaults(self):
        example = yaml.load((ROOT / "examples/github-actions/command-hooks.yml").read_text(),
                            Loader=yaml.BaseLoader)
        self.assertEqual(set(example["on"]), {"push", "workflow_dispatch"})
        job = example["jobs"]["release"]
        self.assertEqual(job["if"], "github.ref == 'refs/heads/main'")
        self.assertEqual(example["concurrency"]["cancel-in-progress"], "false")
        steps = job["steps"]
        self.assertEqual(steps[0]["with"]["persist-credentials"], "false")
        self.assertEqual(steps[-1]["uses"], "narumiruna/changenote@<full-action-commit-sha>")
        self.assertEqual(steps[-1]["with"]["publish-command"], "bash scripts/publish.sh")
        self.assertEqual(steps[-1]["with"]["release-command"], "bash scripts/release.sh")
        self.assertNotIn("scripts/action/", str(steps))
        guide = (ROOT / "docs/github-actions.md").read_text()
        for key in (*self.action["inputs"], *self.action["outputs"]):
            self.assertIn("`" + key + "`", guide)
        self.assertIn("CHANGENOTE_RELEASE_COMMIT", guide)
        self.assertIn("CHANGENOTE_RELEASES", guide)


if __name__ == "__main__":
    unittest.main()
