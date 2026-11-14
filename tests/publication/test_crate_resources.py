"""Check self-contained crate resources and the single bundled skill source."""

import fnmatch
import re
import tomllib
import unittest
from pathlib import Path
from urllib.parse import unquote, urlsplit

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

    def test_packaged_documentation_links_have_targets(self):
        package = (ROOT / "crates/changenote").resolve()
        repository_url = "https://github.com/changenote/changenote/blob/main/"
        for name in ("README.md", "docs/notes.md", "docs/swift.md"):
            document = package / name
            for target in re.findall(r"\]\(([^\s)]+)\)", document.read_text()):
                with self.subTest(document=name, target=target):
                    link = urlsplit(target)
                    if target.startswith(repository_url):
                        source = unquote(link.path.removeprefix(
                            "/changenote/changenote/blob/main/"
                        ))
                        self.assertTrue((ROOT / source).is_file())
                    elif not link.scheme and not link.netloc and link.path:
                        path = (document.parent / unquote(link.path)).resolve()
                        self.assertTrue(path.is_relative_to(package))
                        self.assertTrue(path.is_file(), "Target is absent from the crate")

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
