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
                {"release-commit", "releases", "published", "published-packages"},
            ),
            (
                ROOT / "preview/action.yml",
                "preview",
                {"release-commit", "releases", "ready", "publication-plan"},
            ),
        ):
            metadata = read_yaml(action)
            self.assertEqual(metadata["runs"]["using"], "composite")
            self.assertEqual(set(metadata["outputs"]), expected)
            self.assertEqual(
                metadata["inputs"]["github-token"]["default"], "${{ github.token }}"
            )
            self.assertEqual(metadata["inputs"]["base-branch"]["default"], "")
            if step == "publish":
                self.assertEqual(metadata["inputs"]["expected-release-commit"]["default"], "")
                self.assertEqual(
                    metadata["runs"]["steps"][0]["env"]["INPUT_EXPECTED_RELEASE_COMMIT"],
                    "${{ inputs.expected-release-commit }}",
                )
                self.assertNotIn("${{", metadata["runs"]["steps"][0]["run"])
                fixture.env["INPUT_EXPECTED_RELEASE_COMMIT"] = ""
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
            if step in ("preview", "publish"):
                self.assertEqual(actual["releases"], "[]")
            if step == "preview":
                self.assertEqual(actual["ready"], "true")
                self.assertEqual(actual["release-commit"], "")
                self.assertEqual(actual["publication-plan"], "[]")
                self.assertEqual(
                    metadata["inputs"]["github-token"]["description"],
                    "Token with contents read permission.",
                )
            self.assertFalse((fixture.repo / "scripts/action").exists())

    def test_triggers_concurrency_and_permissions(self):
        workflow = read_yaml(ROOT / ".github/workflows/release.yml")
        self.assertFalse((ROOT / ".github/workflows/release-pr.yml").exists())
        self.assertEqual(workflow["name"], "Release")
        # Routing must run for every event. PR updates must not replace a queued publication.
        self.assertNotIn("concurrency", workflow)
        self.assertNotIn("concurrency", workflow["jobs"]["route"])
        self.assertEqual(workflow["jobs"]["release"]["concurrency"], {
            "group": "changenote-release-pr-main", "cancel-in-progress": "false",
        })
        self.assertEqual(workflow["jobs"]["publish"]["concurrency"], {
            "group": "changenote-publication-main", "cancel-in-progress": "false",
        })
        self.assertEqual(workflow["permissions"], {"contents": "read"})
        self.assertEqual(
            workflow["jobs"]["release"]["permissions"],
            {"contents": "write", "pull-requests": "write"},
        )
        self.assertEqual(
            workflow["jobs"]["publish"]["permissions"], {"contents": "write"}
        )
        # A replaced PR-management job must not cancel its independent publication.
        self.assertEqual(workflow["jobs"]["publish"]["needs"], "route")
        self.assertIn("!cancelled()", workflow["jobs"]["publish"]["if"])
        self.assertEqual(workflow["jobs"]["route"]["permissions"], {"contents": "read", "pull-requests": "read"})
        route_checkout = workflow["jobs"]["route"]["steps"][0]["with"]
        self.assertEqual(route_checkout["fetch-depth"], "0")
        self.assertEqual(route_checkout["persist-credentials"], "false")
        operation = workflow["on"]["workflow_dispatch"]["inputs"]["operation"]
        self.assertEqual(operation["default"], "manage-pr")
        self.assertEqual(operation["options"], ["manage-pr", "retry-publication"])
        self.assertEqual(set(workflow["on"]), {"push", "workflow_dispatch"})
        self.assertEqual(workflow["on"]["push"]["branches"], ["main"])
        self.assertEqual(
            workflow["jobs"]["release"]["if"], "github.ref == 'refs/heads/main' && needs.route.outputs.manage-pr == 'true'"
        )
        for name in ("release", "publish", "crates-io", "wheels"):
            job = workflow["jobs"][name]
            self.assertEqual(job["steps"][0]["with"]["persist-credentials"], "false")
        self.assertEqual(workflow["jobs"]["release"]["steps"][-1]["uses"], "./")
        publish_steps = workflow["jobs"]["publish"]["steps"]
        self.assertEqual(publish_steps[-1]["run"], 'bash scripts/action/publish.sh "$EXPECTED_RELEASE_COMMIT"')
        self.assertEqual(publish_steps[-1]["env"]["EXPECTED_RELEASE_COMMIT"], "${{ steps.preview.outputs.release-commit }}")
        self.assertEqual(publish_steps[-3]["uses"], "./preview")
        self.assertEqual(publish_steps[-2]["env"]["EXPECTED_RELEASE_COMMIT"], "${{ needs.route.outputs.release-commit }}")
        self.assertIn("check-preview", publish_steps[-2]["run"])
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
        prepare_dist = next(
            step
            for step in wheels["steps"]
            if step.get("name") == "Create distribution directory"
        )
        self.assertEqual(prepare_dist["shell"], "bash")
        self.assertEqual(prepare_dist["run"], "mkdir -p source/dist")
        self.assertLess(
            wheels["steps"].index(prepare_dist), wheels["steps"].index(build)
        )
        self.assertLess(
            wheels["steps"].index(prepare_dist), wheels["steps"].index(sdist)
        )
        prepare = next(
            step for step in wheels["steps"] if step.get("name") == "Set release build version"
        )
        self.assertEqual(prepare["id"], "prepare")
        self.assertIn("prepare-python-release.py source", prepare["run"])
        self.assertIn('echo "version=$version" >> "$GITHUB_OUTPUT"', prepare["run"])
        check = next(
            step for step in wheels["steps"] if step.get("name") == "Check installed wheel version"
        )
        self.assertEqual(check["env"]["EXPECTED_VERSION"], "${{ steps.prepare.outputs.version }}")
        self.assertIn('test "$actual" = "changenote $EXPECTED_VERSION"', check["run"])
        self.assertNotIn("< VERSION", check["run"])
        pypi = jobs["pypi"]
        self.assertEqual(pypi["needs"], ["publish", "wheels"])
        self.assertIn("needs.wheels.result == 'success'", pypi["if"])
        self.assertEqual(pypi["permissions"], {"contents": "read"})
        self.assertEqual(pypi["environment"]["name"], "pypi")
        self.assertEqual(pypi["steps"][0]["with"]["pattern"], "python-*")
        upload = pypi["steps"][-1]
        self.assertEqual(
            upload["env"], {"UV_PUBLISH_TOKEN": "${{ secrets.PYPI_TOKEN }}"}
        )
        command = upload["run"]
        self.assertIn("uv publish --trusted-publishing never", command)
        self.assertIn("--check-url https://pypi.org/simple/", command)

    def test_crates_io_uses_validated_commit_and_retries(self):
        jobs = read_yaml(ROOT / ".github/workflows/release.yml")["jobs"]
        crate = jobs["crates-io"]
        self.assertEqual(crate["needs"], "publish")
        self.assertIn("needs.publish.result == 'success'", crate["if"])
        self.assertIn("needs.publish.outputs.release-commit != ''", crate["if"])
        self.assertNotIn("outputs.published", crate["if"])
        self.assertEqual(crate["permissions"], {"contents": "read", "id-token": "write"})
        for name, job in jobs.items():
            if name != "crates-io":
                self.assertNotIn("id-token", job.get("permissions", {}))
        self.assertEqual(crate["concurrency"], {
            "group": "changenote-crates-io-main", "cancel-in-progress": "false",
        })
        self.assertEqual(crate["environment"], {
            "name": "crates-io", "url": "https://crates.io/crates/changenote",
        })
        source, workflow = crate["steps"][:2]
        self.assertEqual(source["with"]["ref"], "${{ needs.publish.outputs.release-commit }}")
        self.assertEqual(source["with"]["path"], "source")
        self.assertEqual(workflow["with"]["path"], "workflow")
        self.assertNotIn("ref", workflow["with"])
        self.assertEqual(workflow["with"]["persist-credentials"], "false")
        prepare = next(step for step in crate["steps"] if step.get("id") == "prepare")
        self.assertIn("workflow/scripts/prepare-python-release.py source", prepare["run"])
        self.assertIn('echo "version=$version" >> "$GITHUB_OUTPUT"', prepare["run"])
        auth = next(step for step in crate["steps"] if step.get("id") == "auth")
        self.assertRegex(auth["uses"], r"^rust-lang/crates-io-auth-action@[0-9a-f]{40}$")
        self.assertNotIn("continue-on-error", auth)
        self.assertLess(crate["steps"].index(prepare), crate["steps"].index(auth))
        upload = crate["steps"][-1]
        self.assertLess(crate["steps"].index(auth), len(crate["steps"]) - 1)
        self.assertNotIn("if", upload)
        self.assertEqual(upload["working-directory"], "source")
        self.assertEqual(upload["env"], {
            "CARGO_REGISTRY_TOKEN": "${{ steps.auth.outputs.token }}",
            "RELEASE_VERSION": "${{ steps.prepare.outputs.version }}",
        })
        self.assertEqual(
            upload["run"],
            'uv run --no-project python ../workflow/scripts/action/publish-crate.py . "$RELEASE_VERSION"',
        )

    def test_job_conditions_execute_routing_and_failure_cases(self):
        jobs = read_yaml(ROOT / ".github/workflows/release.yml")["jobs"]

        def enabled(job, values, cancelled=False):
            condition = jobs[job]["if"].removeprefix("${{").removesuffix("}}").strip()
            condition = condition.replace("!cancelled()", str(not cancelled))
            condition = condition.replace("&&", " and ").replace("||", " or ")
            condition = re.sub(r"(?:github|needs)\.[\w.-]+", lambda match: repr(values[match[0]]), condition)
            return eval(condition, {"__builtins__": {}})

        values = {
            "github.ref": "refs/heads/main",
            "needs.route.result": "success",
            "needs.route.outputs.manage-pr": "true",
            "needs.route.outputs.publish": "false",
            "needs.publish.result": "skipped",
            "needs.publish.outputs.release-commit": "",
            "needs.wheels.result": "skipped",
        }
        self.assertTrue(enabled("release", values))
        for job in ("publish", "crates-io", "wheels", "pypi"):
            self.assertFalse(enabled(job, values))
        # Explicit retry skips PR management. Publication need not create new tags.
        values.update({"needs.route.outputs.manage-pr": "false", "needs.route.outputs.publish": "true"})
        self.assertFalse(enabled("release", values))
        self.assertTrue(enabled("publish", values))
        values.update({"needs.publish.result": "success", "needs.publish.outputs.release-commit": "validated", "needs.wheels.result": "success"})
        for job in ("publish", "crates-io", "wheels", "pypi"):
            self.assertTrue(enabled(job, values))
            self.assertFalse(enabled(job, values, cancelled=True))
        for result in ("failure", "cancelled", "skipped"):
            self.assertFalse(enabled("publish", dict(values, **{"needs.route.result": result})))
            self.assertFalse(enabled("crates-io", dict(values, **{"needs.publish.result": result})))
            self.assertFalse(enabled("wheels", dict(values, **{"needs.publish.result": result})))
            self.assertFalse(enabled("pypi", dict(values, **{"needs.wheels.result": result})))
        self.assertFalse(enabled("crates-io", dict(values, **{"needs.publish.outputs.release-commit": ""})))
        self.assertFalse(enabled("wheels", dict(values, **{"needs.publish.outputs.release-commit": ""})))
        # A PyPI build failure must not prevent independent crates.io recovery.
        self.assertTrue(enabled("crates-io", dict(values, **{"needs.wheels.result": "failure"})))
        # Publish has no release-result predicate: PR failure must not block recovery.
        self.assertNotIn("needs.release.result", jobs["publish"]["if"])

    def test_readme_examples_and_contracts(self):
        readme = (ROOT / "README.md").read_text()
        blocks = re.findall(r"```yaml\n(.*?)```", readme, re.DOTALL)
        examples = [yaml.load(block, Loader=yaml.BaseLoader) for block in blocks]
        workflow = next(example for example in examples if "jobs" in example)
        check = next(example for example in examples if "steps" in example)
        self.assertEqual(check["permissions"], {"contents": "read"})
        self.assertEqual(check["steps"][0]["with"]["fetch-depth"], "0")
        self.assertEqual(check["steps"][0]["with"]["persist-credentials"], "false")
        self.assertEqual(
            check["steps"][-1]["env"]["BASE_BRANCH"], "${{ github.base_ref }}"
        )
        self.assertEqual(
            check["steps"][-1]["run"],
            'changenote check --since "refs/remotes/origin/$BASE_BRANCH"',
        )
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
        preview = next(
            example
            for example in examples
            if example.get("name") == "Publication preview"
        )
        self.assertEqual(preview["permissions"], {"contents": "read"})
        self.assertEqual(
            preview["jobs"]["preview"]["steps"][-1]["uses"],
            "narumiruna/changenote/preview@<full-action-commit-sha>",
        )
        self.assertNotIn(
            "preview", read_yaml(ROOT / ".github/workflows/release.yml")["jobs"]
        )  # Preview is a pre-write step, not a separate production job.
        for path in (
            ROOT / "action.yml",
            ROOT / "publish/action.yml",
            ROOT / "preview/action.yml",
        ):
            action = read_yaml(path)
            for key in (*action["inputs"], *action["outputs"]):
                self.assertIn("`" + key + "`", readme)


if __name__ == "__main__":
    unittest.main()
