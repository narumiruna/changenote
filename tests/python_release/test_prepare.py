"""Check current and historical release preparation without repository fixtures."""

import json
import runpy
import tempfile
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
prepare = runpy.run_path(str(ROOT / "scripts/prepare-python-release.py"))["prepare"]
CARGO_SOURCES = ("Cargo.toml", "crates/changenote/Cargo.toml")


class Prepare(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.fixture("VERSION")

    def write(self, name, content):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)

    def fixture(self, source):
        self.source = source
        self.manifest = "Cargo.toml" if source == "VERSION" else source
        version = "0.1.0" if source == "VERSION" else "1.2.3"
        self.write(self.manifest, f'[package]\nname = "changenote"\nversion = "{version}"\nedition = "2024"\n')
        if source == "crates/changenote/Cargo.toml":
            self.write("Cargo.toml", '[workspace]\nmembers = ["crates/changenote"]\nresolver = "3"\n')
        self.write("Cargo.lock", f'''version = 4

[[package]]
name = "changenote"
version = "{version}"
dependencies = ["example"]

[[package]]
name = "example"
version = "2.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
''')
        self.write(".changenote/config.json", json.dumps({
            "packages": {"changenote": {"path": ".", "version_file": source}}
        }))
        self.write(".changenote/releases.json", json.dumps([
            {"package": "changenote", "version": "1.2.3"}
        ]))
        version_file = self.root / "VERSION"
        if source == "VERSION":
            version_file.write_text("1.2.3\n")
        elif version_file.exists():
            version_file.unlink()

    def snapshot(self):
        return {path.relative_to(self.root): path.read_bytes()
                for path in self.root.rglob("*") if path.is_file()}

    def assert_rejected(self):
        before = self.snapshot()
        with self.assertRaises(ValueError):
            prepare(self.root)
        self.assertEqual(self.snapshot(), before)

    def test_only_legacy_package_versions_change_and_repeat_is_safe(self):
        original = self.snapshot()
        original_manifest = tomllib.loads((self.root / self.manifest).read_text())
        original_lock = tomllib.loads((self.root / "Cargo.lock").read_text())
        self.assertEqual(prepare(self.root), "1.2.3")
        original_manifest["package"]["version"] = "1.2.3"
        original_lock["package"][0]["version"] = "1.2.3"
        self.assertEqual(tomllib.loads((self.root / self.manifest).read_text()), original_manifest)
        self.assertEqual(tomllib.loads((self.root / "Cargo.lock").read_text()), original_lock)
        for path, content in original.items():
            if str(path) not in (self.manifest, "Cargo.lock"):
                self.assertEqual((self.root / path).read_bytes(), content)
        first = self.snapshot()
        self.assertEqual(prepare(self.root), "1.2.3")
        self.assertEqual(self.snapshot(), first)

    def test_cargo_sources_are_read_only_and_repeatable_without_version_file(self):
        for source in CARGO_SOURCES:
            with self.subTest(source=source):
                self.fixture(source)
                before = self.snapshot()
                self.assertEqual(prepare(self.root), "1.2.3")
                self.assertEqual(prepare(self.root), "1.2.3")
                self.assertEqual(self.snapshot(), before)
                self.assertFalse((self.root / "VERSION").exists())

    def test_invalid_sources_do_not_write(self):
        for path, source in (
            ("crates/changenote", "Cargo.toml"),
            (".", "../Cargo.toml"),
            (".", "/Cargo.toml"),
            (".", "crates/other/Cargo.toml"),
            (".", "crates/changenote/../Cargo.toml"),
            (".", "pyproject.toml"),
            (".", None),
        ):
            with self.subTest(path=path, source=source):
                self.write(".changenote/config.json", json.dumps({
                    "packages": {"changenote": {"path": path, "version_file": source}}
                }))
                self.assert_rejected()

    def test_invalid_versions_do_not_write(self):
        for source in ("VERSION", *CARGO_SOURCES):
            for version in ("01.2.3", "1.2", "1.2.3-rc.1", "1.2.3+build", "1.2.3\n4.5.6"):
                with self.subTest(source=source, version=version):
                    self.fixture(source)
                    if source == "VERSION":
                        self.write(source, version)
                    else:
                        manifest = self.root / self.manifest
                        manifest.write_text(manifest.read_text().replace(
                            'version = "1.2.3"', f'version = {json.dumps(version)}', 1
                        ))
                    self.assert_rejected()

    def test_invalid_batches_do_not_write(self):
        for source in ("VERSION", *CARGO_SOURCES):
            for batch in ([], [{"package": "other", "version": "1.2.3"}],
                          [{"package": "changenote", "version": "1.2.4"}],
                          [{"package": "changenote", "version": "1.2.3"}] * 2):
                with self.subTest(source=source, batch=batch):
                    self.fixture(source)
                    self.write(".changenote/releases.json", json.dumps(batch))
                    self.assert_rejected()

    def test_invalid_package_identity_and_lock_do_not_write(self):
        for source in ("VERSION", *CARGO_SOURCES):
            for failure in ("name", "version", "lock-version", "missing", "duplicate", "registry"):
                with self.subTest(source=source, failure=failure):
                    self.fixture(source)
                    manifest = self.root / self.manifest
                    lock = self.root / "Cargo.lock"
                    version = "0.1.0" if source == "VERSION" else "1.2.3"
                    if failure == "name":
                        manifest.write_text(manifest.read_text().replace('name = "changenote"', 'name = "other"'))
                    elif failure == "version":
                        manifest.write_text(manifest.read_text().replace('version = "0.1.0"', 'version = "9.9.9"').replace('version = "1.2.3"', 'version = "9.9.9"'))
                    elif failure == "lock-version":
                        lock.write_text(lock.read_text().replace(f'version = "{version}"', 'version = "9.9.9"', 1))
                    elif failure == "missing":
                        lock.write_text(lock.read_text().replace('name = "changenote"', 'name = "other"'))
                    elif failure == "duplicate":
                        lock.write_text(lock.read_text() + '\n[[package]]\nname = "changenote"\nversion = "1.2.3"\n')
                    else:
                        lock.write_text(lock.read_text().replace('name = "changenote"', 'name = "changenote"\nsource = "registry+https://github.com/rust-lang/crates.io-index"'))
                    self.assert_rejected()


if __name__ == "__main__":
    unittest.main()
