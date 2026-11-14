"""Check CLI-owned release routing without Action scripts or remote writes."""

import copy
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[2] / "scripts/route-release.py"
spec = importlib.util.spec_from_file_location("release_route", SCRIPT)
routing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(routing)

BEFORE = "a" * 40
RELEASE = "b" * 40
AFTER = "c" * 40
REPOSITORY = "changenote/changenote"


class ReleaseRouting(unittest.TestCase):
    def setUp(self):
        self.event = {"before": BEFORE, "after": AFTER, "forced": False, "deleted": False}
        self.history = [AFTER, RELEASE, BEFORE]
        self.changed = {RELEASE}
        self.entry = f"100644 blob {'d' * 40}\t{routing.BATCH}"
        self.batch = [{"package": "changenote", "version": "1.2.3", "tag": "v1.2.3"}]
        self.pull = {
            "state": "closed",
            "merged_at": "now",
            "merge_commit_sha": RELEASE,
            "base": {"ref": "main", "repo": {"full_name": REPOSITORY}},
            "head": {
                "ref": "narumi/chore/changenote-release-main",
                "repo": {"full_name": REPOSITORY},
            },
            "body": routing.MARKER + "\nRelease notes",
        }
        self.pages = [[self.pull]]
        self.command = self.enterContext(patch.object(routing, "command", side_effect=self.read_command))

    def read_command(self, *args):
        if args[:3] == ("git", "cat-file", "-e"):
            self.assertIn(args[3], (BEFORE + "^{commit}", AFTER + "^{commit}"))
            return ""
        if args == ("git", "rev-list", "--first-parent", AFTER):
            return "\n".join(self.history)
        if args[:3] == ("git", "diff", "--name-only"):
            commit = args[4]
            self.assertEqual(args, ("git", "diff", "--name-only", commit + "^", commit, "--", routing.BATCH))
            return routing.BATCH if commit in self.changed else ""
        if args[:2] == ("git", "ls-tree"):
            self.assertEqual(args, ("git", "ls-tree", RELEASE, "--", routing.BATCH))
            return self.entry
        if args == ("git", "show", RELEASE + ":" + routing.BATCH):
            return json.dumps(self.batch)
        if args == ("gh", "api", "--paginate", "--slurp", f"repos/{REPOSITORY}/commits/{RELEASE}/pulls?per_page=100"):
            return json.dumps(self.pages)
        raise AssertionError(f"Unexpected command: {args}")

    def classify(self):
        return routing.route("push", self.event, REPOSITORY, "refs/heads/main")

    def test_managed_merge_with_later_ordinary_commit(self):
        self.assertEqual(self.classify(), {
            "manage-pr": "true", "publish": "true", "release-commit": RELEASE,
        })
        # Only read-only Git and GitHub commands are permitted by the mock.
        self.assertEqual(self.command.call_args.args[0], "gh")

    def test_ordinary_push_ignores_historical_batch(self):
        self.changed.clear()
        self.assertEqual(self.classify(), {
            "manage-pr": "true", "publish": "false", "release-commit": "",
        })
        self.assertFalse(any(call.args[0] == "gh" for call in self.command.call_args_list))

    def test_selects_latest_changed_batch_in_one_push(self):
        self.history.insert(2, "d" * 40)
        self.changed.add("d" * 40)
        self.assertEqual(self.classify()["release-commit"], RELEASE)
        self.assertFalse(any("d" * 40 in call.args for call in self.command.call_args_list))

    def test_paginated_association_ignores_unrelated_pull(self):
        unrelated = copy.deepcopy(self.pull)
        unrelated["head"]["ref"] = "other"
        self.pages = [[unrelated], [self.pull]]
        self.assertEqual(self.classify()["release-commit"], RELEASE)

    def test_all_managed_pr_identity_fields_are_required(self):
        variants = []
        for field, value in (
            ("body", ""),
            ("body", "prefix " + routing.MARKER),
            ("merged_at", None),
            ("merge_commit_sha", BEFORE),
            ("state", "open"),
        ):
            pull = copy.deepcopy(self.pull)
            pull[field] = value
            variants.append(pull)
        for side, field, value in (
            ("head", "ref", "other"),
            ("base", "ref", "other"),
            ("head", "repo", None),
            ("head", "repo", {"full_name": "other/repo"}),
            ("base", "repo", {"full_name": "other/repo"}),
        ):
            pull = copy.deepcopy(self.pull)
            pull[side][field] = value
            variants.append(pull)
        for pull in variants:
            with self.subTest(pull=pull):
                self.pages = [[pull]]
                with self.assertRaisesRegex(ValueError, "exactly one merged managed"):
                    self.classify()

    def test_missing_or_ambiguous_association_is_denied(self):
        for pages in ([[]], [[self.pull, self.pull]], []):
            with self.subTest(pages=pages):
                self.pages = pages
                with self.assertRaises(ValueError):
                    self.classify()

    def test_malformed_api_response_is_denied(self):
        for pages in ({"bad": "response"}, [None], [self.pull]):
            with self.subTest(pages=pages):
                self.pages = pages
                with self.assertRaises(ValueError):
                    self.classify()

    def test_missing_or_nonregular_batch_is_denied(self):
        for entry in ("", "120000 blob abc\t" + routing.BATCH, "100755 blob abc\t" + routing.BATCH):
            with self.subTest(entry=entry):
                self.entry = entry
                with self.assertRaisesRegex(ValueError, "regular file"):
                    self.classify()

    def test_invalid_batch_is_denied_before_api_calls(self):
        for batch in ([], {}, [1], None):
            with self.subTest(batch=batch):
                self.command.reset_mock()
                self.batch = batch
                with self.assertRaisesRegex(ValueError, "non-empty object array"):
                    self.classify()
                self.assertFalse(any(call.args[0] == "gh" for call in self.command.call_args_list))

    def test_forced_deleted_or_incomplete_push_is_denied(self):
        original = self.event.copy()
        for fields in ({"forced": True}, {"deleted": True}, {"forced": None}, {"deleted": None}):
            with self.subTest(fields=fields):
                self.event = dict(original, **fields)
                self.command.reset_mock()
                with self.assertRaisesRegex(ValueError, "Forced or deleted"):
                    self.classify()
                self.command.assert_not_called()

    def test_invalid_push_shas_are_denied(self):
        for field in ("before", "after"):
            for value in ("0" * 40, "A" * 40, "--all", "short"):
                with self.subTest(field=field, value=value):
                    self.event = dict(before=BEFORE, after=AFTER, forced=False, deleted=False)
                    self.event[field] = value
                    with self.assertRaisesRegex(ValueError, "two existing commit SHAs"):
                        self.classify()

    def test_non_first_parent_history_is_denied(self):
        self.history.remove(BEFORE)
        with self.assertRaisesRegex(ValueError, "first-parent history"):
            self.classify()

    def test_git_and_api_failures_do_not_authorize_publication(self):
        read_command = self.read_command
        for failing in ("git", "gh"):
            def command(*args):
                if args[0] == failing:
                    raise subprocess.CalledProcessError(1, args)
                return read_command(*args)

            with self.subTest(command=failing):
                self.command.side_effect = command
                with self.assertRaises(subprocess.CalledProcessError):
                    self.classify()

    def test_manual_operations_do_not_use_git_or_api(self):
        for operation, manage, publish in (
            ("manage-pr", "true", "false"),
            ("retry-publication", "false", "true"),
        ):
            with self.subTest(operation=operation):
                self.assertEqual(routing.route(
                    "workflow_dispatch", {"inputs": {"operation": operation}},
                    REPOSITORY, "refs/heads/main",
                ), {"manage-pr": manage, "publish": publish, "release-commit": ""})
        self.assertEqual(routing.route("workflow_dispatch", {}, REPOSITORY, "refs/heads/main")["manage-pr"], "true")
        with self.assertRaisesRegex(ValueError, "Unknown manual operation"):
            routing.route("workflow_dispatch", {"inputs": {"operation": "bad"}}, REPOSITORY, "refs/heads/main")
        self.command.assert_not_called()

    def test_non_main_never_authorizes_release_operations(self):
        for event in ("push", "workflow_dispatch"):
            self.assertEqual(routing.route(event, {}, REPOSITORY, "refs/heads/other"), {
                "manage-pr": "false", "publish": "false", "release-commit": "",
            })
        self.command.assert_not_called()
        with self.assertRaisesRegex(ValueError, "Unsupported event"):
            routing.route("pull_request", {}, REPOSITORY, "refs/heads/main")

    def test_preview_check_for_queued_runs_and_manual_no_batch(self):
        routing.check_preview(RELEASE, RELEASE, "true")
        routing.check_preview("", RELEASE, "true")
        routing.check_preview("", "", "true")
        for expected, actual, ready in (
            (RELEASE, AFTER, "true"),
            (RELEASE, "", "true"),
            ("", RELEASE, "false"),
        ):
            with self.subTest(expected=expected, actual=actual, ready=ready):
                with self.assertRaises(ValueError):
                    routing.check_preview(expected, actual, ready)

    def test_main_writes_route_outputs(self):
        with tempfile.TemporaryDirectory() as directory:
            event = Path(directory) / "event.json"
            output = Path(directory) / "output"
            event.write_text(json.dumps(self.event))
            with patch.dict(os.environ, {
                "GITHUB_REPOSITORY": REPOSITORY,
                "GITHUB_EVENT_NAME": "push",
                "GITHUB_EVENT_PATH": str(event),
                "GITHUB_REF": "refs/heads/main",
                "GITHUB_OUTPUT": str(output),
            }), patch.object(sys, "argv", [str(SCRIPT)]):
                routing.main()
            self.assertEqual(dict(line.split("=", 1) for line in output.read_text().splitlines()), {
                "manage-pr": "true", "publish": "true", "release-commit": RELEASE,
            })

    def test_main_preview_check_is_read_only(self):
        with patch.dict(os.environ, {
            "EXPECTED_RELEASE_COMMIT": RELEASE,
            "PREVIEW_RELEASE_COMMIT": RELEASE,
            "PREVIEW_READY": "true",
        }), patch.object(sys, "argv", [str(SCRIPT), "check-preview"]):
            routing.main()
        self.command.assert_not_called()


if __name__ == "__main__":
    unittest.main()
