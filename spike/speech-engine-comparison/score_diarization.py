#!/usr/bin/env python3
"""Content-free checks for a frozen diarization trial.

The scorer never emits segment text, source paths, or raw diarizer labels.  Its
public AMI result is a strict word-midpoint attribution proxy, not DER or a
word-attribution accuracy measurement.
"""

from __future__ import annotations

import argparse
import hashlib
import itertools
import json
import math
from collections import defaultdict
from pathlib import Path
from typing import Any

SCHEMA = "nemotron-diarization-score/1"
EDGE_TOLERANCE_SECONDS = 15.0
EPSILON = 1e-9


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def finite_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def normalized_segments(document: Any, duration: float) -> list[tuple[float, float, str]]:
    if not isinstance(document, dict) or not isinstance(document.get("segments"), list):
        raise ValueError("output must contain a segments array")
    result = []
    for raw in document["segments"]:
        if not isinstance(raw, dict):
            raise ValueError("segment must be an object")
        start, end, speaker = raw.get("start"), raw.get("end"), raw.get("speaker")
        if not finite_number(start) or not finite_number(end) or isinstance(speaker, bool) or not isinstance(speaker, (str, int)):
            raise ValueError("segment needs finite start/end and a speaker")
        start, end = float(start), float(end)
        if start < -EPSILON or end < start or end > duration + EPSILON:
            raise ValueError("segment timing is outside its frozen audio duration")
        result.append((max(0.0, start), min(duration, end), str(speaker)))
    return result


def union_intervals(intervals: list[tuple[float, float]]) -> list[tuple[float, float]]:
    merged: list[tuple[float, float]] = []
    for start, end in sorted(intervals):
        if end <= start:
            continue
        if merged and start <= merged[-1][1] + EPSILON:
            merged[-1] = (merged[-1][0], max(merged[-1][1], end))
        else:
            merged.append((start, end))
    return merged


def overlap_intervals(segments: list[tuple[float, float, str]]) -> list[tuple[float, float]]:
    intervals = []
    for index, (start, end, speaker) in enumerate(segments):
        for other_start, other_end, other_speaker in segments[index + 1 :]:
            if speaker == other_speaker:
                continue
            low, high = max(start, other_start), min(end, other_end)
            if high > low:
                intervals.append((low, high))
    return union_intervals(intervals)


def case_summary(segments: list[tuple[float, float, str]], duration: float, wall_seconds: float) -> dict[str, Any]:
    overlaps = overlap_intervals(segments)
    return {
        "segments": len(segments),
        "estimated_speakers": len({speaker for _, _, speaker in segments}),
        "overlap_intervals": len(overlaps),
        "overlap_seconds": round(sum(end - start for start, end in overlaps), 3),
        "wall_seconds_approx": wall_seconds,
        "audio_seconds_per_wall_second": round(duration / wall_seconds, 2) if wall_seconds > 0 else None,
        "silence_behavior": "no_segments" if not segments else "segments_present",
    }


def active_speakers(segments: list[tuple[float, float, str]], moment: float) -> set[str]:
    return {speaker for start, end, speaker in segments if start <= moment <= end}


def reference_active(words: list[dict[str, Any]], moment: float) -> set[str]:
    return {
        str(word["speaker"])
        for word in words
        if float(word["start"]) <= moment <= float(word["end"])
    }


def best_mapping(words: list[dict[str, Any]], segments: list[tuple[float, float, str]]) -> dict[str, str]:
    predicted = sorted({speaker for _, _, speaker in segments})
    reference = sorted({str(word["speaker"]) for word in words})
    weights: dict[tuple[str, str], int] = defaultdict(int)
    for word in words:
        midpoint = (float(word["start"]) + float(word["end"])) / 2
        actual = str(word["speaker"])
        for speaker in active_speakers(segments, midpoint):
            weights[(speaker, actual)] += 1
    # The trial has few anonymous speakers.  Exhaustive assignment is easier to
    # audit than adding a solver dependency.  It permits unmatched predictions.
    best_score = -1
    best_targets: tuple[str | None, ...] | None = None
    choices = reference + [None] * len(predicted)
    for targets in itertools.permutations(choices, len(predicted)):
        if len({target for target in targets if target is not None}) != len(
            [target for target in targets if target is not None]
        ):
            continue
        score = sum(weights[(speaker, target)] for speaker, target in zip(predicted, targets) if target is not None)
        tie_break = tuple("~" if target is None else target for target in targets)
        best_tie_break = tuple("~" if target is None else target for target in best_targets) if best_targets else ()
        if score > best_score or (score == best_score and tie_break < best_tie_break):
            best_score, best_targets = score, targets
    assert best_targets is not None
    return {speaker: target for speaker, target in zip(predicted, best_targets) if target is not None}


def timebase_check(words: list[dict[str, Any]], segments: list[tuple[float, float, str]], duration: float) -> dict[str, Any]:
    if not words or not segments:
        return {"status": "refused", "reason": "reference or diarization output has no timed coverage"}
    starts = [float(word["start"]) for word in words]
    ends = [float(word["end"]) for word in words]
    if min(starts) < -EPSILON or max(ends) > duration + EPSILON:
        return {"status": "refused", "reason": "manual word timings fall outside frozen audio duration"}
    observed_start = min(start for start, _, _ in segments)
    observed_end = max(end for _, end, _ in segments)
    if abs(observed_start - min(starts)) > EDGE_TOLERANCE_SECONDS or abs(observed_end - max(ends)) > EDGE_TOLERANCE_SECONDS:
        return {"status": "refused", "reason": "diarization and reference extents differ by more than the edge tolerance"}
    return {
        "status": "admissible",
        "check": "all reference timings in audio bounds; diarization and reference start/end extents within 15 seconds",
        "limit": "this rejects obvious clock shifts but does not prove sample-clock alignment",
    }


