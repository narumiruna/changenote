"""Check workspace metadata and package-local resource copies without builds."""

import json
import shutil
import tempfile
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RESOURCES = {
    "skills/changenote/SKILL.md": "crates/changenote/assets/changenote.SKILL.md",
    "LICENSE": "crates/changenote/LICENSE",
}


class Workspace(unittest.TestCase):
    def assert_resources_match(self, root):
        for source, packaged in RESOURCES.items():
            self.assertEqual((root / source).read_bytes(), (root / packaged).read_bytes(), packaged)

    def test_package_resources_match_canonical_files(self):
        self.assert_resources_match(ROOT)

    def test_changed_resource_copy_fails_the_equality_check(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for source, packaged in RESOURCES.items():
                for name in (source, packaged):
                    target = root / name
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(ROOT / source, target)
            self.assert_resources_match(root)
            for packaged in RESOURCES.values():
                target = root / packaged
                original = target.read_bytes()
                target.write_bytes(original + b"changed\n")
                with self.assertRaises(AssertionError):
                    self.assert_resources_match(root)
                target.write_bytes(original)

    def test_workspace_and_distribution_share_one_explicit_version(self):
        workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())
        member = "crates/changenote"
        self.assertEqual(workspace, {"workspace": {
            "members": [member], "default-members": [member], "resolver": "3",
        }})
        manifest = tomllib.loads((ROOT / member / "Cargo.toml").read_text())
        package = manifest["package"]
        self.assertEqual(package["name"], "changenote")
        self.assertIsInstance(package["version"], str)
        self.assertEqual(package["license"], "MIT")
        self.assertEqual((ROOT / member / package["readme"]).resolve(), ROOT / "README.md")
        self.assertFalse((ROOT / member / "Cargo.lock").exists())
        lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
        local = [p for p in lock["package"] if p["name"] == "changenote" and "source" not in p]
        self.assertEqual(len(local), 1)
        self.assertEqual(local[0]["version"], package["version"])
        project = tomllib.loads((ROOT / "pyproject.toml").read_text())
        self.assertEqual(project["tool"]["maturin"]["manifest-path"], f"{member}/Cargo.toml")
        self.assertEqual(project["project"]["dynamic"], ["version"])
        keys = {key["file"] for key in project["tool"]["uv"]["cache-keys"]}
        self.assertTrue({"Cargo.toml", "Cargo.lock", f"{member}/Cargo.toml",
                         f"{member}/src/**/*.rs", "skills/changenote/SKILL.md",
                         f"{member}/assets/changenote.SKILL.md"} <= keys)
        config = json.loads((ROOT / ".changenote/config.json").read_text())
        self.assertEqual(config["packages"], {
            "changenote": {"path": ".", "version_file": f"{member}/Cargo.toml"}
        })


if __name__ == "__main__":
    unittest.main()
