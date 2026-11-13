"""Inspect completed historical batches without enabling old-format publication."""

import json
import unittest

import test_actions
from test_preview import snapshot


class LegacyBatches(unittest.TestCase):
    def setUp(self):
        self.new_fixture()

    def new_fixture(self):
        self.f = test_actions.Actions("test_no_batch")
        self.f.setUp()
        self.addCleanup(self.f.doCleanups)
        self.f.git("config", "gc.auto", "0")
        self.f.git("config", "maintenance.auto", "false")
        event = self.f.top / "event.json"
        event.write_text(json.dumps({"forced": False, "deleted": False}))
        self.f.env.update(GITHUB_EVENT_NAME="push", GITHUB_EVENT_PATH=str(event))

    def legacy_release(self, flat=False):
        f = self.f
        if flat:
            return f.legacy_release()
        f.note()
        batch = f.cli("status") + "\n"
        f.cli("version")
        (f.repo / ".changenote/releases.json").write_text(batch)
        sha = f.commit("historical grouped release")
        f.git("push", "origin", "main")
        return sha

    def complete_publication(self, sha, draft=False, tag=True):
        f = self.f
        if tag:
            f.git("tag", "-a", "v1.0.1", sha, "-m", "v1.0.1")
            f.git("push", "origin", "refs/tags/v1.0.1")
        f.set_state(releases=[{"tag_name": "v1.0.1", "draft": draft, "body": "keep"}])

    def assert_no_remote_writes(self, before):
        state, remote = before
        self.assertEqual(snapshot(self.f.remote), remote)
        self.assertEqual(self.f.state()["releases"], state["releases"])
        calls = self.f.state().get("calls", [])[len(state.get("calls", [])):]
        self.assertTrue(all(call[0] == "api" and "--method" not in call for call in calls))

    def test_completed_grouped_and_flat_batches_are_readonly(self):
        for flat in (False, True):
            with self.subTest(flat=flat):
                if flat:
                    self.new_fixture()
                sha = self.legacy_release(flat=flat)
                self.complete_publication(sha)
                self.f.later_commit()
                before = self.f.state(), snapshot(self.f.remote)
                source = snapshot(self.f.repo)
                _, out = self.f.action("preview")
                self.assertEqual(out["ready"], "true")
                self.assertEqual(out["release-commit"], sha)
                self.assertEqual(json.loads(out["publication-plan"])[0]["action"], "none")
                self.assertEqual(snapshot(self.f.repo), source)
                _, out = self.f.action("publish")
                self.assertEqual(out["published"], "false")
                self.assertEqual(out["published-packages"], "[]")
                self.assertEqual(out["release-commit"], sha)
                self.assert_no_remote_writes(before)

    def test_auto_and_retry_accept_complete_legacy_batch(self):
        sha = self.legacy_release()
        self.complete_publication(sha)
        before = self.f.state(), snapshot(self.f.remote)
        _, out = self.f.action("release")
        self.assertEqual(out["release-commit"], "")
        self.assertEqual(out["publication-needed"], "false")
        self.assertEqual(out["ready"], "true")
        self.assertEqual(out["published"], "false")
        self.f.env.update(GITHUB_EVENT_NAME="workflow_dispatch",
                          INPUT_OPERATION="retry-publication")
        _, out = self.f.action("release")
        self.assertEqual(out["release-commit"], sha)
        self.assertEqual(out["ready"], "true")
        self.assertEqual(out["published"], "false")
        self.assert_no_remote_writes(before)

    def test_auto_manages_next_pr_after_complete_legacy_batch(self):
        sha = self.legacy_release()
        self.complete_publication(sha)
        self.f.note("Next release", "next")
        _, out = self.f.action("release")
        self.assertEqual(out["has-notes"], "true")
        self.assertEqual(out["pr-operation"], "created")
        self.assertEqual(out["ready"], "true")
        self.assertEqual(out["published"], "false")
        self.assertTrue((self.f.repo / ".changenote/next.md").exists())
        self.assertEqual(len(self.f.state()["releases"]), 1)

    def test_next_metadata_only_batch_still_publishes(self):
        sha = self.legacy_release()
        self.complete_publication(sha)
        self.f.note("Next release", "next")
        self.f.cli("version")
        latest = self.f.commit("next release")
        self.f.git("push", "origin", "main")
        _, out = self.f.action("release")
        self.assertEqual(out["release-commit"], latest)
        self.assertEqual(out["published"], "true")
        self.assertEqual([r["tag_name"] for r in self.f.state()["releases"]],
                         ["v1.0.1", "v1.0.2"])

    def test_missing_or_draft_publication_never_creates_remote_objects(self):
        for kind in ("missing-release", "missing-tag", "draft", "wrong-tag"):
            with self.subTest(kind=kind):
                self.new_fixture()
                sha = self.legacy_release()
                self.complete_publication(sha if kind != "wrong-tag" else sha + "^",
                                          draft=kind == "draft", tag=kind != "missing-tag")
                if kind == "missing-release":
                    self.f.set_state(releases=[])
                before = self.f.state(), snapshot(self.f.remote)
                result, out = self.f.action("preview", ok=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("complete existing publication", result.stderr)
                self.assertEqual(out["ready"], "false")
                self.assertEqual(json.loads(out["publication-plan"])[0]["action"],
                                 "create-release" if kind == "missing-release" else "conflict")
                result, out = self.f.action("publish", ok=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(out["published"], "false")
                self.assert_no_remote_writes(before)

    def test_unpublished_legacy_batch_is_not_published(self):
        self.legacy_release()
        before = self.f.state(), snapshot(self.f.remote)
        result, out = self.f.action("release", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("complete existing publication", result.stderr)
        self.assertEqual(out["published"], "false")
        self.assert_no_remote_writes(before)

    def test_complete_remote_objects_do_not_bypass_batch_or_tree_checks(self):
        for kind in ("notes", "field", "format", "tree"):
            with self.subTest(kind=kind):
                self.new_fixture()
                self.legacy_release()
                path = self.f.repo / ".changenote/releases.json"
                if kind == "tree":
                    (self.f.repo / "unrelated.txt").write_text("not generated")
                else:
                    batch = json.loads(path.read_text())
                    if kind == "notes":
                        batch[0]["notes"] = ["tampered"]
                    elif kind == "field":
                        batch[0]["unknown"] = True
                    path.write_text(json.dumps(batch) if kind == "format"
                                    else json.dumps(batch, indent=2) + "\n")
                self.f.git("add", "--", ".changenote/releases.json")
                if kind == "tree":
                    self.f.git("add", "--", "unrelated.txt")
                self.f.git("commit", "--amend", "--no-edit")
                sha = self.f.git("rev-parse", "HEAD")
                self.f.git("push", "--force", "origin", "main")
                self.complete_publication(sha)
                before = self.f.state(), snapshot(self.f.remote)
                result, out = self.f.action("preview", ok=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("parent release plan" if kind in {"notes", "field"}
                              else "generated release tree", result.stderr)
                self.assertEqual(out["ready"], "false")
                self.assert_no_remote_writes(before)

    def test_every_legacy_batch_member_must_be_published(self):
        f = self.f
        (f.repo / "other").mkdir()
        (f.repo / "other/VERSION").write_text("1.0.0\n")
        (f.repo / ".changenote/config.json").write_text(json.dumps({"packages": {
            "pkg": {"path": ".", "version_file": "VERSION"},
            "other": {"path": "other", "version_file": "VERSION"},
        }}))
        f.cli("add", "shared", "--package", "pkg", "--package", "other",
              "--bump", "patch", "--message", "Fix shared behavior")
        f.commit("shared fixture")
        sha = self.legacy_release()
        tags = [item["tag"] for item in json.loads((f.repo / ".changenote/releases.json").read_text())]
        for tag in tags:
            f.git("tag", "-a", tag, sha, "-m", tag)
            f.git("push", "origin", "refs/tags/" + tag)
        f.set_state(releases=[{"tag_name": tags[0], "draft": False}])
        before = f.state(), snapshot(f.remote)
        result, out = f.action("preview", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(out["ready"], "false")
        self.assertEqual([item["action"] for item in json.loads(out["publication-plan"])],
                         ["none", "create-release"])
        result, out = f.action("publish", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(out["published"], "false")
        self.assert_no_remote_writes(before)
        f.set_state(releases=[{"tag_name": tag, "draft": False} for tag in tags])
        _, out = f.action("preview")
        self.assertEqual(out["ready"], "true")

    def test_api_errors_are_not_treated_as_complete_publication(self):
        sha = self.legacy_release()
        self.complete_publication(sha)
        self.f.set_state(releases_api_fail=True)
        result, out = self.f.action("preview", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("API unavailable", result.stderr)
        self.assertEqual(out["ready"], "false")


if __name__ == "__main__":
    unittest.main()
