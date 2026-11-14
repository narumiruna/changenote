"""Check self-contained crate resources and their canonical repository sources."""

import fnmatch
import runpy
import tempfile
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
resources = runpy.run_path(str(ROOT / "scripts/sync-crate-resources.py"))
sync = resources["sync"]
RESOURCES = resources["RESOURCES"]


class CrateResources(unittest.TestCase):
    def fixture(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        for name in RESOURCES:
            source = root / name
            source.parent.mkdir(parents=True, exist_ok=True)
            source.write_bytes(b"exact bytes\r\n\x00" + name.encode())
        return root

    def test_repository_copies_match_canonical_resources(self):
        self.assertEqual(sync(ROOT, check=True), [],
                         "Run uv run --no-project python scripts/sync-crate-resources.py")

    def test_sync_is_exact_and_idempotent(self):
        root = self.fixture()
        self.assertEqual(sync(root), list(RESOURCES))
        for name in RESOURCES:
            self.assertEqual((root / name).read_bytes(),
                             (root / "crates/changenote" / name).read_bytes())
        self.assertEqual(sync(root), [])
        self.assertEqual(sync(root, check=True), [])

    def test_check_reports_missing_and_stale_copies_without_writes(self):
        root = self.fixture()
        self.assertEqual(sync(root, check=True), list(RESOURCES))
        self.assertFalse((root / "crates").exists())
        sync(root)
        missing = root / "crates/changenote" / RESOURCES[0]
        stale = root / "crates/changenote" / RESOURCES[1]
        missing.unlink()
        stale.write_bytes(b"stale")
        self.assertEqual(sync(root, check=True), list(RESOURCES[:2]))
        self.assertFalse(missing.exists())
        self.assertEqual(stale.read_bytes(), b"stale")
        self.assertEqual(sync(root), list(RESOURCES[:2]))
        self.assertEqual(sync(root, check=True), [])

    def test_workspace_and_package_include_required_inputs(self):
        workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())
        self.assertNotIn("package", workspace)
        self.assertEqual(workspace["workspace"]["members"], ["crates/changenote"])
        manifest = tomllib.loads((ROOT / "crates/changenote/Cargo.toml").read_text())
        self.assertEqual(manifest["package"]["name"], "changenote")
        self.assertIsInstance(manifest["package"]["version"], str)
        included = manifest["package"]["include"]
        for name in (*RESOURCES, "src/main.rs",
                     "tests/fixtures/planning/provenance/fixture/.changenote/config.json"):
            with self.subTest(name=name):
                self.assertTrue(any(fnmatch.fnmatchcase(name, pattern) for pattern in included))
                self.assertTrue((ROOT / "crates/changenote" / name).is_file())
        python = tomllib.loads((ROOT / "pyproject.toml").read_text())
        self.assertEqual(python["tool"]["maturin"]["manifest-path"],
                         "crates/changenote/Cargo.toml")


if __name__ == "__main__":
    unittest.main()
