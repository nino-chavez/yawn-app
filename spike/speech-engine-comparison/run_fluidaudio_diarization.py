#!/usr/bin/env python3
"""Run FluidAudio diarization arms on one frozen WAV for score_diarization.py.

Each arm gets its own directory holding the corpus manifest, run receipt, and
``ami-full.json`` in the ``{"segments": [{start, end, speaker}]}`` shape the
Nemotron trial produced, so the existing scorer runs on it unchanged.  Outputs
stay outside the source tree; only content-free scores belong in Git.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import time
import wave
from pathlib import Path
from typing import Any

CASE_ID = "ami-full"
ARMS: dict[str, list[str]] = {
    "nemotron3-offline": ["nemotron3-diarize", "{audio}", "--variant", "offline", "--output", "{raw}"],
    "sortformer-offline": ["sortformer", "{audio}", "--offline", "--output", "{raw}"],
    "pipeline-offline": ["process", "{audio}", "--mode", "offline", "--output", "{raw}"],
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def wav_seconds(path: Path) -> float:
    with wave.open(str(path), "rb") as audio:
        return audio.getnframes() / audio.getframerate()


def rttm_segments(text: str) -> list[dict[str, Any]]:
    segments = []
    for line in text.splitlines():
        fields = line.split()
        if len(fields) >= 8 and fields[0] == "SPEAKER":
            start, duration = float(fields[3]), float(fields[4])
            segments.append({"start": start, "end": start + duration, "speaker": fields[7]})
    return segments


def json_segments(document: Any) -> list[dict[str, Any]]:
    """Find the one list of timed, speaker-tagged objects in a FluidAudio result."""
    found: list[list[dict[str, Any]]] = []

    def visit(node: Any) -> None:
        if isinstance(node, dict):
            for value in node.values():
                visit(value)
        elif isinstance(node, list):
            if node and all(isinstance(item, dict) for item in node) and timed(node[0]):
                found.append(node)
            for item in node:
                visit(item)

    if not found and isinstance(document, (dict, list)):
        visit(document)
    if len(found) != 1:
        raise ValueError(f"expected one segment list, found {len(found)}")
    return [normalize(item) for item in found[0]]


START_KEYS = ("start", "startTime", "startTimeSeconds", "startSeconds")
END_KEYS = ("end", "endTime", "endTimeSeconds", "endSeconds")
SPEAKER_KEYS = ("speaker", "speakerId", "speakerIndex", "speaker_id")


def pick(item: dict[str, Any], keys: tuple[str, ...]) -> Any:
    for key in keys:
        if key in item:
            return item[key]
    raise KeyError(f"none of {keys} in segment")


def timed(item: dict[str, Any]) -> bool:
    return any(k in item for k in START_KEYS) and any(k in item for k in END_KEYS) and any(k in item for k in SPEAKER_KEYS)


def normalize(item: dict[str, Any]) -> dict[str, Any]:
    return {"start": float(pick(item, START_KEYS)), "end": float(pick(item, END_KEYS)), "speaker": str(pick(item, SPEAKER_KEYS))}


def run_arm(cli: Path, arm: str, audio: Path, root: Path) -> None:
    arm_dir = root / arm
    arm_dir.mkdir(parents=True, exist_ok=True)
    raw = arm_dir / ("raw.rttm" if arm.startswith("nemotron3") else "raw.json")
    command = [str(cli)] + [part.format(audio=audio, raw=raw) for part in ARMS[arm]]
    started = time.monotonic()
    with (arm_dir / "run.log").open("w") as log:
        completed = subprocess.run(command, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, timeout=3600)
    wall = time.monotonic() - started
    receipt: dict[str, Any] = {"exit_code": completed.returncode, "wall_seconds_approx": round(wall, 2)}
    if completed.returncode == 0:
        text = raw.read_text(encoding="utf-8")
        segments = rttm_segments(text) if raw.suffix == ".rttm" else json_segments(json.loads(text))
        output = arm_dir / f"{CASE_ID}.json"
        output.write_text(json.dumps({"segments": segments}, indent=1) + "\n", encoding="utf-8")
        receipt |= {"output_sha256": sha256(output), "output_bytes": output.stat().st_size}
    corpus = {"cases": [{"id": CASE_ID, "audio": str(audio), "audio_sha256": sha256(audio), "duration": wav_seconds(audio)}]}
    corpus_path = arm_dir / "corpus.json"
    corpus_path.write_text(json.dumps(corpus, indent=1) + "\n", encoding="utf-8")
    receipt["audio_sha256"] = corpus["cases"][0]["audio_sha256"]
    run_receipt = {
        "schema": "fluidaudio-diarization-local-run/1",
        "arm": arm,
        "command": " ".join(["fluidaudiocli"] + ARMS[arm]),
        "manifest_sha256": sha256(corpus_path),
        "cases": {CASE_ID: receipt},
    }
    (arm_dir / "run-receipt.json").write_text(json.dumps(run_receipt, indent=1) + "\n", encoding="utf-8")
    print(f"{arm}: exit {completed.returncode}, {wall:.1f} s")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True, help="built fluidaudiocli binary")
    parser.add_argument("--audio", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True, help="private output root outside the repository")
    parser.add_argument("--arm", choices=sorted(ARMS), action="append")
    args = parser.parse_args()
    for arm in args.arm or list(ARMS):
        run_arm(args.cli.resolve(), arm, args.audio.resolve(), args.out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
