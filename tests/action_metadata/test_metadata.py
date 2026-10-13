"""Check Action/workflow YAML and execute composite run commands in a consumer fixture."""

from pathlib import Path
import re
import sys
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests/actions"))
import test_actions  # noqa: E402


def read_yaml(path):
    return yaml.load(path.read_text(), Loader=yaml.BaseLoader)


class Metadata(unittest.TestCase):
    def test_inputs_outputs_and_composite_paths(self):
        fixture = test_actions.Actions("test_no_batch")
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        for action, step, expected in (
            (
                ROOT / "action.yml",
                "version",
                {"has-notes", "pr-number", "pr-operation"},
            ),
            (
                ROOT / "publish/action.yml",
                "publish",
                {"release-commit", "published", "published-packages"},
            ),
        ):
            metadata = read_yaml(action)
            self.assertEqual(metadata["runs"]["using"], "composite")
            self.assertEqual(set(metadata["outputs"]), expected)
            self.assertEqual(
                metadata["inputs"]["github-token"]["default"], "${{ github.token }}"
            )
            self.assertEqual(metadata["inputs"]["base-branch"]["default"], "")
            run = metadata["runs"]["steps"][0]
            self.assertEqual(run["id"], step)
            self.assertEqual(run["shell"], "bash")
            self.assertEqual(run["env"]["ACTION_PATH"], "${{ github.action_path }}")
            self.assertEqual(run["env"]["GH_TOKEN"], "${{ inputs.github-token }}")
            for key, value in metadata["outputs"].items():
                self.assertEqual(
                    value["value"], "${{ steps." + step + ".outputs." + key + " }}"
                )
            fixture.env["ACTION_PATH"] = str(action.parent)
            output = fixture.top / "metadata-output"
            output.write_text("")
            fixture.env["GITHUB_OUTPUT"] = str(output)
            fixture.run_cmd(["bash", "-c", run["run"]])
            actual = dict(
                line.split("=", 1) for line in output.read_text().splitlines()
            )
            self.assertEqual(set(actual), expected)
            self.assertFalse((fixture.repo / "scripts/action").exists())

    def test_triggers_concurrency_and_permissions(self):
        pr = read_yaml(ROOT / ".github/workflows/release-pr.yml")
        publish = read_yaml(ROOT / ".github/workflows/release.yml")
        self.assertEqual(pr["concurrency"], publish["concurrency"])
        self.assertEqual(pr["concurrency"]["cancel-in-progress"], "false")
        self.assertEqual(pr["permissions"], {"contents": "read"})
        self.assertEqual(
            pr["jobs"]["release"]["permissions"],
            {"contents": "write", "pull-requests": "write"},
        )
        self.assertEqual(pr["jobs"]["publish"]["permissions"], {"contents": "write"})
        self.assertEqual(pr["jobs"]["publish"]["needs"], "release")
        self.assertIn("always()", pr["jobs"]["publish"]["if"])
        self.assertEqual(publish["permissions"], {"contents": "write"})
        self.assertEqual(set(pr["on"]), {"push", "workflow_dispatch"})
        self.assertEqual(pr["on"]["push"]["branches"], ["main"])
        self.assertEqual(set(publish["on"]), {"workflow_dispatch"})
        for workflow in (pr, publish):
            self.assertEqual(
                workflow["jobs"]["release"]["if"], "github.ref == 'refs/heads/main'"
            )
            checkout = workflow["jobs"]["release"]["steps"][0]
            self.assertEqual(checkout["with"]["persist-credentials"], "false")
        self.assertEqual(pr["jobs"]["release"]["steps"][-1]["uses"], "./")
        self.assertEqual(pr["jobs"]["publish"]["steps"][-1]["uses"], "./publish")
        self.assertEqual(publish["jobs"]["release"]["steps"][-1]["uses"], "./publish")
        ci = read_yaml(ROOT / ".github/workflows/ci.yml")
        self.assertTrue({"actions", "python", "check"}.issubset(ci["jobs"]))

    def test_readme_examples_and_contracts(self):
        readme = (ROOT / "README.md").read_text()
        blocks = re.findall(r"```yaml\n(.*?)```", readme, re.DOTALL)
        workflow = yaml.load(blocks[0], Loader=yaml.BaseLoader)
        self.assertEqual(workflow["permissions"], {"contents": "read"})
        self.assertEqual(
            workflow["jobs"]["release"]["permissions"],
            {"contents": "write", "pull-requests": "write"},
        )
        self.assertEqual(
            workflow["jobs"]["publish"]["permissions"], {"contents": "write"}
        )
        self.assertEqual(
            workflow["jobs"]["release"]["steps"][-1]["uses"],
            "narumiruna/changenote@<full-action-commit-sha>",
        )
        publish = yaml.load(blocks[1], Loader=yaml.BaseLoader)
        self.assertEqual(
            publish[0]["uses"], "narumiruna/changenote/publish@<full-action-commit-sha>"
        )
        for path in (ROOT / "action.yml", ROOT / "publish/action.yml"):
            action = read_yaml(path)
            for key in (*action["inputs"], *action["outputs"]):
                self.assertIn("`" + key + "`", readme)


if __name__ == "__main__":
    unittest.main()
