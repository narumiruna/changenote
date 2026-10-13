"""Check build-only version synchronization without changing release data."""

import json
import runpy
import tempfile
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
prepare = runpy.run_path(str(ROOT / "scripts/prepare-python-release.py"))["prepare"]


class Prepare(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for name in (
            "Cargo.toml",
            "Cargo.lock",
            "VERSION",
            ".changenote/releases.json",
        ):
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes((ROOT / name).read_bytes())
        (self.root / "VERSION").write_text("1.2.3\n")
        (self.root / ".changenote/releases.json").write_text(
            json.dumps([{"package": "changenote", "version": "1.2.3"}])
        )

    def test_only_package_versions_change_and_repeat_is_safe(self):
        original_lock = tomllib.loads((self.root / "Cargo.lock").read_text())
        original_manifest = tomllib.loads((self.root / "Cargo.toml").read_text())
        original_batch = (self.root / ".changenote/releases.json").read_bytes()
        prepare(self.root)
        original_manifest["package"]["version"] = "1.2.3"
        for package in original_lock["package"]:
            if package["name"] == "changenote":
                package["version"] = "1.2.3"
        self.assertEqual(
            tomllib.loads((self.root / "Cargo.toml").read_text()), original_manifest
        )
        self.assertEqual(
            tomllib.loads((self.root / "Cargo.lock").read_text()), original_lock
        )
        self.assertEqual(
            (self.root / ".changenote/releases.json").read_bytes(), original_batch
        )
        self.assertEqual((self.root / "VERSION").read_text(), "1.2.3\n")
        first = [
            (self.root / name).read_bytes() for name in ("Cargo.toml", "Cargo.lock")
        ]
        prepare(self.root)
        self.assertEqual(
            [(self.root / name).read_bytes() for name in ("Cargo.toml", "Cargo.lock")],
            first,
        )

    def test_invalid_version_does_not_write(self):
        for version in ("01.2.3", "1.2", "1.2.3-rc.1", "1.2.3+build", "1.2.3\n4.5.6"):
            with self.subTest(version=version):
                (self.root / "VERSION").write_text(version)
                self.assert_rejected()

    def test_batch_mismatch_does_not_write(self):
        (self.root / "VERSION").write_text("1.2.4")
        self.assert_rejected()

    def test_lock_mismatch_does_not_write(self):
        path = self.root / "Cargo.toml"
        text = path.read_text()
        version = tomllib.loads(text)["package"]["version"]
        path.write_text(text.replace(f'version = "{version}"', 'version = "9.9.9"', 1))
        self.assert_rejected()

    def assert_rejected(self):
        paths = [self.root / name for name in ("Cargo.toml", "Cargo.lock")]
        before = [path.read_bytes() for path in paths]
        with self.assertRaises(ValueError):
            prepare(self.root)
        self.assertEqual([path.read_bytes() for path in paths], before)


if __name__ == "__main__":
    unittest.main()
