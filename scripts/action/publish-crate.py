"""Publish the prepared release crate, or skip an existing crates.io version."""

import json
import os
import subprocess
import sys
from pathlib import Path
from urllib.error import HTTPError
from urllib.request import Request, urlopen


def publish(root: Path, version: str) -> None:
    request = Request(
        f"https://crates.io/api/v1/crates/changenote/{version}",
        headers={
            "User-Agent": "changenote-release (https://github.com/narumiruna/changenote)"
        },
    )
    try:
        with urlopen(request, timeout=30) as response:
            published = json.load(response)["version"]
    except HTTPError as error:
        if error.code != 404:
            raise
        error.close()
    else:
        if published["crate"] != "changenote" or published["num"] != version:
            raise ValueError("crates.io response does not match the release package")
        if published["yanked"] is not False:
            raise ValueError("The crates.io version is yanked or has invalid metadata")
        print(f"changenote {version} is already on crates.io; skipping upload.")
        return

    if not os.environ.get("CARGO_REGISTRY_TOKEN"):
        raise ValueError("CARGO_REGISTRY_TOKEN is required to publish to crates.io")
    # Legacy VERSION releases have build-only Cargo version changes in this checkout.
    subprocess.run(
        [
            "cargo",
            "publish",
            "--locked",
            "--registry",
            "crates-io",
            "--package",
            "changenote",
            "--allow-dirty",
        ],
        cwd=root,
        check=True,
    )


if __name__ == "__main__":
    publish(Path(sys.argv[1]), sys.argv[2])
