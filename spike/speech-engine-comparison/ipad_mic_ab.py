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
import datetime as dt
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
# Round 2 (README, "Microphone mode test"): volume:mode per take.
DEFAULT_TAKES = ("50:measurement,50:voice_processing,50:measurement_full_gain,50:measurement,"
                 "25:measurement,25:default,12:measurement,12:default")
LABELS = {"measurement": "Measurement", "measurement_full_gain": "Measurement, full input gain",
          "default": "Default", "video_recording": "Video recording", "voice_processing": "Voice processing"}


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


def parse_takes(spec: str) -> list[tuple[int, str]]:
    takes = []
    for item in spec.split(","):
        volume, mode = item.strip().split(":")
        if mode not in LABELS:
            raise SystemExit(f"unknown mode {mode}")
        takes.append((int(volume), mode))
    return takes


def play(args: argparse.Namespace) -> int:
    args.scratch.mkdir(parents=True, exist_ok=True)
    clip = args.scratch / "passage.aiff"
    if not clip.exists():
        subprocess.run(["say", "-v", VOICE, "-o", str(clip), PASSAGE], check=True)
    log = args.scratch / "takes-log.jsonl"
    takes = parse_takes(args.takes)
    for number, (volume, mode) in enumerate(takes, 1):
        input(f"\nTake {number} of {len(takes)}: set the Mac volume to {volume}. On the iPad choose "
              f"Microphone (test) = {LABELS[mode]}, tick consent, keep 1 day, tap Start recording, then press Enter here. ")
        while output_volume() != str(volume):
            input(f"The Mac volume is {output_volume()}, not {volume}. Set it to {volume}, then press Enter. ")
        started = dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")
        subprocess.run(["afplay", str(clip)], check=True)
        with log.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps({"take": number, "mode": mode, "volume": volume, "played_at": started}) + "\n")
        print("Done. Wait two seconds, tap Stop recording, then wait for the transcript before the next take.")
    print(f"\nAll {len(takes)} takes played. Log: {log}")
    return 0


def score(args: argparse.Namespace) -> int:
    played = []
    if args.log and args.log.exists():
        played = [json.loads(line) for line in args.log.read_text(encoding="utf-8").splitlines() if line.strip()]
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
        created = dt.datetime.fromisoformat(meeting["created_at"].replace("Z", "+00:00"))
        # A take is played within two minutes after its recording starts. The
        # Mac and iPad clocks differ and both round to whole seconds, so a
        # little negative skew still matches.
        match = next((take for take in played
                      if -10 <= (dt.datetime.fromisoformat(take["played_at"]) - created).total_seconds() <= 120
                      and take["mode"] == check["requested_mode"]), None)
        if played and not match and args.assume_volume is None:
            print(json.dumps({"meeting": folder.name[:8], "warning": "no logged take matches this recording; its volume is unknown and it is left out of every ranking"}))
        rows.append({
            "volume": match["volume"] if match else args.assume_volume,
            "engine_restarts": check.get("engine_restarts"),
            "input_gain": session.get("input_gain"),
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
            "duration_seconds": check.get("duration_seconds", 0),
        })
    rows.sort(key=lambda row: row["created_at"] or "")
    for row in rows:
        print(json.dumps(row, sort_keys=True))
    for volume in sorted({row["volume"] for row in rows if row["volume"] is not None}, reverse=True):
        group = [row for row in rows if row["volume"] == volume]
        measurement = [row for row in group if row["requested_mode"] == "measurement"]
        if len(measurement) >= 2:
            first, last = measurement[0], measurement[-1]
            drift = {"recall_change": round(last["word_recall"] - first["word_recall"], 3),
                     "speech_level_change_db": round(last["speech_dbfs"] - first["speech_dbfs"], 2)}
            drift["setup_held"] = abs(drift["recall_change"]) <= 0.05 and abs(drift["speech_level_change_db"]) <= 2.0
            print(json.dumps({"volume": volume, "measurement_first_vs_last": drift}, sort_keys=True))
        baseline = max((row["word_recall"] for row in measurement), default=None)
        eligible = [row for row in group if row["clipped_samples"] == 0 and row["duration_seconds"] > 0
                    and (baseline is None or row["word_recall"] >= baseline - 0.05)]
        best = sorted(eligible, key=lambda row: (-row["word_recall"], -row["speech_minus_noise_db"]))
        print(json.dumps({"volume": volume, "ranking": [row["requested_mode"] for row in best],
                          "rule": "word recall first, then speech minus noise; out if it clips, recorded nothing, or recalls more than 5 points below measurement"}))
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    play_parser = sub.add_parser("play")
    play_parser.add_argument("--scratch", type=Path, required=True, help="folder for the generated clip and take log, outside the repo")
    play_parser.add_argument("--takes", default=DEFAULT_TAKES, help="comma list of volume:mode, in order")
    play_parser.set_defaults(function=play)
    score_parser = sub.add_parser("score")
    score_parser.add_argument("meetings", type=Path, nargs="+", help="pulled meeting folders")
    score_parser.add_argument("--log", type=Path, help="takes-log.jsonl written by play, to attach each take's volume")
    score_parser.add_argument("--assume-volume", type=int, help="volume for takes with no log entry (round 1 ran at 50)")
    score_parser.set_defaults(function=score)
    args = parser.parse_args()
    return args.function(args)


if __name__ == "__main__":
    sys.exit(main())
