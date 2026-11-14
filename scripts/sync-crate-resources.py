"""Copy canonical repository resources into the self-contained Cargo package."""

import argparse
from pathlib import Path

RESOURCES = (
    "README.md",
    "LICENSE",
    "schema.json",
    "skills/changenote/SKILL.md",
    "docs/notes.md",
    "docs/swift.md",
)


def sync(root: Path, *, check: bool = False) -> list[str]:
    stale = []
    for name in RESOURCES:
        content = (root / name).read_bytes()
        target = root / "crates/changenote" / name
        if target.is_file() and target.read_bytes() == content:
            continue
        stale.append(name)
        if not check:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
    return stale


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Report stale copies without writing")
    args = parser.parse_args()
    stale = sync(Path(__file__).resolve().parents[1], check=args.check)
    if args.check and stale:
        parser.exit(
            1,
            "Run uv run --no-project python scripts/sync-crate-resources.py\n"
            + "\n".join(stale) + "\n",
        )
