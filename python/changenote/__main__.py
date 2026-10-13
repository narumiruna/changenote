"""Run the Rust CLI with the caller's arguments and working directory."""

import os
import subprocess
import sys

from . import find_changenote_bin


def main() -> None:
    binary = find_changenote_bin()
    args = [binary, *sys.argv[1:]]
    if sys.platform == "win32":
        try:
            result = subprocess.run(args, check=False)
        except KeyboardInterrupt:
            raise SystemExit(130) from None
        raise SystemExit(result.returncode)
    os.execv(binary, args)


if __name__ == "__main__":
    main()
