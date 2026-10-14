"""Read-only publication preview tests; run on Linux with the real Rust CLI."""

import json
import os
from pathlib import Path
import shutil
import unittest

import test_actions


def snapshot(root):
    return {
        str(p.relative_to(root)): ("link", os.readlink(p))
        if p.is_symlink()
        else p.read_bytes()
        for p in root.rglob("*")
        if p.is_file() or p.is_symlink()
    }


class Preview(unittest.TestCase):
    def setUp(self):
        self.f = test_actions.Actions("test_no_batch")
        self.f.setUp()
        self.addCleanup(self.f.doCleanups)
        # Prevent unrelated background Git maintenance from changing snapshots.
        self.f.git("config", "gc.auto", "0")
        self.f.git("config", "maintenance.auto", "false")
        self.f.env["TMPDIR"] = str(self.f.top / "preview-temp")
        Path(self.f.env["TMPDIR"]).mkdir()

    def preview(self, ok=True):
        f = self.f
        before = snapshot(f.repo)
        remote_before = snapshot(f.remote)
        state = f.state()
        real_git = shutil.which("git", path=os.environ["PATH"])
        audit = f.top / "git-audit"
        wrapper = f.bin / "git"
        wrapper.write_text(
            '#!/usr/bin/env bash\nprintf "%s\\n" "$*" >> "$PREVIEW_GIT_AUDIT"\nfor arg in "$@"; do\n  case "$arg" in tag|push|commit|update-ref) echo "Forbidden preview write: $arg" >&2; exit 90;; esac\ndone\nexec "$PREVIEW_REAL_GIT" "$@"\n'
        )
        wrapper.chmod(0o755)
        f.env.update(PREVIEW_REAL_GIT=real_git, PREVIEW_GIT_AUDIT=str(audit))
        try:
            result, outputs = f.action("preview", ok=ok)
        finally:
            wrapper.unlink()
        self.assertEqual(snapshot(f.repo), before)
        self.assertEqual(snapshot(f.remote), remote_before)
        self.assertEqual(f.state()["pulls"], state["pulls"])
        self.assertEqual(f.state()["releases"], state["releases"])
        calls = f.state().get("calls", [])[len(state.get("calls", [])) :]
        self.assertTrue(
            all(call[0] == "api" and "--method" not in call for call in calls)
        )
        self.assertEqual(list(Path(f.env["TMPDIR"]).iterdir()), [])
        self.assertNotIn(
            "test-token", result.stdout + result.stderr + json.dumps(outputs)
        )
        return result, outputs

    def test_no_batch_and_dirty_source_are_unchanged(self):
        (self.f.repo / "untracked").write_text("keep")
        _, out = self.preview()
        self.assertEqual(
            out, {"ready": "true", "release-commit": "", "publication-plan": "[]"}
        )

    def test_current_and_legacy_after_later_commit(self):
        for legacy in (False, True):
            with self.subTest(legacy=legacy):
                if legacy:
                    self.doCleanups()
                    self.setUp()
                sha = self.f.legacy_release() if legacy else self.f.release()
                self.f.later_commit()
                _, out = self.preview()
                self.assertEqual(out["ready"], "true")
                self.assertEqual(out["release-commit"], sha)
                self.assertEqual(
                    json.loads(out["publication-plan"]),
                    [
                        {
                            "name": "pkg",
                            "version": "1.0.1",
                            "tag": "v1.0.1",
                            "action": "create-tag-and-release",
                            "conflicts": [],
                        }
                    ],
                )

    def test_existing_matching_tag_and_complete_publication(self):
        sha = self.f.release()
        self.f.git("tag", "v1.0.1", sha)
        self.f.git("push", "origin", "refs/tags/v1.0.1")
        _, out = self.preview()
        self.assertEqual(
            json.loads(out["publication-plan"])[0]["action"], "create-release"
        )
        self.f.set_state(
            releases=[{"tag_name": "v1.0.1", "draft": False, "body": "keep"}]
        )
        _, out = self.preview()
        self.assertEqual(json.loads(out["publication-plan"])[0]["action"], "none")

    def test_conflicts_are_reported_not_published(self):
        sha = self.f.release()
        self.f.set_state(releases=[{"tag_name": "v1.0.1", "draft": False}])
        result, out = self.preview(ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(out["ready"], "false")
        self.assertIn(
            "has no remote tag", json.loads(out["publication-plan"])[0]["conflicts"][0]
        )
        self.f.git("tag", "v1.0.1", sha + "^")
        self.f.git("push", "origin", "refs/tags/v1.0.1")
        self.f.set_state(releases=[{"tag_name": "v1.0.1", "draft": True}])
        result, out = self.preview(ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(len(json.loads(out["publication-plan"])[0]["conflicts"]), 2)

    def fixed_release(self):
        f = self.f
        (f.repo / "b").mkdir()
        (f.repo / "b/VERSION").write_text("2.0.0\n")
        (f.repo / ".changenote/config.json").write_text(
            json.dumps(
                {
                    "packages": {
                        "pkg": {"path": ".", "version_file": "VERSION"},
                        "pkg-b": {"path": "b", "version_file": "VERSION"},
                    },
                    "fixed": [["pkg", "pkg-b"]],
                }
            )
        )
        f.commit("fixed config")
        sha = f.release()
        self.assertIn(
            "pkg-b@2.0.1\n\nReleased with its fixed version group; no direct changes.",
            f.state()["pulls"][0]["body"],
        )
        return sha

    def test_fixed_groups_partial_publication_and_historical_config(self):
        sha = self.fixed_release()
        self.f.git("tag", "changenote/pkg@v2.0.1", sha)
        self.f.git("push", "origin", "refs/tags/changenote/pkg@v2.0.1")
        self.f.set_state(
            releases=[{"tag_name": "changenote/pkg@v2.0.1", "draft": False}]
        )
        # Later config must not change reconstruction of the saved batch.
        path = self.f.repo / ".changenote/config.json"
        config = json.loads(path.read_text())
        config.pop("fixed")
        path.write_text(json.dumps(config))
        self.f.commit("remove group later")
        self.f.git("push", "origin", "main")
        _, out = self.preview()
        self.assertEqual(out["release-commit"], sha)
        self.assertEqual(
            [p["action"] for p in json.loads(out["publication-plan"])],
            ["none", "create-tag-and-release"],
        )
        _, published = self.f.action("publish")
        self.assertEqual(
            json.loads(published["published-packages"]),
            [{"name": "pkg-b", "version": "2.0.1"}],
        )
        self.assertEqual(
            self.f.state()["releases"][-1]["body"],
            "Released with its fixed version group; no direct changes.\n",
        )

    def test_all_entries_inspected_before_conflict_failure(self):
        sha = self.fixed_release()
        self.f.git("tag", "changenote/pkg@v2.0.1", sha + "^")
        self.f.git("push", "origin", "refs/tags/changenote/pkg@v2.0.1")
        _, out = self.preview(ok=False)
        self.assertEqual(
            [p["action"] for p in json.loads(out["publication-plan"])],
            ["conflict", "create-tag-and-release"],
        )

    def test_invalid_tree_and_active_fixed_legacy_are_rejected(self):
        self.fixed_release()
        (self.f.repo / "b/CHANGELOG.md").write_text("tampered")
        self.f.git("add", "--", "b/CHANGELOG.md")
        self.f.git("commit", "--amend", "--no-edit")
        self.f.git("push", "--force", "origin", "main")
        result, out = self.preview(ok=False)
        self.assertIn(
            "legacy output does not support active fixed groups", result.stderr
        )
        self.assertEqual(out["ready"], "false")
        self.assertEqual(out["publication-plan"], "[]")

    def test_legacy_mixed_profile_rejected(self):
        self.f.legacy_release()
        (self.f.repo / "CHANGELOG.md").write_text(
            "# Changelog\n\nIntroduction\n\n## 1.0.1\n\n### Patch Changes\n\n- Fix behavior\n\n## 1.0.0\n\nOld\n"
        )
        self.f.git("add", "--", "CHANGELOG.md")
        self.f.git("commit", "--amend", "--no-edit")
        self.f.git("push", "--force", "origin", "main")
        result, out = self.preview(ok=False)
        self.assertIn("generated release tree", result.stderr)
        self.assertEqual(out["ready"], "false")

    def test_api_and_fetch_failures_do_not_claim_readiness(self):
        self.f.set_state(api_fail=True)
        result, out = self.preview(ok=False)
        self.assertIn("API unavailable", result.stderr)
        self.assertEqual(out["ready"], "false")
        self.f.set_state(api_fail=False, remote=str(self.f.top / "absent.git"))
        result, out = self.preview(ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(out["ready"], "false")

    def test_release_api_failure_and_invalid_response_do_not_look_absent(self):
        self.f.release()
        self.f.set_state(releases_api_fail=True)
        result, out = self.preview(ok=False)
        self.assertIn("Release API unavailable", result.stderr)
        self.assertEqual(out["ready"], "false")
        self.assertEqual(out["publication-plan"], "[]")
        self.f.set_state(
            releases_api_fail=False, releases=[{"tag_name": "v1.0.1", "draft": "false"}]
        )
        result, out = self.preview(ok=False)
        self.assertIn("Invalid GitHub release response", result.stderr)
        self.assertEqual(out["ready"], "false")
        self.assertEqual(out["publication-plan"], "[]")

    def test_missing_tool_and_invalid_ref_fail_without_changes(self):
        original = self.f.env["PATH"]
        (self.f.bin / "changenote").unlink()
        for tool in ("bash", "uname", "jq"):
            (self.f.bin / tool).symlink_to(shutil.which(tool))
        # preview() supplies an audited git wrapper, but no changenote is available.
        self.f.env["PATH"] = str(self.f.bin)
        result, out = self.preview(ok=False)
        self.assertIn("Missing prerequisite: changenote", result.stderr)
        self.assertEqual(out["ready"], "false")
        self.f.env["PATH"] = original
        self.f.env["INPUT_BASE_BRANCH"] = "../bad"
        shutil.copy2(
            Path(__file__).resolve().parents[2] / "target/debug/changenote",
            self.f.bin / "changenote",
        )
        result, out = self.preview(ok=False)
        self.assertIn("Invalid base branch", result.stderr)
        self.assertEqual(out["ready"], "false")

    def test_deleted_malformed_and_root_batches(self):
        self.f.release()
        batch = self.f.repo / ".changenote/releases.json"
        for text in ("bad", "[]"):
            batch.write_text(text)
            self.f.commit("invalid batch")
            self.f.git("push", "origin", "main")
            result, out = self.preview(ok=False)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(out["ready"], "false")
        batch.unlink()
        self.f.commit("delete batch")
        self.f.git("push", "origin", "main")
        result, out = self.preview(ok=False)
        self.assertIn("Release batch was removed", result.stderr)
        self.assertEqual(out["ready"], "false")
        batch.write_text("[]")
        self.f.git("add", "--", ".")
        tree = self.f.git("write-tree")
        root = self.f.git("commit-tree", tree, "-m", "root batch")
        self.f.git("reset", "--hard", root)
        self.f.git("push", "--force", "origin", "main")
        result, out = self.preview(ok=False)
        self.assertIn("root commit", result.stderr)
        self.assertEqual(out["ready"], "false")

    def test_parent_batch_symlink_is_rejected(self):
        self.f.note()
        (self.f.repo / "prior.json").write_text("keep")
        batch = self.f.repo / ".changenote/releases.json"
        batch.symlink_to("../prior.json")
        self.f.commit("symlink parent")
        batch.unlink()
        self.f.cli("version")
        self.f.commit("release")
        self.f.git("push", "origin", "main")
        result, out = self.preview(ok=False)
        self.assertIn("cannot be symlinks", result.stderr)
        self.assertEqual(out["ready"], "false")


if __name__ == "__main__":
    unittest.main()
