"""Verify releases that leave notes added after Release PR generation pending."""

import json
import os
import unittest

import test_actions


class StaleRelease(unittest.TestCase):
    def setUp(self):
        self.f = test_actions.Actions("test_no_batch")
        self.f.setUp()
        self.addCleanup(self.f.doCleanups)
        self.pending_path = ".changenote/newer.md"

    def release(self, mode="merge", bump="minor", package="pkg", no_lockfiles=False):
        f = self.f
        f.note()
        if no_lockfiles:
            f.git("checkout", "-b", "historical-release")
            f.cli("version", "--no-lockfiles")
            head = f.commit("historical release")
            f.git("checkout", "main")
        else:
            f.action()
            f.git("fetch", "origin", test_actions.BRANCH)
            head = f.git("rev-parse", "FETCH_HEAD")
        f.cli("add", "newer", "--" + bump, package, "-m", "A later change")
        if self.pending_path != ".changenote/newer.md":
            (f.repo / ".changenote/newer.md").rename(f.repo / self.pending_path)
        f.commit("new note before the release merge")
        f.git("push", "origin", "main")
        if mode == "merge":
            f.git("merge", "--no-ff", head, "-m", "merge stale Release PR")
        elif mode == "rebase":
            f.git("cherry-pick", head)
        else:
            f.git("merge", "--squash", head)
            f.commit("squash stale Release PR")
        release = f.git("rev-parse", "HEAD")
        f.git("push", "origin", "main")
        pulls = f.state()["pulls"]
        for pull in pulls:
            pull.update(state="closed", merged_at="now")
        f.set_state(pulls=pulls)
        return release

    def assert_published(self, release, version="1.0.1"):
        f = self.f
        pending = (f.repo / self.pending_path).read_bytes()
        _, preview = f.action("preview")
        self.assertEqual(preview["ready"], "true")
        self.assertEqual(preview["release-commit"], release)
        _, published = f.action("publish")
        self.assertEqual(published["release-commit"], release)
        self.assertEqual(published["published"], "true")
        for body in (entry["body"] for entry in f.state()["releases"]):
            self.assertNotIn("A later change", body)
        self.assertEqual((f.repo / self.pending_path).read_bytes(), pending)
        self.assertEqual(f.git("status", "--porcelain"), "")
        plan = json.loads(f.cli("status", "--include-note-paths"))
        self.assertTrue(all(item["old_version"] == version for item in plan))
        self.assertEqual(
            {path for item in plan for path in item["note_paths"]},
            {self.pending_path},
        )
        _, retry = f.action("publish")
        self.assertEqual(retry["published"], "false")

    def test_merge_squash_and_rebase_keep_same_and_higher_bump_notes(self):
        for mode in ("merge", "squash", "rebase"):
            for bump in ("patch", "minor"):
                with self.subTest(mode=mode, bump=bump):
                    self.doCleanups()
                    self.setUp()
                    release = self.release(mode=mode, bump=bump)
                    self.assert_published(release)
                    self.f.action()
                    self.assertIn("A later change", self.f.state()["pulls"][-1]["body"])

    def test_pending_paths_are_literal_and_nul_delimited(self):
        self.pending_path = ".changenote/new[1]\n*.md"
        self.assert_published(self.release())

    @unittest.skipUnless(os.name == "posix", "non-UTF-8 POSIX note paths")
    def test_non_utf8_pending_paths_survive_legacy_replay_and_retry(self):
        f = self.f
        f.npm_project()
        self.pending_path = os.fsdecode(b".changenote/new[\xff]\n*.md")
        release = self.release(no_lockfiles=True)
        pending = (f.repo / self.pending_path).read_bytes()
        for attempt in range(2):
            _, preview = f.action("preview")
            self.assertEqual(preview["ready"], "true")
            self.assertEqual(preview["release-commit"], release)
            _, published = f.action("publish")
            self.assertEqual(published["published"], "true" if attempt == 0 else "false")
            self.assertEqual(published["release-commit"], release)
            self.assertEqual((f.repo / self.pending_path).read_bytes(), pending)
            self.assertEqual(f.git("status", "--porcelain"), "")
        self.assertEqual(len(f.state()["releases"]), 1)
        self.assertNotIn("A later change", f.state()["releases"][0]["body"])
        self.assertEqual(json.loads(f.cli("status"))[0]["notes"], ["A later change"])
        self.assertEqual(json.loads((f.repo / "package-lock.json").read_text())["version"], "1.0.0")

    def test_fixed_group_uses_only_consumed_notes(self):
        f = self.f
        (f.repo / "b").mkdir()
        (f.repo / "b/VERSION").write_text("2.0.0\n")
        (f.repo / ".changenote/config.json").write_text(json.dumps({
            "packages": {
                "pkg": {"path": ".", "version_file": "VERSION"},
                "pkg-b": {"path": "b", "version_file": "VERSION"},
            },
            "fixed": [["pkg", "pkg-b"]],
        }))
        f.commit("fixed group")
        f.git("push", "origin", "main")
        release = self.release(package="pkg-b")
        self.assert_published(release, version="2.0.1")
        self.assertEqual(len(f.state()["releases"]), 2)

    def test_no_lockfile_profile_restores_pending_notes_on_retry(self):
        self.f.npm_project()
        release = self.release(no_lockfiles=True)
        self.assert_published(release)
        self.assertEqual(
            json.loads(self.f.git("show", release + ":package-lock.json"))["version"],
            "1.0.0",
        )

    def test_pending_note_edits_or_extra_deletions_are_rejected(self):
        for edit in ("modify", "delete"):
            with self.subTest(edit=edit):
                self.doCleanups()
                self.setUp()
                self.release(bump="patch")
                path = self.f.repo / self.pending_path
                if edit == "modify":
                    path.write_text(path.read_text().replace("A later change", "Tampered"))
                else:
                    path.unlink()
                self.f.git("add", "--", self.pending_path)
                self.f.git("commit", "--amend", "--no-edit")
                self.f.git("push", "--force", "origin", "main")
                for action in ("preview", "publish"):
                    result, outputs = self.f.action(action, ok=False)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("generated release tree", result.stderr)
                    if action == "preview":
                        self.assertEqual(outputs["ready"], "false")
                self.assertEqual(self.f.state()["releases"], [])
                self.assertEqual(self.f.git("ls-remote", "--tags", "origin"), "")


if __name__ == "__main__":
    unittest.main()
