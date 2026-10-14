"""Read-only routing for this repository's Release workflow."""

import json
import os
from pathlib import Path
import re
import subprocess
import sys

BATCH = ".changenote/releases.json"
MARKER = "<!-- changenote-release-pr -->"


def command(*args):
    return subprocess.check_output(args, text=True).strip()


def route(event_name, event, repository, ref):
    result = {"manage-pr": "false", "publish": "false", "release-commit": ""}
    if ref != "refs/heads/main":
        return result
    if event_name == "workflow_dispatch":
        operation = event.get("inputs", {}).get("operation", "manage-pr")
        if operation == "manage-pr":
            result["manage-pr"] = "true"
        elif operation == "retry-publication":
            result["publish"] = "true"
        else:
            raise ValueError("Unknown manual operation")
        return result
    if event_name != "push":
        raise ValueError("Unsupported event")
    if event.get("forced") is not False or event.get("deleted") is not False:
        raise ValueError("Forced or deleted pushes cannot authorize publication")
    before, after = event["before"], event["after"]
    for sha in (before, after):
        if not re.fullmatch(r"[0-9a-f]{40}", sha) or sha == "0" * 40:
            raise ValueError("Push requires two existing commit SHAs")
        command("git", "cat-file", "-e", sha + "^{commit}")
    # Require the previous branch tip on the first-parent chain, not just ancestry.
    history = command("git", "rev-list", "--first-parent", after).splitlines()
    if before not in history:
        raise ValueError("Previous tip is not on the first-parent history")
    result["manage-pr"] = "true"
    release = ""
    for commit in history[: history.index(before)]:
        changed = command(
            "git", "diff", "--name-only", commit + "^", commit, "--", BATCH
        )
        if changed:
            release = commit
            break
    if not release:
        return result
    entry = command("git", "ls-tree", release, "--", BATCH).split()
    if not entry or entry[0] != "100644":
        raise ValueError("Release batch must be a regular file")
    batch = json.loads(command("git", "show", release + ":" + BATCH))
    if (
        not isinstance(batch, list)
        or not batch
        or not all(isinstance(item, dict) for item in batch)
    ):
        raise ValueError("Release batch must be a non-empty object array")
    pages = json.loads(
        command(
            "gh",
            "api",
            "--paginate",
            "--slurp",
            f"repos/{repository}/commits/{release}/pulls?per_page=100",
        )
    )
    if (
        not isinstance(pages, list)
        or not pages
        or not all(isinstance(page, list) for page in pages)
    ):
        raise ValueError("Invalid pull request API response")
    pulls = [pull for page in pages for pull in page]
    matches = [
        pull
        for pull in pulls
        if (
            pull.get("state") == "closed"
            and pull.get("merged_at")
            and pull.get("merge_commit_sha") == release
            and pull.get("base", {}).get("ref") == "main"
            and pull.get("base", {}).get("repo", {}).get("full_name") == repository
            and pull.get("head", {}).get("ref")
            == "narumi/chore/changenote-release-main"
            and (pull.get("head", {}).get("repo") or {}).get("full_name") == repository
            and (pull.get("body") or "").startswith(MARKER)
        )
    ]
    if len(matches) != 1:
        raise ValueError("New batch requires exactly one merged managed Release PR")
    result.update({"publish": "true", "release-commit": release})
    return result


def check_preview(expected, actual, ready):
    if ready != "true":
        raise ValueError("Publication preview is not ready")
    if expected and expected != actual:
        raise ValueError("Release batch changed after routing; use an explicit retry")


def main():
    if sys.argv[1:] and sys.argv[1] == "check-preview":
        check_preview(
            os.environ["EXPECTED_RELEASE_COMMIT"],
            os.environ["PREVIEW_RELEASE_COMMIT"],
            os.environ["PREVIEW_READY"],
        )
        return
    repository = os.environ["GITHUB_REPOSITORY"]
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ValueError("Invalid repository")
    result = route(
        os.environ["GITHUB_EVENT_NAME"],
        json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text()),
        repository,
        os.environ["GITHUB_REF"],
    )
    with Path(os.environ["GITHUB_OUTPUT"]).open("a") as output:
        for key, value in result.items():
            output.write(f"{key}={value}\n")


if __name__ == "__main__":
    try:
        main()
    except (
        ValueError,
        KeyError,
        IndexError,
        AttributeError,
        TypeError,
        subprocess.CalledProcessError,
    ) as error:
        sys.exit(f"Release routing failed: {error}")
