"""Set the Cargo build version in a disposable release checkout only."""

import json
import re
import sys
import tomllib
from pathlib import Path


def prepare(root: Path) -> None:
    version = (root / "VERSION").read_text().strip()
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", version):
        raise ValueError("VERSION must contain a stable SemVer version")
    batch = json.loads((root / ".changenote/releases.json").read_text())
    if (
        len(batch) != 1
        or batch[0]["package"] != "changenote"
        or batch[0]["version"] != version
    ):
        raise ValueError("VERSION must match the changenote release batch")

    manifest_path = root / "Cargo.toml"
    lock_path = root / "Cargo.lock"
    manifest = manifest_path.read_text()
    lock = lock_path.read_text()
    package = tomllib.loads(manifest)["package"]
    locked = [p for p in tomllib.loads(lock)["package"] if p["name"] == "changenote"]
    if package["name"] != "changenote" or len(locked) != 1 or "source" in locked[0]:
        raise ValueError("Expected one local changenote Cargo package")
    if package["version"] != locked[0]["version"]:
        raise ValueError("Cargo.toml and Cargo.lock versions must match")

    # Change only the root package. Dependency versions remain locked.
    manifest, manifest_count = re.subn(
        r'(?m)(^\[package\]\n(?:(?!\[)[^\n]*\n)*?version = ")[^"]+("$)',
        lambda match: match[1] + version + match[2],
        manifest,
    )
    lock, lock_count = re.subn(
        r'(?m)(^\[\[package\]\]\nname = "changenote"\nversion = ")[^"]+("$)',
        lambda match: match[1] + version + match[2],
        lock,
    )
    if manifest_count != 1 or lock_count != 1:
        raise ValueError("Cannot locate the Cargo package version fields")
    manifest_path.write_text(manifest)
    lock_path.write_text(lock)


if __name__ == "__main__":
    prepare(Path(sys.argv[1]))
