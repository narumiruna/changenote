"""Public publication guards and verified metadata with real Git and mocked GitHub."""

import json
import re
import unittest

import test_actions
from test_preview import snapshot


class PublicationMetadata(unittest.TestCase):
    def setUp(self):
        self.f = test_actions.Actions("test_no_batch")
        self.f.setUp()
        self.addCleanup(self.f.doCleanups)
        self.f.env.pop("INPUT_EXPECTED_RELEASE_COMMIT", None)
        self.f.git("config", "gc.auto", "0")
        self.f.git("config", "maintenance.auto", "false")

    def composite(self, action="publish", expected="", ok=True):
        # Execute the actual YAML command and resolve its environment expressions.
        # These public Actions use single-line run commands and env values only.
        path = test_actions.SOURCE / action / "action.yml"
        text = path.read_text()
        commands = re.findall(r"^      run: (.+)$", text, re.MULTILINE)
        self.assertEqual(len(commands), 1)
        self.assertNotIn("${{", commands[0])
        inputs = {
            "github-token": self.f.env["GH_TOKEN"],
            "base-branch": "main",
            "expected-release-commit": expected,
        }
        bindings = re.findall(
            r"^        ([A-Z_]+): \$\{\{ (github\.action_path|inputs\.[\w-]+) \}\}$",
            text, re.MULTILINE,
        )
        if action == "publish":
            self.assertIn(
                ("INPUT_EXPECTED_RELEASE_COMMIT", "inputs.expected-release-commit"),
                bindings,
            )
        for key, expression in bindings:
            self.f.env[key] = (
                str(path.parent) if expression == "github.action_path"
                else inputs[expression.removeprefix("inputs.")]
            )
        return self.invoke(["bash", "-c", commands[0]], ok=ok)

    def invoke(self, command, ok=True):
        output = self.f.top / "publication-metadata-output"
        output.write_text("")
        self.f.env["GITHUB_OUTPUT"] = str(output)
        result = self.f.run_cmd(command, ok=ok)
        values = dict(line.split("=", 1) for line in output.read_text().splitlines())
        return result, values

    def positional(self, expected, env="", ok=True):
        self.f.env["INPUT_EXPECTED_RELEASE_COMMIT"] = env
        return self.invoke([
            "bash", str(test_actions.SOURCE / "scripts/action/publish.sh"), expected,
        ], ok=ok)

    def assert_releases(self, outputs, expected):
        value = outputs["releases"]
        self.assertEqual(json.loads(value), expected)
        self.assertEqual(value, json.dumps(expected, separators=(",", ":")))
        self.assertTrue(all(set(item) == {"package", "version", "tag"} for item in expected))

    def assert_no_publication(self):
        self.assertEqual(self.f.state()["releases"], [])
        self.assertEqual(self.f.git("ls-remote", "--tags", "origin"), "")

    def multi_release(self):
        f = self.f
        (f.repo / "b").mkdir()
        (f.repo / "b/VERSION").write_text("2.0.0\n")
        (f.repo / ".changenote/config.json").write_text(json.dumps({
            "packages": {
                "pkg": {"path": ".", "version_file": "VERSION", "tag_prefix": "sdk/v"},
                "pkg-b": {"path": "b", "version_file": "VERSION", "tag_prefix": "go/sub/v"},
            },
            "fixed": [["pkg", "pkg-b"]],
        }))
        f.commit("custom tags and fixed group")
        f.git("push", "origin", "main")
        return f.release()

    def test_yaml_guard_uses_verified_batch_not_branch_tip(self):
        release = self.f.release()
        self.f.later_commit()
        before = self.f.git("rev-parse", "HEAD")
        _, outputs = self.composite(expected=release)
        self.assertEqual(outputs["release-commit"], release)
        self.assertEqual(outputs["published"], "true")
        self.assert_releases(outputs, [{"package": "pkg", "version": "1.0.1", "tag": "v1.0.1"}])
        self.assertEqual(self.f.git("rev-parse", "HEAD"), before)
        self.assertEqual(self.f.git("status", "--porcelain"), "")

    def test_invalid_yaml_input_is_data_and_never_publishes(self):
        self.f.release()
        for expected in (
            "a" * 39, "a" * 41, "A" * 40, "g" * 40,
            " " + "a" * 40, "a" * 40 + "\n", "$(touch injected)",
        ):
            with self.subTest(expected=expected):
                result, outputs = self.composite(expected=expected, ok=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("lowercase 40-hex", result.stderr)
                self.assert_releases(outputs, [])
                self.assertEqual(outputs["release-commit"], "")
                self.assertEqual(outputs["published-packages"], "[]")
                self.assert_no_publication()
                self.assertFalse((self.f.repo / "injected").exists())

    def test_no_batch_and_guarded_no_batch(self):
        for action in ("preview", "publish"):
            _, outputs = self.composite(action)
            self.assertEqual(outputs["release-commit"], "")
            self.assert_releases(outputs, [])
        result, outputs = self.composite(expected="a" * 40, ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Release batch changed", result.stderr)
        self.assert_releases(outputs, [])
        self.assert_no_publication()

    def test_stale_guard_does_not_publish_either_batch(self):
        previous = self.f.release()
        latest = self.f.release()
        self.assertNotEqual(previous, latest)
        result, outputs = self.composite(expected=previous, ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Release batch changed", result.stderr)
        self.assertEqual(outputs["release-commit"], "")
        self.assert_releases(outputs, [])
        self.assert_no_publication()
        # Omission still selects the latest batch, without a guard.
        _, outputs = self.composite()
        self.assertEqual(outputs["release-commit"], latest)
        self.assert_releases(outputs, [{"package": "pkg", "version": "1.0.2", "tag": "v1.0.2"}])

    def test_legacy_empty_positional_guard_and_conflicting_env(self):
        _, outputs = self.positional("")
        self.assert_releases(outputs, [])
        release = self.f.release()
        for positional, env, diagnostic in (
            ("", "", "Release batch changed"),
            ("a" * 40, "", "Release batch changed"),
            ("", release, "Conflicting positional"),
            ("a" * 40, release, "Conflicting positional"),
            (release, "a" * 40, "Conflicting positional"),
        ):
            with self.subTest(positional=positional, env=env):
                result, outputs = self.positional(positional, env=env, ok=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(diagnostic, result.stderr)
                self.assert_releases(outputs, [])
                self.assert_no_publication()
        _, outputs = self.positional(release)
        self.assertEqual(outputs["published"], "true")
        _, outputs = self.positional(release, env=release)
        self.assertEqual(outputs["published"], "false")
        self.assertEqual(outputs["release-commit"], release)

    def test_partial_publication_and_retry_return_complete_historical_metadata(self):
        release = self.multi_release()
        self.f.git("tag", "sdk/v2.0.1", release)
        self.f.git("push", "origin", "refs/tags/sdk/v2.0.1")
        self.f.set_state(releases=[{"tag_name": "sdk/v2.0.1", "draft": False, "body": "keep"}])
        path = self.f.repo / ".changenote/config.json"
        config = json.loads(path.read_text())
        config["packages"]["pkg"]["tag_prefix"] = "later/v"
        path.write_text(json.dumps(config))
        self.f.commit("later tag configuration")
        self.f.git("push", "origin", "main")
        expected = [
            {"package": "pkg", "version": "2.0.1", "tag": "sdk/v2.0.1"},
            {"package": "pkg-b", "version": "2.0.1", "tag": "go/sub/v2.0.1"},
        ]
        _, preview = self.composite("preview")
        self.assert_releases(preview, expected)
        for attempt in range(2):
            _, outputs = self.composite(expected=release)
            self.assertEqual(outputs["release-commit"], release)
            self.assert_releases(outputs, expected)
            self.assertEqual(outputs["published"], "true" if attempt == 0 else "false")
            self.assertEqual(json.loads(outputs["published-packages"]),
                             [{"name": "pkg-b", "version": "2.0.1"}] if attempt == 0 else [])
        self.assertEqual(self.f.state()["releases"][0]["body"], "keep")

    def test_preview_conflicts_retain_full_plan_and_verified_metadata(self):
        release = self.multi_release()
        self.f.git("tag", "sdk/v2.0.1", release + "^")
        self.f.git("push", "origin", "refs/tags/sdk/v2.0.1")
        tags = self.f.git("ls-remote", "--tags", "origin")
        result, outputs = self.composite("preview", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(outputs["ready"], "false")
        self.assertEqual(outputs["release-commit"], release)
        self.assertEqual([item["action"] for item in json.loads(outputs["publication-plan"])],
                         ["conflict", "create-tag-and-release"])
        self.assertEqual(len(json.loads(outputs["releases"])), 2)
        self.assertEqual(self.f.git("ls-remote", "--tags", "origin"), tags)
        self.assertEqual(self.f.state()["releases"], [])

    def test_invalid_release_tree_never_emits_metadata(self):
        self.f.release()
        (self.f.repo / "VERSION").write_text("9.9.9\n")
        self.f.git("add", "--", "VERSION")
        self.f.git("commit", "--amend", "--no-edit")
        release = self.f.git("rev-parse", "HEAD")
        self.f.git("push", "--force", "origin", "main")
        for action in ("preview", "publish"):
            result, outputs = self.composite(action, expected=release, ok=False)
            self.assertNotEqual(result.returncode, 0)
            self.assert_releases(outputs, [])
            self.assertEqual(outputs["release-commit"], "")
        self.assert_no_publication()

    def test_preview_metadata_does_not_modify_dirty_source(self):
        release = self.f.release()
        (self.f.repo / "VERSION").write_text("dirty\n")
        (self.f.repo / "untracked").write_text("keep\n")
        self.f.git("add", "--", "VERSION")
        before = snapshot(self.f.repo)
        remote = snapshot(self.f.remote)
        _, outputs = self.composite("preview")
        self.assertEqual(outputs["release-commit"], release)
        self.assert_releases(outputs, [{"package": "pkg", "version": "1.0.1", "tag": "v1.0.1"}])
        self.assertEqual(snapshot(self.f.repo), before)
        self.assertEqual(snapshot(self.f.remote), remote)
        self.assertEqual(self.f.state()["releases"], [])


if __name__ == "__main__":
    unittest.main()
