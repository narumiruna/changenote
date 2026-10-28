"""Exercise trusted CLI installation and cache activation without building Rust."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ReleaseSetup(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.top = Path(temporary.name)
        self.source = self.top / "trusted action"
        (self.source / "release").mkdir(parents=True)
        (self.source / "src").mkdir()
        (self.source / "Cargo.toml").write_text('[package]\nname = "changenote"\nversion = "9.8.7"\n')
        (self.source / "Cargo.lock").write_text("trusted lock\n")
        (self.source / "src/main.rs").write_text("fn main() {}\n")
        self.consumer = self.top / "consumer"
        (self.consumer / "src").mkdir(parents=True)
        (self.consumer / "Cargo.toml").write_text('[package]\nname = "changenote"\nversion = "0.0.0"\nbuild = "build.rs"\n')
        (self.consumer / "Cargo.lock").write_text("consumer lock\n")
        (self.consumer / "src/main.rs").write_text("untrusted consumer source\n")
        (self.consumer / "rust-toolchain.toml").write_text('[toolchain]\nchannel = "not-selected-toolchain"\n')
        (self.consumer / "build.rs").write_text('compile_error!("must not build consumer");\n')
        self.bin = self.top / "bin"
        self.bin.mkdir()
        self.calls = self.top / "cargo-calls"
        stub = self.bin / "cargo"
        stub.write_text(
            '#!/usr/bin/env bash\nset -eu\n'
            'printf "%s\\0" "$@" >> "$CARGO_CALLS"\n'
            'printf "%s" "$RUSTUP_TOOLCHAIN" > "$CARGO_CALLS.toolchain"\n'
            'while [[ $# -gt 0 ]]; do\n'
            '  if [[ $1 == --root ]]; then root=$2; shift; fi\n'
            '  shift\ndone\n'
            'mkdir -p "$root/bin"\n'
            'printf "%s\\n" \'#!/bin/sh\' \'printf "%s\\n" "changenote 9.8.7"\' > "$root/bin/changenote"\n'
            'chmod +x "$root/bin/changenote"\n'
        )
        stub.chmod(0o755)
        self.output = self.top / "output"
        self.path_file = self.top / "path"
        self.path_file.write_text("")
        self.env = dict(os.environ, PATH=str(self.bin) + ":" + os.environ["PATH"],
                        ACTION_PATH=str(self.source / "release"),
                        RUNNER_TEMP=str(self.top / "runner temp"),
                        GITHUB_OUTPUT=str(self.output), GITHUB_PATH=str(self.path_file),
                        INPUT_RUST_TOOLCHAIN="stable", CARGO_CALLS=str(self.calls))

    def run_setup(self, operation, ok=True):
        self.output.write_text("")
        result = subprocess.run(
            ["bash", str(ROOT / "scripts/action/install-cli.sh"), operation],
            cwd=self.consumer, env=self.env, text=True, capture_output=True, timeout=30,
        )
        if ok:
            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        values = dict(line.split("=", 1) for line in self.output.read_text().splitlines())
        return result, values

    def key(self):
        _, values = self.run_setup("key")
        self.assertEqual(set(values), {"directory", "key"})
        self.assertTrue(values["key"].startswith("changenote-"))
        self.assertRegex(Path(values["directory"]).name, r"^[0-9a-f]{64}$")
        self.assertEqual(Path(values["directory"]).parent, Path(self.env["RUNNER_TEMP"]) / "changenote-cli")
        return values

    def test_source_and_toolchain_define_cache_not_consumer_checkout(self):
        original = self.key()
        (self.consumer / "Cargo.toml").write_text("malicious replacement\n")
        (self.consumer / "Cargo.lock").write_text("malicious lock\n")
        (self.consumer / "src/main.rs").write_text("malicious source\n")
        self.assertEqual(self.key(), original)
        for path in ("Cargo.toml", "Cargo.lock", "src/main.rs"):
            with self.subTest(path=path):
                source = self.source / path
                before = source.read_text()
                source.write_text(before + "\n")
                self.assertNotEqual(self.key(), original)
                source.write_text(before)
        self.env["INPUT_RUST_TOOLCHAIN"] = "1.85.0"
        self.assertNotEqual(self.key(), original)
        self.assertFalse(self.calls.exists())
        self.assertEqual(self.path_file.read_text(), "")

    def test_install_uses_action_source_and_activates_verified_binary(self):
        key = self.key()
        self.run_setup("install")
        self.assertEqual(self.calls.read_bytes().split(b"\0")[:-1], [
            b"install", b"--path", os.fsencode(self.source), b"--locked",
            b"--root", os.fsencode(key["directory"]),
        ])
        self.assertEqual(Path(str(self.calls) + ".toolchain").read_text(), "stable")
        self.assertEqual(self.path_file.read_text(), "")
        self.run_setup("activate")
        self.assertEqual(self.path_file.read_text(), key["directory"] + "/bin\n")
        self.assertEqual((self.consumer / "src/main.rs").read_text(), "untrusted consumer source\n")
        self.assertFalse((self.consumer / "target").exists())

    def test_cache_hit_activation_does_not_call_cargo(self):
        directory = Path(self.key()["directory"])
        (directory / "bin").mkdir(parents=True)
        cached = directory / "bin/changenote"
        cached.write_text('#!/bin/sh\nprintf "%s\\n" "changenote 9.8.7"\n')
        cached.chmod(0o755)
        self.run_setup("activate")
        self.assertEqual(self.path_file.read_text(), str(directory / "bin") + "\n")
        self.assertFalse(self.calls.exists())
        self.env["INPUT_RUST_TOOLCHAIN"] = "nightly"
        result, _ = self.run_setup("activate", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.path_file.read_text(), str(directory / "bin") + "\n")

    def test_missing_mismatched_and_symlinked_cached_cli_are_not_activated(self):
        directory = Path(self.key()["directory"])
        cached = directory / "bin/changenote"
        result, _ = self.run_setup("activate", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.path_file.read_text(), "")
        cached.parent.mkdir(parents=True)
        cached.write_text('#!/bin/sh\nprintf "%s\\n" "changenote 0.0.0"\n')
        cached.chmod(0o755)
        result, _ = self.run_setup("activate", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("version differs", result.stderr)
        self.assertEqual(self.path_file.read_text(), "")
        cached.unlink()
        target = self.top / "consumer-cli"
        target.write_text('#!/bin/sh\nprintf "%s\\n" "changenote 9.8.7"\n')
        target.chmod(0o755)
        cached.symlink_to(target)
        result, _ = self.run_setup("activate", ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("regular file", result.stderr)
        self.assertEqual(self.path_file.read_text(), "")
        self.assertFalse(self.calls.exists())


if __name__ == "__main__":
    unittest.main()
