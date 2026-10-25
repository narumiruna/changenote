"""Release routing and publication guards use real Git and mocked GitHub APIs."""

import copy
import importlib.util
import json
import unittest
from unittest.mock import patch

import test_actions

SCRIPT = test_actions.SOURCE / "scripts/action/route-release.py"
spec = importlib.util.spec_from_file_location("release_route", SCRIPT)
routing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(routing)


class ReleaseRouting(unittest.TestCase):
    def setUp(self):
        self.fixture = test_actions.Actions("test_no_batch")
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.before = self.fixture.git("rev-parse", "HEAD")

    def classify(self, before=None, after=None, **event_fields):
        fixture = self.fixture
        event = dict(
            before=before or self.before,
            after=after or fixture.git("rev-parse", "HEAD"),
            forced=False,
            deleted=False,
        )
        event.update(event_fields)
        event_path = fixture.top / "event.json"
        event_path.write_text(json.dumps(event))
        output = fixture.top / "routing-output"
        output.write_text("")
        fixture.env.update(
            GITHUB_EVENT_PATH=str(event_path),
            GITHUB_EVENT_NAME="push",
            GITHUB_REF="refs/heads/main",
            GITHUB_OUTPUT=str(output),
        )
        refs_before = fixture.git("show-ref")
        status_before = fixture.git("status", "--porcelain")
        result = fixture.run_cmd(
            ["uv", "run", "--no-project", "python", str(SCRIPT)], ok=False
        )
        self.assertEqual(fixture.git("show-ref"), refs_before)
        self.assertEqual(fixture.git("status", "--porcelain"), status_before)
        values = dict(line.split("=", 1) for line in output.read_text().splitlines())
        return result, values

    def managed(self, release):
        pr = copy.deepcopy(self.fixture.state()["pulls"][-1])
        pr.update(state="closed", merged_at="now", merge_commit_sha=release)
        pr["base"]["repo"] = {"full_name": "test/repo"}
        return pr

    def associate(self, release, pulls=None):
        self.fixture.set_state(
            commit_pulls={
                release: [pulls if pulls is not None else [self.managed(release)]]
            }
        )

    def assert_denied(self, **kwargs):
        result, outputs = self.classify(**kwargs)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertEqual(outputs, {})
        self.assertEqual(self.fixture.state()["releases"], [])
        self.assertEqual(self.fixture.git("ls-remote", "--tags", "origin"), "")

    def test_merge_squash_rebase_with_later_commit(self):
        for mode in ("merge", "squash", "rebase"):
            with self.subTest(mode=mode):
                if mode != "merge":
                    self.fixture.doCleanups()
                    self.setUp()
                release = self.fixture.release(mode)
                self.associate(release)
                self.fixture.later_commit()
                result, values = self.classify()
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    values,
                    {"manage-pr": "true", "publish": "true", "release-commit": release},
                )

    def test_ordinary_push_after_historical_release_does_not_query_api(self):
        release = self.fixture.release()
        self.fixture.later_commit()
        self.fixture.set_state(api_fail=True)
        result, values = self.classify(before=release)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            values, {"manage-pr": "true", "publish": "false", "release-commit": ""}
        )

    def test_release_publication_is_independent_of_a_later_pr_only_push(self):
        release = self.fixture.release()
        self.associate(release)
        result, publication = self.classify(after=release)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(publication["publish"], "true")
        self.fixture.later_commit()
        result, later = self.classify(before=release)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(later, {
            "manage-pr": "true", "publish": "false", "release-commit": "",
        })
        # The earlier publication still uses its authorized immutable selection.
        _, outputs = self.publish_selected(publication["release-commit"])
        self.assertEqual(outputs["release-commit"], release)
        self.assertEqual(outputs["published"], "true")

    def test_latest_of_two_releases_in_one_push(self):
        self.fixture.release()
        self.fixture.note("second release", "second")
        self.fixture.action()
        self.fixture.git("fetch", "origin", test_actions.BRANCH)
        self.fixture.git("merge", "--squash", "FETCH_HEAD")
        latest = self.fixture.commit("second release merge")
        self.associate(latest)
        result, values = self.classify()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(values["release-commit"], latest)

    def test_paginated_association_ignores_unrelated_pr(self):
        release = self.fixture.release()
        managed = self.managed(release)
        unrelated = copy.deepcopy(managed)
        unrelated["head"]["ref"] = "other"
        self.fixture.set_state(commit_pulls={release: [[unrelated], [managed]]})
        result, values = self.classify()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(values["publish"], "true")

    def test_newer_remote_batch_blocks_stale_preview(self):
        release = self.fixture.release()
        self.fixture.note("second release", "second")
        self.fixture.action()
        self.fixture.git("fetch", "origin", test_actions.BRANCH)
        self.fixture.git("merge", "--squash", "FETCH_HEAD")
        newer = self.fixture.commit("second release merge")
        self.fixture.git("push", "origin", "main")
        with self.assertRaises(ValueError):
            routing.check_preview(release, newer, "true")
        result, outputs = self.publish_selected(release, ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(outputs["release-commit"], "")
        self.assertEqual(self.fixture.state()["releases"], [])
        self.assertEqual(self.fixture.git("ls-remote", "--tags", "origin"), "")

    def test_identity_failures_and_ambiguity(self):
        release = self.fixture.release()
        original = self.managed(release)
        variants = []
        for field, value in (
            ("body", ""),
            ("merged_at", None),
            ("merge_commit_sha", self.before),
            ("state", "open"),
        ):
            pr = copy.deepcopy(original)
            pr[field] = value
            pr["title"] = "chore(release): version packages"
            variants.append([pr])
        for side, field, value in (
            ("head", "ref", "other"),
            ("base", "ref", "other"),
            ("head", "repo", None),
            ("base", "repo", {"full_name": "other/repo"}),
        ):
            pr = copy.deepcopy(original)
            pr[side][field] = value
            variants.append([pr])
        variants.extend([[], [original, original]])
        for pulls in variants:
            with self.subTest(pulls=pulls):
                self.associate(release, pulls)
                self.assert_denied()

    def test_api_failure_and_malformed_response(self):
        release = self.fixture.release()
        self.associate(release)
        self.fixture.set_state(api_fail=True)
        self.assert_denied()
        self.fixture.set_state(
            api_fail=False, commit_pulls={release: {"bad": "response"}}
        )
        self.assert_denied()

    def test_invalid_deleted_and_symlink_batches(self):
        self.fixture.release()
        path = self.fixture.repo / routing.BATCH
        for value in ("bad json", "[]", "{}", "[1]", None, "symlink"):
            with self.subTest(value=value):
                if path.exists() or path.is_symlink():
                    path.unlink()
                if value == "symlink":
                    path.symlink_to("config.json")
                elif value is not None:
                    path.write_text(value)
                self.fixture.commit("invalid batch")
                self.assert_denied()

    def test_missing_history_forced_deleted_and_non_first_parent(self):
        self.fixture.later_commit()
        for fields in (
            {"forced": True},
            {"deleted": True},
            {"before": "a" * 40},
            {"before": "0" * 40},
            {"before": "--all"},
            {"before": self.fixture.git("rev-parse", "HEAD"), "after": self.before},
        ):
            with self.subTest(fields=fields):
                self.assert_denied(**fields)
        self.fixture.git("checkout", "-b", "side", self.before)
        (self.fixture.repo / "side.txt").write_text("side")
        side = self.fixture.commit("side")
        self.fixture.git("checkout", "main")
        self.fixture.git("merge", "--no-ff", "side", "-m", "merge side")
        self.assert_denied(before=side)

    def test_manual_operations_and_non_main(self):
        for operation, manage, publish in (
            ("manage-pr", "true", "false"),
            ("retry-publication", "false", "true"),
        ):
            with (
                self.subTest(operation=operation),
                patch.object(
                    routing,
                    "command",
                    side_effect=AssertionError("No API or Git calls expected"),
                ),
            ):
                self.assertEqual(
                    routing.route(
                        "workflow_dispatch",
                        {"inputs": {"operation": operation}},
                        "test/repo",
                        "refs/heads/main",
                    ),
                    {"manage-pr": manage, "publish": publish, "release-commit": ""},
                )
        self.assertEqual(
            routing.route("workflow_dispatch", {}, "test/repo", "refs/heads/main")[
                "manage-pr"
            ],
            "true",
        )
        self.assertEqual(
            routing.route("push", {}, "test/repo", "refs/heads/other")["publish"],
            "false",
        )
        with self.assertRaises(ValueError):
            routing.route(
                "workflow_dispatch",
                {"inputs": {"operation": "bad"}},
                "test/repo",
                "refs/heads/main",
            )

    def test_preview_check_for_queued_runs_and_manual_no_batch(self):
        routing.check_preview("old", "old", "true")
        routing.check_preview("", "latest", "true")
        routing.check_preview("", "", "true")
        for expected, actual, ready in (
            ("old", "new", "true"),
            ("old", "", "true"),
            ("", "latest", "false"),
        ):
            with self.assertRaises(ValueError):
                routing.check_preview(expected, actual, ready)

    def publish_selected(self, expected, ok=True):
        output = self.fixture.top / "publish-output"
        output.write_text("")
        self.fixture.env["GITHUB_OUTPUT"] = str(output)
        result = self.fixture.run_cmd(
            ["bash", str(test_actions.SOURCE / "scripts/action/publish.sh"), expected],
            ok=ok,
        )
        return result, dict(
            line.split("=", 1) for line in output.read_text().splitlines()
        )

    def test_publish_rechecks_after_preview_before_writes(self):
        release = self.fixture.release()
        result, outputs = self.publish_selected(self.before, ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("changed after preview", result.stderr)
        self.assertEqual(outputs["release-commit"], "")
        self.assertEqual(self.fixture.state()["releases"], [])
        self.assertEqual(self.fixture.git("ls-remote", "--tags", "origin"), "")
        self.fixture.later_commit()
        _, first = self.publish_selected(release)
        self.assertEqual(first["published"], "true")
        _, retry = self.publish_selected(release)
        self.assertEqual(retry["published"], "false")
        self.assertEqual(retry["release-commit"], release)

    def test_no_batch_guard_and_batch_appearing_after_preview(self):
        _, outputs = self.publish_selected("")
        self.assertEqual(outputs["release-commit"], "")
        self.fixture.release()
        result, _ = self.publish_selected("", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.fixture.state()["releases"], [])


if __name__ == "__main__":
    unittest.main()
