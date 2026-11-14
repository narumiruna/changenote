"""Root hooks use managed merges, local Git remotes, and dummy commands only."""

import copy
import json
from pathlib import Path
import shutil
import unittest

import test_actions


class CommandHooks(unittest.TestCase):
    def setUp(self):
        self.f = test_actions.Actions("test_no_batch")
        self.f.setUp()
        self.addCleanup(self.f.doCleanups)
        self.before = self.f.git("rev-parse", "HEAD")
        self.f.git("config", "gc.auto", "0")
        self.f.git("config", "maintenance.auto", "false")
        self.calls = self.f.top / "hook-calls"
        self.event = self.f.top / "event.json"
        self.f.env.update(
            INPUT_GITHUB_TOKEN="management-token",
            CALLS=str(self.calls),
            TMPDIR=str(self.f.top / "hook-temp"),
        )
        Path(self.f.env["TMPDIR"]).mkdir()

    def associate(self, sha):
        pulls = self.f.state()["pulls"]
        pr = copy.deepcopy(pulls[-1]) if pulls else {
            "base": {"ref": "main"},
            "head": {"ref": test_actions.BRANCH, "repo": {"full_name": "test/repo"}},
            "body": "<!-- changenote-release-pr -->",
        }
        pr.update(state="closed", merged_at="now", merge_commit_sha=sha)
        pr["base"]["repo"] = {"full_name": "test/repo"}
        associations = self.f.state().get("commit_pulls", {})
        associations[sha] = [[pr]]
        self.f.set_state(commit_pulls=associations)

    def merged(self, mode="squash"):
        sha = self.f.release(mode)
        self.associate(sha)
        return sha

    def run_hooks(self, publish="", release="", after=None, before=None,
                  event="push", fields=None, ok=True):
        sha = after or self.f.git("rev-parse", "HEAD")
        payload = dict(before=before or self.before, after=sha, forced=False, deleted=False)
        payload.update(fields or {})
        self.event.write_text(json.dumps(payload))
        self.f.env.update(
            GITHUB_EVENT_PATH=str(self.event), GITHUB_EVENT_NAME=event,
            GITHUB_REF="refs/heads/" + (self.f.env.get("INPUT_BASE_BRANCH") or self.f.state().get("default_branch", "main")), GITHUB_SHA=sha,
            INPUT_PUBLISH_SCRIPT=publish, INPUT_RELEASE_SCRIPT=release,
        )
        result, out = self.f.action("lifecycle", ok=ok)
        self.assertEqual(list(Path(self.f.env["TMPDIR"]).iterdir()), [])
        self.assertNotIn("management-token", result.stdout + result.stderr + json.dumps(out))
        return result, out

    def lines(self):
        return self.calls.read_text().splitlines() if self.calls.exists() else []

    def test_pr_creation_has_no_commands_and_preserves_pr_outputs(self):
        self.f.note("Literal $(touch injected) and `touch injected`")
        _, out = self.run_hooks('echo publish >> "$CALLS"', 'echo release >> "$CALLS"')
        self.assertEqual(out["pr-operation"], "created")
        self.assertEqual(out["pr-number"], "1")
        self.assertEqual(out["has-notes"], "true")
        self.assertEqual(out["release-commit"], "")
        self.assertEqual(out["releases"], "[]")
        self.assertEqual(self.lines(), [])
        self.assertFalse((self.f.repo / "injected").exists())

    def test_all_command_combinations_and_metadata_without_publication(self):
        sha = self.merged()
        for publish, release, expected in (
            ("", "", []),
            ('echo publish >> "$CALLS"', "", ["publish"]),
            ("", 'echo release >> "$CALLS"', ["release"]),
            ('echo publish >> "$CALLS"', 'echo release >> "$CALLS"', ["publish", "release"]),
            (" \n\t", "\t ", []),
        ):
            with self.subTest(expected=expected):
                self.calls.unlink(missing_ok=True)
                _, out = self.run_hooks(publish, release)
                self.assertEqual(self.lines(), expected)
                self.assertEqual(out["release-commit"], sha)
                self.assertEqual(json.loads(out["releases"]), [
                    {"package": "pkg", "version": "1.0.1", "tag": "v1.0.1"},
                ])
                self.assertEqual(self.f.git("ls-remote", "--tags", "origin"), "")
                self.assertEqual(self.f.state()["releases"], [])

    def test_commands_share_exact_commit_artifacts_origin_and_caller_environment(self):
        sha = self.merged()
        self.f.later_commit()
        self.f.env.update(GH_TOKEN="caller-token", CUSTOM_TOKEN="dummy-registry-token")
        tracked = {p: (self.f.repo / p).read_bytes()
                   for p in self.f.git("ls-files").splitlines()}
        head = self.f.git("rev-parse", "HEAD")
        identity = self.f.git("config", "--local", "--list")
        script = '''
        test "$(git rev-parse HEAD)" = "$CHANGENOTE_RELEASE_COMMIT"
        test "$CHANGENOTE_RELEASE_COMMIT" = "$EXPECTED_COMMIT"
        test "$(git remote get-url origin)" = "$EXPECTED_ORIGIN"
        test ! -e source.txt
        test "$GH_TOKEN" = caller-token
        test "$CUSTOM_TOKEN" = dummy-registry-token
        test -z "${INPUT_GITHUB_TOKEN+x}"
        test -z "$(git config --get-regexp 'credential|extraheader' || true)"
        printf '%s\n' "$CHANGENOTE_RELEASES" > built-artifact
        '''
        self.f.env.update(EXPECTED_COMMIT=sha, EXPECTED_ORIGIN=str(self.f.remote))
        self.run_hooks(script, 'cat built-artifact >> "$CALLS"', after=sha)
        self.assertEqual(json.loads(self.lines()[0])[0]["version"], "1.0.1")
        self.assertEqual(self.f.git("rev-parse", "HEAD"), head)
        self.assertEqual(self.f.git("config", "--local", "--list"), identity)
        self.assertEqual(self.f.git("diff", "--cached"), "")
        self.assertEqual(tracked, {p: (self.f.repo / p).read_bytes() for p in tracked})
        self.assertFalse((self.f.repo / "built-artifact").exists())

    def test_management_token_is_not_implicitly_available_to_commands(self):
        self.merged()
        self.f.env.pop("GH_TOKEN", None)
        self.run_hooks('test -z "${GH_TOKEN+x}"; test -z "${INPUT_GITHUB_TOKEN+x}"; '
                       'test -z "${INPUT_PUBLISH_SCRIPT+x}"; test -z "${INPUT_RELEASE_SCRIPT+x}"')

    def test_failure_stops_order_without_rollback_or_automatic_retry(self):
        self.merged()
        for command in ('echo upload >> "$CALLS"; exit 7',
                        'echo upload >> "$CALLS"; false | true; echo unexpected >> "$CALLS"'):
            with self.subTest(command=command):
                self.calls.unlink(missing_ok=True)
                result, _ = self.run_hooks(command, 'echo release >> "$CALLS"', ok=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.lines(), ["upload"])
        self.calls.unlink()
        result, _ = self.run_hooks('echo upload >> "$CALLS"', 'exit 9', ok=False)
        self.assertEqual(result.returncode, 9)
        self.assertEqual(self.lines(), ["upload"])
        self.assertEqual(self.f.state()["releases"], [])

    def test_changed_head_prevents_the_next_command(self):
        self.merged()
        result, _ = self.run_hooks('git commit --allow-empty -m hook-commit',
                                    'echo unexpected >> "$CALLS"', ok=False)
        self.assertIn("changed the selected release HEAD", result.stderr)
        self.assertEqual(self.lines(), [])

    def test_original_run_retries_even_when_github_publication_is_complete(self):
        sha = self.merged()
        self.f.action("publish")
        for _ in range(2):
            self.run_hooks('echo upload >> "$CALLS"', 'echo release >> "$CALLS"', after=sha)
        self.assertEqual(self.lines(), ["upload", "release", "upload", "release"])
        self.assertEqual(len(self.f.state()["releases"]), 1)

    def test_unrelated_push_and_manual_dispatch_never_run_hooks(self):
        sha = self.merged()
        self.f.later_commit()
        _, out = self.run_hooks('echo unexpected >> "$CALLS"', before=sha)
        self.assertEqual(out["release-commit"], "")
        _, out = self.run_hooks('echo unexpected >> "$CALLS"', event="workflow_dispatch",
                                fields={"inputs": {"operation": "retry-publication"}})
        self.assertEqual(out["release-commit"], "")
        self.assertEqual(self.lines(), [])

    def test_merge_squash_rebase_and_missing_release_api(self):
        for mode in ("merge", "squash", "rebase"):
            with self.subTest(mode=mode):
                if mode != "merge":
                    self.doCleanups()
                    self.setUp()
                sha = self.merged(mode)
                self.f.set_state(releases_api_fail=True)
                _, out = self.run_hooks('echo publish >> "$CALLS"')
                self.assertEqual(out["release-commit"], sha)
                self.assertEqual(self.lines(), ["publish"])

    def test_disabled_changelog_does_not_block_hooks_or_metadata(self):
        config = self.f.repo / ".changenote/config.json"
        value = json.loads(config.read_text())
        value["changelog"] = {"enabled": False}
        config.write_text(json.dumps(value))
        self.f.commit("disable changelog")
        self.f.git("push", "origin", "main")
        sha = self.merged()
        _, out = self.run_hooks('test ! -e CHANGELOG.md; echo publish >> "$CALLS"')
        self.assertEqual(out["release-commit"], sha)
        self.assertEqual(self.lines(), ["publish"])
        result, _ = self.f.action("preview", ok=False)
        self.assertNotEqual(result.returncode, 0)

    def test_pr_failure_does_not_suppress_valid_merged_batch(self):
        sha = self.merged()
        self.f.note("Next release", "next")
        self.f.set_state(pr_fail=True)
        result, out = self.run_hooks('echo publish >> "$CALLS"', after=sha, ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.lines(), ["publish"])
        self.assertEqual(out["release-commit"], sha)

    def test_invalid_authorization_and_unsafe_events_fail_before_commands(self):
        sha = self.merged()
        for event, fields in (
            ("pull_request", {}), ("pull_request_target", {}),
            ("push", {"forced": True}), ("push", {"deleted": True}),
            ("push", {"before": "a" * 40}), ("push", {"before": "0" * 40}),
        ):
            with self.subTest(event=event, fields=fields):
                result, _ = self.run_hooks('echo unexpected >> "$CALLS"', event=event,
                                            fields=fields, ok=False)
                self.assertNotEqual(result.returncode, 0)
        self.f.set_state(commit_pulls={sha: [[]]})
        result, _ = self.run_hooks('echo unexpected >> "$CALLS"', ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.f.set_state(api_fail=True)
        result, _ = self.run_hooks('echo unexpected >> "$CALLS"', ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.lines(), [])

    def test_mismatched_event_sha_is_rejected_before_commands(self):
        self.merged()
        result, _ = self.run_hooks('echo unexpected >> "$CALLS"',
                                    fields={"after": self.before}, ok=False)
        self.assertIn("does not match the triggering commit", result.stderr)
        self.assertEqual(self.lines(), [])

    def test_invalid_tree_is_not_execution_authority(self):
        self.merged()
        batch = self.f.repo / ".changenote/releases.json"
        batch.write_text(batch.read_text() + "\n")
        (self.f.repo / "injected.txt").write_text("not a release output")
        sha = self.f.commit("tampered tree")
        self.f.git("push", "origin", "main")
        self.associate(sha)
        result, _ = self.run_hooks('echo unexpected >> "$CALLS"', ok=False)
        self.assertIn("parent release plan", result.stderr)
        self.assertEqual(self.lines(), [])

    def test_complete_tree_replay_rejects_unrelated_edits(self):
        self.merged()
        (self.f.repo / "unrelated").write_text("not a generated output")
        self.f.git("add", "unrelated")
        self.f.git("commit", "--amend", "--no-edit")
        sha = self.f.git("rev-parse", "HEAD")
        self.f.git("push", "--force", "origin", "main")
        self.associate(sha)
        result, _ = self.run_hooks('echo unexpected >> "$CALLS"', ok=False)
        self.assertIn("generated release tree", result.stderr)
        self.assertEqual(self.lines(), [])

    def test_note_and_pr_metadata_are_not_evaluated(self):
        marker = self.f.top / "injected"
        self.f.env.update(MARKER=str(marker), INPUT_PR_TITLE='$(touch "$MARKER")')
        self.f.note('Literal $(touch "$MARKER") and `touch "$MARKER"`', "literal")
        self.merged()
        self.run_hooks('echo publish >> "$CALLS"')
        self.assertFalse(marker.exists())
        self.assertEqual(self.lines(), ["publish"])

    def test_legacy_batch_cannot_execute_custom_commands(self):
        sha = self.f.legacy_release()
        sha = self.f.git("rev-parse", "HEAD")
        self.associate(sha)
        _, out = self.run_hooks()
        self.assertEqual(out["release-commit"], sha)
        result, _ = self.run_hooks('echo unexpected >> "$CALLS"', ok=False)
        self.assertIn("historical note-bearing", result.stderr)
        self.assertEqual(self.lines(), [])

    def test_newer_batch_does_not_replace_the_selected_commit(self):
        sha = self.merged()
        self.f.note("Second release", "second")
        self.f.action()
        self.f.git("fetch", "origin", test_actions.BRANCH)
        self.f.git("merge", "--squash", "FETCH_HEAD")
        self.f.commit("second release")
        self.f.git("push", "origin", "main")
        result, _ = self.run_hooks('echo unexpected >> "$CALLS"', after=sha, ok=False)
        self.assertIn("changed after routing", result.stderr)
        self.assertEqual(self.lines(), [])

    def test_new_batch_during_reconstruction_stops_before_commands(self):
        sha = self.merged()
        self.f.note("Second release", "second")
        self.f.action()
        self.f.git("fetch", "origin", test_actions.BRANCH)
        self.f.git("merge", "--squash", "FETCH_HEAD")
        newer = self.f.commit("second release")
        self.f.git("push", "origin", "main")
        self.f.run_cmd(["git", "--git-dir=" + str(self.f.remote),
                        "update-ref", "refs/heads/main", sha])
        real_git = shutil.which("git")
        counter = self.f.top / "fetch-count"
        counter.write_text("0")
        wrapper = self.f.bin / "git"
        wrapper.write_text('''#!/usr/bin/env bash
set -euo pipefail
case "$*" in
  *"fetch --no-tags "*" refs/heads/main")
    n=$(<"$FETCH_COUNT")
    n=$((n + 1))
    echo "$n" > "$FETCH_COUNT"
    if [[ $n == 2 ]]; then
      "$REAL_GIT" --git-dir="$HOOK_REMOTE" update-ref refs/heads/main "$NEWER_COMMIT"
    fi
    ;;
esac
exec "$REAL_GIT" "$@"
''')
        wrapper.chmod(0o755)
        self.f.env.update(REAL_GIT=real_git, FETCH_COUNT=str(counter),
                          HOOK_REMOTE=str(self.f.remote), NEWER_COMMIT=newer)
        result, _ = self.run_hooks('echo unexpected >> "$CALLS"', after=sha, ok=False)
        self.assertIn("changed before command execution", result.stderr)
        self.assertEqual(self.lines(), [])

    def test_metadata_covers_the_whole_batch_not_one_command_per_package(self):
        config = self.f.repo / ".changenote/config.json"
        value = json.loads(config.read_text())
        (self.f.repo / "other").mkdir()
        (self.f.repo / "other/VERSION").write_text("2.0.0\n")
        value["packages"]["other"] = {"path": "other", "version_file": "VERSION"}
        config.write_text(json.dumps(value))
        self.f.cli("add", "other", "--package", "other", "--bump", "patch", "--message", "Fix other")
        self.f.commit("other package")
        self.f.git("push", "origin", "main")
        self.merged()
        _, out = self.run_hooks('printf "%s\\n" "$CHANGENOTE_RELEASES" >> "$CALLS"')
        self.assertEqual(len(self.lines()), 1)
        self.assertEqual(len(json.loads(self.lines()[0])), 2)
        self.assertEqual(self.lines()[0], out["releases"])

    def test_non_main_base_uses_its_reserved_managed_branch(self):
        self.f.git("branch", "-m", "stable")
        self.f.git("push", "origin", "stable")
        self.f.env["INPUT_BASE_BRANCH"] = "stable"
        self.f.cli("add", "fix", "--package", "pkg", "--bump", "patch", "--message", "Fix")
        self.f.commit("note")
        self.f.git("push", "origin", "stable")
        self.f.action()
        self.f.git("fetch", "origin", "narumi/chore/changenote-release-stable")
        self.f.git("merge", "--squash", "FETCH_HEAD")
        sha = self.f.commit("stable release")
        self.f.git("push", "origin", "stable")
        self.associate(sha)
        _, out = self.run_hooks('echo publish >> "$CALLS"')
        self.assertEqual(out["release-commit"], sha)
        self.assertEqual(self.lines(), ["publish"])
        self.f.env.pop("INPUT_BASE_BRANCH")
        self.f.set_state(default_branch="stable")
        self.calls.unlink()
        _, out = self.run_hooks('echo publish >> "$CALLS"')
        self.assertEqual(out["release-commit"], sha)
        self.assertEqual(self.lines(), ["publish"])


if __name__ == "__main__":
    unittest.main()
