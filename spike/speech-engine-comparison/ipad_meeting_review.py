#!/usr/bin/env python3
"""Private, content-free Mac review tools for exported iPad Ink meetings."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import importlib.util
import json
import math
import random
import sys
import wave
from pathlib import Path
from typing import Any

MEETING_SCHEMA = "ipad-ink-meeting/1"
PACKET_SCHEMA = "ipad-ink-blind-review/1"
EPSILON = 1e-9
PREDICTION_RULE = "speaker active for at least max(0.5 seconds, 10% of turn duration)"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def finite(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def parse_time(value: Any) -> dt.datetime:
    if not isinstance(value, str):
        raise ValueError("not an ISO 8601 string")
    parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError("ISO 8601 timestamp needs an offset")
    return parsed


def iso_time(value: dt.datetime) -> str:
    return value.astimezone(dt.timezone.utc).isoformat().replace("+00:00", "Z")


def wav_details(path: Path) -> tuple[float, list[str]]:
    problems: list[str] = []
    try:
        with wave.open(str(path), "rb") as audio:
            if audio.getframerate() != 16000:
                problems.append("audio.wav is not 16 kHz")
            if audio.getnchannels() != 1:
                problems.append("audio.wav is not mono")
            if audio.getsampwidth() != 2 or audio.getcomptype() != "NONE":
                problems.append("audio.wav is not 16-bit PCM")
            return audio.getnframes() / audio.getframerate(), problems
    except (wave.Error, EOFError) as error:
        return 0.0, [f"audio.wav is invalid: {error}"]


def validate_meeting(meeting_dir: Path) -> tuple[dict[str, Any], list[str]]:
    errors: list[str] = []
    meeting_path = meeting_dir / "meeting.json"
    strokes_path = meeting_dir / "strokes.json"
    transcript_path = meeting_dir / "transcript.json"
    audio_path = meeting_dir / "audio.wav"
    meeting: Any = {}
    if not meeting_path.is_file():
        errors.append("meeting.json is missing")
    else:
        try:
            meeting = load_json(meeting_path)
        except (OSError, json.JSONDecodeError) as error:
            errors.append(f"meeting.json is invalid: {error}")
    created: dt.datetime | None = None
    if isinstance(meeting, dict):
        if meeting.get("schema") != MEETING_SCHEMA:
            errors.append("meeting.json schema is not ipad-ink-meeting/1")
        if meeting.get("consent_everyone_audible") is not True:
            errors.append("meeting.json consent_everyone_audible is not true")
        if meeting.get("keep_days") not in (1, 7, 30):
            errors.append("meeting.json keep_days is not 1, 7, or 30")
        try:
            created = parse_time(meeting.get("created_at"))
        except ValueError:
            errors.append("meeting.json created_at is not ISO 8601")
    elif meeting_path.is_file():
        errors.append("meeting.json is not an object")
    seconds = 0.0
    if not audio_path.is_file():
        errors.append("audio.wav is missing")
    else:
        seconds, audio_errors = wav_details(audio_path)
        errors.extend(audio_errors)
    strokes: Any = []
    if not strokes_path.is_file():
        errors.append("strokes.json is missing")
    else:
        try:
            strokes = load_json(strokes_path)
            if not isinstance(strokes, list):
                errors.append("strokes.json is not a list")
                strokes = []
            else:
                for index, stroke in enumerate(strokes):
                    if not isinstance(stroke, dict) or set(stroke) != {"index", "start_seconds", "end_seconds"}:
                        errors.append(f"strokes.json item {index} is not {{index, start_seconds, end_seconds}}")
                        continue
                    start, end = stroke["start_seconds"], stroke["end_seconds"]
                    if not ((finite(start) and finite(end)) or (start is None and end is None)):
                        errors.append(f"strokes.json item {index} has invalid times")
        except (OSError, json.JSONDecodeError) as error:
            errors.append(f"strokes.json is invalid: {error}")
    transcript: Any = []
    if not transcript_path.is_file():
        errors.append("transcript.json is missing")
    else:
        try:
            transcript = load_json(transcript_path)
            if not isinstance(transcript, list):
                errors.append("transcript.json is not a list")
                transcript = []
            else:
                for index, turn in enumerate(transcript):
                    if not isinstance(turn, dict) or set(turn) != {"start", "end", "text", "final"}:
                        errors.append(f"transcript.json item {index} is not {{start, end, text, final}}")
                    elif not finite(turn["start"]) or not finite(turn["end"]) or not isinstance(turn["text"], str) or turn["final"] is not True:
                        errors.append(f"transcript.json item {index} is invalid or not final")
        except (OSError, json.JSONDecodeError) as error:
            errors.append(f"transcript.json is invalid: {error}")
    timed = sum(1 for item in strokes if isinstance(item, dict) and finite(item.get("start_seconds")) and finite(item.get("end_seconds")))
    total_turn_seconds = sum(float(item["end"]) - float(item["start"]) for item in transcript if isinstance(item, dict) and finite(item.get("start")) and finite(item.get("end")))
    summary = {
        "audio_seconds": round(seconds, 3),
        "audio_sha256": sha256(audio_path) if audio_path.is_file() else None,
        "keep_days": meeting.get("keep_days") if isinstance(meeting, dict) else None,
        "audio_delete_after": iso_time(created + dt.timedelta(days=meeting["keep_days"])) if created and isinstance(meeting, dict) and meeting.get("keep_days") in (1, 7, 30) else None,
        "strokes": {"timed": timed, "untimed": len(strokes) - timed},
        "transcript_turns": len(transcript),
        "transcript_turn_seconds": round(total_turn_seconds, 3),
    }
    return summary, errors


def command_check(args: argparse.Namespace) -> int:
    summary, errors = validate_meeting(args.meeting_dir)
    if errors:
        print(json.dumps({"status": "invalid", "problems": errors}, sort_keys=True))
        return 1
    print(json.dumps({"status": "valid", **summary}, sort_keys=True))
    return 0


def load_segments(path: Path) -> list[dict[str, Any]]:
    document = load_json(path)
    segments = document.get("segments") if isinstance(document, dict) else None
    if not isinstance(segments, list):
        raise ValueError("segments file must contain a segments array")
    result = []
    for item in segments:
        if not isinstance(item, dict) or not finite(item.get("start")) or not finite(item.get("end")) or not isinstance(item.get("speaker"), (str, int)):
            raise ValueError("segments must have finite start/end and a speaker")
        if float(item["end"]) < float(item["start"]):
            raise ValueError("segment end precedes start")
        result.append({"start": float(item["start"]), "end": float(item["end"]), "speaker": str(item["speaker"])})
    return result


def union_seconds(intervals: list[tuple[float, float]]) -> float:
    total, latest = 0.0, None
    for start, end in sorted(intervals):
        if end <= start:
            continue
        if latest is None or start > latest:
            total += end - start
            latest = end
        elif end > latest:
            total += end - latest
            latest = end
    return total


def predicted_set(segments: list[dict[str, Any]], start: float, end: float) -> set[str]:
    threshold = max(0.5, 0.1 * (end - start))
    by_speaker: dict[str, list[tuple[float, float]]] = {}
    for segment in segments:
        low, high = max(start, segment["start"]), min(end, segment["end"])
        if high > low:
            by_speaker.setdefault(segment["speaker"], []).append((low, high))
    return {speaker for speaker, intervals in by_speaker.items() if union_seconds(intervals) + EPSILON >= threshold}


def exclusive_intervals(segments: list[dict[str, Any]], speaker: str, duration: float) -> list[tuple[float, float]]:
    """Runs of at least 5 s in which no other speaker is ever active.

    Short silences inside a run are allowed: conversational speech is full of
    them. A run also needs at least 3 s of the speaker's own speech in its
    first 8 s, the part an identity clip uses.
    """
    points = {0.0, duration}
    for segment in segments:
        points.update((max(0.0, segment["start"]), min(duration, segment["end"])))
    ordered = sorted(points)
    runs: list[list[float]] = []
    for start, end in zip(ordered, ordered[1:]):
        active = {item["speaker"] for item in segments if item["start"] <= start + EPSILON and item["end"] >= end - EPSILON}
        if active <= {speaker}:
            if runs and abs(runs[-1][1] - start) <= EPSILON:
                runs[-1][1] = end
            else:
                runs.append([start, end])
    own = [(item["start"], item["end"]) for item in segments if item["speaker"] == speaker]
    intervals: list[tuple[float, float]] = []
    for start, end in runs:
        if end - start < 5.0:
            continue
        window_end = min(end, start + 8.0)
        spoken = union_seconds([(max(start, low), min(window_end, high)) for low, high in own])
        if spoken + EPSILON >= 3.0:
            intervals.append((start, end))
    return intervals


def identity_windows(segments: list[dict[str, Any]], duration: float) -> list[tuple[str, float, float]]:
    active = {}
    for speaker in {item["speaker"] for item in segments}:
        active[speaker] = union_seconds([(item["start"], item["end"]) for item in segments if item["speaker"] == speaker])
    speakers = [speaker for speaker, _ in sorted(active.items(), key=lambda item: (-item[1], item[0]))[:2]]
    if len(speakers) != 2:
        raise ValueError("identity clips need two predicted speakers")
    windows = []
    for speaker in speakers:
        candidates = []
        for start, end in exclusive_intervals(segments, speaker, duration):
            length = min(8.0, end - start)
            candidates.append((start, start + length))
        first = [item for item in candidates if item[0] < duration / 2]
        second = [item for item in candidates if item[1] > duration / 2]
        chosen = []
        if first:
            chosen.append(first[0])
        if second:
            chosen.append(second[-1])
        for candidate in candidates:
            if len(chosen) == 2:
                break
            if candidate not in chosen:
                chosen.append(candidate)
        if len(chosen) != 2:
            raise ValueError("identity clips need two exclusive 5-second windows per speaker")
        windows.extend((speaker, start, end) for start, end in chosen)
    return windows


def sample_groups(groups: dict[tuple[str, ...], list[dict[str, Any]]], seed: int | None) -> list[dict[str, Any]]:
    """Up to three turns per predicted group: one seeded pick from each third of
    the group's turns in time order, so the sample is reproducible and spread
    across the meeting."""
    picker = random.Random(seed)
    chosen: list[dict[str, Any]] = []
    for group, candidates in sorted(groups.items()):
        candidates = sorted(candidates, key=lambda item: item["start"])
        if len(candidates) > 3:
            thirds = [candidates[len(candidates) * part // 3 : len(candidates) * (part + 1) // 3] for part in range(3)]
            pool = [picker.choice(third) for third in thirds]
        else:
            pool = candidates[:]
        for item in sorted(pool, key=lambda value: value["start"]):
            chosen.append({**item, "selection_group": "|".join(group) if group else "neither"})
    return chosen


def cut_wav(source: Path, destination: Path, start: float, end: float) -> None:
    with wave.open(str(source), "rb") as input_audio:
        rate = input_audio.getframerate()
        first = round(start * rate)
        frames = max(0, round(end * rate) - first)
        input_audio.setpos(first)
        data = input_audio.readframes(frames)
        params = input_audio.getparams()
    with wave.open(str(destination), "wb") as output:
        output.setparams(params)
        output.writeframes(data)


def packet(args: argparse.Namespace) -> int:
    summary, errors = validate_meeting(args.meeting_dir)
    if errors:
        raise ValueError("meeting is invalid: " + "; ".join(errors))
    segments = load_segments(args.segments)
    transcript = load_json(args.meeting_dir / "transcript.json")
    out = args.out
    out.mkdir(parents=True, exist_ok=False)
    clips = out / "clips"
    clips.mkdir()
    duration = float(summary["audio_seconds"])
    records: list[dict[str, Any]] = []
    for number, (speaker, start, end) in enumerate(identity_windows(segments, duration), 1):
        records.append({"label": f"I{number}", "kind": "identity", "start": start, "end": end, "predicted_set": [speaker], "selection_group": "identity"})
    groups: dict[tuple[str, ...], list[dict[str, Any]]] = {}
    for turn in transcript:
        start, end = float(turn["start"]), float(turn["end"])
        if 2.5 <= end - start <= 9.0:
            speakers = tuple(sorted(predicted_set(segments, start, end)))
            groups.setdefault(speakers, []).append({"start": start, "end": end, "predicted_set": list(speakers)})
    chosen = sample_groups(groups, args.seed)
    order = random.Random(args.order_seed)
    order.shuffle(chosen)
    for number, item in enumerate(chosen):
        item["label"] = chr(ord("A") + number)
        item["kind"] = "balanced"
        records.append(item)
    for record in records:
        path = clips / f"{record['label']}.wav"
        cut_wav(args.meeting_dir / "audio.wav", path, record["start"], record["end"])
        record["sha256"] = sha256(path)
        record["start"] = round(record["start"], 6)
        record["end"] = round(record["end"], 6)
    manifest = {"schema": PACKET_SCHEMA, "seed": args.seed, "order_seed": args.order_seed, "prediction_rule": PREDICTION_RULE, "meeting_audio_sha256": summary["audio_sha256"], "delete_after": summary["audio_delete_after"], "clips": records}
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    labels = {record["label"]: {"speaker_set": None, "note": ""} for record in records}
    (out / "labels.json").write_text(json.dumps(labels, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({"status": "created", "clips": len(records), "delete_after": summary["audio_delete_after"]}, sort_keys=True))
    return 0


def score(args: argparse.Namespace) -> int:
    manifest, labels = load_json(args.packet_dir / "manifest.json"), load_json(args.labels)
    if not isinstance(manifest, dict) or manifest.get("schema") != PACKET_SCHEMA or not isinstance(labels, dict):
        raise ValueError("invalid packet manifest or labels")
    identities: dict[str, set[str]] = {}
    for clip in manifest.get("clips", []):
        if clip.get("kind") == "identity":
            answer = labels.get(clip.get("label"), {}).get("speaker_set") if isinstance(labels.get(clip.get("label")), dict) else None
            predicted = clip.get("predicted_set")
            if isinstance(answer, list) and len(answer) == 1 and answer[0] in ("me", "other") and isinstance(predicted, list) and len(predicted) == 1:
                identities.setdefault(predicted[0], set()).add(answer[0])
    mapping = {speaker: next(iter(people)) for speaker, people in identities.items() if len(people) == 1}
    if len(mapping) != 2 or len(set(mapping.values())) != 2:
        args.out.write_text(json.dumps({"status": "identity_unresolved"}, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"status": "identity_unresolved"}))
        return 1
    outcome: dict[str, Any] = {"status": "scored", "labelled": 0, "unclear": 0, "model_matches": 0, "baseline_matches": 0, "groups": {}, "limit": "stratified sample of selected turns, not whole-meeting accuracy or DER"}
    for clip in manifest.get("clips", []):
        if clip.get("kind") != "balanced":
            continue
        answer = labels.get(clip.get("label"), {}).get("speaker_set") if isinstance(labels.get(clip.get("label")), dict) else None
        if answer == "unclear":
            outcome["unclear"] += 1
            continue
        if not isinstance(answer, list) or not set(answer).issubset({"me", "other"}):
            continue
        actual = set(answer)
        # A predicted speaker with no identity mapping (a third cluster) stays
        # in the set, so it can never match the operator's label.
        predicted = {mapping.get(speaker, f"unmapped:{speaker}") for speaker in clip.get("predicted_set", [])}
        group = str(clip.get("selection_group", "neither"))
        row = outcome["groups"].setdefault(group, {"labelled": 0, "model_matches": 0, "baseline_matches": 0})
        outcome["labelled"] += 1
        row["labelled"] += 1
        if predicted == actual:
            outcome["model_matches"] += 1
            row["model_matches"] += 1
        if {"me"} == actual:
            outcome["baseline_matches"] += 1
            row["baseline_matches"] += 1
    args.out.write_text(json.dumps(outcome, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({key: outcome[key] for key in ("status", "labelled", "unclear", "model_matches", "baseline_matches")}, sort_keys=True))
    return 0


def sweep(args: argparse.Namespace) -> int:
    now = parse_time(args.now) if args.now else dt.datetime.now(dt.timezone.utc)
    removed, unreadable, kept = 0, 0, 0
    for path in args.root.rglob("manifest.json"):
        try:
            document = load_json(path)
            if not isinstance(document, dict) or document.get("schema") != PACKET_SCHEMA:
                continue
            expired = parse_time(document["delete_after"]) <= now
        except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError):
            unreadable += 1
            print(json.dumps({"status": "unreadable_manifest"}))
            continue
        clips = path.parent / "clips"
        if expired and clips.exists():
            for clip in clips.iterdir():
                if clip.is_file():
                    clip.unlink()
                else:
                    raise ValueError("packet clips folder contains a directory")
            clips.rmdir()
            removed += 1
        else:
            kept += 1
    print(json.dumps({"removed": removed, "kept": kept, "unreadable": unreadable}, sort_keys=True))
    return 0


def label_speakers(args: argparse.Namespace) -> int:
    summary, errors = validate_meeting(args.meeting_dir)
    if errors:
        raise ValueError("meeting is invalid: " + "; ".join(errors))
    module_path = Path(__file__).with_name("run_fluidaudio_diarization.py")
    spec = importlib.util.spec_from_file_location("fluidaudio_diarization", module_path)
    module = importlib.util.module_from_spec(spec)
    assert spec and spec.loader
    spec.loader.exec_module(module)
    for arm in ("nemotron3-c128-split-w8a8", "pipeline-offline"):
        module.run_arm(args.cli.resolve(), arm, (args.meeting_dir / "audio.wav").resolve(), args.out.resolve())
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    check_parser = sub.add_parser("check"); check_parser.add_argument("meeting_dir", type=Path); check_parser.set_defaults(function=command_check)
    label_parser = sub.add_parser("label-speakers"); label_parser.add_argument("meeting_dir", type=Path); label_parser.add_argument("--cli", type=Path, required=True); label_parser.add_argument("--out", type=Path, required=True); label_parser.set_defaults(function=label_speakers)
    packet_parser = sub.add_parser("packet"); packet_parser.add_argument("meeting_dir", type=Path); packet_parser.add_argument("--segments", type=Path, required=True); packet_parser.add_argument("--out", type=Path, required=True); packet_parser.add_argument("--seed", type=int); packet_parser.add_argument("--order-seed", type=int); packet_parser.set_defaults(function=packet)
    score_parser = sub.add_parser("score"); score_parser.add_argument("packet_dir", type=Path); score_parser.add_argument("--labels", type=Path, required=True); score_parser.add_argument("--out", type=Path, required=True); score_parser.set_defaults(function=score)
    sweep_parser = sub.add_parser("sweep"); sweep_parser.add_argument("root", type=Path); sweep_parser.add_argument("--now"); sweep_parser.set_defaults(function=sweep)
    args = parser.parse_args()
    try:
        return args.function(args)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(json.dumps({"status": "error", "problem": str(error)}), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