def public_proxy(reference: Any, segments: list[tuple[float, float, str]], duration: float) -> dict[str, Any]:
    words = reference.get("words") if isinstance(reference, dict) else None
    if not isinstance(words, list) or not words:
        return {"status": "refused", "reason": "reference has no words"}
    for word in words:
        if not isinstance(word, dict) or not all(key in word for key in ("start", "end", "speaker")):
            return {"status": "refused", "reason": "reference word shape is invalid"}
        if not finite_number(word["start"]) or not finite_number(word["end"]) or float(word["end"]) < float(word["start"]):
            return {"status": "refused", "reason": "reference word timing is invalid"}
    alignment = timebase_check(words, segments, duration)
    if alignment["status"] != "admissible":
        return {"status": "refused", "timebase": alignment}
    mapping = best_mapping(words, segments)
    predicted_order = sorted({speaker for _, _, speaker in segments})
    anonymized = {speaker: f"P{index + 1}" for index, speaker in enumerate(predicted_order)}
    mapping_rows = [
        {"predicted_channel": anonymized[speaker], "reference_channel": mapping.get(speaker)}
        for speaker in predicted_order
    ]
    reference_overlap = predicted_overlap = uncovered = correct = incorrect = 0
    denominator = 0
    for word in words:
        midpoint = (float(word["start"]) + float(word["end"])) / 2
        actual = str(word["speaker"])
        active_reference = reference_active(words, midpoint)
        active_predicted = active_speakers(segments, midpoint)
        if len(active_reference) != 1:
            reference_overlap += 1
            continue
        denominator += 1
        if not active_predicted:
            uncovered += 1
            incorrect += 1
        elif len(active_predicted) != 1:
            predicted_overlap += 1
            incorrect += 1
        elif mapping.get(next(iter(active_predicted))) == actual:
            correct += 1
        else:
            incorrect += 1
    return {
        "status": "scored",
        "timebase": alignment,
        "method": "strict-global-one-to-one-speaker-map-plus-word-midpoint",
        "mapping": mapping_rows,
        "denominator": denominator,
        "excluded_simultaneous_reference_words": reference_overlap,
        "predicted_overlap_words_counted_incorrect": predicted_overlap,
        "uncovered_words_counted_incorrect": uncovered,
        "correct": correct,
        "incorrect": incorrect,
        "strict_midpoint_proxy": round(correct / denominator, 4) if denominator else None,
        "limit": "proxy only; not official DER or word-attribution accuracy",
    }


def self_test() -> None:
    reference = {"words": [
        {"start": 0.0, "end": 1.0, "speaker": "A"},
        {"start": 2.0, "end": 3.0, "speaker": "B"},
    ]}
    correct = [(0.0, 1.0, "x"), (2.0, 3.0, "y")]
    result = public_proxy(reference, correct, 3.0)
    assert result["status"] == "scored" and result["strict_midpoint_proxy"] == 1.0
    wrong = public_proxy(reference, [(0.0, 1.0, "x"), (2.0, 3.0, "x")], 3.0)
    assert wrong["strict_midpoint_proxy"] < 1.0, "wrong global mapping must lower the proxy"
    overlap = public_proxy(reference, [(0.0, 1.0, "x"), (0.0, 1.0, "y"), (2.0, 3.0, "y")], 3.0)
    assert overlap["predicted_overlap_words_counted_incorrect"] == 1
    shifted = {"words": [{"start": 30.0, "end": 31.0, "speaker": "A"}]}
    assert public_proxy(shifted, [(0.0, 1.0, "x")], 40.0)["status"] == "refused"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--receipt", type=Path)
    parser.add_argument("--corpus", type=Path)
    parser.add_argument("--outputs-dir", type=Path)
    parser.add_argument("--ami-reference", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("self-test: passed")
        return 0
    if not all((args.receipt, args.corpus, args.outputs_dir, args.ami_reference, args.out)):
        parser.error("receipt, corpus, outputs-dir, ami-reference, and out are required")
    receipt, corpus = load_json(args.receipt), load_json(args.corpus)
    cases = {case["id"]: case for case in corpus["cases"]}
    report: dict[str, Any] = {"schema": SCHEMA, "receipt_checks": {}, "cases": {}}
    manifest_matches = sha256(args.corpus) == receipt.get("manifest_sha256")
    if not manifest_matches:
        raise ValueError("frozen corpus manifest hash does not match the run receipt")
    report["receipt_checks"]["manifest_sha256_matches"] = True
    for case_id, observed in sorted(receipt.get("cases", {}).items()):
        case = cases.get(case_id)
        output_path = args.outputs_dir / f"{case_id}.json"
        if not isinstance(case, dict) or not output_path.is_file():
            raise ValueError(f"{case_id}: frozen case or output is missing")
        output_bytes = output_path.stat().st_size
        output_hash = sha256(output_path)
        input_hash = sha256(Path(case["audio"]))
        valid = (
            input_hash == case.get("audio_sha256") == observed.get("audio_sha256")
            and output_hash == observed.get("output_sha256")
            and output_bytes == observed.get("output_bytes")
            and observed.get("exit_code") == 0
        )
        if not valid:
            raise ValueError(f"{case_id}: frozen input or output does not match the run receipt")
        report["receipt_checks"][case_id] = {"status": "verified"}
        segments = normalized_segments(load_json(output_path), float(case["duration"]))
        report["cases"][case_id] = case_summary(segments, float(case["duration"]), float(observed["wall_seconds_approx"]))
        if case_id == "ami-full":
            report["cases"][case_id]["public_attribution_proxy"] = public_proxy(
                load_json(args.ami_reference), segments, float(case["duration"])
            )
    args.out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
