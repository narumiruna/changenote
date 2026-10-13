"""Locate the Rust changenote executable installed with this distribution."""

from pathlib import Path
import sys
import sysconfig


def find_changenote_bin() -> str:
    """Return the installed binary path without searching the system PATH."""
    name = "changenote.exe" if sys.platform == "win32" else "changenote"
    directories = [sysconfig.get_path("scripts")]
    # uv run --with can load this module from a different environment than sys.prefix.
    site_packages = Path(__file__).resolve().parent.parent
    if site_packages.name == "site-packages":
        if sys.platform == "win32":
            directories.append(str(site_packages.parent.parent / "Scripts"))
        else:
            directories.append(str(site_packages.parent.parent.parent / "bin"))
    directories.append(
        sysconfig.get_path("scripts", scheme=sysconfig.get_preferred_scheme("user"))
    )
    for directory in directories:
        if directory:
            path = Path(directory) / name
            if path.is_file():
                return str(path)
    raise FileNotFoundError("The changenote binary is missing; reinstall changenote.")
