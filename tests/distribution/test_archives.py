"""Build and install Cargo and Python source archives outside the checkout."""

import os
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PACKAGE = ROOT / "crates/changenote"
VERSION = tomllib.loads((PACKAGE / "Cargo.toml").read_text())["package"]["version"]
SKILL = (ROOT / "skills/changenote/SKILL.md").read_bytes()


class Archives(unittest.TestCase):
    def run_command(self, args, cwd, env=None):
        result = subprocess.run(
            [str(arg) for arg in args], cwd=cwd, env=env,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=300,
        )
        self.assertEqual(result.returncode, 0, f"{args}\n{result.stderr.decode(errors='replace')}")
        return result.stdout

    def check_cli(self, command, cwd):
        self.assertEqual(self.run_command([*command, "--version"], cwd), f"changenote {VERSION}\n".encode())
        self.assertEqual(self.run_command([*command, "skill"], cwd), SKILL)
        self.assertIn(b"Language-independent change notes and releases", self.run_command([*command, "--help"], cwd))
        for shell in ("bash", "zsh", "fish"):
            self.assertIn(b"changenote", self.run_command([*command, "completion", shell], cwd))
        self.assertFalse((cwd / ".changenote").exists())

    def test_cargo_archive_installs_without_the_repository(self):
        with tempfile.TemporaryDirectory(prefix="changenote-crate-") as directory:
            work = Path(directory)
            target = work / "target"
            self.run_command([
                "cargo", "package", "--locked", "-p", "changenote", "--allow-dirty",
                "--target-dir", target,
            ], ROOT)
            archive = target / "package" / f"changenote-{VERSION}.crate"
            with tarfile.open(archive) as package:
                package.extractall(work / "source", filter="data")
            source = work / "source" / f"changenote-{VERSION}"
            self.assertEqual((source / "assets/changenote.SKILL.md").read_bytes(), SKILL)
            self.assertEqual((source / "README.md").read_bytes(), (ROOT / "README.md").read_bytes())
            self.assertEqual((source / "LICENSE").read_bytes(), (ROOT / "LICENSE").read_bytes())
            self.assertTrue((source / "Cargo.lock").is_file())
            self.run_command([
                "cargo", "install", "--locked", "--debug", "--path", source,
                "--root", work / "installed", "--target-dir", target,
            ], work)
            binary = work / "installed/bin" / ("changenote.exe" if os.name == "nt" else "changenote")
            self.check_cli([binary], work)
            print(f"Verified Cargo archive installation: {archive}", flush=True)

    def test_git_install_finds_the_single_workspace_member(self):
        with tempfile.TemporaryDirectory(prefix="changenote-git-") as directory:
            work = Path(directory)
            source = work / "source"
            source.mkdir()
            for name in ("Cargo.toml", "Cargo.lock", "README.md"):
                shutil.copyfile(ROOT / name, source / name)
            shutil.copytree(PACKAGE, source / "crates/changenote")
            self.run_command(["git", "init", "-q"], source)
            self.run_command(["git", "add", "--", "Cargo.toml", "Cargo.lock", "README.md", "crates/changenote"], source)
            self.run_command([
                "git", "-c", "user.name=Test", "-c", "user.email=test@example.com",
                "commit", "-qm", "test: workspace installation fixture",
            ], source)
            self.run_command([
                "cargo", "install", "--locked", "--debug", "--git", source.as_uri(),
                "--root", work / "installed", "--target-dir", work / "target",
            ], work)
            binary = work / "installed/bin" / ("changenote.exe" if os.name == "nt" else "changenote")
            self.check_cli([binary], work)
            print(f"Verified Git installation: {source}", flush=True)

    def test_python_sdist_builds_an_installable_wheel_without_the_repository(self):
        with tempfile.TemporaryDirectory(prefix="changenote-sdist-") as directory:
            work = Path(directory)
            env = os.environ.copy()
            env["CARGO_TARGET_DIR"] = str(work / "target")
            self.run_command(["uv", "build", "--sdist", "--out-dir", work / "sdist"], ROOT, env)
            archives = list((work / "sdist").glob("*.tar.gz"))
            self.assertEqual(len(archives), 1)
            with tarfile.open(archives[0]) as package:
                names = package.getnames()
                prefix = names[0].split("/")[0]
                for name in ("Cargo.toml", "Cargo.lock", "pyproject.toml",
                             "crates/changenote/Cargo.toml", "crates/changenote/src/main.rs",
                             "crates/changenote/assets/changenote.SKILL.md"):
                    self.assertIn(f"{prefix}/{name}", names)
            self.run_command([
                "uv", "build", archives[0], "--wheel", "--out-dir", work / "wheels",
            ], work, env)
            self.run_command(["uv", "venv", work / "venv"], work)
            scripts = work / "venv" / ("Scripts" if os.name == "nt" else "bin")
            interpreter = scripts / ("python.exe" if os.name == "nt" else "python")
            self.run_command([
                "uv", "pip", "install", "--python", interpreter, "--no-index",
                "--find-links", work / "wheels", f"changenote=={VERSION}",
            ], work)
            binary = scripts / ("changenote.exe" if os.name == "nt" else "changenote")
            self.check_cli([binary], work)
            runner = ["uv", "run", "--no-project", "--python", interpreter, "python"]
            self.check_cli([*runner, "-m", "changenote"], work)
            self.assertEqual(self.run_command([
                *runner, "-c", "from importlib.metadata import version; print(version('changenote'))",
            ], work), f"{VERSION}\n".encode())
            self.run_command([
                *runner, "-m", "unittest", "discover", "-s", ROOT / "python/tests", "-v",
            ], work)
            print(f"Verified sdist-to-wheel installation: {archives[0]}", flush=True)


if __name__ == "__main__":
    unittest.main()
