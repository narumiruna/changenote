"""Exercise the check composite from a consumer directory with a trusted CLI stub."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]


class CheckAction(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.top = Path(temporary.name)
        self.repo = self.top / "consumer"
        self.repo.mkdir()
        self.bin = self.top / "bin"
        self.bin.mkdir()
        self.output = self.top / "output"
        self.summary = self.top / "summary"
        self.calls = self.top / "calls"
        self.report = {
            "merge_base": "a" * 40,
            "changed_packages": ["pkg"],
            "note_paths": [".changenote/new.md"],
            "outside_paths": [],
            "skip_reason": None,
            "passed": True,
            "covered_packages": ["pkg"],
            "uncovered_packages": [],
        }
        self.plan = [{
            "package": "pkg", "old_version": "1.0.0", "version": "1.0.1",
            "bump": "patch", "tag": "sdk/v1.0.1", "notes": ["Old", "New"],
            "note_paths": [".changenote/old.md", ".changenote/new.md"],
        }]
        self.metadata = yaml.load(
            (ROOT / "check/action.yml").read_text(), Loader=yaml.BaseLoader
        )
        self.env = dict(os.environ, PATH=str(self.bin) + ":" + os.environ["PATH"])
        for key in ("GH_TOKEN", "GITHUB_TOKEN", "GIT_INDEX_FILE", "GIT_DIR", "GIT_WORK_TREE"):
            self.env.pop(key, None)
        self.env.update({
            "ACTION_PATH": str(ROOT / "check"),
            "INPUT_SINCE": "origin/main", "INPUT_COVERAGE": "package",
            "INPUT_SKIP_RELEASE": "false", "INPUT_REASON": "",
            "GITHUB_OUTPUT": str(self.output), "GITHUB_STEP_SUMMARY": str(self.summary),
            "CLI_CALLS": str(self.calls), "CHECK_RESULT": "0", "STATUS_RESULT": "0",
            "CHECK_DIAGNOSTIC": "", "FORBIDDEN": str(self.top / "forbidden"),
        })
        stub = self.bin / "changenote"
        stub.write_text(
            '#!/usr/bin/env bash\nset -eu\n'
            'printf "%s\\0" "$@" >> "$CLI_CALLS"\n'
            'case "$1" in\n'
            'check) printf "%s" "$CHECK_DIAGNOSTIC" >&2; '
            'printf "%s\\n" "$CHECK_REPORT"; exit "$CHECK_RESULT" ;;\n'
            'status) printf "%s\\n" "$RELEASE_PLAN"; exit "$STATUS_RESULT" ;;\n'
            '*) exit 99 ;;\nesac\n'
        )
        stub.chmod(0o755)
        for tool in ("gh", "curl", "wget", "cargo", "npm", "uv"):
            path = self.bin / tool
            path.write_text('#!/usr/bin/env bash\nprintf forbidden > "$FORBIDDEN"\nexit 99\n')
            path.chmod(0o755)

    def run_action(self, report=None, plan=None):
        self.output.write_text("")
        self.summary.write_text("")
        self.env["CHECK_REPORT"] = json.dumps(self.report) if report is None else report
        self.env["RELEASE_PLAN"] = json.dumps(self.plan) if plan is None else plan
        result = subprocess.run(
            ["bash", "-c", self.metadata["runs"]["steps"][0]["run"]],
            cwd=self.repo, env=self.env, text=True, capture_output=True, timeout=30,
        )
        self.assertFalse((self.top / "forbidden").exists())
        self.assertEqual(list(self.repo.iterdir()), [])
        outputs = dict(line.split("=", 1) for line in self.output.read_text().splitlines())
        return result, outputs

    def test_metadata_and_consumer_path(self):
        self.assertEqual(self.metadata["runs"]["using"], "composite")
        inputs = self.metadata["inputs"]
        self.assertEqual(set(inputs), {"since", "coverage", "skip-release", "reason"})
        self.assertEqual(inputs["since"]["required"], "true")
        self.assertNotIn("default", inputs["since"])
        self.assertEqual(inputs["coverage"]["default"], "package")
        step = self.metadata["runs"]["steps"][0]
        self.assertEqual(step["shell"], "bash")
        self.assertEqual(step["env"]["ACTION_PATH"], "${{ github.action_path }}")
        self.assertNotIn("${{", step["run"])
        for key in inputs:
            self.assertEqual(step["env"]["INPUT_" + key.upper().replace("-", "_")], "${{ inputs." + key + " }}")
        result, outputs = self.run_action()
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertEqual(set(outputs), {"check-report", "release-plan", "passed"})
        for key in outputs:
            self.assertEqual(self.metadata["outputs"][key]["value"], "${{ steps.check.outputs." + key + " }}")
        self.assertEqual(json.loads(outputs["check-report"]), self.report)
        self.assertEqual(json.loads(outputs["release-plan"]), self.plan)
        self.assertEqual(outputs["passed"], "true")
        self.assertEqual(self.calls.read_bytes().split(b"\0")[:-1], [
            b"check", b"--since=origin/main", b"--coverage=package",
            b"status", b"--include-note-paths",
        ])
        self.assertIn("PR note check", self.summary.read_text())
        self.assertIn("All pending releases", self.summary.read_text())
        self.assertIn("older pending notes", self.summary.read_text())
        self.assertIn("<th>Next version</th>", self.summary.read_text())
        self.assertIn("<td>1.0.1</td>", self.summary.read_text())
        self.assertIn("<pre>Old</pre>", self.summary.read_text())

    def test_missing_notes_preserve_failure_and_complete_outputs(self):
        self.env["CHECK_RESULT"] = "1"
        self.report.update(passed=False, covered_packages=[], uncovered_packages=["pkg"])
        result, outputs = self.run_action()
        self.assertEqual(result.returncode, 1)
        self.assertEqual(outputs["passed"], "false")
        self.assertEqual(json.loads(outputs["release-plan"]), self.plan)
        self.assertIn("::error::PR note check failed", result.stdout)

    def test_untrusted_markdown_and_command_data_are_escaped(self):
        text = '</pre>\n```\n~~~~\n[link](https://evil.invalid)\n<script>bad</script>\n& %\r\n::error file=x::injected'
        self.plan[0]["notes"] = [text]
        self.plan[0]["note_paths"] = [".changenote/x,foo=bar%\r\n::error::name.md"]
        self.env["CHECK_DIAGNOSTIC"] = text
        result, outputs = self.run_action()
        self.assertEqual(result.returncode, 0)
        self.assertEqual(json.loads(outputs["release-plan"])[0]["notes"], [text])
        summary = self.summary.read_text()
        self.assertEqual(summary.count("</pre>"), 1)
        self.assertIn("&lt;script&gt;", summary)
        self.assertIn("&lt;/pre&gt;", summary)
        self.assertNotIn("<script>", summary)
        self.assertIn("%25%0D%0A::error file=x::injected", result.stdout)
        self.assertNotIn("\n::error", result.stdout)
        self.assertEqual(len(result.stdout.splitlines()), 2)

    def test_multiline_paths_and_exemptions_cannot_open_markdown_blocks(self):
        text = "outside\r\n\r\n![spoof](https://evil.invalid/image)\n\n# heading"
        self.report["outside_paths"] = [text]
        self.report["note_paths"] = [text]
        self.report["skip_reason"] = text
        self.plan[0]["note_paths"] = [text]
        self.plan[0]["notes"] = ["Safe note"]
        result, outputs = self.run_action()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(outputs["check-report"])["outside_paths"], [text])
        summary = self.summary.read_text()
        self.assertNotIn("\n\n![spoof]", summary)
        self.assertNotIn("\n\n# heading", summary)
        self.assertIn("&#13;&#10;&#13;&#10;![spoof]", summary)
        self.assertIn("&#10;&#10;# heading", summary)

    def test_input_data_is_not_shell_syntax(self):
        self.env.update(INPUT_SINCE="--odd; $(touch injected)", INPUT_COVERAGE="pr",
                        INPUT_SKIP_RELEASE="true", INPUT_REASON="Docs `touch injected`\n% reason")
        self.report.pop("covered_packages")
        self.report.pop("uncovered_packages")
        self.report["skip_reason"] = self.env["INPUT_REASON"]
        result, _ = self.run_action()
        self.assertEqual(result.returncode, 0)
        args = self.calls.read_bytes().decode().split("\0")
        self.assertIn("--since=" + self.env["INPUT_SINCE"], args)
        self.assertIn("--reason=" + self.env["INPUT_REASON"], args)
        self.assertIn("--skip-release", args)

    def test_invalid_inputs_do_not_call_cli(self):
        for values in (
            {"INPUT_SINCE": ""}, {"INPUT_COVERAGE": "strict"},
            {"INPUT_SKIP_RELEASE": "yes"}, {"INPUT_SKIP_RELEASE": "true"},
            {"INPUT_SKIP_RELEASE": "true", "INPUT_REASON": " \n\t"},
            {"INPUT_REASON": "not paired"},
        ):
            with self.subTest(values=values):
                saved = self.env.copy()
                self.env.update(values)
                result, outputs = self.run_action()
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(outputs, {})
                self.assertFalse(self.calls.exists())
                self.env = saved

    def test_malformed_or_missing_reports_do_not_emit_outputs(self):
        for report in ("", "{}", "[]", "null", "bad", json.dumps(self.report) * 2,
                       json.dumps(dict(self.report, passed="true")),
                       json.dumps(dict(self.report, note_paths=None)),
                       json.dumps(dict(self.report, uncovered_packages=None))):
            with self.subTest(report=report):
                result, outputs = self.run_action(report=report)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(outputs, {})
                self.assertEqual(self.summary.read_text(), "")
        self.env["CHECK_RESULT"] = "2"
        result, outputs = self.run_action(report="")
        self.assertEqual(result.returncode, 2)
        self.assertEqual(outputs, {})

    def test_inconsistent_exit_status_does_not_emit_outputs(self):
        self.report["passed"] = False
        result, outputs = self.run_action()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(outputs, {})

    def test_bad_pending_plan_does_not_emit_outputs(self):
        for plan in ("", "{}", "[] []", '[{"package":"pkg"}]'):
            with self.subTest(plan=plan):
                result, outputs = self.run_action(plan=plan)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(outputs, {})
        self.env.update(CHECK_RESULT="7", STATUS_RESULT="1")
        self.report["passed"] = False
        result, outputs = self.run_action()
        self.assertEqual(result.returncode, 7)
        self.assertEqual(outputs, {})


if __name__ == "__main__":
    unittest.main()
