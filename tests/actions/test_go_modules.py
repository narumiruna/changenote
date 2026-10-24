"""Go consumer release staging and replay with local Git and the GitHub mock."""

import json
import unittest

import test_actions


class GoModules(unittest.TestCase):
    def setUp(self):
        self.f = test_actions.Actions("test_no_batch")
        self.f.setUp()
        self.addCleanup(self.f.doCleanups)
        f = self.f
        (f.repo / "sdk/v2").mkdir(parents=True)
        (f.repo / "sdk/v2/VERSION").write_text("2.2.0\n")
        self.sources = {
            "go.mod": b"module example.com/repo\n\ngo 1.24.0\n",
            "go.sum": b"",
            "module.go": b"package module\n",
            "go.work": b"go 1.24.0\n\nuse (\n .\n ./sdk/v2\n)\n",
            "sdk/v2/go.mod": b"module example.com/repo/sdk/v2\n\ngo 1.24.0\n",
            "sdk/v2/go.sum": b"",
            "sdk/v2/module.go": b"package module\n",
        }
        for path, content in self.sources.items():
            (f.repo / path).write_bytes(content)
        (f.repo / ".changenote/config.json").write_text(json.dumps({
            "packages": {
                "pkg": {"path": ".", "version_file": "VERSION", "tag_prefix": "v"},
                "sdk": {"path": "sdk/v2", "version_file": "VERSION", "tag_prefix": "sdk/v"},
            }
        }))
        # Fail explicitly if a release operation tries to run Go.
        (f.bin / "go").write_text("#!/bin/sh\nexit 99\n")
        (f.bin / "go").chmod(0o755)
        f.commit("Go consumer modules")
        f.git("push", "origin", "main")
        f.cli("add", "both", "--patch", "pkg", "--patch", "sdk", "-m", "Fix both modules")
        f.commit("Go release note")
        f.git("push", "origin", "main")
        self.outputs = {
            "VERSION", "CHANGELOG.md", "sdk/v2/VERSION", "sdk/v2/CHANGELOG.md",
            ".changenote/releases.json", ".changenote/both.md",
        }

    def test_staging_excludes_unrelated_go_changes(self):
        f = self.f
        plan = f.top / "plan.json"
        plan.write_text(f.cli("status"))
        f.cli("version")
        (f.repo / "module.go").write_text("package module\n// unrelated\n")
        (f.repo / "sdk/v2/go.mod").write_text("module example.com/repo/sdk/v2\n\ngo 1.25.0\n")
        (f.repo / "untracked.go").write_text("package module\n")
        f.run_cmd([
            "bash", "-c", 'source "$1"; stage_release "$2"', "stage",
            str(test_actions.SOURCE / "scripts/action/common.sh"), str(plan),
        ])
        self.assertEqual(set(f.git("diff", "--cached", "--name-only").splitlines()), self.outputs)
        self.assertEqual(set(f.git("diff", "--name-only").splitlines()), {"module.go", "sdk/v2/go.mod"})
        self.assertIn("untracked.go", f.git("ls-files", "--others", "--exclude-standard"))
        for path, content in self.sources.items():
            if path not in {"module.go", "sdk/v2/go.mod"}:
                self.assertEqual((f.repo / path).read_bytes(), content)

    def test_release_pr_preview_and_publication_replay_exact_go_tags(self):
        f = self.f
        _, outputs = f.action()
        self.assertEqual(outputs["pr-operation"], "created")
        f.git("fetch", "origin", test_actions.BRANCH)
        self.assertEqual(set(f.git("diff", "--name-only", "HEAD", "FETCH_HEAD").splitlines()), self.outputs)
        f.git("merge", "--squash", "FETCH_HEAD")
        release = f.commit("Go release")
        f.git("push", "origin", "main")
        for path, content in self.sources.items():
            self.assertEqual((f.repo / path).read_bytes(), content)
        self.assertEqual((f.repo / "VERSION").read_text(), "1.0.1\n")
        self.assertEqual((f.repo / "sdk/v2/VERSION").read_text(), "2.2.1\n")
        expected_tags = {"v1.0.1", "sdk/v2.2.1"}
        batch = json.loads((f.repo / ".changenote/releases.json").read_text())
        self.assertEqual({entry["tag"] for entry in batch}, expected_tags)
        f.later_commit()
        before_tags = f.git("ls-remote", "--tags", "origin")
        _, preview = f.action("preview")
        self.assertEqual(preview["ready"], "true")
        self.assertEqual(preview["release-commit"], release)
        self.assertEqual({entry["tag"] for entry in json.loads(preview["publication-plan"])}, expected_tags)
        self.assertEqual(f.git("ls-remote", "--tags", "origin"), before_tags)
        self.assertEqual(f.state()["releases"], [])
        f.action("publish")
        self.assertEqual({entry["tag_name"] for entry in f.state()["releases"]}, expected_tags)
        for tag in expected_tags:
            self.assertIn(release, f.git("ls-remote", "origin", f"refs/tags/{tag}^{{}}"))
        self.assertEqual(f.git("status", "--porcelain"), "")


if __name__ == "__main__":
    unittest.main()
