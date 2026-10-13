from importlib.metadata import version
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from changenote import find_changenote_bin
from changenote.__main__ import main


class BinaryLocatorTests(unittest.TestCase):
    def test_current_environment_binary(self):
        with tempfile.TemporaryDirectory() as directory:
            name = "changenote.exe" if sys.platform == "win32" else "changenote"
            binary = Path(directory) / name
            binary.touch()
            with patch("changenote.sysconfig.get_path", return_value=directory):
                self.assertEqual(find_changenote_bin(), str(binary))

    def test_user_install_fallback(self):
        with tempfile.TemporaryDirectory() as directory:
            scripts = Path(directory) / "user"
            scripts.mkdir()
            name = "changenote.exe" if sys.platform == "win32" else "changenote"
            binary = scripts / name
            binary.touch()
            with (
                patch("changenote.sysconfig.get_path", side_effect=[directory, str(scripts)]),
                patch("changenote.__file__", str(Path(directory) / "module/__init__.py")),
            ):
                self.assertEqual(find_changenote_bin(), str(binary))

    def test_missing_binary_does_not_search_path(self):
        with tempfile.TemporaryDirectory() as directory:
            with (
                patch("changenote.sysconfig.get_path", return_value=directory),
                patch("changenote.__file__", str(Path(directory) / "module/__init__.py")),
            ):
                with self.assertRaisesRegex(FileNotFoundError, "reinstall changenote"):
                    find_changenote_bin()

    def test_module_environment_scripts(self):
        with tempfile.TemporaryDirectory() as directory:
            prefix = Path(directory)
            if sys.platform == "win32":
                module = prefix / "Lib/site-packages/changenote/__init__.py"
                binary = prefix / "Scripts/changenote.exe"
            else:
                module = prefix / "lib/python3.14/site-packages/changenote/__init__.py"
                binary = prefix / "bin/changenote"
            binary.parent.mkdir()
            binary.touch()
            with (
                patch("changenote.sysconfig.get_path", return_value=str(prefix / "missing")),
                patch("changenote.__file__", str(module)),
            ):
                self.assertEqual(find_changenote_bin(), str(binary))


class RunnerTests(unittest.TestCase):
    def test_posix_exec_preserves_arguments(self):
        with (
            patch("changenote.__main__.sys.platform", "linux"),
            patch("changenote.__main__.sys.argv", ["module", "add", "note", "-m", "words with spaces"]),
            patch("changenote.__main__.find_changenote_bin", return_value="/venv/bin/changenote"),
            patch("changenote.__main__.os.execv") as execute,
        ):
            main()
            execute.assert_called_once_with(
                "/venv/bin/changenote",
                ["/venv/bin/changenote", "add", "note", "-m", "words with spaces"],
            )

    def test_windows_exit_code(self):
        with (
            patch("changenote.__main__.sys.platform", "win32"),
            patch("changenote.__main__.sys.argv", ["module", "status"]),
            patch("changenote.__main__.find_changenote_bin", return_value="changenote.exe"),
            patch("changenote.__main__.subprocess.run", return_value=subprocess.CompletedProcess([], 7)) as execute,
        ):
            with self.assertRaises(SystemExit) as error:
                main()
            self.assertEqual(error.exception.code, 7)
            execute.assert_called_once_with(["changenote.exe", "status"], check=False)

    def test_windows_interrupt(self):
        with (
            patch("changenote.__main__.sys.platform", "win32"),
            patch("changenote.__main__.find_changenote_bin", return_value="changenote.exe"),
            patch("changenote.__main__.subprocess.run", side_effect=KeyboardInterrupt),
        ):
            with self.assertRaises(SystemExit) as error:
                main()
            self.assertEqual(error.exception.code, 130)

    def test_installed_module_passes_working_directory_and_exit_code(self):
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run(
                [sys.executable, "-m", "changenote", "status"],
                cwd=directory,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("error:", result.stderr)
            self.assertNotIn("Traceback", result.stderr)
            self.assertFalse((Path(directory) / ".changenote").exists())

    def test_python_distribution_version_matches_rust_binary(self):
        result = subprocess.run(
            [find_changenote_bin(), "--version"],
            capture_output=True,
            text=True,
            check=True,
        )
        self.assertEqual(result.stdout.strip(), f"changenote {version('changenote')}")

    def test_installed_module_runs_rust_help(self):
        result = subprocess.run(
            [sys.executable, "-m", "changenote", "--help"],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0)
        self.assertIn("Language-independent change notes and releases", result.stdout)


if __name__ == "__main__":
    unittest.main()
