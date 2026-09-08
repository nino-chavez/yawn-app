#!/usr/bin/env python3
"""Collect an exact Git range for a human-reviewed Yawn changelog.

This command reads Git history only. It does not tag, publish, or infer that
a commit shipped. Commit messages are evidence to review, not reader copy.
"""
import argparse
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def git(*args, repo=ROOT):
    return subprocess.check_output(
        ["git", "-C", str(repo), *args], stdin=subprocess.DEVNULL,
        text=True, timeout=30,
    ).strip()


def collect(previous, target, version, repo=ROOT):
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("version must be X.Y.Z")
    start = git("rev-parse", "--verify", "--end-of-options", previous + "^{commit}", repo=repo)
    end = git("rev-parse", "--verify", "--end-of-options", target + "^{commit}", repo=repo)
    subprocess.run(
        ["git", "-C", str(repo), "merge-base", "--is-ancestor", start, end],
        stdin=subprocess.DEVNULL, check=True, timeout=30,
    )
    commits = []
    for sha in git("rev-list", "--reverse", f"{start}..{end}", repo=repo).splitlines():
        commits.append({
            "commit": sha,
            "subject": git("show", "-s", "--format=%s", sha, repo=repo),
            "body": git("show", "-s", "--format=%b", sha, repo=repo),
            "files": git("diff-tree", "--root", "--no-commit-id", "--name-only", "-r", sha, repo=repo).splitlines(),
        })
    return {
        "schema": "yawn-release-corpus/1", "version": version,
        "status": "input-for-review", "previousRef": previous, "targetRef": target,
        "previousCommit": start, "targetCommit": end,
        "range": f"{start}..{end}",
        "compareUrl": f"https://github.com/nino-chavez/yawn-app/compare/{start}...{end}",
        "commits": commits,
        "changedFiles": git("diff", "--name-status", start, end, repo=repo).splitlines(),
        "writingBrief": [
            "Write for a person deciding whether to update Yawn.",
            "Read the relevant diffs before turning commit messages into product claims.",
            "Lead with the main user-visible change, then group Added, Improved, and Fixed items.",
            "Include Before upgrading and Known limitations when the evidence requires them.",
            "Explain triggers and resulting behavior. Combine related commits; omit internal-only work.",
            "Keep draft, tested, installed, and publicly released states distinct.",
            "Do not turn synthetic tests into accuracy or real-meeting quality claims.",
            "Retain the exact source commit, prior release, date, and version in the release record.",
            "Review the draft before publishing. Never publish this raw corpus as release notes.",
        ],
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--from", dest="previous", required=True, help="Previous published release tag")
    parser.add_argument("--to", dest="target", required=True, help="Candidate commit or build tag")
    parser.add_argument("--version", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = collect(args.previous, args.target, args.version)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(f"Collected {len(result['commits'])} commits for review: {args.output}")
