#!/usr/bin/env python3
"""Compare the iPad Ink microphone modes on one fixed, known source.

play   Speak a public-domain passage from this Mac's speaker once per take,
       waiting for Enter between takes and warning if the volume changes.
score  Read each pulled meeting's recording-check.json and transcript.json and
       rank the modes by word recall of the passage, then by speech minus noise.

The ranking rule, the take order and its limits are in spike/ipad-ink/README.md,
"Microphone mode test". No audio is written to the repository; the clip is
generated in a scratch folder.
"""
from __future__ import annotations

import argparse
import difflib
import json
import re
import subprocess
import sys
from pathlib import Path

# The opening of the Gettysburg Address (1863), public domain.
PASSAGE = (
    "Four score and seven years ago our fathers brought forth on this continent, "
    "a new nation, conceived in Liberty, and dedicated to the proposition that all "
    "men are created equal. Now we are engaged in a great civil war, testing whether "
    "that nation, or any nation so conceived and so dedicated, can long endure. We "
    "are met on a great battle-field of that war. We have come to dedicate a portion "
    "of that field, as a final resting place for those who here gave their lives "
    "that that nation might live. It is altogether fitting and proper that we should "
    "do this."
)
VOICE = "Samantha"
TAKES = ["measurement", "default", "video_recording", "voice_processing", "measurement"]


def words(text: str) -> list[str]:
    return re.findall(r"[a-z0-9']+", text.lower().replace("-", " "))


def word_recall(reference: str, heard: str) -> float:
    ref, hyp = words(reference), words(heard)
    if not ref:
        return 0.0
    matched = sum(block.size for block in difflib.SequenceMatcher(a=ref, b=hyp, autojunk=False).get_matching_blocks())
    return matched / len(ref)


def output_volume() -> str:
    result = subprocess.run(["osascript", "-e", "output volume of (get volume settings)"], capture_output=True, text=True)
    return result.stdout.strip() or "unknown"


def play(args: argparse.Namespace) -> int:
    args.scratch.mkdir(parents=True, exist_ok=True)
    clip = args.scratch / "passage.aiff"
    if not clip.exists():
        subprocess.run(["say", "-v", VOICE, "-o", str(clip), PASSAGE], check=True)
    volume = output_volume()
    print(f"Mac output volume: {volume}. Keep it there for every take.")
    for number, mode in enumerate(TAKES, 1):
        input(f"\nTake {number} of {len(TAKES)}: on the iPad choose Microphone (test) = {mode}, "
              "tick consent, keep 1 day, tap Start recording, then press Enter here. ")
        now = output_volume()
        if now != volume:
            print(f"Warning: the volume changed from {volume} to {now}. Set it back before this take counts.")
        subprocess.run(["afplay", str(clip)], check=True)
        print("Done. Wait two seconds, tap Stop recording, then wait for the transcript before the next take.")
    return 0


def score(args: argparse.Namespace) -> int:
    rows = []
    for folder in args.meetings:
        check_path, transcript_path = folder / "recording-check.json", folder / "transcript.json"
        if not check_path.exists():
            print(json.dumps({"meeting": folder.name, "status": "no recording-check.json"}))
            continue
        check = json.loads(check_path.read_text(encoding="utf-8"))
        meeting = json.loads((folder / "meeting.json").read_text(encoding="utf-8"))
        transcript = json.loads(transcript_path.read_text(encoding="utf-8")) if transcript_path.exists() else []
        heard = " ".join(segment.get("text", "") for segment in transcript if segment.get("final", True))
        level, session = check["level"], check["session"]
        rows.append({
            "created_at": meeting.get("created_at"),
            "requested_mode": check["requested_mode"],
            "session_mode": session["mode"].replace("AVAudioSessionMode", ""),
            "voice_processing": session["voice_processing"],
            "input": session.get("data_source") or session.get("input_port_name"),
            "speech_dbfs": level["window_rms_p90_dbfs"],
            "noise_dbfs": level["window_rms_p10_dbfs"],
            "speech_minus_noise_db": level["speech_minus_noise_db"],
            "peak_dbfs": level["peak_dbfs"],
            "clipped_samples": level["clipped_samples"],
            "word_recall": round(word_recall(PASSAGE, heard), 3),
        })
    rows.sort(key=lambda row: row["created_at"] or "")
    for row in rows:
        print(json.dumps(row, sort_keys=True))
    measurement = [row for row in rows if row["requested_mode"] == "measurement"]
    if len(measurement) >= 2:
        first, last = measurement[0], measurement[-1]
        drift = {"recall_change": round(last["word_recall"] - first["word_recall"], 3),
                 "speech_level_change_db": round(last["speech_dbfs"] - first["speech_dbfs"], 2)}
        drift["setup_held"] = abs(drift["recall_change"]) <= 0.05 and abs(drift["speech_level_change_db"]) <= 2.0
        print(json.dumps({"measurement_first_vs_last": drift}, sort_keys=True))
    baseline = max((row["word_recall"] for row in measurement), default=None)
    eligible = [row for row in rows if row["clipped_samples"] == 0 and (baseline is None or row["word_recall"] >= baseline)]
    best = sorted(eligible, key=lambda row: (-row["word_recall"], -row["speech_minus_noise_db"]))
    print(json.dumps({"ranking": [row["requested_mode"] for row in best],
                      "rule": "word recall first, then speech minus noise; a mode that clips or recalls less than measurement is out"}))
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    play_parser = sub.add_parser("play")
    play_parser.add_argument("--scratch", type=Path, required=True, help="folder for the generated clip, outside the repo")
    play_parser.set_defaults(function=play)
    score_parser = sub.add_parser("score")
    score_parser.add_argument("meetings", type=Path, nargs="+", help="pulled meeting folders")
    score_parser.set_defaults(function=score)
    args = parser.parse_args()
    return args.function(args)


if __name__ == "__main__":
    sys.exit(main())
