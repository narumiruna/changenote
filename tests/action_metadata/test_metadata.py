"""Check Action/workflow YAML and execute composite run commands in a consumer fixture."""

import re
import sys
import unittest
from pathlib import Path

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
        workflow = read_yaml(ROOT / ".github/workflows/release.yml")
        self.assertFalse((ROOT / ".github/workflows/release-pr.yml").exists())
        self.assertEqual(workflow["name"], "Release")
        self.assertEqual(workflow["concurrency"]["group"], "changenote-release-main")
        self.assertEqual(workflow["concurrency"]["cancel-in-progress"], "false")
        self.assertEqual(workflow["permissions"], {"contents": "read"})
        self.assertEqual(
            workflow["jobs"]["release"]["permissions"],
            {"contents": "write", "pull-requests": "write"},
        )
        self.assertEqual(
            workflow["jobs"]["publish"]["permissions"], {"contents": "write"}
        )
        self.assertEqual(workflow["jobs"]["publish"]["needs"], "release")
        self.assertIn("always()", workflow["jobs"]["publish"]["if"])
        self.assertEqual(set(workflow["on"]), {"push", "workflow_dispatch"})
        self.assertEqual(workflow["on"]["push"]["branches"], ["main"])
        self.assertEqual(
            workflow["jobs"]["release"]["if"], "github.ref == 'refs/heads/main'"
        )
        for name in ("release", "publish", "wheels"):
            job = workflow["jobs"][name]
            self.assertEqual(job["steps"][0]["with"]["persist-credentials"], "false")
        self.assertEqual(workflow["jobs"]["release"]["steps"][-1]["uses"], "./")
        self.assertEqual(workflow["jobs"]["publish"]["steps"][-1]["uses"], "./publish")
        ci = read_yaml(ROOT / ".github/workflows/ci.yml")
        self.assertTrue({"actions", "python", "check"}.issubset(ci["jobs"]))

    def test_pypi_builds_validated_commit_and_retries(self):
        jobs = read_yaml(ROOT / ".github/workflows/release.yml")["jobs"]
        self.assertEqual(
            jobs["publish"]["outputs"]["release-commit"],
            "${{ steps.publish.outputs.release-commit }}",
        )
        wheels = jobs["wheels"]
        self.assertEqual(wheels["needs"], "publish")
        self.assertIn("needs.publish.result == 'success'", wheels["if"])
        self.assertIn("needs.publish.outputs.release-commit != ''", wheels["if"])
        self.assertNotIn("outputs.published", wheels["if"])
        self.assertEqual(
            wheels["steps"][0]["with"]["ref"],
            "${{ needs.publish.outputs.release-commit }}",
        )
        targets = {entry["target"] for entry in wheels["strategy"]["matrix"]["include"]}
        self.assertEqual(
            targets,
            {
                "x86_64-unknown-linux-gnu",
                "aarch64-unknown-linux-gnu",
                "x86_64-apple-darwin",
                "aarch64-apple-darwin",
                "x86_64-pc-windows-msvc",
            },
        )
        build = next(
            step for step in wheels["steps"] if step.get("name") == "Build wheel"
        )
        self.assertTrue(build["uses"].startswith("PyO3/maturin-action@"))
        self.assertEqual(build["with"]["manylinux"], "2_17")
        self.assertIn("--locked", build["with"]["args"])
        self.assertIn("--compatibility pypi", build["with"]["args"])
        sdist = next(
            step
            for step in wheels["steps"]
            if step.get("name") == "Build source archive"
        )
        self.assertEqual(sdist["if"], "matrix.target == 'x86_64-unknown-linux-gnu'")
        self.assertEqual(sdist["with"]["command"], "sdist")
        pypi = jobs["pypi"]
        self.assertEqual(pypi["needs"], ["publish", "wheels"])
        self.assertIn("needs.wheels.result == 'success'", pypi["if"])
        self.assertEqual(pypi["permissions"], {"id-token": "write"})
        self.assertEqual(pypi["environment"]["name"], "pypi")
        self.assertEqual(pypi["steps"][0]["with"]["pattern"], "python-*")
        command = pypi["steps"][-1]["run"]
        self.assertIn("uv publish --trusted-publishing always", command)
        self.assertIn("--check-url https://pypi.org/simple/", command)

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
        self.assertEqual(
            workflow["jobs"]["publish"]["steps"][-1]["uses"],
            "narumiruna/changenote/publish@<full-action-commit-sha>",
        )
        for path in (ROOT / "action.yml", ROOT / "publish/action.yml"):
            action = read_yaml(path)
            for key in (*action["inputs"], *action["outputs"]):
                self.assertIn("`" + key + "`", readme)


if __name__ == "__main__":
    unittest.main()
