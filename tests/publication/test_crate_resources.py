"""Check self-contained crate resources and the single bundled skill source."""

import fnmatch
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RESOURCE_COPIES = (
    "README.md",
    "LICENSE",
    "schema.json",
    "docs/notes.md",
    "docs/swift.md",
)


class CrateResources(unittest.TestCase):
    def test_repository_copies_match_canonical_resources(self):
        for name in RESOURCE_COPIES:
            with self.subTest(name=name):
                self.assertEqual(
                    (ROOT / name).read_bytes(),
                    (ROOT / "crates/changenote" / name).read_bytes(),
                    "Copy the canonical resource into crates/changenote before committing",
                )

    def test_skill_has_one_package_local_source(self):
        self.assertTrue((ROOT / "crates/changenote/SKILL.md").is_file())
        self.assertFalse((ROOT / "skills").exists())
        self.assertFalse((ROOT / "crates/changenote/skills").exists())
        self.assertFalse((ROOT / "scripts/sync-crate-resources.py").exists())

    def test_workspace_and_package_include_required_inputs(self):
        workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())
        self.assertNotIn("package", workspace)
        self.assertEqual(workspace["workspace"]["members"], ["crates/changenote"])
        manifest = tomllib.loads((ROOT / "crates/changenote/Cargo.toml").read_text())
        self.assertEqual(manifest["package"]["name"], "changenote")
        self.assertIsInstance(manifest["package"]["version"], str)
        included = manifest["package"]["include"]
        for name in (*RESOURCE_COPIES, "SKILL.md", "src/main.rs",
                     "tests/fixtures/planning/provenance/fixture/.changenote/config.json"):
            with self.subTest(name=name):
                self.assertTrue(any(fnmatch.fnmatchcase(name, pattern) for pattern in included))
                self.assertTrue((ROOT / "crates/changenote" / name).is_file())
        python = tomllib.loads((ROOT / "pyproject.toml").read_text())
        self.assertEqual(python["tool"]["maturin"]["manifest-path"],
                         "crates/changenote/Cargo.toml")


if __name__ == "__main__":
    unittest.main()
