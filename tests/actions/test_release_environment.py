"""Prepare current and historical lockfile inputs without changing consumer files."""

import json
import shutil
import unittest

import test_actions

SCRIPT = test_actions.SOURCE / "scripts/action/release-environment.sh"


class ReleaseEnvironment(unittest.TestCase):
    def setUp(self):
        self.fixture = test_actions.Actions("test_no_batch")
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.fixture.env.update(GITHUB_REF="refs/heads/main", INPUT_OPERATION="manage-pr")
        self.calls = self.fixture.top / "package-calls"
        self.fixture.env["PACKAGE_CALLS"] = str(self.calls)
        self.fixture.env["GITHUB_TOKEN"] = "must-not-reach-package-managers"

    def snapshot(self):
        return (self.fixture.git("show-ref"), self.fixture.git("status", "--porcelain"),
                self.fixture.git("write-tree"))

    def run_environment(self, mode="discover", ok=True):
        output = self.fixture.top / "environment-output"
        output.write_text("")
        self.fixture.env["GITHUB_OUTPUT"] = str(output)
        before = self.snapshot()
        result = self.fixture.run_cmd(["bash", str(SCRIPT), mode], ok=ok)
        self.assertEqual(self.snapshot(), before)
        values = dict(line.split("=", 1) for line in output.read_text().splitlines())
        return result, values

    def stub(self, tool, fail=False):
        if tool == "uv":
            real_uv = shutil.which("uv")
            (self.fixture.bin / "gh").write_text(
                '#!/usr/bin/env bash\nexec "' + real_uv + '" run --no-project python "$MOCK_GH_SCRIPT" "$@"\n'
            )
        path = self.fixture.bin / tool
        path.write_text(
            '#!/usr/bin/env bash\nset -eu\n'
            '[ -z "${GH_TOKEN:-}" ] && [ -z "${GITHUB_TOKEN:-}" ] || exit 42\n'
            'printf "%s\\n" "$PWD" "$0" "$*" >> "$PACKAGE_CALLS"\n'
            + ('exit 17\n' if fail else '')
        )
        path.chmod(0o755)

    def save(self):
        self.fixture.commit("package environment")
        self.fixture.git("push", "origin", "main")

    def configure(self, config):
        (self.fixture.repo / ".changenote/config.json").write_text(json.dumps(config))

    def test_plain_version_and_no_pending_notes_need_no_tools(self):
        _, values = self.run_environment()
        self.assertEqual(values, {"cargo": "false", "npm": "false", "uv": "false", "pr-environment-ready": "true"})
        self.fixture.note()
        _, values = self.run_environment()
        self.assertEqual(values, {"cargo": "false", "npm": "false", "uv": "false", "pr-environment-ready": "true"})
        self.run_environment("warm")
        self.assertFalse(self.calls.exists())

    def npm_output(self):
        web = self.fixture.repo / "web"
        web.mkdir()
        version = (self.fixture.repo / "VERSION").read_text().strip()
        (web / "package.json").write_text(json.dumps({"name": "web", "version": version}))
        (web / "package-lock.json").write_text(json.dumps({"name": "web", "version": version, "lockfileVersion": 3}))
        self.configure({"packages": {"pkg": {
            "path": ".", "version_file": "VERSION", "version_outputs": ["web/package.json"]
        }}})
        self.save()
        self.fixture.note()

    def test_additional_manifest_warms_its_lockfile_without_tokens_or_scripts(self):
        self.npm_output()
        self.stub("npm")
        _, values = self.run_environment()
        self.assertEqual(values, {"cargo": "false", "npm": "true", "uv": "false", "pr-environment-ready": "true"})
        self.assertFalse(self.calls.exists())
        self.run_environment("warm")
        calls = self.calls.read_text().splitlines()
        self.assertTrue(calls[0].endswith("/repository/web"), calls)
        self.assertEqual(calls[2], "install --ignore-scripts --no-audit --no-fund")

    def test_shared_workspace_lockfile_is_prepared_once(self):
        repo = self.fixture.repo
        for name in ("a", "b"):
            directory = repo / "packages" / name
            directory.mkdir(parents=True)
            (directory / "package.json").write_text(json.dumps({"name": name, "version": "1.0.0"}))
        (repo / "package.json").write_text('{"name":"workspace","version":"1.0.0"}')
        (repo / "package-lock.json").write_text('{}')
        self.configure({"packages": {
            "pkg": {"path": "packages/a", "version_file": "package.json"},
            "other": {"path": "packages/b", "version_file": "package.json"}
        }})
        self.save()
        self.fixture.cli("add", "shared", "--package", "pkg", "--package", "other", "--bump", "patch", "-m", "Shared change")
        self.save()
        self.stub("npm")
        self.run_environment("warm")
        self.assertEqual(len(self.calls.read_text().splitlines()), 3)
        self.assertTrue(self.calls.read_text().splitlines()[0].endswith("/repository"))

    def test_historical_parent_environment_is_used_after_live_config_changes(self):
        self.fixture.npm_project()
        release = self.fixture.release()
        (self.fixture.repo / "VERSION").write_text("1.0.1\n")
        self.configure({"packages": {"pkg": {"path": ".", "version_file": "VERSION"}}})
        (self.fixture.repo / "package.json").unlink()
        (self.fixture.repo / "package-lock.json").unlink()
        self.save()
        self.stub("npm")
        self.fixture.env.update(INPUT_OPERATION="select")
        _, values = self.run_environment()
        self.assertEqual(values["npm"], "true")
        self.run_environment("warm")
        self.assertIn("install --ignore-scripts", self.calls.read_text())
        self.fixture.env.update(INPUT_OPERATION="publish", INPUT_EXPECTED_RELEASE_COMMIT=release)
        self.run_environment("discover")

    def test_missing_lockfile_does_not_install_a_package_manager(self):
        self.npm_output()
        (self.fixture.repo / "web/package-lock.json").unlink()
        self.save()
        _, values = self.run_environment()
        self.assertEqual(values["npm"], "false")

    def test_cargo_and_uv_fetch_without_project_builds(self):
        for manifest, content, lock, tool, expected in (
            ("Cargo.toml", '[package]\nname="pkg"\nversion="1.0.0"\n', "Cargo.lock", "cargo", "fetch"),
            ("pyproject.toml", '[project]\nname="pkg"\nversion="1.0.0"\n', "uv.lock", "uv", "sync --no-install-project --no-install-workspace --no-build"),
        ):
            with self.subTest(tool=tool):
                (self.fixture.repo / manifest).write_text(content)
                (self.fixture.repo / lock).write_text("fixture\n")
                self.configure({"packages": {"pkg": {"path": ".", "version_file": manifest}}})
                self.save()
                if not (self.fixture.repo / ".changenote/fix.md").exists():
                    self.fixture.note()
                self.stub(tool)
                _, values = self.run_environment()
                self.assertEqual(values[tool], "true")
                self.run_environment("warm")
                self.assertIn(expected, self.calls.read_text())
                self.calls.unlink()
                (self.fixture.bin / tool).unlink()

    def test_unsafe_locks_fail_before_cache_commands(self):
        self.npm_output()
        self.stub("npm")
        lock = self.fixture.repo / "web/package-lock.json"
        outside = self.fixture.top / "outside-lock"
        outside.write_text("unchanged")
        lock.unlink()
        lock.symlink_to(outside)
        self.save()
        result, _ = self.run_environment("warm", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("symlink", result.stderr)
        self.assertEqual(outside.read_text(), "unchanged")
        self.assertFalse(self.calls.exists())

    def test_stale_selection_and_wrong_branch_do_not_warm(self):
        self.fixture.npm_project()
        self.fixture.release()
        self.stub("npm")
        self.fixture.env.update(INPUT_OPERATION="publish", INPUT_EXPECTED_RELEASE_COMMIT="f" * 40)
        result, _ = self.run_environment("warm", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("changed after selection", result.stderr)
        self.assertFalse(self.calls.exists())
        self.fixture.env.update(INPUT_OPERATION="select", GITHUB_REF="refs/heads/other")
        result, _ = self.run_environment("warm", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.calls.exists())

    def test_auto_pr_cache_failure_does_not_block_historical_publication(self):
        release = self.fixture.release()
        self.npm_output()
        self.stub("npm", fail=True)
        self.fixture.env.update(INPUT_OPERATION="auto")
        _, discovery = self.run_environment()
        self.assertEqual(discovery["npm"], "true")
        _, warm = self.run_environment("warm")
        self.assertEqual(warm["pr-environment-ready"], "false")
        event = self.fixture.top / "event.json"
        event.write_text(json.dumps({"forced": False, "deleted": False}))
        self.fixture.env.update(
            INPUT_PR_ENVIRONMENT_READY=warm["pr-environment-ready"],
            GITHUB_EVENT_NAME="push", GITHUB_EVENT_PATH=str(event),
        )
        result, outputs = self.fixture.action("release", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(outputs["published"], "true")
        self.assertEqual(outputs["release-commit"], release)
        self.assertEqual(outputs["pr-operation"], "none")

    def test_cache_failure_leaves_consumer_unchanged(self):
        self.npm_output()
        self.stub("npm", fail=True)
        result, _ = self.run_environment("warm", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("install --ignore-scripts", self.calls.read_text())
        self.assertEqual(self.fixture.state()["releases"], [])


if __name__ == "__main__":
    unittest.main()
