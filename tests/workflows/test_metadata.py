"""Check CLI workflow permissions, release selection, and distribution gates."""

import re
import unittest
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
ACTION_REVISION = "4b2d07126b8a4da4b44aa5fb8256dd81a69e2e3a"
ACTION_ROOT = "changenote/action@" + ACTION_REVISION
ACTION_PREVIEW = "changenote/action/preview@" + ACTION_REVISION
ACTION_PUBLISH = "changenote/action/publish@" + ACTION_REVISION


def read_yaml(path):
    return yaml.load(path.read_text(), Loader=yaml.BaseLoader)


class Metadata(unittest.TestCase):
    def test_workflow_action_pins_and_credentials(self):
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            for job in read_yaml(path)["jobs"].values():
                for step in job.get("steps", []):
                    uses = step.get("uses")
                    if not uses:
                        continue
                    with self.subTest(workflow=path.name, action=uses):
                        self.assertRegex(uses, r"^[^@]+@[0-9a-f]{40}$")
                        if uses.startswith("actions/checkout@"):
                            self.assertEqual(step["with"]["persist-credentials"], "false")
                        if uses.startswith("dtolnay/rust-toolchain@"):
                            self.assertEqual(step["with"]["toolchain"], "stable")

    def test_dependabot_cooldowns(self):
        updates = read_yaml(ROOT / ".github/dependabot.yml")["updates"]
        self.assertEqual(
            {update["package-ecosystem"] for update in updates},
            {"github-actions", "cargo", "uv"},
        )
        for update in updates:
            with self.subTest(ecosystem=update["package-ecosystem"]):
                self.assertGreaterEqual(int(update["cooldown"]["default-days"]), 7)

    def test_public_actions_and_local_cli_builds(self):
        jobs = read_yaml(ROOT / ".github/workflows/release.yml")["jobs"]
        self.assertNotIn("preview", jobs)  # Preview must run before writes in the publication job.
        references = []
        for name in ("release", "publish"):
            steps = jobs[name]["steps"]
            self.assertEqual(steps[0]["with"]["ref"], "${{ github.sha }}")
            build = next(step for step in steps if step.get("name") == "Test and build")
            self.assertIn("cargo test --locked", build["run"])
            self.assertIn("cargo build --locked --release", build["run"])
            self.assertIn('echo "$GITHUB_WORKSPACE/target/release" >> "$GITHUB_PATH"', build["run"])
            for step in steps:
                uses = step.get("uses", "")
                if uses not in {ACTION_ROOT, ACTION_PREVIEW, ACTION_PUBLISH}:
                    continue
                references.append(uses)
                self.assertLess(steps.index(build), steps.index(step))
                self.assertEqual(step["with"]["base-branch"], "main")
                expected_inputs = {"base-branch"}
                if uses == ACTION_PUBLISH:
                    expected_inputs.add("expected-release-commit")
                self.assertEqual(set(step["with"]), expected_inputs)
        self.assertEqual(references, [ACTION_ROOT, ACTION_PREVIEW, ACTION_PUBLISH])
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        self.assertNotIn("scripts/action/", workflow)
        self.assertNotIn("uses: $/", workflow)
        self.assertNotIn("changenote/action/release@", workflow)
        route = next(step for step in jobs["route"]["steps"] if step.get("id") == "route")
        self.assertEqual(route["run"], "uv run --no-project python scripts/route-release.py")

    def test_ci_keeps_cli_gates_without_action_tests_or_lint_filters(self):
        jobs = read_yaml(ROOT / ".github/workflows/ci.yml")["jobs"]
        self.assertEqual(set(jobs), {"workflows", "python", "check"})
        runs = "\n".join(step.get("run", "") for step in jobs["workflows"]["steps"])
        for suite in ("workflows", "publication", "python_release"):
            self.assertIn(f"python -m unittest discover -s tests/{suite} -v", runs)
        self.assertIn("--with pyyaml", runs)
        lint = next(step for step in jobs["workflows"]["steps"] if step.get("name") == "Check workflows")
        self.assertIn("actionlint@v1.7.11", lint["run"])
        self.assertIn('"$(go env GOPATH)/bin/actionlint"', lint["run"])
        self.assertNotIn("-ignore", lint["run"])
        ci = (ROOT / ".github/workflows/ci.yml").read_text()
        for removed in ("tests/actions", "tests/action_metadata", "scripts/action/", "shellcheck"):
            self.assertNotIn(removed, ci)
        rust = [step.get("run") for step in jobs["check"]["steps"]]
        for gate in (
            "cargo fmt --check",
            "cargo clippy --locked --all-targets -- -D warnings",
            "cargo test --locked",
            "cargo run --locked -p changenote -- status",
        ):
            self.assertIn(gate, rust)
        distribution = [step.get("run") for step in jobs["python"]["steps"]]
        for gate in (
            "uv sync --locked",
            "uv run --locked python -m unittest discover -s python/tests",
            "uv build --out-dir dist/python-binary",
            "uvx --isolated --no-index --find-links dist/python-binary changenote --help",
        ):
            self.assertIn(gate, distribution)

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
        self.assertEqual(workflow["jobs"]["publish"]["permissions"], {"contents": "write"})
        # PR-management failure must not cancel independent publication.
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
        self.assertEqual(workflow["jobs"]["release"]["steps"][-1]["uses"], ACTION_ROOT)

    def test_publication_checks_route_then_preview_before_writes(self):
        steps = read_yaml(ROOT / ".github/workflows/release.yml")["jobs"]["publish"]["steps"]
        preview, check, publish = steps[-3:]
        self.assertEqual(preview["uses"], ACTION_PREVIEW)
        self.assertEqual(preview["id"], "preview")
        self.assertEqual(check["env"], {
            "EXPECTED_RELEASE_COMMIT": "${{ needs.route.outputs.release-commit }}",
            "PREVIEW_RELEASE_COMMIT": "${{ steps.preview.outputs.release-commit }}",
            "PREVIEW_READY": "${{ steps.preview.outputs.ready }}",
        })
        self.assertEqual(check["run"], "uv run --no-project python scripts/route-release.py check-preview")
        self.assertNotIn("continue-on-error", check)
        self.assertNotIn("if", check)
        self.assertEqual(publish["uses"], ACTION_PUBLISH)
        self.assertEqual(publish["id"], "publish")
        self.assertEqual(publish["with"]["expected-release-commit"], "${{ steps.preview.outputs.release-commit }}")
        # Empty expected commits disable the public guard. Do not call it for no batch.
        self.assertEqual(publish["if"], "steps.preview.outputs.release-commit != ''")
        for commit, selected in (("", False), ("a" * 40, True)):
            condition = publish["if"].replace("steps.preview.outputs.release-commit", repr(commit))
            self.assertEqual(eval(condition, {"__builtins__": {}}), selected)

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
        self.assertEqual(wheels["steps"][0]["with"]["path"], "source")
        self.assertEqual(wheels["steps"][1]["with"]["path"], "workflow")
        self.assertNotIn("ref", wheels["steps"][1]["with"])
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
        build = next(step for step in wheels["steps"] if step.get("name") == "Build wheel")
        self.assertTrue(build["uses"].startswith("PyO3/maturin-action@"))
        self.assertEqual(build["with"]["manylinux"], "2_17")
        self.assertIn("--locked", build["with"]["args"])
        self.assertIn("--compatibility pypi", build["with"]["args"])
        sdist = next(step for step in wheels["steps"] if step.get("name") == "Build source archive")
        self.assertEqual(sdist["if"], "matrix.target == 'x86_64-unknown-linux-gnu'")
        self.assertEqual(sdist["with"]["command"], "sdist")
        prepare_dist = next(step for step in wheels["steps"] if step.get("name") == "Create distribution directory")
        self.assertEqual(prepare_dist["shell"], "bash")
        self.assertEqual(prepare_dist["run"], "mkdir -p source/dist")
        self.assertLess(wheels["steps"].index(prepare_dist), wheels["steps"].index(build))
        self.assertLess(wheels["steps"].index(prepare_dist), wheels["steps"].index(sdist))
        prepare = next(step for step in wheels["steps"] if step.get("name") == "Set release build version")
        self.assertEqual(prepare["id"], "prepare")
        self.assertIn("workflow/scripts/prepare-python-release.py source", prepare["run"])
        self.assertIn('echo "version=$version" >> "$GITHUB_OUTPUT"', prepare["run"])
        check = next(step for step in wheels["steps"] if step.get("name") == "Check installed wheel version")
        self.assertEqual(check["env"]["EXPECTED_VERSION"], "${{ steps.prepare.outputs.version }}")
        self.assertIn('test "$actual" = "changenote $EXPECTED_VERSION"', check["run"])
        self.assertNotIn("< VERSION", check["run"])
        pypi = jobs["pypi"]
        self.assertEqual(pypi["needs"], ["publish", "wheels"])
        self.assertIn("needs.wheels.result == 'success'", pypi["if"])
        self.assertEqual(pypi["permissions"], {"contents": "read", "id-token": "write"})
        self.assertEqual(pypi["environment"]["name"], "pypi")
        self.assertEqual(pypi["steps"][0]["with"]["pattern"], "python-*")
        upload = pypi["steps"][-1]
        self.assertNotIn("env", upload)
        self.assertNotIn("PYPI_TOKEN", (ROOT / ".github/workflows/release.yml").read_text())
        command = upload["run"]
        self.assertIn("uv publish --trusted-publishing always", command)
        self.assertIn("--check-url https://pypi.org/simple/", command)

    def test_macos_wheels_use_supported_native_runners(self):
        wheels = read_yaml(ROOT / ".github/workflows/release.yml")["jobs"]["wheels"]
        runners = {
            entry["target"]: entry["runner"]
            for entry in wheels["strategy"]["matrix"]["include"]
            if entry["target"].endswith("-apple-darwin")
        }
        self.assertEqual(runners, {
            "x86_64-apple-darwin": "macos-15-intel",
            "aarch64-apple-darwin": "macos-15",
        })
        self.assertEqual(wheels["runs-on"], "${{ matrix.runner }}")

    def test_crates_io_uses_validated_commit_and_retries(self):
        jobs = read_yaml(ROOT / ".github/workflows/release.yml")["jobs"]
        crate = jobs["crates-io"]
        self.assertEqual(crate["needs"], "publish")
        self.assertIn("needs.publish.result == 'success'", crate["if"])
        self.assertIn("needs.publish.outputs.release-commit != ''", crate["if"])
        self.assertNotIn("outputs.published", crate["if"])
        self.assertEqual(crate["permissions"], {"contents": "read", "id-token": "write"})
        for name, job in jobs.items():
            if name not in {"crates-io", "pypi"}:
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
            'uv run --no-project python ../workflow/scripts/publish-crate.py . "$RELEASE_VERSION"',
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
        self.assertNotIn("needs.release.result", jobs["publish"]["if"])
        off_main = dict(values, **{"github.ref": "refs/heads/other"})
        self.assertFalse(enabled("release", off_main))
        self.assertFalse(enabled("publish", off_main))


if __name__ == "__main__":
    unittest.main()
