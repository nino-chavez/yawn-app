#!/usr/bin/env python3
"""Render the planted-trap script to two 16 kHz mono legs with macOS voices.

This is the direct-decode arm of docs/planted-trap-evaluation-scope.md. Each
speaker's lines go on their own leg at the script's timeline; the other leg is
silent while they speak, as on a clean headphone capture. A third file mixes
both legs for engines that take one stream.

The voices read the key's written text, so `say` decides how "$1,249" or
"ENG-4417" is pronounced. Synthetic speech is cleaner than a person, so trap
survival here is an upper bound, not a meeting-quality claim.

Output goes outside the repository, with a `speech-engine-corpus/1` manifest
that spike/speech-engine-comparison/run_comparison.py accepts.

Usage:
    python notes/render_trap_audio.py --out ~/Library/Application\\ Support/yawn-research/trap-check
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
import wave
from pathlib import Path

RATE = 16_000
GAP_SECONDS = 0.4
REPO = Path(__file__).resolve().parent.parent
KEY = REPO / "notes" / "fixtures" / "synthetic" / "trap-briefing.json"
VOICES = {"Me": "Samantha", "Them": "Daniel"}


def sha256(path: Path) -> str:
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def speak(text: str, voice: str, scratch: Path) -> bytes:
    """Return 16-bit mono 16 kHz PCM frames for one line."""
    target = scratch / "line.wav"
    subprocess.run(
        ["say", "-v", voice, "--file-format=WAVE", "--data-format=LEI16@16000",
         "-o", str(target), text],
        check=True, stdin=subprocess.DEVNULL, timeout=120,
    )
    with wave.open(str(target), "rb") as audio:
        if (audio.getnchannels(), audio.getsampwidth(), audio.getframerate()) != (1, 2, RATE):
            raise SystemExit("say did not produce mono 16 kHz 16-bit PCM")
        return audio.readframes(audio.getnframes())


def write_wav(path: Path, frames: bytes) -> None:
    with wave.open(str(path), "wb") as audio:
        audio.setnchannels(1)
        audio.setsampwidth(2)
        audio.setframerate(RATE)
        audio.writeframes(frames)


def mix(a: bytes, b: bytes) -> bytes:
    import array

    left, right = array.array("h", a), array.array("h", b)
    return array.array(
        "h", (max(-32768, min(32767, x + y)) for x, y in zip(left, right, strict=True))
    ).tobytes()


def render(out: Path) -> dict:
    key = json.loads(KEY.read_text())
    gap = b"\0\0" * int(RATE * GAP_SECONDS)
    legs = {speaker: bytearray() for speaker in key["speakers"]}
    timeline = []
    with tempfile.TemporaryDirectory() as tmp:
        for line in key["lines"]:
            frames = speak(line["text"], VOICES[line["speaker"]], Path(tmp)) + gap
            start = len(next(iter(legs.values()))) / 2 / RATE
            for speaker, leg in legs.items():
                leg.extend(frames if speaker == line["speaker"] else b"\0" * len(frames))
            timeline.append({"line": line["id"], "start_seconds": round(start, 3),
                             "end_seconds": round(start + len(frames) / 2 / RATE, 3)})

    out.mkdir(parents=True, exist_ok=True, mode=0o700)
    files = {f"trap-{speaker.lower()}": bytes(leg) for speaker, leg in legs.items()}
    files["trap-mixed"] = mix(bytes(legs["Me"]), bytes(legs["Them"]))
    cases = []
    for case_id, frames in files.items():
        path = out / f"{case_id}.wav"
        if path.exists():
            raise SystemExit(f"refusing to overwrite {path.name}")
        write_wav(path, frames)
        cases.append({"id": case_id, "audio": str(path), "audio_sha256": sha256(path)})
    manifest = {
        "schema": "speech-engine-corpus/1",
        "key_sha256": sha256(KEY),
        "voices": VOICES,
        "say_note": "macOS `say`; voice assets vary by OS version, so hashes pin this render only",
        "cases": cases,
        "timeline": timeline,
    }
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    out = args.out.expanduser().resolve()
    if out.is_relative_to(REPO.resolve()):
        print("refusing to write audio inside the repository", file=sys.stderr)
        return 2
    manifest = render(out)
    seconds = manifest["timeline"][-1]["end_seconds"]
    print(f"rendered {len(manifest['cases'])} files, {seconds:.1f} s, to {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
