"""Exercise language-independent configuration with the real CLI and local GitHub mock."""
import json
import os
import unittest

import test_actions


class ConfigPolicies(unittest.TestCase):
    def setUp(self):
        self.f = test_actions.Actions("test_no_batch")
        self.f.setUp()
        self.addCleanup(self.f.doCleanups)

    def config(self, **settings):
        path = self.f.repo / ".changenote/config.json"
        value = json.loads(path.read_text())
        value.update(settings)
        path.write_text(json.dumps(value))
        self.f.commit("configure policies")

    def test_file_patterns_agree_with_cli_and_preserve_note_discovery(self):
        f = self.f
        self.config(changed_file_patterns=["src/**"])
        f.git("branch", "comparison")
        (f.repo / "docs").mkdir()
        (f.repo / "docs/guide.md").write_text("Docs")
        f.commit("docs only")
        f.env.update(INPUT_SINCE="comparison", INPUT_COVERAGE="package")
        _, outputs = f.action("check")
        self.assertEqual(json.loads(outputs["check-report"])["changed_packages"], [])
        (f.repo / "src").mkdir()
        (f.repo / "src/main.rs").write_text("Source")
        f.commit("source without note")
        result, outputs = f.action("check", ok=False)
        self.assertNotEqual(result.returncode, 0)
        expected = f.run_cmd([str(test_actions.CLI), "check", "--since", "comparison", "--coverage", "package"], ok=False)
        self.assertEqual(json.loads(outputs["check-report"]), json.loads(expected.stdout))
        f.cli("add", "source", "--patch", "pkg", "-m", "Fix source")
        f.commit("add excluded-path note")
        _, outputs = f.action("check")
        self.assertTrue(json.loads(outputs["check-report"])["passed"])

    def test_linked_baseline_keeps_inactive_member_and_replays(self):
        f = self.f
        (f.repo / "b").mkdir()
        (f.repo / "b/VERSION").write_text("2.4.9\n")
        config = json.loads((f.repo / ".changenote/config.json").read_text())
        config["packages"]["b"] = {"path": "b", "version_file": "VERSION"}
        self.config(packages=config["packages"], linked=[["pkg", "b"]])
        f.git("push", "origin", "main")
        release = f.release()
        self.assertEqual((f.repo / "b/VERSION").read_text(), "2.4.9\n")
        self.assertFalse((f.repo / "b/CHANGELOG.md").exists())
        self.assertIn("Linked version baseline: 2.4.9", f.state()["pulls"][-1]["body"])
        self.assertEqual(json.loads(f.git("show", release + ":.changenote/releases.json"))[0]["version"], "2.4.10")
        f.action("preview")
        _, result = f.action("publish")
        self.assertEqual(result["published"], "true")

    def test_fixed_globs_replay_historical_not_live_membership(self):
        f = self.f
        (f.repo / "b").mkdir()
        (f.repo / "b/VERSION").write_text("2.0.0\n")
        config = json.loads((f.repo / ".changenote/config.json").read_text())
        config["packages"]["b"] = {"path": "b", "version_file": "VERSION"}
        self.config(packages=config["packages"], fixed=[["*"]])
        f.git("push", "origin", "main")
        f.release()
        (f.repo / "c").mkdir()
        (f.repo / "c/VERSION").write_text("3.0.0\n")
        config["packages"]["c"] = {"path": "c", "version_file": "VERSION"}
        self.config(packages=config["packages"])
        f.git("push", "origin", "main")
        _, preview = f.action("preview")
        self.assertEqual(len(json.loads(preview["publication-plan"])), 2)
        _, published = f.action("publish")
        self.assertEqual(len(json.loads(published["published-packages"])), 2)
        self.assertFalse((f.repo / "c/CHANGELOG.md").exists())

    def test_ignore_keeps_paused_and_stale_notes_and_publishes_historical_batch(self):
        f = self.f
        (f.repo / "b").mkdir()
        (f.repo / "b/VERSION").write_text("1.0.0\n")
        config = json.loads((f.repo / ".changenote/config.json").read_text())
        config["packages"]["b"] = {"path": "b", "version_file": "VERSION"}
        self.config(packages=config["packages"], ignore=["b"])
        f.cli("add", "paused", "--minor", "b", "-m", "Paused change")
        f.commit("paused note")
        paused = (f.repo / ".changenote/paused.md").read_bytes()
        f.git("push", "origin", "main")
        f.note()
        f.action()
        f.git("fetch", "origin", test_actions.BRANCH)
        head = f.git("rev-parse", "FETCH_HEAD")
        f.cli("add", "newer", "--minor", "pkg", "-m", "Later active change")
        f.commit("note after release generation")
        newer = (f.repo / ".changenote/newer.md").read_bytes()
        f.git("push", "origin", "main")
        f.git("merge", "--squash", head)
        f.commit("merge stale release")
        f.git("push", "origin", "main")
        self.assertEqual((f.repo / ".changenote/paused.md").read_bytes(), paused)
        self.assertEqual((f.repo / ".changenote/newer.md").read_bytes(), newer)
        self.config(ignore=["pkg"])
        f.git("push", "origin", "main")
        _, preview = f.action("preview")
        self.assertEqual(preview["ready"], "true")
        _, result = f.action("publish")
        self.assertEqual(result["published"], "true")
        for body in (entry["body"] for entry in f.state()["releases"]):
            self.assertNotIn("Paused change", body)
            self.assertNotIn("Later active change", body)
        self.assertEqual((f.repo / ".changenote/paused.md").read_bytes(), paused)
        self.assertEqual((f.repo / ".changenote/newer.md").read_bytes(), newer)
        self.assertFalse((f.repo / "b/CHANGELOG.md").exists())

    def test_native_template_matches_pr_and_verified_release_without_execution(self):
        f = self.f
        sentinel = f.top / "template-command-ran"
        template = f"$(touch {sentinel}) {{package}}@{{version}} [{{bump}}]\n\n{{summary}}"
        self.config(changelog={"note_template": template})
        f.git("push", "origin", "main")
        f.release()
        expected = f"$(touch {sentinel}) pkg@1.0.1 [patch]"
        self.assertIn(expected, (f.repo / "CHANGELOG.md").read_text())
        self.assertIn(expected, f.state()["pulls"][-1]["body"])
        f.action("publish")
        self.assertIn(expected, f.state()["releases"][-1]["body"])
        self.assertFalse(sentinel.exists())

    def test_disabled_changelog_stages_no_file_and_blocks_historical_publication(self):
        for existing in (False, True):
            with self.subTest(existing=existing):
                self.doCleanups()
                self.setUp()
                f = self.f
                if existing:
                    (f.repo / "CHANGELOG.md").write_text("## 1.0.1\n\nStale body\n")
                self.config(changelog={"enabled": False})
                f.git("push", "origin", "main")
                release = f.release()
                self.assertEqual((f.repo / "CHANGELOG.md").exists(), existing)
                changed = f.git("diff-tree", "--no-commit-id", "--name-only", "-r", release)
                self.assertNotIn("CHANGELOG.md", changed)
                # Live settings cannot enable bodies for a historically disabled release.
                self.config(changelog={"enabled": True})
                f.git("push", "origin", "main")
                for action in ("preview", "publish"):
                    result, _ = f.action(action, ok=False)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("disabled", result.stderr)
                    self.assertEqual(f.state()["releases"], [])
                    self.assertEqual(f.git("--git-dir=" + str(f.remote), "tag"), "")

    def test_later_disabled_output_does_not_block_an_older_enabled_batch(self):
        f = self.f
        f.release()
        self.config(changelog={"enabled": False})
        f.git("push", "origin", "main")
        _, preview = f.action("preview")
        self.assertEqual(preview["ready"], "true")
        _, published = f.action("publish")
        self.assertEqual(published["published"], "true")

    @unittest.skipUnless(os.name == "posix", "non-UTF-8 POSIX note paths")
    def test_release_pr_without_linked_groups_retains_non_utf8_note_support(self):
        f = self.f
        f.note()
        target = f.repo / os.fsdecode(b".changenote/note-\xff.md")
        (f.repo / ".changenote/fix.md").rename(target)
        f.commit("rename note")
        f.git("push", "origin", "main")
        _, outputs = f.action()
        self.assertEqual(outputs["pr-operation"], "created")

    def test_base_branch_does_not_retarget_release_or_explicit_check(self):
        f = self.f
        self.config(base_branch="unavailable-comparison")
        f.env.update(INPUT_SINCE="HEAD", INPUT_COVERAGE="package")
        _, outputs = f.action("check")
        self.assertEqual(outputs["passed"], "true")
        f.git("push", "origin", "main")
        f.release()
        _, preview = f.action("preview")
        self.assertEqual(preview["ready"], "true")
        _, published = f.action("publish")
        self.assertEqual(published["published"], "true")
