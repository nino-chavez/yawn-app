#!/usr/bin/env python3
"""Stamp and check the source freshness of the staged runtime.

`worker/build_runtime.sh` copies repo sources into `apps/desktop/runtime`, and
the packaging lanes bundle that staging verbatim. Nothing else ties the staging
to the sources it came from: on 2026-09-01 a preview bundle shipped a current
Rust app beside a two-week-old meeting-capture helper and Python worker, and
both failed live capture. This module is the gate against that class.

`stamp` records a digest per runtime-relevant source file into
`source-digest.json` beside `app-runtime.json`. `check` recomputes the digests
and refuses on any difference. The stamp is a deliberate sibling of the
runtime manifest, not a field inside it: `prepare-preview-bundle.sh` re-runs
`build_manifest.py` against the bundle at sign time, so anything that manifest
carries is regenerated from *current* sources then — a stamp there would be
refreshed at the exact moment it must stay old.

The covered set mirrors `build_runtime.sh`'s copy and build steps; the
contract test in `tests/test_source_digest.py` parses that script and fails
when a copy step names a source this module does not cover.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
STAMP_NAME = "source-digest.json"
SCHEMA = "runtime-source-digest/1"

# Every file build_runtime.sh copies into the staging tree, by staging step.
COPIED_SOURCES = (
    "worker/__init__.py",
    "worker/main.py",
    "worker/adapters.py",
    "worker/product_contracts.py",
    "worker/storage.py",
    "worker/fbank.py",
    "worker/transcription.py",
    "worker/speech_results.py",
    "worker/apple_speech.py",
    "worker/embedding.py",
    "worker/note_bridge.py",
    "worker/note_generator_mlx.py",
    "spike/verify_capture.py",
    "spike/capture_health.py",
    "spike/dual_capture.py",
    "spike/speaker_gate.py",
    "spike/aec_bound.py",
    "notes/transcript.py",
    "notes/summarize.py",
    "notes/mlx_minilm.py",
    "notes/candidate_first.py",
)

# The staging tooling itself: a changed copy step or manifest writer means the
# staging may no longer reflect what a rebuild would produce.
TOOLING_SOURCES = (
    "worker/build_runtime.sh",
    "worker/build_manifest.py",
    "worker/source_digest.py",
)

# Swift packages built into staged binaries (audiotee, meeting-capture,
# permission-probe). Tests/ is excluded: it cannot change a shipped binary.
SWIFT_PACKAGES = (
    "capture/audiotee",
    "capture/permission-probe",
    "capture/apple-speech",
)


def _load_build_manifest():
    path = Path(__file__).resolve().with_name("build_manifest.py")
    spec = importlib.util.spec_from_file_location("yawn_build_manifest", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_build_manifest = _load_build_manifest()


def _validator_sources() -> tuple[str, ...]:
    """Relative paths of the note-validator sources, from their one owner."""
    return tuple(
        str(source.relative_to(REPO))
        for source in _build_manifest.VALIDATOR_SOURCES.values()
    )


def _swift_package_files(repo: Path) -> list[str]:
    files: list[str] = []
    for package in SWIFT_PACKAGES:
        root = repo / package
        for name in ("Package.swift", "Package.resolved"):
            if (root / name).is_file():
                files.append(f"{package}/{name}")
        sources = root / "Sources"
        if not sources.is_dir():
            raise SystemExit(f"runtime source tree is missing: {package}/Sources")
        files.extend(
            str(path.relative_to(repo))
            for path in sources.rglob("*")
            if path.is_file()
        )
    return files


def runtime_source_files(repo: Path) -> list[str]:
    """Sorted relative paths of every source the staged runtime derives from."""
    files = set(COPIED_SOURCES) | set(TOOLING_SOURCES) | set(_validator_sources())
    files.update(
        str(path.relative_to(repo))
        for path in (repo / "worker").glob("requirements-*.lock")
    )
    files.update(_swift_package_files(repo))
    return sorted(files)


def source_digests(repo: Path) -> dict[str, str]:
    digests = {}
    for relative in runtime_source_files(repo):
        path = repo / relative
        if not path.is_file():
            raise SystemExit(f"runtime source is missing: {relative}")
        digests[relative] = _build_manifest.sha256(path)
    return digests


def stamp_document(repo: Path) -> dict:
    return {"schema": SCHEMA, "files": source_digests(repo)}


def write_stamp(stage: Path, repo: Path) -> Path:
    target = stage / STAMP_NAME
    contents = (json.dumps(stamp_document(repo), indent=2) + "\n").encode("utf-8")
    _build_manifest.atomic_write(target, contents)
    return target


def check_stamp(stage: Path, repo: Path) -> list[str]:
    """Empty when the staging is fresh; otherwise the honest refusal, one line
    per finding, distinguishing never-staged and never-stamped from went-stale.
    """
    if not stage.is_dir():
        return [f"runtime is not staged at {stage} — run worker/build_runtime.sh"]
    stamp = stage / STAMP_NAME
    if not stamp.is_file():
        return [
            (
                "staged runtime carries no source stamp — it was staged before the"
                " freshness gate existed, not proven stale; run worker/build_runtime.sh"
                " to stage and stamp it"
            )
        ]
    try:
        document = json.loads(stamp.read_text(encoding="utf-8"))
    except ValueError:
        return [f"runtime source stamp is unreadable: {stamp} — run worker/build_runtime.sh"]
    if document.get("schema") != SCHEMA:
        return [
            (
                f"runtime source stamp has unrecognized schema {document.get('schema')!r}"
                " — run worker/build_runtime.sh"
            )
        ]
    recorded = document.get("files")
    if not isinstance(recorded, dict):
        return [
            (
                f"runtime source stamp carries no file digests: {stamp}"
                " — run worker/build_runtime.sh"
            )
        ]
    current = source_digests(repo)
    problems = []
    for path in sorted(set(recorded) | set(current)):
        if path not in current:
            problems.append(f"source removed since staging: {path}")
        elif path not in recorded:
            problems.append(f"source added since staging: {path}")
        elif recorded[path] != current[path]:
            problems.append(f"source changed since staging: {path}")
    if problems:
        problems.append("runtime staging predates source — run worker/build_runtime.sh")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("stamp", "check"))
    parser.add_argument("stage", type=Path, help="the staged runtime root")
    arguments = parser.parse_args()
    stage = arguments.stage.resolve()
    if arguments.command == "stamp":
        target = write_stamp(stage, REPO)
        count = len(json.loads(target.read_text(encoding="utf-8"))["files"])
        print(f"stamped {count} runtime sources into {target}")
        return 0
    problems = check_stamp(stage, REPO)
    if problems:
        for line in problems:
            print(f"source-digest: {line}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
