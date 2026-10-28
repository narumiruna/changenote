"""Keep high-level consumer examples free of setup and publication shell scripts."""

from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]


def load(name):
    return yaml.load((ROOT / "examples/github-actions" / name).read_text(), Loader=yaml.BaseLoader)


class ReleaseExamples(unittest.TestCase):
    def test_minimal_workflow_only_needs_checkout_and_action_inputs(self):
        workflow = load("release.yml")
        self.assertEqual(workflow["permissions"], {"contents": "write", "pull-requests": "write"})
        self.assertEqual(set(workflow["on"]), {"push", "workflow_dispatch"})
        self.assertEqual(workflow["concurrency"]["cancel-in-progress"], "false")
        steps = workflow["jobs"]["release"]["steps"]
        self.assertEqual(len(steps), 2)
        self.assertEqual(steps[0]["with"]["persist-credentials"], "false")
        self.assertEqual(steps[1]["uses"], "narumiruna/changenote/release@<full-action-commit-sha>")
        self.assertEqual(steps[1]["with"], {"operation": "${{ inputs.operation || 'auto' }}"})

    def test_logice_keeps_exact_commit_ci_and_artifacts_separate(self):
        workflow = load("logice-release.yml")
        jobs = workflow["jobs"]
        self.assertEqual(workflow["permissions"], {"contents": "read"})
        self.assertNotIn("concurrency", workflow)
        self.assertNotIn("concurrency", jobs["select"])
        self.assertEqual(jobs["validate"]["needs"], "select")
        self.assertEqual(jobs["validate"]["uses"], "./.github/workflows/ci.yml")
        expected = "${{ needs.select.outputs.release-commit }}"
        self.assertEqual(jobs["validate"]["with"]["revision"], expected)
        self.assertEqual(jobs["publish"]["needs"], ["select", "validate"])
        self.assertIn("needs.validate.result == 'success'", jobs["publish"]["if"])
        self.assertEqual(jobs["publish"]["steps"][-1]["with"]["expected-release-commit"], expected)
        self.assertEqual(jobs["publish"]["steps"][0]["with"]["ref"], expected)
        self.assertEqual(jobs["publish"]["permissions"], {"contents": "write"})
        self.assertEqual(jobs["release"]["permissions"], {"contents": "write", "pull-requests": "write"})
        self.assertEqual(jobs["release"]["concurrency"]["cancel-in-progress"], "false")
        self.assertEqual(jobs["publish"]["concurrency"]["cancel-in-progress"], "false")
        self.assertNotEqual(jobs["release"]["concurrency"]["group"], jobs["publish"]["concurrency"]["group"])
        self.assertEqual(jobs["docker"]["uses"], "./.github/workflows/docker.yml")
        self.assertEqual(jobs["docker"]["with"]["revision"], "${{ needs.publish.outputs.release-commit }}")
        self.assertEqual(jobs["docker"]["with"]["version-tag"], "${{ fromJSON(needs.publish.outputs.releases)[0].tag }}")
        self.assertIn("inputs.operation == 'retry-publication'", jobs["select"]["steps"][-1]["with"]["retry"])
        self.assertIn("inputs.operation == 'retry-publication'", jobs["docker"]["if"])
        self.assertNotIn("needs.release", jobs["publish"]["if"])

    def test_all_consumer_setup_is_inside_the_public_action(self):
        for name in ("release.yml", "logice-release.yml"):
            workflow = load(name)
            for job in workflow["jobs"].values():
                for step in job.get("steps", []):
                    self.assertNotIn("run", step)
                    self.assertIn("uses", step)
                    if "with" in step and "persist-credentials" in step["with"]:
                        self.assertEqual(step["with"]["persist-credentials"], "false")
            text = (ROOT / "examples/github-actions" / name).read_text()
            self.assertNotIn("scripts/action/", text)
            self.assertNotIn("setup-changenote", text)
            self.assertNotIn("VERSION", text.replace("CHANGENOTE_NODE_VERSION", "").replace("CHANGENOTE_NPM_VERSION", ""))

    @unittest.skipUnless(shutil.which("actionlint"), "actionlint is not on PATH")
    def test_workflows_lint_with_consumer_workflow_contracts(self):
        for name in ("release.yml", "logice-release.yml"):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                workflows = root / ".github/workflows"
                workflows.mkdir(parents=True)
                shutil.copyfile(ROOT / "examples/github-actions" / name, workflows / "release.yml")
                for called, inputs in (("ci.yml", ["revision"]), ("docker.yml", ["revision", "version-tag"])):
                    stub = {
                        "on": {"workflow_call": {"inputs": {
                            key: {"type": "string", "required": True} for key in inputs
                        }}},
                        "jobs": {"check": {"runs-on": "ubuntu-latest", "steps": [{"run": "true"}]}},
                    }
                    (workflows / called).write_text(yaml.safe_dump(stub))
                subprocess.run(["git", "init", "--quiet", directory], check=True, capture_output=True)
                result = subprocess.run(["actionlint", str(workflows / "release.yml")],
                                        cwd=root, capture_output=True, text=True, timeout=30)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_public_contract_is_documented(self):
        action = yaml.load((ROOT / "release/action.yml").read_text(), Loader=yaml.BaseLoader)
        guide = (ROOT / "docs/github-actions.md").read_text()
        for key in (*action["inputs"], *action["outputs"]):
            self.assertIn("`" + key + "`", guide)
        self.assertIn("full-tree", guide)
        self.assertIn("Hosted cache restoration", guide)
        self.assertIn("docs/github-actions.md", (ROOT / "README.md").read_text())


if __name__ == "__main__":
    unittest.main()
