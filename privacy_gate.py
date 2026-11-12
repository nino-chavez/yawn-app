#!/usr/bin/env python3
"""Refuse transcript-like content in tracked JSON without printing its values.

Run before committing:

    python3 privacy_gate.py

The check reads the Git index, the corresponding tracked working-tree files, and
unignored untracked JSON. Scanning the index prevents a safe working copy from
hiding sensitive content that is already staged; scanning the other two catches
the inverse before staging. Intentionally ignored research data stays outside
this commit gate.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

SENSITIVE_KEYS = frozenset(
    {
        "claim",
        "content",
        "decision",
        "excerpt",
        "heard",
        "open_question",
        "passage",
        "phrase",
        "quote",
        "quotes",
        "segments",
        "source_quote",
        "source_text",
        "summary",
        "text",
        "tokens",
        "transcript",
        "transcript_text",
        "turn",
        "turns",
        "utterance",
        "utterances",
        "word",
        "words",
    }
)
# Invented test material may hold speech-shaped fields. A file is exempt only
# when it sits directly in this directory AND its top-level object carries
# `"synthetic": true`. Either alone is not enough, and the index and the
# working copy are judged separately, so a staged file without the header is
# still refused. Never put a real recording's text here: the header is an
# attestation that a person wrote the content from scratch.
SYNTHETIC_FIXTURE_DIR = "notes/fixtures/synthetic"
TRANSCRIPT_SCHEMAS = (
    "capture-protocol",
    "evidence-bound-note",
    "mic-segments",
    "note/",
    "transcript",
)


def git(repo: Path, *args: str) -> bytes:
    proc = subprocess.run(
        ["git", "-C", str(repo), *args],
        check=False,
        capture_output=True,
    )
    if proc.returncode:
        message = proc.stderr.decode(errors="replace").strip()
        raise RuntimeError(message or f"git {' '.join(args)} failed")
    return proc.stdout


def tracked_json(repo: Path) -> list[Path]:
    raw = git(repo, "ls-files", "-z", "--", "*.json", "*.JSON")
    return [Path(item.decode()) for item in raw.split(b"\0") if item]


def untracked_json(repo: Path) -> list[Path]:
    raw = git(
        repo,
        "ls-files",
        "--others",
        "--exclude-standard",
        "-z",
        "--",
        "*.json",
        "*.JSON",
    )
    return [Path(item.decode()) for item in raw.split(b"\0") if item]


def json_path(parts: tuple[object, ...]) -> str:
    rendered = "$"
    for part in parts:
        # Property names are data too. A transcript can encode human speech in a
        # key, so refusal output never repeats arbitrary object keys.
        rendered += f"[{part}]" if isinstance(part, int) else ".*"
    return rendered


def contains_language(value: object) -> bool:
    if isinstance(value, str):
        return bool(value.strip())
    if isinstance(value, list):
        return any(contains_language(item) for item in value)
    if isinstance(value, dict):
        return any(contains_language(item) for item in value.values())
    return False


def key_resembles_language(value: object) -> bool:
    return isinstance(value, str) and len(value.split()) >= 3


def find_sensitive_content(value: object) -> list[str]:
    """Return locations only. Never return or render the sensitive values."""
    findings: list[str] = []

    def walk(node: object, path: tuple[object, ...]) -> None:
        if isinstance(node, dict):
            schema = node.get("schema")
            if isinstance(schema, str) and any(
                marker in schema.casefold() for marker in TRANSCRIPT_SCHEMAS
            ):
                findings.append(f"{json_path((*path, 'schema'))} declares speech data")

            for key, child in node.items():
                normalised = str(key).casefold().replace("-", "_")
                child_path = (*path, key)
                if key_resembles_language(key):
                    findings.append(
                        f"{json_path(child_path)} object key resembles human-language content"
                    )
                if normalised in SENSITIVE_KEYS and contains_language(child):
                    findings.append(
                        f"{json_path(child_path)} field {normalised!r} "
                        "contains human-language content"
                    )
                walk(child, child_path)
        elif isinstance(node, list):
            for index, child in enumerate(node):
                walk(child, (*path, index))

    walk(value, ())
    return findings


def in_synthetic_dir(relative: Path) -> bool:
    return relative.parent.as_posix() == SYNTHETIC_FIXTURE_DIR


def declares_synthetic(document: object) -> bool:
    return isinstance(document, dict) and document.get("synthetic") is True


def parse_and_scan(payload: bytes, label: str, relative: Path | None = None) -> list[str]:
    try:
        document = json.loads(payload)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        return [f"{label}: cannot inspect tracked JSON safely ({exc.__class__.__name__})"]
    if relative is not None and in_synthetic_dir(relative) and declares_synthetic(document):
        return []
    return [f"{label}: {finding}" for finding in find_sensitive_content(document)]


def synthetic_fixtures(repo: Path) -> list[Path]:
    return [
        relative
        for relative in [*tracked_json(repo), *untracked_json(repo)]
        if in_synthetic_dir(relative)
    ]


def scan_repo(repo: Path) -> list[str]:
    findings: list[str] = []
    tracked = tracked_json(repo)
    for relative in tracked:
        path_label = relative.as_posix()
        index_payload = git(repo, "show", f":{path_label}")
        findings.extend(parse_and_scan(index_payload, f"index:{path_label}", relative))

        worktree_path = repo / relative
        if worktree_path.exists():
            findings.extend(
                parse_and_scan(worktree_path.read_bytes(), f"worktree:{path_label}", relative)
            )

    for relative in untracked_json(repo):
        path_label = relative.as_posix()
        findings.extend(
            parse_and_scan((repo / relative).read_bytes(), f"untracked:{path_label}", relative)
        )
    return findings


def run_self_test() -> int:
    safe = {"measurements": [{"score": 0.42, "sha256": "abc"}]}
    unsafe = {
        "schema": "transcript/1",
        "turns": [{"text": "do not print this"}],
    }
    alternate = {"segments": [{"tokens": ["also private"]}]}
    hostile_key = {"private words as a key": 1}
    locations = find_sensitive_content(unsafe)
    hostile_locations = find_sensitive_content(hostile_key)
    checks = {
        "numeric measurement artifact passes": not find_sensitive_content(safe),
        "transcript schema is refused": any("declares speech data" in item for item in locations),
        "speech text is refused": any("field 'text'" in item for item in locations),
        "segment token shape is refused": bool(find_sensitive_content(alternate)),
        "sensitive value is never returned": all(
            "do not print this" not in item for item in locations
        ),
        "arbitrary property name is never returned": all(
            "private words as a key" not in item for item in hostile_locations
        ),
        "human-language object key is refused": bool(hostile_locations),
    }
    speech_shaped = json.dumps({"synthetic": True, "turns": [{"text": "invented line"}]})
    unmarked = json.dumps({"turns": [{"text": "invented line"}]})
    marked_false = json.dumps({"synthetic": "true", "turns": [{"text": "invented line"}]})
    inside = Path(SYNTHETIC_FIXTURE_DIR) / "fixture.json"
    nested = Path(SYNTHETIC_FIXTURE_DIR) / "deeper" / "fixture.json"
    outside = Path("notes") / "fixture.json"
    checks.update(
        {
            "synthetic fixture with header in its directory passes": not parse_and_scan(
                speech_shaped.encode(), "t", inside
            ),
            "synthetic directory without header is refused": bool(
                parse_and_scan(unmarked.encode(), "t", inside)
            ),
            "header that is not literally true is refused": bool(
                parse_and_scan(marked_false.encode(), "t", inside)
            ),
            "header outside the directory is refused": bool(
                parse_and_scan(speech_shaped.encode(), "t", outside)
            ),
            "header in a subdirectory is refused": bool(
                parse_and_scan(speech_shaped.encode(), "t", nested)
            ),
            "header without a path is refused": bool(
                parse_and_scan(speech_shaped.encode(), "t")
            ),
        }
    )
    failed = [name for name, passed in checks.items() if not passed]
    if failed:
        for name in failed:
            print(f"FAIL: {name}", file=sys.stderr)
        return 1
    print(f"privacy gate self-test: OK ({len(checks)} checks)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return run_self_test()

    repo = Path(__file__).resolve().parent
    findings = scan_repo(repo)
    if findings:
        print(
            "privacy gate: REFUSED — repository JSON contains transcript-like content",
            file=sys.stderr,
        )
        for finding in findings[:20]:
            print(f"  {finding}", file=sys.stderr)
        if len(findings) > 20:
            print(f"  ... {len(findings) - 20} more location(s)", file=sys.stderr)
        print("Values were deliberately not printed.", file=sys.stderr)
        return 1

    print(
        "privacy gate: OK "
        f"({len(tracked_json(repo))} tracked and "
        f"{len(untracked_json(repo))} untracked JSON files inspected; "
        f"{len(synthetic_fixtures(repo))} in {SYNTHETIC_FIXTURE_DIR}/ judged by their header)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
