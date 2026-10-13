"""Run with cargo build --locked; uv run --no-project python -m unittest discover -s tests/actions."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SOURCE = Path(__file__).resolve().parents[2]
CLI = SOURCE / "target/debug/changenote"
BRANCH = "narumi/chore/changenote-release-main"


class Actions(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.top = Path(self.tmp.name)
        self.remote = self.top / "remote.git"
        self.repo = self.top / "consumer"
        self.bin = self.top / "bin"
        self.bin.mkdir()
        self.env = dict(
            os.environ,
            GH_TOKEN="test-token",
            GITHUB_REPOSITORY="test/repo",
            GIT_AUTHOR_NAME="Test",
            GIT_COMMITTER_NAME="Test",
            GIT_AUTHOR_EMAIL="test@example.com",
            GIT_COMMITTER_EMAIL="test@example.com",
        )
        self.env.pop("GITHUB_ACTIONS", None)
        self.env.pop("INPUT_BASE_BRANCH", None)
        self.env["PATH"] = str(self.bin) + ":" + self.env["PATH"]
        self.state_path = self.top / "github.json"
        self.env["MOCK_GH_STATE"] = str(self.state_path)
        self.env["MOCK_GH_SCRIPT"] = str(SOURCE / "tests/actions/mock_gh.py")
        (self.bin / "gh").write_text(
            '#!/usr/bin/env bash\nexec uv run --no-project python "$MOCK_GH_SCRIPT" "$@"\n'
        )
        (self.bin / "gh").chmod(0o755)
        shutil.copy2(CLI, self.bin / "changenote")
        # An unborn remote HEAD proves that all fetches use the selected base ref.
        self.run_cmd(
            ["git", "init", "--bare", "--initial-branch=unused", str(self.remote)],
            cwd=self.top,
        )
        self.run_cmd(["git", "init", "-b", "main", str(self.repo)], cwd=self.top)
        self.git("remote", "add", "origin", str(self.remote))
        self.cli("init", "--name", "pkg", "--version", "1.0.0")
        self.commit("initial")
        self.git("push", "origin", "main")
        self.set_state(remote=str(self.remote), pulls=[], releases=[])

    def run_cmd(self, args, cwd=None, ok=True):
        result = subprocess.run(
            args,
            cwd=cwd or self.repo,
            env=self.env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        if ok and result.returncode:
            self.fail(f"{args}\n{result.stdout}\n{result.stderr}")
        return result

    def git(self, *args):
        return self.run_cmd(["git", *args]).stdout.strip()

    def cli(self, *args):
        return self.run_cmd([str(CLI), *args]).stdout.strip()

    def commit(self, message):
        self.git("add", "--", ".")
        self.git("commit", "-m", message)
        return self.git("rev-parse", "HEAD")

    def set_state(self, **fields):
        state = (
            json.loads(self.state_path.read_text()) if self.state_path.exists() else {}
        )
        state.update(fields)
        self.state_path.write_text(json.dumps(state))

    def state(self):
        return json.loads(self.state_path.read_text())

    def note(self, message="Fix behavior", name="fix"):
        self.cli(
            "add", name, "--package", "pkg", "--bump", "patch", "--message", message
        )
        self.commit("note")
        self.git("push", "origin", "main")

    def action(self, name="version", ok=True):
        output = self.top / "outputs"
        output.write_text("")
        self.env["GITHUB_OUTPUT"] = str(output)
        result = self.run_cmd(
            ["bash", str(SOURCE / f"scripts/action/{name}.sh")], ok=ok
        )
        values = dict(line.split("=", 1) for line in output.read_text().splitlines())
        return result, values

    def release(self, mode="squash"):
        self.note()
        self.action()
        self.git("fetch", "origin", BRANCH)
        if mode == "merge":
            self.git("merge", "--no-ff", "FETCH_HEAD", "-m", "merge release")
        elif mode == "rebase":
            self.git("cherry-pick", "FETCH_HEAD")
        else:
            self.git("merge", "--squash", "FETCH_HEAD")
            self.commit("squash release")
        sha = self.git("rev-parse", "HEAD")
        self.git("push", "origin", "main")
        pulls = self.state()["pulls"]
        pulls[0]["state"] = "closed"
        pulls[0]["merged_at"] = "now"
        self.set_state(pulls=pulls)
        return sha

    def later_commit(self):
        (self.repo / "source.txt").write_text("unrelated change")
        self.commit("source change")
        self.git("push", "origin", "main")

    def test_pr_lifecycle_and_outputs(self):
        _, outputs = self.action()
        self.assertEqual(
            outputs, {"has-notes": "false", "pr-operation": "none", "pr-number": ""}
        )
        self.note("Multiline\n$(touch injected); `echo dangerous`")
        _, outputs = self.action()
        self.assertEqual(outputs["pr-operation"], "created")
        self.assertEqual(outputs["pr-number"], "1")
        self.assertEqual(outputs["has-notes"], "true")
        first = self.git("ls-remote", "origin", "refs/heads/" + BRANCH)
        _, outputs = self.action()
        self.assertEqual(outputs["pr-operation"], "updated")
        self.assertEqual(first, self.git("ls-remote", "origin", "refs/heads/" + BRANCH))
        self.assertFalse((self.repo / "injected").exists())
        self.assertIn("1.0.0 → 1.0.1 (patch)", self.state()["pulls"][0]["body"])
        self.assertIn("$(touch injected)", self.state()["pulls"][0]["body"])
        self.note("More fixes", "second")
        self.action()
        self.assertEqual(len(self.state()["pulls"]), 1)
        self.git("rm", ".changenote/fix.md", ".changenote/second.md")
        self.commit("remove notes")
        self.git("push", "origin", "main")
        _, outputs = self.action()
        self.assertEqual(outputs["pr-operation"], "closed")
        self.assertTrue(self.git("ls-remote", "origin", "refs/heads/" + BRANCH))

    def test_closed_and_merged_pr_next_batch(self):
        self.note()
        self.action()
        pulls = self.state()["pulls"]
        pulls[0]["state"] = "closed"
        self.set_state(pulls=pulls)
        self.action()
        self.assertEqual(len(self.state()["pulls"]), 2)

    def test_body_size_and_special_title(self):
        self.note("😀" * 20000)
        self.env["INPUT_PR_TITLE"] = "chore(release): $(touch bad)"
        self.action()
        pr = self.state()["pulls"][0]
        self.assertLess(len(pr["body"].encode()), 60000)
        self.assertIn("exceed", pr["body"])
        self.assertEqual(pr["title"], self.env["INPUT_PR_TITLE"])

    def test_reject_dirty_invalid_notes_and_no_permission(self):
        (self.repo / "untracked").write_text("x")
        result, _ = self.action(ok=False)
        self.assertIn("clean checkout", result.stderr)
        (self.repo / "untracked").unlink()
        self.set_state(no_write=True)
        result, _ = self.action(ok=False)
        self.assertIn("write permission", result.stderr)
        self.set_state(no_write=False)
        (self.repo / ".changenote/bad.md").write_text("bad")
        self.commit("invalid note")
        self.git("push", "origin", "main")
        result, _ = self.action(ok=False)
        self.assertIn("invalid note", result.stderr)
        self.assertFalse(self.git("ls-remote", "origin", "refs/heads/" + BRANCH))

    def test_missing_tool_api_and_fetch_failure(self):
        (self.bin / "changenote").unlink()
        original_path = self.env["PATH"]
        for tool in ("bash", "uname", "git", "jq"):
            (self.bin / tool).symlink_to(shutil.which(tool))
        self.env["PATH"] = str(self.bin)
        result, _ = self.action(ok=False)
        self.assertIn("Missing prerequisite: changenote", result.stderr)
        self.env["PATH"] = original_path
        shutil.copy2(CLI, self.bin / "changenote")
        self.set_state(api_fail=True)
        result, _ = self.action(ok=False)
        self.assertIn("API unavailable", result.stderr)
        self.set_state(api_fail=False, remote=str(self.top / "missing.git"))
        result, _ = self.action(ok=False)
        self.assertNotEqual(result.returncode, 0)

    def test_orphan_and_unmarked_branch_rejected(self):
        self.note()
        self.git("push", "origin", "HEAD:refs/heads/" + BRANCH)
        result, _ = self.action(ok=False)
        self.assertIn("no managed PR history", result.stderr)
        self.set_state(
            pulls=[
                {
                    "number": 9,
                    "state": "open",
                    "body": "unrelated",
                    "head": {"ref": BRANCH, "repo": {"full_name": "test/repo"}},
                    "base": {"ref": "main"},
                }
            ]
        )
        result, _ = self.action(ok=False)
        self.assertIn("unmanaged open PR", result.stderr)

    def test_unrelated_generated_file_not_staged(self):
        self.note()
        wrapper = self.bin / "changenote"
        # The Action copies the executable away from its original directory.
        wrapper.write_text(
            f'#!/usr/bin/env bash\n"{CLI}" "$@"\nif [[ $1 == version ]]; then echo noise > noise.txt; fi\n'
        )
        wrapper.chmod(0o755)
        self.action()
        self.git("fetch", "origin", BRANCH)
        self.assertNotIn(
            "noise.txt", self.git("ls-tree", "-r", "--name-only", "FETCH_HEAD")
        )

    def test_shallow_and_stale_checkout(self):
        self.note()
        shallow = self.top / "shallow"
        self.run_cmd(
            [
                "git",
                "clone",
                "--depth=1",
                "--branch=main",
                self.remote.as_uri(),
                str(shallow),
            ],
            cwd=self.top,
        )
        self.note("newer", "second")
        self.repo = shallow
        self.action()
        self.assertIn("newer", self.state()["pulls"][0]["body"])
        self.assertEqual(self.git("rev-parse", "--is-shallow-repository"), "false")

    def test_lease_conflict(self):
        self.note()
        self.action()
        old = self.git("ls-remote", "origin", "refs/heads/" + BRANCH).split()[0]
        self.note("more", "second")
        # Move the remote branch after the Action reads its expected SHA.
        concurrent = self.git("rev-parse", "HEAD")
        wrapper = self.bin / "changenote"
        wrapper.write_text(
            f'#!/usr/bin/env bash\n"{CLI}" "$@"\nif [[ $1 == version ]]; then git --git-dir="{self.remote}" update-ref "refs/heads/{BRANCH}" "{concurrent}"; fi\n'
        )
        wrapper.chmod(0o755)
        result, _ = self.action(ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("stale info", result.stderr)
        self.assertNotEqual(old, concurrent)
        self.assertEqual(
            concurrent,
            self.git("ls-remote", "origin", "refs/heads/" + BRANCH).split()[0],
        )

    def test_merge_squash_rebase_and_later_commit(self):
        # Each case has a separate consumer repository and mocked API state.
        for mode in ("merge", "squash", "rebase"):
            with self.subTest(mode=mode):
                if mode != "merge":
                    self.doCleanups()
                    self.setUp()
                release = self.release(mode)
                self.later_commit()
                _, outputs = self.action("publish")
                self.assertEqual(outputs["release-commit"], release)
                self.assertEqual(outputs["published"], "true")
                self.assertEqual(
                    json.loads(outputs["published-packages"]),
                    [{"name": "pkg", "version": "1.0.1"}],
                )
                self.assertEqual(
                    self.git(
                        "--git-dir=" + str(self.remote), "rev-parse", "v1.0.1^{commit}"
                    ),
                    release,
                )
                self.assertEqual(
                    self.git(
                        "--git-dir=" + str(self.remote), "cat-file", "-t", "v1.0.1"
                    ),
                    "tag",
                )
                self.assertEqual(self.state()["releases"][0]["body"], "Fix behavior\n")
                _, outputs = self.action("publish")
                self.assertEqual(outputs["published"], "false")
                self.assertEqual(outputs["published-packages"], "[]")
                self.note("next batch", "next")
                _, outputs = self.action("publish")
                self.assertEqual(outputs["release-commit"], release)
                self.action()
                self.assertEqual(len(self.state()["pulls"]), 2)

    def test_no_batch(self):
        _, outputs = self.action("publish")
        self.assertEqual(
            outputs,
            {"published": "false", "published-packages": "[]", "release-commit": ""},
        )

    def test_manual_batch_edits_and_versions_rejected(self):
        self.release()
        batch = self.repo / ".changenote/releases.json"
        data = json.loads(batch.read_text())
        data[0]["notes"] = ["tampered"]
        batch.write_text(json.dumps(data))
        self.commit("edit batch")
        self.git("push", "origin", "main")
        result, _ = self.action("publish", ok=False)
        self.assertIn("parent release plan", result.stderr)
        self.assertEqual(self.state()["releases"], [])

    def test_release_tree_tampering_rejected(self):
        self.release()
        (self.repo / "VERSION").write_text("9.0.0\n")
        self.git("add", "VERSION")
        self.git("commit", "--amend", "--no-edit")
        self.git("push", "--force", "origin", "main")
        result, _ = self.action("publish", ok=False)
        self.assertIn("generated release tree", result.stderr)
        self.assertEqual(self.state()["releases"], [])

    def test_malformed_empty_removed_batch(self):
        for value in ("bad json", "[]", None):
            with self.subTest(value=value):
                path = self.repo / ".changenote/releases.json"
                if value is None:
                    path.unlink()
                else:
                    path.write_text(value)
                self.commit("bad batch")
                self.git("push", "origin", "main")
                result, _ = self.action("publish", ok=False)
                self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.state()["releases"], [])

    def test_tag_conflict_preflight(self):
        release = self.release()
        self.git("tag", "v1.0.1", release + "^")
        self.git("push", "origin", "refs/tags/v1.0.1")
        result, _ = self.action("publish", ok=False)
        self.assertIn("another commit", result.stderr)
        self.assertEqual(self.state()["releases"], [])

    def test_local_tag_conflict(self):
        release = self.release()
        self.git("tag", "v1.0.1", release + "^")
        result, _ = self.action("publish", ok=False)
        self.assertIn("different commit", result.stderr)
        self.assertFalse(self.git("ls-remote", "origin", "refs/tags/v1.0.1"))

    def test_release_failure_then_retry_after_later_commit(self):
        release = self.release()
        self.set_state(release_fail="v1.0.1")
        result, _ = self.action("publish", ok=False)
        self.assertIn("Release creation failed", result.stderr)
        self.assertEqual(
            self.git("--git-dir=" + str(self.remote), "rev-parse", "v1.0.1^{commit}"),
            release,
        )
        self.later_commit()
        self.set_state(release_fail=None)
        _, outputs = self.action("publish")
        self.assertEqual(outputs["published"], "true")
        self.assertEqual(len(self.state()["releases"]), 1)

    def test_release_create_race(self):
        self.release()
        self.set_state(release_race="v1.0.1")
        self.action("publish")
        self.assertEqual(len(self.state()["releases"]), 1)

    def test_push_failure_not_hidden(self):
        self.release()
        hook = self.remote / "hooks/pre-receive"
        hook.write_text("#!/usr/bin/env bash\nexit 1\n")
        hook.chmod(0o755)
        result, _ = self.action("publish", ok=False)
        self.assertIn("Tag push failed", result.stderr)
        self.assertEqual(self.state()["releases"], [])

    def test_draft_and_missing_release_tag_rejected(self):
        self.release()
        self.set_state(releases=[{"tag_name": "v1.0.1", "draft": True}])
        result, _ = self.action("publish", ok=False)
        self.assertIn("draft", result.stderr)
        self.set_state(releases=[{"tag_name": "v1.0.1", "draft": False}])
        result, _ = self.action("publish", ok=False)
        self.assertIn("no remote tag", result.stderr)

    def test_pr_creation_failure_rolls_back_only_new_branch(self):
        self.note()
        self.set_state(pr_fail=True)
        result, _ = self.action(ok=False)
        self.assertIn("PR creation failed", result.stderr)
        self.assertFalse(self.git("ls-remote", "origin", "refs/heads/" + BRANCH))
        self.set_state(pr_fail=False)
        self.action()
        self.assertEqual(len(self.state()["pulls"]), 1)

    def test_pr_lost_response_is_recovered(self):
        self.note()
        self.set_state(pr_lost_response=True)
        _, outputs = self.action()
        self.assertEqual(outputs["pr-operation"], "created")
        self.assertEqual(outputs["pr-number"], "1")
        self.assertEqual(len(self.state()["pulls"]), 1)
        self.assertTrue(self.git("ls-remote", "origin", "refs/heads/" + BRANCH))

    def test_identity_and_origin_credentials_are_not_changed(self):
        self.note()
        self.git("config", "remote.origin.pushurl", str(self.top / "wrong.git"))
        self.git("config", "http.https://github.com/.extraheader", "old credentials")
        config_before = self.git("config", "--local", "--list")
        self.action()
        self.assertEqual(config_before, self.git("config", "--local", "--list"))
        self.assertTrue(self.git("ls-remote", str(self.remote), "refs/heads/" + BRANCH))

    def log_cli_calls(self):
        log = self.top / "cli-calls"
        self.env["CLI_CALL_LOG"] = str(log)
        wrapper = self.bin / "changenote"
        wrapper.write_text(
            f'#!/usr/bin/env bash\nprintf "%s\\n" "$1" >> "$CLI_CALL_LOG"\nexec "{CLI}" "$@"\n'
        )
        wrapper.chmod(0o755)
        return log

    def test_batch_symlink_blocks_pr_before_version(self):
        self.note()
        batch = self.repo / ".changenote/releases.json"
        for target in ("../VERSION", "../missing-batch.json"):
            with self.subTest(target=target):
                if batch.is_symlink():
                    batch.unlink()
                batch.symlink_to(target)
                self.commit("batch symlink")
                self.git("push", "origin", "main")
                log = self.log_cli_calls()
                result, _ = self.action(ok=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(
                    "cannot be symlinks: .changenote/releases.json", result.stderr
                )
                self.assertNotIn(
                    "version", log.read_text().splitlines() if log.exists() else []
                )
                self.assertEqual((self.repo / "VERSION").read_text(), "1.0.0\n")
                self.assertFalse((self.repo / "missing-batch.json").exists())
                self.assertTrue(batch.is_symlink())
                self.assertEqual(self.git("status", "--porcelain"), "")
                self.assertFalse(
                    self.git("ls-remote", "origin", "refs/heads/" + BRANCH)
                )
                self.assertEqual(self.state()["pulls"], [])

    def test_stage_release_checks_seeded_batch_path(self):
        self.note()
        plan = self.top / "plan.json"
        plan.write_text(self.cli("status"))
        self.cli("version")
        batch = self.repo / ".changenote/releases.json"
        batch.unlink()
        batch.symlink_to("../VERSION")
        self.env["COMMON_SCRIPT"] = str(SOURCE / "scripts/action/common.sh")
        self.env["PLAN_PATH"] = str(plan)
        result = self.run_cmd(
            ["bash", "-c", 'source "$COMMON_SCRIPT"; stage_release "$PLAN_PATH"'],
            ok=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("cannot be symlinks: .changenote/releases.json", result.stderr)
        self.assertEqual(self.git("diff", "--cached", "--name-only"), "")

    def test_batch_symlink_blocks_publication_before_reconstruction(self):
        self.note()
        target = self.repo / "prior.json"
        target.write_text("prior batch\n")
        batch = self.repo / ".changenote/releases.json"
        batch.symlink_to("../prior.json")
        self.commit("parent batch symlink")
        # A regular batch at HEAD must not hide a symlink in its parent.
        batch.unlink()
        self.cli("version")
        self.commit("release")
        self.git("push", "origin", "main")
        log = self.log_cli_calls()
        result, _ = self.action("publish", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("cannot be symlinks: .changenote/releases.json", result.stderr)
        self.assertNotIn(
            "version", log.read_text().splitlines() if log.exists() else []
        )
        self.assertEqual(target.read_text(), "prior batch\n")
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.assertFalse(self.git("ls-remote", "origin", "refs/tags/v1.0.1"))
        self.assertEqual(self.state()["releases"], [])

    def test_action_rejects_symlink_output(self):
        self.git("mv", "VERSION", "real-version")
        (self.repo / "VERSION").symlink_to("real-version")
        self.commit("symlink version")
        self.git("push", "origin", "main")
        self.note()
        result, _ = self.action(ok=False)
        self.assertIn("cannot be symlinks", result.stderr)
        self.assertFalse(self.git("ls-remote", "origin", "refs/heads/" + BRANCH))

    def test_tag_move_during_release_is_reported(self):
        release = self.release()
        self.set_state(
            release_move_tag="v1.0.1", move_sha=self.git("rev-parse", release + "^")
        )
        result, _ = self.action("publish", ok=False)
        self.assertIn("changed while creating", result.stderr)

    def test_root_commit_batch_is_rejected(self):
        self.note()
        self.cli("version")
        self.git("add", "--", ".")
        tree = self.git("write-tree")
        root = self.git("commit-tree", tree, "-m", "root batch")
        self.git("reset", "--hard", root)
        self.git("push", "--force", "origin", "main")
        result, _ = self.action("publish", ok=False)
        self.assertIn("root commit", result.stderr)

    def test_tag_push_race_accepts_only_correct_commit(self):
        release = self.release()
        hook = self.remote / "hooks/pre-receive"
        hook.write_text(
            f'#!/usr/bin/env bash\nenv -u GIT_QUARANTINE_PATH -u GIT_OBJECT_DIRECTORY -u GIT_ALTERNATE_OBJECT_DIRECTORIES git --git-dir="{self.remote}" update-ref refs/tags/v1.0.1 "{release}"\nexit 1\n'
        )
        hook.chmod(0o755)
        _, outputs = self.action("publish")
        self.assertEqual(outputs["published"], "true")
        self.assertEqual(len(self.state()["releases"]), 1)

    def test_noop_detects_concurrent_branch_move(self):
        self.note()
        self.action()
        concurrent = self.git("rev-parse", "HEAD")
        wrapper = self.bin / "changenote"
        wrapper.write_text(
            f'#!/usr/bin/env bash\n"{CLI}" "$@"\nif [[ $1 == version ]]; then git --git-dir="{self.remote}" update-ref "refs/heads/{BRANCH}" "{concurrent}"; fi\n'
        )
        wrapper.chmod(0o755)
        result, _ = self.action(ok=False)
        self.assertIn("moved after generation", result.stderr)

    def test_tag_push_race_rejects_wrong_commit(self):
        release = self.release()
        wrong = self.git("rev-parse", release + "^")
        hook = self.remote / "hooks/pre-receive"
        hook.write_text(
            f'#!/usr/bin/env bash\nenv -u GIT_QUARANTINE_PATH -u GIT_OBJECT_DIRECTORY -u GIT_ALTERNATE_OBJECT_DIRECTORIES git --git-dir="{self.remote}" update-ref refs/tags/v1.0.1 "{wrong}"\nexit 1\n'
        )
        hook.chmod(0o755)
        result, _ = self.action("publish", ok=False)
        self.assertIn("Tag push failed", result.stderr)
        self.assertEqual(self.state()["releases"], [])

    def test_multiple_packages_partial_tag_push(self):
        (self.repo / "b").mkdir()
        (self.repo / "b/VERSION").write_text("2.0.0\n")
        config = {
            "packages": {
                "pkg": {"path": ".", "version_file": "VERSION"},
                "pkg-b": {"path": "b", "version_file": "VERSION"},
            }
        }
        (self.repo / ".changenote/config.json").write_text(json.dumps(config))
        self.commit("second package")
        self.cli(
            "add",
            "both",
            "-p",
            "pkg",
            "-p",
            "pkg-b",
            "--bump",
            "minor",
            "--message",
            "Shared change",
        )
        self.commit("notes")
        self.git("push", "origin", "main")
        self.action()
        self.git("fetch", "origin", BRANCH)
        self.git("merge", "--squash", "FETCH_HEAD")
        self.commit("release")
        self.git("push", "origin", "main")
        hook = self.remote / "hooks/pre-receive"
        hook.write_text(
            "#!/usr/bin/env bash\nwhile read -r old new ref; do\n  if [[ $ref == refs/tags/changenote/pkg-b@v2.1.0 ]]; then exit 1; fi\ndone\n"
        )
        hook.chmod(0o755)
        result, _ = self.action("publish", ok=False)
        self.assertIn("Tag push failed", result.stderr)
        self.assertEqual(len(self.state()["releases"]), 1)
        self.assertFalse(
            self.git("ls-remote", "origin", "refs/tags/changenote/pkg-b@v2.1.0")
        )
        hook.unlink()
        _, outputs = self.action("publish")
        self.assertEqual(
            json.loads(outputs["published-packages"]),
            [{"name": "pkg-b", "version": "2.1.0"}],
        )
        self.assertEqual(len(self.state()["releases"]), 2)

    def test_multiple_packages_partial_publication(self):
        (self.repo / "b").mkdir()
        (self.repo / "b/VERSION").write_text("2.0.0\n")
        config = {
            "packages": {
                "pkg": {"path": ".", "version_file": "VERSION"},
                "pkg-b": {"path": "b", "version_file": "VERSION"},
            }
        }
        (self.repo / ".changenote/config.json").write_text(json.dumps(config))
        self.commit("second package")
        self.cli(
            "add",
            "both",
            "-p",
            "pkg",
            "-p",
            "pkg-b",
            "--bump",
            "minor",
            "--message",
            "Shared\nchange",
        )
        self.commit("notes")
        self.git("push", "origin", "main")
        self.action()
        self.assertIn("pkg-b", self.state()["pulls"][0]["body"])
        self.git("fetch", "origin", BRANCH)
        self.git("merge", "--squash", "FETCH_HEAD")
        self.commit("release")
        self.git("push", "origin", "main")
        self.set_state(release_fail="changenote/pkg-b@v2.1.0")
        result, _ = self.action("publish", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(len(self.state()["releases"]), 1)
        self.set_state(release_fail=None)
        _, outputs = self.action("publish")
        self.assertEqual(
            json.loads(outputs["published-packages"]),
            [{"name": "pkg-b", "version": "2.1.0"}],
        )
        self.assertEqual(len(self.state()["releases"]), 2)


if __name__ == "__main__":
    unittest.main()
