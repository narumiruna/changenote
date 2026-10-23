"""Local release replay and read-only PR checks with the built CLI."""

import json
import shutil
import unittest

import test_actions


class ExtendedOutputs(unittest.TestCase):
    def setUp(self):
        self.f = test_actions.Actions("test_no_batch")
        self.f.setUp()
        self.addCleanup(self.f.doCleanups)

    def configure_outputs(self):
        f = self.f
        f.npm_project()
        (f.repo / "nested").mkdir()
        (f.repo / "nested/package.json").write_text('{"name":"nested","version":"1.0.0"}\n')
        (f.repo / "pyproject.toml").write_text('[project]\nname = "pkg"\nversion = "1.0.0"\n')
        (f.repo / "unrelated.txt").write_text("unchanged\n")
        (f.repo / ".changenote/config.json").write_text(json.dumps({
            "packages": {"pkg": {
                "path": ".", "version_file": "VERSION",
                "version_outputs": ["package.json", "nested/package.json", "pyproject.toml"],
                "tag_prefix": "sdk/v",
            }}
        }))
        f.commit("additional outputs and custom tags")
        f.git("push", "origin", "main")

    def prepare_release(self):
        self.configure_outputs()
        f = self.f
        f.note()
        self.plan = f.top / "stage-plan.json"
        self.plan.write_text(f.cli("status"))
        f.cli("version")

    def stage(self, ok=True):
        return self.f.run_cmd([
            "bash", "-c", 'source "$1"; stage_release "$2"', "stage",
            str(test_actions.SOURCE / "scripts/action/common.sh"), str(self.plan),
        ], ok=ok)

    def test_custom_tag_and_all_outputs_stage_and_replay(self):
        self.configure_outputs()
        f = self.f
        release = f.release()
        self.assertEqual(f.git("show", f"{release}:VERSION"), "1.0.1")
        for path in ("package.json", "nested/package.json", "package-lock.json"):
            self.assertEqual(json.loads(f.git("show", f"{release}:{path}"))["version"], "1.0.1")
        self.assertIn('version = "1.0.1"', f.git("show", f"{release}:pyproject.toml"))
        batch = json.loads(f.git("show", f"{release}:.changenote/releases.json"))
        self.assertEqual(batch[0]["tag"], "sdk/v1.0.1")
        f.later_commit()
        _, preview = f.action("preview")
        self.assertEqual(preview["ready"], "true")
        self.assertEqual(json.loads(preview["publication-plan"])[0]["tag"], "sdk/v1.0.1")
        f.action("publish")
        self.assertEqual(f.state()["releases"][0]["tag_name"], "sdk/v1.0.1")
        self.assertIn(release, f.git("ls-remote", "origin", "refs/tags/sdk/v1.0.1^{}"))
        self.assertEqual(f.git("status", "--porcelain"), "")

    def test_remote_tag_namespaces_block_preview_and_publication(self):
        self.configure_outputs()
        f = self.f
        release = f.release()
        for existing in ("sdk", "sdk/v1.0.1/child"):
            with self.subTest(existing=existing):
                f.git("tag", existing, release)
                f.git("push", "origin", "refs/tags/" + existing)
                before = f.git("ls-remote", "--tags", "origin")
                result, outputs = f.action("preview", ok=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(outputs["ready"], "false")
                plan = json.loads(outputs["publication-plan"])
                self.assertEqual(plan[0]["action"], "conflict")
                self.assertIn("namespace conflict", plan[0]["conflicts"][0])
                result, outputs = f.action("publish", ok=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(outputs["published"], "false")
                self.assertEqual(f.state()["releases"], [])
                self.assertEqual(f.git("ls-remote", "--tags", "origin"), before)
                f.git("push", "origin", ":refs/tags/" + existing)
                f.git("tag", "-d", existing)

    def test_additional_outputs_preserve_no_lockfile_replay(self):
        self.configure_outputs()
        f = self.f
        f.note()
        f.cli("version", "--no-lockfiles")
        release = f.commit("historical release without lockfile synchronization")
        f.git("push", "origin", "main")
        f.action("publish")
        self.assertEqual(json.loads(f.git("show", f"{release}:package-lock.json"))["version"], "1.0.0")
        self.assertEqual(f.state()["releases"][0]["tag_name"], "sdk/v1.0.1")

    def test_staging_deduplicates_shared_lock_and_excludes_unrelated_files(self):
        self.prepare_release()
        f = self.f
        (f.repo / "unrelated.txt").write_text("do not stage\n")
        (f.repo / "untracked.txt").write_text("do not stage\n")
        # Capture the literal git-add arguments to prove shared locks occur once.
        real_git = shutil.which("git")
        args_file = f.top / "git-add-args"
        f.env.update(REAL_GIT=real_git, ADD_ARGS=str(args_file))
        wrapper = f.bin / "git"
        wrapper.write_text(
            '#!/usr/bin/env bash\n'
            'if [[ ${1:-} == --literal-pathspecs && ${2:-} == add ]]; then\n'
            '  printf "%s\\0" "$@" > "$ADD_ARGS"\nfi\n'
            'exec "$REAL_GIT" "$@"\n'
        )
        wrapper.chmod(0o755)
        self.stage()
        expected = {
            ".changenote/releases.json", ".changenote/fix.md", "VERSION",
            "package.json", "nested/package.json", "pyproject.toml",
            "package-lock.json", "CHANGELOG.md",
        }
        staged = set(f.git("diff", "--cached", "--name-only").splitlines())
        self.assertEqual(staged, expected)
        args = args_file.read_bytes().decode().split("\0")[3:-1]
        self.assertEqual(set(args), expected)
        self.assertEqual(len(args), len(expected))
        self.assertIn("unrelated.txt", f.git("diff", "--name-only"))
        self.assertIn("untracked.txt", f.git("ls-files", "--others", "--exclude-standard"))

    def test_nearest_lockfiles_are_staged_not_more_distant_locks(self):
        self.configure_outputs()
        f = self.f
        # Both configured npm manifests now share the nested lock. The root
        # lock exists but is not the nearest supported lock for either output.
        (f.repo / "nested/child").mkdir()
        (f.repo / "nested/child/package.json").write_text('{"name":"child","version":"1.0.0"}\n')
        (f.repo / "nested/package-lock.json").write_text('{"version":"1.0.0"}\n')
        config_path = f.repo / ".changenote/config.json"
        config = json.loads(config_path.read_text())
        config["packages"]["pkg"]["version_outputs"] = ["nested/package.json", "nested/child/package.json"]
        config_path.write_text(json.dumps(config))
        f.commit("nested lockfile")
        f.git("push", "origin", "main")
        f.note()
        self.plan = f.top / "stage-plan.json"
        self.plan.write_text(f.cli("status"))
        f.cli("version")
        (f.repo / "package-lock.json").write_text('{"unrelated":"change"}\n')
        self.stage()
        staged = set(f.git("diff", "--cached", "--name-only").splitlines())
        self.assertIn("nested/package-lock.json", staged)
        self.assertNotIn("package-lock.json", staged)

    def test_additional_output_symlink_is_rejected_before_staging(self):
        self.prepare_release()
        f = self.f
        target = f.top / "outside.json"
        target.write_text('{"version":"1.0.1"}\n')
        manifest = f.repo / "nested/package.json"
        manifest.unlink()
        manifest.symlink_to(target)
        result = self.stage(ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("cannot be symlinks", result.stderr)
        self.assertEqual(f.git("diff", "--cached", "--name-only"), "")

    def test_additional_manifest_lock_symlink_is_rejected_before_staging(self):
        self.prepare_release()
        f = self.f
        lock = f.repo / "package-lock.json"
        target = f.top / "outside-lock.json"
        lock.replace(target)
        lock.symlink_to(target)
        result = self.stage(ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("cannot be symlinks", result.stderr)
        self.assertEqual(f.git("diff", "--cached", "--name-only"), "")

    def test_additional_output_directory_symlink_is_rejected_before_staging(self):
        self.prepare_release()
        f = self.f
        directory = f.repo / "nested"
        target = f.top / "outside-directory"
        directory.rename(target)
        directory.symlink_to(target, target_is_directory=True)
        result = self.stage(ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("cannot be symlinks", result.stderr)
        self.assertEqual(f.git("diff", "--cached", "--name-only"), "")


class CheckWithCli(unittest.TestCase):
    def setUp(self):
        self.f = test_actions.Actions("test_no_batch")
        self.f.setUp()
        self.addCleanup(self.f.doCleanups)
        self.f.env.pop("GH_TOKEN", None)
        self.f.env.pop("GITHUB_TOKEN", None)
        self.f.env.update(INPUT_COVERAGE="package", INPUT_SKIP_RELEASE="false", INPUT_REASON="")
        self.f.env["GITHUB_STEP_SUMMARY"] = str(self.f.top / "summary")
        # None of these tools are necessary for a read-only check or status.
        for tool in ("gh", "npm", "cargo", "uv", "curl"):
            path = self.f.bin / tool
            path.write_text('#!/bin/sh\nexit 99\n')
            path.chmod(0o755)

    def snapshot(self):
        f = self.f
        return (
            {str(path.relative_to(f.repo)): path.read_bytes()
             for path in f.repo.rglob("*") if path.is_file() and ".git" not in path.relative_to(f.repo).parts},
            (f.repo / ".git/index").read_bytes(),
            f.git("show-ref"),
        )

    def test_pr_failure_has_complete_older_pending_plan_and_no_writes(self):
        f = self.f
        f.cli("add", "old", "--patch", "pkg", "-m", "Older pending note")
        base = f.commit("old pending note")
        (f.repo / "source.txt").write_text("new change\n")
        f.commit("source without new note")
        f.env["INPUT_SINCE"] = base
        before = self.snapshot()
        result, outputs = f.action("check", ok=False)
        self.assertEqual(result.returncode, 1, result.stderr + result.stdout)
        report = json.loads(outputs["check-report"])
        self.assertEqual(report["uncovered_packages"], ["pkg"])
        self.assertEqual(report["note_paths"], [])
        self.assertEqual(outputs["passed"], "false")
        self.assertEqual(json.loads(outputs["release-plan"])[0]["note_paths"], [".changenote/old.md"])
        self.assertEqual(before, self.snapshot())
        f.env.update(INPUT_SKIP_RELEASE="true", INPUT_REASON="Reviewed documentation-only change")
        result, outputs = f.action("check")
        self.assertEqual(outputs["passed"], "true")
        self.assertEqual(before, self.snapshot())

    def test_new_note_passes_and_dirty_checkout_has_no_outputs(self):
        f = self.f
        f.env["INPUT_SINCE"] = f.git("rev-parse", "HEAD")
        f.cli("add", "new", "--patch", "pkg", "-m", "New note")
        f.commit("new note")
        before = self.snapshot()
        _, outputs = f.action("check")
        self.assertEqual(outputs["passed"], "true")
        self.assertEqual(before, self.snapshot())
        (f.repo / "untracked").write_text("dirty\n")
        result, outputs = f.action("check", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(outputs, {})


if __name__ == "__main__":
    unittest.main()
