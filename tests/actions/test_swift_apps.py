"""Native xcconfig staging and release replay with local Git and the GitHub mock."""

import json
import shutil
import unittest

import test_actions


class SwiftApps(unittest.TestCase):
    def setUp(self):
        self.f = test_actions.Actions("test_no_batch")
        self.f.setUp()
        self.addCleanup(self.f.doCleanups)
        f = self.f
        self.versions = {
            "Configuration/Version.xcconfig": (
                b"// Release settings; example version 1.0.0\r\n"
                b"MARKETING_VERSION = 1.0.0 // public version\r\n"
                b"CURRENT_PROJECT_VERSION = 41\r\n"
                b"PRODUCT_BUNDLE_IDENTIFIER = com.example.App\r\n"
            ),
            "Configuration/Release.xcconfig": (
                b"// Distribution settings\n"
                b"\tMARKETING_VERSION = 1.0.0 // keep this comment\n"
                b"CURRENT_PROJECT_VERSION = 9083"
            ),
            "companion/VERSION": b"1.0.0\n",
            "companion/Configuration/App.xcconfig": (
                b"MARKETING_VERSION = 1.0.0 // companion version\n"
                b"CURRENT_PROJECT_VERSION = 17\n"
            ),
        }
        self.expected_versions = {
            path: content.replace(b"MARKETING_VERSION = 1.0.0", b"MARKETING_VERSION = 1.0.1")
            if path.endswith(".xcconfig") else b"1.0.1\n"
            for path, content in self.versions.items()
        }
        self.sources = {
            "App.xcodeproj/project.pbxproj": (
                b"// !$*UTF8*$!\n{\n"
                b"  buildSettings = { MARKETING_VERSION = 1.0.0; CURRENT_PROJECT_VERSION = 41; };\n"
                b"}\n"
            ),
            "project.yml": b"name: App\nsettings:\n  CURRENT_PROJECT_VERSION: 41\n",
            "Info.plist": (
                b'<?xml version="1.0" encoding="UTF-8"?>\n<plist version="1.0"><dict>\n'
                b"<key>CFBundleShortVersionString</key><string>$(MARKETING_VERSION)</string>\n"
                b"<key>CFBundleVersion</key><string>$(CURRENT_PROJECT_VERSION)</string>\n"
                b"</dict></plist>\n"
            ),
            "unrelated.txt": b"unrelated tracked content\n",
        }
        for path, content in {**self.versions, **self.sources}.items():
            target = f.repo / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
        (f.repo / ".changenote/config.json").write_text(json.dumps({
            "packages": {
                "pkg": {
                    "path": ".", "version_file": "Configuration/Version.xcconfig",
                    "version_outputs": ["Configuration/Release.xcconfig"], "tag_prefix": "app/v",
                },
                "companion": {
                    "path": "companion", "version_file": "VERSION",
                    "version_outputs": ["Configuration/App.xcconfig"], "tag_prefix": "companion/v",
                },
            }
        }))
        # Only the GitHub mock may use uv to run Python. Release operations
        # must not invoke Xcode, Swift, project generators, or package managers.
        f.env["MOCK_UV"] = shutil.which("uv")
        (f.bin / "gh").write_text(
            '#!/usr/bin/env bash\nexec "$MOCK_UV" run --no-project python "$MOCK_GH_SCRIPT" "$@"\n'
        )
        self.tool_calls = f.top / "unexpected-tool-calls"
        f.env["UNEXPECTED_TOOL_CALLS"] = str(self.tool_calls)
        for tool in ("xcodebuild", "xcrun", "swift", "xcodegen", "tuist", "pod", "carthage",
                     "npm", "npx", "pnpm", "yarn", "cargo", "uv", "go", "bundle"):
            executable = f.bin / tool
            executable.write_text(
                '#!/bin/sh\nprintf "%s\\n" "$0 $*" >> "$UNEXPECTED_TOOL_CALLS"\nexit 99\n'
            )
            executable.chmod(0o755)
        f.commit("Swift app configuration")
        f.git("push", "origin", "main")
        f.cli("add", "both", "--patch", "pkg", "--patch", "companion", "-m", "Fix both apps")
        f.commit("Swift app release note")
        f.git("push", "origin", "main")
        self.outputs = set(self.versions) | {
            "CHANGELOG.md", "companion/CHANGELOG.md",
            ".changenote/releases.json", ".changenote/both.md",
        }
        self.tags = {"app/v1.0.1", "companion/v1.0.1"}

    def tearDown(self):
        self.assertFalse(self.tool_calls.exists(), "Release operations invoked a build or package tool")

    def assert_version_bytes(self):
        for path, content in self.expected_versions.items():
            with self.subTest(path=path):
                self.assertEqual((self.f.repo / path).read_bytes(), content)

    def test_staging_includes_only_configured_release_outputs(self):
        f = self.f
        plan = f.top / "plan.json"
        plan.write_text(f.cli("status"))
        f.cli("version")
        self.assert_version_bytes()
        for path, content in self.sources.items():
            self.assertEqual((f.repo / path).read_bytes(), content)
            (f.repo / path).write_bytes(content + b"// unrelated dirty edit\n")
        (f.repo / "untracked.swift").write_text("// do not stage\n")
        f.run_cmd([
            "bash", "-c", 'source "$1"; stage_release "$2"', "stage",
            str(test_actions.SOURCE / "scripts/action/common.sh"), str(plan),
        ])
        self.assertEqual(set(f.git("diff", "--cached", "--name-only").splitlines()), self.outputs)
        self.assertEqual(set(f.git("diff", "--name-only").splitlines()), set(self.sources))
        self.assertEqual(f.git("ls-files", "--others", "--exclude-standard"), "untracked.swift")
        self.assert_version_bytes()

    def test_release_pr_later_preview_and_publication_replay_exact_bytes_and_tags(self):
        f = self.f
        _, outputs = f.action()
        self.assertEqual(outputs["pr-operation"], "created")
        f.git("fetch", "origin", test_actions.BRANCH)
        self.assertEqual(set(f.git("diff", "--name-only", "HEAD", "FETCH_HEAD").splitlines()), self.outputs)
        f.git("merge", "--squash", "FETCH_HEAD")
        release = f.commit("Swift app release")
        f.git("push", "origin", "main")
        self.assert_version_bytes()
        for path, content in self.sources.items():
            self.assertEqual((f.repo / path).read_bytes(), content)
        batch = json.loads((f.repo / ".changenote/releases.json").read_text())
        self.assertEqual({entry["tag"] for entry in batch}, self.tags)
        f.later_commit()
        before_refs = f.git("ls-remote", "origin")
        _, preview = f.action("preview")
        self.assertEqual(preview["ready"], "true")
        self.assertEqual(preview["release-commit"], release)
        self.assertEqual(json.loads(preview["publication-plan"]), [
            {"name": name, "version": "1.0.1", "tag": tag,
             "action": "create-tag-and-release", "conflicts": []}
            for name, tag in (("companion", "companion/v1.0.1"), ("pkg", "app/v1.0.1"))
        ])
        self.assertEqual(f.git("ls-remote", "origin"), before_refs)
        self.assertEqual(f.state()["releases"], [])
        _, published = f.action("publish")
        self.assertEqual(published["release-commit"], release)
        self.assertEqual(published["published"], "true")
        self.assertEqual({entry["tag_name"] for entry in f.state()["releases"]}, self.tags)
        for tag in self.tags:
            self.assertEqual(f.git("--git-dir=" + str(f.remote), "cat-file", "-t", tag), "tag")
            self.assertIn(release, f.git("ls-remote", "origin", f"refs/tags/{tag}^{{}}"))
        self.assert_version_bytes()
        self.assertEqual(f.git("status", "--porcelain"), "")

    def test_extra_generated_project_edit_blocks_preview_and_publication_before_writes(self):
        f = self.f
        f.cli("version")
        project = "App.xcodeproj/project.pbxproj"
        (f.repo / project).write_bytes(self.sources[project] + b"// custom generated project edit\n")
        f.commit("Swift release with an extra generated project edit")
        f.git("push", "origin", "main")
        before_refs = f.git("ls-remote", "origin")
        push_attempts = f.top / "unexpected-push"
        hook = f.remote / "hooks/pre-receive"
        hook.write_text(f"#!/bin/sh\nprintf 'push attempted\\n' >> '{push_attempts}'\nexit 99\n")
        hook.chmod(0o755)
        for action in ("preview", "publish"):
            with self.subTest(action=action):
                result, _ = f.action(action, ok=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("beyond the generated release tree", result.stderr)
                self.assertEqual(f.git("ls-remote", "origin"), before_refs)
                self.assertFalse(push_attempts.exists())
                self.assertEqual(f.state()["releases"], [])
                self.assertFalse(any(call[:2] == ["release", "create"] for call in f.state()["calls"]))
                self.assertEqual(f.git("tag", "--list"), "")
                self.assertEqual(f.git("status", "--porcelain"), "")


if __name__ == "__main__":
    unittest.main()
