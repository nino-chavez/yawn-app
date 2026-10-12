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
import os
import subprocess
import time
import wave
from pathlib import Path
from typing import Any

CASE_ID = "ami-full"
# Diarizers emit on a frame grid (80 ms for Nemotron and Sortformer), so a segment
# that runs to the end of the file can end up to one frame past the audio.  Only
# that overshoot is trimmed; anything larger is left for the scorer to reject.
END_OVERSHOOT_TOLERANCE_SECONDS = 0.08
ARMS: dict[str, list[str]] = {
    "nemotron3-offline": ["nemotron3-diarize", "{audio}", "--variant", "offline", "--output", "{raw}"],
    # FluidAudio's documented iPhone/iPad picks: Neural Engine only, W8A8, 95 MB.
    "nemotron3-fast32-split-w8a8": ["nemotron3-diarize", "{audio}", "--variant", "fast32-split-w8a8", "--output", "{raw}"],
    "nemotron3-c128-split-w8a8": ["nemotron3-diarize", "{audio}", "--variant", "c128-split-w8a8", "--output", "{raw}"],
    "sortformer-offline": ["sortformer", "{audio}", "--offline", "--output", "{raw}"],
    "pipeline-offline": ["process", "{audio}", "--mode", "offline", "--output", "{raw}"],
    # The Nemotron trial's own runtime (NeMo-Speech.cpp, GGUF), as a baseline on new audio.
    "nemotron-gguf": ["{nemo}", "diarize", "{audio}", "--model", "{gguf}", "--device", "metal", "--preset", "v3-offline",
                      "--format", "json", "--output", "{raw}"],
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


def run_arm(cli: Path, arm: str, audio: Path, root: Path, nemo: Path | None = None, gguf: Path | None = None) -> None:
    arm_dir = root / arm
    arm_dir.mkdir(parents=True, exist_ok=True)
    raw = arm_dir / ("raw.rttm" if arm.startswith("nemotron3") else "raw.json")
    output = arm_dir / f"{CASE_ID}.json"
    # A re-run into the same directory must not reuse an earlier pass's files:
    # a CLI that exits 0 without writing would otherwise reproduce the old hash.
    for stale in (raw, output):
        stale.unlink(missing_ok=True)
    parts = [part.format(audio=audio, raw=raw, nemo=nemo, gguf=gguf) for part in ARMS[arm]]
    command = parts if ARMS[arm][0] == "{nemo}" else [str(cli)] + parts
    env = dict(os.environ)
    if ARMS[arm][0] == "{nemo}":
        if nemo is None or gguf is None:
            raise SystemExit(f"{arm} needs --nemo-cli and --gguf")
        # The build's rpath can point at a removed checkout; its libraries sit beside the binary.
        env["DYLD_LIBRARY_PATH"] = str(nemo.parent)
    started = time.monotonic()
    with (arm_dir / "run.log").open("w") as log:
        completed = subprocess.run(
            command, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, timeout=3600, env=env
        )
    wall = time.monotonic() - started
    receipt: dict[str, Any] = {"exit_code": completed.returncode, "wall_seconds_approx": round(wall, 2)}
    if completed.returncode == 0 and not raw.is_file():
        receipt["exit_code"] = "missing-output"
    duration = wav_seconds(audio)
    if receipt["exit_code"] == 0:
        text = raw.read_text(encoding="utf-8")
        segments = rttm_segments(text) if raw.suffix == ".rttm" else json_segments(json.loads(text))
        trimmed = [s["end"] - duration for s in segments if 0 < s["end"] - duration <= END_OVERSHOOT_TOLERANCE_SECONDS]
        for segment in segments:
            if 0 < segment["end"] - duration <= END_OVERSHOOT_TOLERANCE_SECONDS:
                segment["end"] = duration
        output.write_text(json.dumps({"segments": segments}, indent=1) + "\n", encoding="utf-8")
        receipt |= {"output_sha256": sha256(output), "output_bytes": output.stat().st_size}
        receipt["trimmed_end_overshoot"] = {"segments": len(trimmed), "max_seconds": round(max(trimmed, default=0.0), 4)}
    corpus = {"cases": [{"id": CASE_ID, "audio": str(audio), "audio_sha256": sha256(audio), "duration": duration}]}
    corpus_path = arm_dir / "corpus.json"
    corpus_path.write_text(json.dumps(corpus, indent=1) + "\n", encoding="utf-8")
    receipt["audio_sha256"] = corpus["cases"][0]["audio_sha256"]
    run_receipt = {
        "schema": "fluidaudio-diarization-local-run/1",
        "arm": arm,
        "command": " ".join(ARMS[arm] if ARMS[arm][0] == "{nemo}" else ["fluidaudiocli"] + ARMS[arm]),
        "manifest_sha256": sha256(corpus_path),
        "cases": {CASE_ID: receipt},
    }
    (arm_dir / "run-receipt.json").write_text(json.dumps(run_receipt, indent=1) + "\n", encoding="utf-8")
    print(f"{arm}: exit {receipt['exit_code']}, {wall:.1f} s")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True, help="built fluidaudiocli binary")
    parser.add_argument("--audio", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True, help="private output root outside the repository")
    parser.add_argument("--arm", choices=sorted(ARMS), action="append")
    parser.add_argument("--nemo-cli", type=Path, help="NeMo-Speech.cpp nemo-speech binary, for the nemotron-gguf arm")
    parser.add_argument("--gguf", type=Path, help="pinned Nemotron 3 Diarization GGUF, for the nemotron-gguf arm")
    args = parser.parse_args()
    nemo = args.nemo_cli.resolve() if args.nemo_cli else None
    gguf = args.gguf.resolve() if args.gguf else None
    for arm in args.arm or [name for name in ARMS if name != "nemotron-gguf" or nemo]:
        run_arm(args.cli.resolve(), arm, args.audio.resolve(), args.out, nemo, gguf)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
