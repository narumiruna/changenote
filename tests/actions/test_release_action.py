"""High-level release operations with real local Git and the file-backed gh mock."""

import json
from pathlib import Path
import unittest

import test_actions
from test_preview import snapshot


OUTPUTS = {
    "release-commit", "releases", "published", "published-packages",
    "publication-plan", "publication-needed", "ready", "has-notes",
    "pr-number", "pr-operation",
}
RELEASES = [{"package": "pkg", "version": "1.0.1", "tag": "v1.0.1"}]


class ReleaseAction(unittest.TestCase):
    def setUp(self):
        self.f = test_actions.Actions("test_no_batch")
        self.f.setUp()
        self.addCleanup(self.f.doCleanups)
        for key in tuple(self.f.env):
            if key.startswith("INPUT_"):
                self.f.env.pop(key)
        self.f.git("config", "gc.auto", "0")
        self.f.git("config", "maintenance.auto", "false")
        self.event = self.f.top / "event.json"
        self.event.write_text(json.dumps({"forced": False, "deleted": False}))
        self.f.env["GITHUB_EVENT_PATH"] = str(self.event)
        self.f.env["TMPDIR"] = str(self.f.top / "release-temp")
        Path(self.f.env["TMPDIR"]).mkdir()

    def run_release(self, operation=None, event="push", retry="false", expected="", ok=True):
        f = self.f
        f.env.update(GITHUB_EVENT_NAME=event, INPUT_RETRY=retry,
                     INPUT_EXPECTED_RELEASE_COMMIT=expected)
        f.env.pop("INPUT_OPERATION", None)
        if operation is not None:
            f.env["INPUT_OPERATION"] = operation
        result, outputs = f.action("release", ok=ok)
        self.assertNotIn("test-token", result.stdout + result.stderr + json.dumps(outputs))
        self.assertEqual(list(Path(f.env["TMPDIR"]).iterdir()), [])
        if result.returncode == 0:
            self.assertEqual(set(outputs), OUTPUTS)
        return result, outputs

    def select_readonly(self, **kwargs):
        f = self.f
        source, remote, state = snapshot(f.repo), snapshot(f.remote), f.state()
        result, outputs = self.run_release("select", **kwargs)
        self.assertEqual(snapshot(f.repo), source)
        self.assertEqual(snapshot(f.remote), remote)
        self.assertEqual(f.state()["pulls"], state["pulls"])
        self.assertEqual(f.state()["releases"], state["releases"])
        calls = f.state().get("calls", [])[len(state.get("calls", [])):]
        self.assertTrue(all(call[0] == "api" and "--method" not in call for call in calls))
        return result, outputs

    def assert_selected(self, outputs, sha):
        self.assertEqual(outputs["release-commit"], sha)
        self.assertEqual(json.loads(outputs["releases"]), RELEASES)
        self.assertEqual(outputs["ready"], "true")

    def test_no_batch_is_a_noop_for_each_operation(self):
        for operation, event in ((None, "push"), (None, "workflow_dispatch"),
                                 ("manage-pr", "push"), ("select", "push"),
                                 ("retry-publication", "workflow_dispatch")):
            with self.subTest(operation=operation, event=event):
                _, out = self.run_release(operation, event=event)
                self.assertEqual(out["release-commit"], "")
                self.assertEqual(out["releases"], "[]")
                self.assertEqual(out["publication-plan"], "[]")
                self.assertEqual(out["publication-needed"], "false")
                self.assertEqual(out["published"], "false")
                self.assertEqual(out["published-packages"], "[]")
                self.assertEqual(out["has-notes"], "false")
                self.assertEqual(out["pr-number"], "")
                self.assertEqual(out["pr-operation"], "none")
        self.assertEqual(self.f.state()["releases"], [])
        self.assertEqual(self.f.git("ls-remote", "--tags", "origin"), "")

    def test_default_push_creates_pr_without_a_saved_batch(self):
        self.f.note("Literal $(touch injected) and `touch injected`")
        _, out = self.run_release()
        self.assertEqual(out["has-notes"], "true")
        self.assertEqual(out["pr-operation"], "created")
        self.assertEqual(out["pr-number"], "1")
        self.assertEqual(out["release-commit"], "")
        self.assertEqual(out["ready"], "true")
        self.assertFalse((self.f.repo / "injected").exists())
        self.assertEqual(self.f.state()["releases"], [])

    def test_manual_auto_and_explicit_manage_pr_do_not_publish_pending_batch(self):
        self.f.release()
        self.f.note("Next release", "next")
        for operation, event in ((None, "workflow_dispatch"), ("manage-pr", "push")):
            with self.subTest(operation=operation):
                _, out = self.run_release(operation, event=event)
                self.assertEqual(out["has-notes"], "true")
                self.assertIn(out["pr-operation"], ("created", "updated"))
                self.assertEqual(out["release-commit"], "")
                self.assertEqual(out["releases"], "[]")
                self.assertEqual(out["publication-plan"], "[]")
                self.assertEqual(out["published"], "false")
        self.assertEqual(self.f.state()["releases"], [])
        self.assertEqual(self.f.git("ls-remote", "--tags", "origin"), "")

    def test_default_push_manages_next_pr_and_publishes_latest_verified_batch(self):
        sha = self.f.release()
        self.f.later_commit()
        self.f.note("Next release", "next")
        pending = (self.f.repo / ".changenote/next.md").read_bytes()
        _, out = self.run_release()
        self.assert_selected(out, sha)
        self.assertEqual(out["publication-needed"], "true")
        self.assertEqual(json.loads(out["publication-plan"])[0]["action"], "create-tag-and-release")
        self.assertEqual(out["has-notes"], "true")
        self.assertEqual(out["pr-operation"], "created")
        self.assertEqual(out["pr-number"], "2")
        self.assertEqual(out["published"], "true")
        self.assertEqual(json.loads(out["published-packages"]), [{"name": "pkg", "version": "1.0.1"}])
        self.assertEqual((self.f.repo / ".changenote/next.md").read_bytes(), pending)
        self.assertEqual(self.f.state()["releases"][0]["tag_name"], "v1.0.1")

    def test_select_uses_remote_batch_without_changing_dirty_stale_source(self):
        sha = self.f.release()
        self.f.later_commit()
        self.f.git("reset", "--hard", sha + "^")
        (self.f.repo / "VERSION").write_text("dirty source\n")
        (self.f.repo / "untracked").write_text("keep")
        self.f.set_state(no_write=True, omit_permissions=True)
        _, out = self.select_readonly()
        self.assert_selected(out, sha)
        self.assertEqual(out["publication-needed"], "true")
        self.assertEqual(out["published"], "false")
        self.assertEqual(out["pr-operation"], "none")

    def test_completed_batch_is_selected_only_for_explicit_manual_retry(self):
        sha = self.f.release()
        self.f.action("publish")
        for event, retry, selected in (("push", "false", False),
                                       ("workflow_dispatch", "false", False),
                                       ("workflow_dispatch", "true", True)):
            with self.subTest(event=event, retry=retry):
                _, out = self.select_readonly(event=event, retry=retry)
                self.assertEqual(out["publication-needed"], "false")
                self.assertEqual(out["ready"], "true")
                self.assertEqual(json.loads(out["publication-plan"])[0]["action"], "none")
                self.assertEqual(out["release-commit"], sha if selected else "")
                self.assertEqual(json.loads(out["releases"]), RELEASES if selected else [])
        _, out = self.run_release("retry-publication", event="workflow_dispatch")
        self.assert_selected(out, sha)
        self.assertEqual(out["published"], "false")
        self.assertEqual(out["published-packages"], "[]")
        self.assertEqual(out["pr-operation"], "none")
        _, out = self.run_release()
        self.assertEqual(out["release-commit"], "")
        self.assertEqual(out["published"], "false")

    def test_manual_retry_finishes_partial_publication_without_managing_pr(self):
        sha = self.f.release()
        self.f.set_state(release_fail="v1.0.1")
        result, out = self.run_release("retry-publication", event="workflow_dispatch", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(out["ready"], "false")
        self.assertEqual(self.f.state()["releases"], [])
        self.assertIn(sha, self.f.git("ls-remote", "--tags", "origin"))
        self.f.set_state(release_fail=None)
        _, out = self.run_release("retry-publication", event="workflow_dispatch")
        self.assert_selected(out, sha)
        self.assertEqual(json.loads(out["publication-plan"])[0]["action"], "create-release")
        self.assertEqual(out["published"], "true")
        self.assertEqual(out["pr-operation"], "none")
        self.assertEqual(len(self.f.state()["pulls"]), 1)

    def test_publish_uses_full_expected_commit_and_rejects_stale_selection(self):
        sha = self.f.release()
        _, selected = self.select_readonly()
        self.assert_selected(selected, sha)
        self.f.note("Second batch", "second")
        self.f.cli("version")
        latest = self.f.commit("second release")
        self.f.git("push", "origin", "main")
        result, out = self.run_release("publish", expected=sha, ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("changed after preview", result.stderr)
        self.assertEqual(out["ready"], "false")
        self.assertEqual(out["release-commit"], "")
        self.assertEqual(self.f.state()["releases"], [])
        self.assertEqual(self.f.git("ls-remote", "--tags", "origin"), "")
        _, out = self.run_release("publish", expected=latest)
        self.assertEqual(out["release-commit"], latest)
        self.assertEqual(json.loads(out["releases"]), [
            {"package": "pkg", "version": "1.0.2", "tag": "v1.0.2"},
        ])
        self.assertEqual(out["published"], "true")
        self.assertEqual(out["ready"], "true")
        self.assertEqual(out["pr-operation"], "none")

    def test_auto_still_publishes_when_pr_management_fails(self):
        sha = self.f.release()
        self.f.note("Next release", "next")
        self.f.set_state(pr_fail=True)
        result, out = self.run_release(ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("PR permission denied", result.stderr)
        self.assert_selected(out, sha)
        self.assertEqual(out["published"], "true")
        self.assertEqual(self.f.state()["releases"][0]["tag_name"], "v1.0.1")

    def test_conflict_and_invalid_release_tree_fail_before_remote_writes(self):
        self.f.release()
        self.f.set_state(releases=[{"tag_name": "v1.0.1", "draft": False}])
        result, out = self.select_readonly(ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(out["ready"], "false")
        self.assertEqual(out["published"], "false")
        self.f.set_state(releases=[])
        (self.f.repo / "unrelated.txt").write_text("not a generated release output")
        self.f.git("add", "--", "unrelated.txt")
        self.f.git("commit", "--amend", "--no-edit")
        self.f.git("push", "--force", "origin", "main")
        result, out = self.select_readonly(ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("generated release tree", result.stderr)
        self.assertEqual(out["ready"], "false")
        self.assertEqual(out["release-commit"], "")
        self.assertEqual(out["releases"], "[]")

    def test_selected_rust_toolchain_is_enforced_during_replay(self):
        sha = self.f.release()
        calls = self.f.top / "toolchain-calls"
        binary = self.f.bin / "changenote"
        binary.unlink()
        binary.write_text(
            '#!/usr/bin/env bash\nprintf "%s\\n" "$RUSTUP_TOOLCHAIN" >> "$TOOLCHAIN_CALLS"\nexec "$REAL_CLI" "$@"\n'
        )
        binary.chmod(0o755)
        self.f.env.update(TOOLCHAIN_CALLS=str(calls), REAL_CLI=str(test_actions.CLI),
                          INPUT_RUST_TOOLCHAIN="stable", RUSTUP_TOOLCHAIN="not-selected-toolchain")
        _, outputs = self.run_release("publish", expected=sha)
        self.assert_selected(outputs, sha)
        self.assertEqual(set(calls.read_text().splitlines()), {"stable"})

    def test_invalid_inputs_and_unsafe_events_fail_before_api_calls(self):
        baseline = self.f.env.copy()
        cases = [
            {"INPUT_OPERATION": "unknown"},
            {"GITHUB_EVENT_NAME": "pull_request"},
            {"GITHUB_EVENT_NAME": "pull_request_target"},
            {"INPUT_OPERATION": "retry-publication"},
            {"INPUT_RETRY": "yes"},
            {"INPUT_OPERATION": "select", "INPUT_RETRY": "true"},
            {"GITHUB_EVENT_NAME": "workflow_dispatch", "INPUT_RETRY": "true"},
            {"INPUT_OPERATION": "publish"},
            {"INPUT_OPERATION": "publish", "INPUT_EXPECTED_RELEASE_COMMIT": "a" * 39},
            {"INPUT_OPERATION": "publish", "INPUT_EXPECTED_RELEASE_COMMIT": "g" * 40},
            {"INPUT_EXPECTED_RELEASE_COMMIT": "a" * 40},
            {"INPUT_NPM_VERSION": "latest"},
            {"INPUT_NPM_VERSION": "1.2.3; touch injected"},
            {"INPUT_RUST_TOOLCHAIN": "stable; touch injected"},
        ]
        for values in cases:
            with self.subTest(values=values):
                self.f.env = {**baseline, "GITHUB_EVENT_NAME": "push", **values}
                result, out = self.f.action("release", ok=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(out, {})
        self.f.env = dict(baseline, GITHUB_EVENT_NAME="push")
        for payload in ({"forced": True, "deleted": False},
                        {"forced": False, "deleted": True}, {},
                        {"forced": "false", "deleted": False}):
            with self.subTest(payload=payload):
                self.event.write_text(json.dumps(payload))
                result, out = self.f.action("release", ok=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(out, {})
        self.assertEqual(self.f.state().get("calls", []), [])
        self.assertFalse((self.f.repo / "injected").exists())


if __name__ == "__main__":
    unittest.main()
