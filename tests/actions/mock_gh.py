"""File-backed GitHub mock. Only test repositories can use this executable."""

import json
import os
from pathlib import Path
import subprocess
import sys

path = Path(os.environ["MOCK_GH_STATE"])
state = json.loads(path.read_text())
args = sys.argv[1:]
state.setdefault("calls", []).append(args)


def save():
    path.write_text(json.dumps(state))


def emit(value):
    save()
    print(json.dumps(value))
    sys.exit(0)


def error(message):
    save()
    print(message, file=sys.stderr)
    sys.exit(1)


if state.get("api_fail"):
    error("API unavailable")
if args[0] == "api":
    endpoint = next(a for a in args[1:] if a.startswith("repos/"))
    method = args[args.index("--method") + 1] if "--method" in args else "GET"
    payload = (
        json.loads(Path(args[args.index("--input") + 1]).read_text())
        if "--input" in args
        else {}
    )
    if endpoint == "repos/test/repo":
        repo = {
            "clone_url": state["remote"],
            "default_branch": "main",
        }
        if not state.get("omit_permissions"):
            repo["permissions"] = {"push": not state.get("no_write")}
        emit(repo)
    if "/commits/" in endpoint and "/pulls?" in endpoint:
        sha = endpoint.split("/commits/", 1)[1].split("/", 1)[0]
        emit(state.get("commit_pulls", {}).get(sha, [[]]))
    if "/pulls?" in endpoint:
        for pr in state["pulls"]:
            sha = subprocess.check_output(
                [
                    "git",
                    "ls-remote",
                    state["remote"],
                    "refs/heads/" + pr["head"]["ref"],
                ],
                text=True,
            ).split()
            if sha:
                pr["head"]["sha"] = sha[0]
        emit([state["pulls"]])
    if endpoint.endswith("/pulls") and method == "POST":
        if state.get("pr_fail"):
            error("PR permission denied")
        pr = {
            "number": len(state["pulls"]) + 1,
            "state": "open",
            "body": payload["body"],
            "title": payload["title"],
            "head": {"ref": payload["head"], "repo": {"full_name": "test/repo"}},
            "base": {"ref": payload["base"]},
        }
        state["pulls"].append(pr)
        if state.get("pr_lost_response"):
            error("Lost PR creation response")
        emit(pr)
    if "/pulls/" in endpoint and method == "PATCH":
        number = int(endpoint.rsplit("/", 1)[1])
        pr = next(pr for pr in state["pulls"] if pr["number"] == number)
        pr.update({k: v for k, v in payload.items() if k in ("state", "title", "body")})
        emit(pr)
    if "/releases?" in endpoint:
        if state.get("releases_api_fail"):
            error("Release API unavailable")
        emit([state["releases"]])
if args[:2] == ["release", "create"]:
    tag = args[2]
    if state.get("release_move_tag") == tag:
        subprocess.run(
            [
                "git",
                "--git-dir=" + state["remote"],
                "update-ref",
                "refs/tags/" + tag,
                state["move_sha"],
            ],
            check=True,
        )
    if state.get("release_fail") == tag:
        error("Release failed")
    if state.get("release_race") == tag:
        state["releases"].append({"tag_name": tag, "draft": False})
        error("Release already exists")
    notes = Path(args[args.index("--notes-file") + 1]).read_text()
    state["releases"].append({"tag_name": tag, "draft": False, "body": notes})
    emit({"tag_name": tag})
error("Unexpected gh invocation: " + repr(args))
