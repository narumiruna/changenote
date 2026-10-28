"""Real package-manager checks in temporary consumer repositories, without dependencies."""

import json
import shutil
import sys
import tomllib
import unittest

import test_actions
from test_preview import snapshot


class ReleaseIsolation(unittest.TestCase):
    def setUp(self):
        self.f = test_actions.Actions("test_no_batch")
        self.f.setUp()
        self.addCleanup(self.f.doCleanups)
        f = self.f
        for key in tuple(f.env):
            if key.startswith("INPUT_"):
                f.env.pop(key)
        for key in ("RUSTUP_TOOLCHAIN", "UV_PROJECT_ENVIRONMENT", "VIRTUAL_ENV"):
            f.env.pop(key, None)
        self.temp = f.top / "action-temp"
        self.temp.mkdir()
        f.env.update(
            GITHUB_REF="refs/heads/main",
            GITHUB_EVENT_NAME="workflow_dispatch",
            TMPDIR=str(self.temp),
            CARGO_HOME=str(f.top / "cargo-home"),
            CARGO_NET_OFFLINE="true",
            UV_CACHE_DIR=str(f.top / "uv-cache"),
            UV_OFFLINE="true",
            UV_PYTHON=sys.executable,
            UV_PYTHON_DOWNLOADS="never",
            NODE_COMPILE_CACHE=str(f.top / "node-cache"),
            npm_config_cache=str(f.top / "npm-cache"),
            npm_config_offline="true",
            npm_config_ignore_scripts="false",
            npm_config_update_notifier="false",
        )
        for repo in (f.repo, f.remote):
            f.run_cmd(["git", "-C", str(repo), "config", "gc.auto", "0"])
            f.run_cmd(["git", "-C", str(repo), "config", "maintenance.auto", "false"])

    def require_tool(self, tool):
        path = shutil.which(tool, path=self.f.env["PATH"])
        if path is None:
            self.skipTest(f"{tool} is not installed")
        return path

    def configure(self, manifest):
        (self.f.repo / ".changenote/config.json").write_text(json.dumps({
            "packages": {"pkg": {"path": ".", "version_file": manifest}}
        }))

    def save(self, message):
        commit = self.f.commit(message)
        self.f.git("push", "origin", "main")
        return commit

    def readonly(self, command):
        f = self.f
        source, remote, state = snapshot(f.repo), snapshot(f.remote), f.state()
        result = command()
        self.assertEqual(snapshot(f.repo), source)
        self.assertEqual(snapshot(f.remote), remote)
        self.assertEqual(f.state()["pulls"], state["pulls"])
        self.assertEqual(f.state()["releases"], state["releases"])
        calls = f.state().get("calls", [])[len(state.get("calls", [])):]
        self.assertTrue(all(call[0] == "api" and "--method" not in call for call in calls))
        # uv can retain interpreter lock files in TMPDIR, outside the Action tree.
        leftovers = [p for p in self.temp.iterdir()
                     if not (p.is_file() and p.name.startswith("uv-") and p.suffix == ".lock")]
        self.assertEqual(leftovers, [])
        return result

    def environment(self, mode, operation):
        f = self.f
        output = f.top / "environment-output"
        output.write_text("")
        f.env.update(GITHUB_OUTPUT=str(output), INPUT_OPERATION=operation)
        result = self.readonly(lambda: f.run_cmd([
            "bash", str(test_actions.SOURCE / "scripts/action/release-environment.sh"), mode
        ]))
        values = dict(line.split("=", 1) for line in output.read_text().splitlines())
        self.assertEqual(values["pr-environment-ready"], "true")
        return result, values

    def preview(self, commit, version):
        self.f.env["INPUT_OPERATION"] = "select"
        _, out = self.readonly(lambda: self.f.action("release"))
        self.assertEqual(out["ready"], "true")
        self.assertEqual(out["release-commit"], commit)
        self.assertEqual(json.loads(out["publication-plan"]), [{
            "name": "pkg", "version": version, "tag": f"v{version}",
            "action": "create-tag-and-release", "conflicts": [],
        }])
        self.assertEqual(out["published"], "false")

    def legacy_release(self, name):
        self.f.note(name=name)
        # Reconstruct only a legacy fixture, never version the source repository.
        self.f.cli("version", "--no-lockfiles")
        return self.save("legacy release")

    def plain_current_config(self, version):
        (self.f.repo / "VERSION").write_text(version + "\n")
        self.configure("VERSION")
        self.save("use a plain version file after the release")

    def cargo_project(self):
        self.require_tool("cargo")
        f = self.f
        (f.repo / "Cargo.toml").write_text(
            '[package]\nname = "isolation-fixture"\nversion = "1.0.0"\nedition = "2021"\n'
        )
        (f.repo / "src").mkdir()
        (f.repo / "src/lib.rs").write_text("// No dependencies or build scripts.\n")
        self.configure("Cargo.toml")
        f.env["RUSTUP_TOOLCHAIN"] = "stable"
        try:
            f.run_cmd(["cargo", "generate-lockfile", "--offline"])
        finally:
            f.env.pop("RUSTUP_TOOLCHAIN")
        # An invalid consumer toolchain must not trigger a download.
        f.env["RUSTUP_AUTO_INSTALL"] = "0"
        self.save("dependency-free Cargo project")

    def test_cargo_warm_and_preview_accept_two_legacy_unsynchronized_releases(self):
        self.cargo_project()
        first = self.legacy_release("first")
        second = self.legacy_release("second")
        parent = second + "^"
        manifest = tomllib.loads(self.f.git("show", f"{parent}:Cargo.toml"))
        lock = tomllib.loads(self.f.git("show", f"{parent}:Cargo.lock"))
        self.assertEqual(manifest["package"]["version"], "1.0.1")
        self.assertEqual(lock["package"][0]["version"], "1.0.0")
        self.assertEqual(self.f.git("show", f"{first}:Cargo.lock"),
                         self.f.git("show", f"{second}:Cargo.lock"))
        self.plain_current_config("1.0.2")
        _, tools = self.environment("discover", "select")
        self.assertEqual(tools["cargo"], "true")
        self.environment("warm", "select")
        self.preview(second, "1.0.2")

    def test_stable_selection_overrides_consumer_and_historical_rust_toolchains(self):
        self.require_tool("rustup")
        self.cargo_project()
        f = self.f
        toolchain = f.repo / "rust-toolchain.toml"
        toolchain.write_text('[toolchain]\nchannel = "missing-historical-toolchain"\n')
        self.save("conflicting historical toolchain")
        f.note()
        # No input selects stable. Warm and PR generation must both ignore the file.
        self.environment("warm", "manage-pr")
        f.env["INPUT_OPERATION"] = "manage-pr"
        _, out = f.action("release")
        self.assertEqual(out["pr-operation"], "created")
        f.git("fetch", "origin", test_actions.BRANCH)
        f.git("merge", "--squash", "FETCH_HEAD")
        release = self.save("merge synchronized Cargo release")
        lock = tomllib.loads(f.git("show", f"{release}:Cargo.lock"))
        self.assertEqual(lock["package"][0]["version"], "1.0.1")
        toolchain.write_text('[toolchain]\nchannel = "missing-consumer-toolchain"\n')
        self.plain_current_config("1.0.1")
        for selected in (None, "stable"):
            with self.subTest(rust_toolchain=selected):
                if selected is None:
                    f.env.pop("INPUT_RUST_TOOLCHAIN", None)
                else:
                    f.env["INPUT_RUST_TOOLCHAIN"] = selected
                self.environment("warm", "select")
                # This synchronized release cannot pass with the legacy replay profile.
                self.preview(release, "1.0.1")

    def uv_isolation(self, *, environment, symlink, historical):
        self.require_tool("uv")
        f = self.f
        outside = f.top / "outside-environment"
        outside.mkdir()
        (outside / "marker").write_text("Do not change this environment.\n")
        if symlink:
            (f.repo / ".venv").symlink_to(outside, target_is_directory=True)
        (f.repo / "pyproject.toml").write_text(
            '[project]\nname = "isolation-fixture"\nversion = "1.0.0"\n'
            'requires-python = ">=3.10"\ndependencies = []\n'
        )
        self.configure("pyproject.toml")
        # Resolve the real uv from the same PATH that the Action uses.
        f.run_cmd(["uv", "lock", "--offline"])
        self.save("dependency-free uv project")
        if historical:
            self.legacy_release("first")
            release = self.legacy_release("second")
            parent = release + "^"
            manifest = tomllib.loads(f.git("show", f"{parent}:pyproject.toml"))
            lock = tomllib.loads(f.git("show", f"{parent}:uv.lock"))
            self.assertEqual(manifest["project"]["version"], "1.0.1")
            self.assertEqual(lock["package"][0]["version"], "1.0.0")
            self.plain_current_config("1.0.2")
            operation = "select"
        else:
            f.note()
            operation = "manage-pr"
        if environment:
            f.env["UV_PROJECT_ENVIRONMENT"] = str(outside)
        before = snapshot(outside)
        _, tools = self.environment("discover", operation)
        self.assertEqual(tools["uv"], "true")
        result, _ = self.environment("warm", operation)
        self.assertIn("/uv-environment", result.stderr)
        self.assertIn(str(self.temp), result.stderr)
        self.assertEqual(snapshot(outside), before)
        if symlink:
            self.assertTrue((f.repo / ".venv").is_symlink())
            self.assertEqual((f.repo / ".venv").resolve(), outside)
        else:
            self.assertFalse((f.repo / ".venv").exists())
        if historical:
            self.preview(release, "1.0.2")
            self.assertEqual(snapshot(outside), before)

    def test_uv_current_warm_does_not_use_consumer_environment_variable(self):
        self.uv_isolation(environment=True, symlink=False, historical=False)

    def test_uv_current_warm_does_not_follow_committed_venv_symlink(self):
        self.uv_isolation(environment=False, symlink=True, historical=False)

    def test_uv_historical_warm_uses_private_environment_with_both_overrides(self):
        self.uv_isolation(environment=True, symlink=True, historical=True)

    def test_npm_current_and_historical_warm_do_not_run_lifecycle_hooks(self):
        self.require_tool("npm")
        f = self.f
        marker = f.top / "lifecycle-marker"
        marker.write_text("unchanged\n")
        f.env["LIFECYCLE_MARKER"] = str(marker)
        hooks = ("preinstall", "install", "postinstall", "prepublish",
                 "preprepare", "prepare", "postprepare")
        scripts = {hook: 'node -e "require(\'fs\').appendFileSync('
                   'process.env.LIFECYCLE_MARKER, \'' + hook + '\\n\')"'
                   for hook in hooks}
        (f.repo / "package.json").write_text(json.dumps({
            "name": "isolation-fixture", "version": "1.0.0", "scripts": scripts
        }))
        self.configure("package.json")
        f.run_cmd(["npm", "install", "--package-lock-only", "--ignore-scripts",
                   "--offline", "--no-audit", "--no-fund"])
        self.save("dependency-free npm project with lifecycle hooks")
        f.note(name="first")
        self.environment("warm", "manage-pr")
        self.assertEqual(marker.read_text(), "unchanged\n")
        f.cli("version", "--no-lockfiles")
        self.save("first legacy npm release")
        release = self.legacy_release("second")
        parent = release + "^"
        self.assertEqual(json.loads(f.git("show", f"{parent}:package.json"))["version"], "1.0.1")
        self.assertEqual(json.loads(f.git("show", f"{parent}:package-lock.json"))["version"], "1.0.0")
        self.plain_current_config("1.0.2")
        _, tools = self.environment("discover", "select")
        self.assertEqual(tools["npm"], "true")
        self.environment("warm", "select")
        self.preview(release, "1.0.2")
        self.assertEqual(marker.read_text(), "unchanged\n")


if __name__ == "__main__":
    unittest.main()
