#!/usr/bin/env python3
"""Score transcript-engine research runs against a frozen local reference.

The score report deliberately excludes engine transcript text.  It uses a
pinned RapidFuzz dependency for exact whole-transcript token edit distance.
AMI's temporally sorted mixed-channel manual words remain unsuitable for a
manual-WER claim; this reports the edit counts and marks that limitation.
"""

from __future__ import annotations

import argparse
import json
import math
import re
from collections import defaultdict
from pathlib import Path
from typing import Any

SCHEMA = "speech-engine-comparison-score/1"
TOKEN = re.compile(r"[a-z0-9]+(?:'[a-z0-9]+)?", re.I)
LOCAL_TOLERANCE_SECONDS = 2.0
ANNOTATION_GAP_MINIMUM_SECONDS = 10.0
RAPIDFUZZ_VERSION = "3.14.3"

try:
    import rapidfuzz
    from rapidfuzz.distance import Levenshtein
except ImportError:  # The scorer returns an explicit error instead of a slow fallback.
    rapidfuzz = None
    Levenshtein = None


def tokens(text: Any) -> list[str]:
    return [match.group(0).casefold() for match in TOKEN.finditer(str(text or ""))]


def finite_time(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(float(value))


def valid_reference(reference: Any) -> str | None:
    if not isinstance(reference, dict) or reference.get("schema") != "speech-reference/1":
        return "reference must use schema speech-reference/1"
    words = reference.get("words")
    if not isinstance(words, list) or not words:
        return "reference has no manual words"
    for word in words:
        if not isinstance(word, dict) or not finite_time(word.get("start")) or not finite_time(word.get("end")):
            return "reference has missing or invalid manual word timings"
    return None


def rapidfuzz_error() -> str | None:
    if rapidfuzz is None or Levenshtein is None:
        return f"rapidfuzz=={RAPIDFUZZ_VERSION} is required for exact transcript edit distance"
    if rapidfuzz.__version__ != RAPIDFUZZ_VERSION:
        return f"rapidfuzz must be pinned to {RAPIDFUZZ_VERSION}; found {rapidfuzz.__version__}"
    return None


def edit_counts(reference_tokens: list[str], observed_tokens: list[str]) -> tuple[int, int, int]:
    """Exact RapidFuzz Levenshtein edits: substitutions, deletions, insertions."""
    dependency_error = rapidfuzz_error()
    if dependency_error:
        raise RuntimeError(dependency_error)
    substitutions = deletions = insertions = 0
    for edit in Levenshtein.editops(reference_tokens, observed_tokens):
        if edit.tag == "replace":
            substitutions += 1
        elif edit.tag == "delete":
            deletions += 1
        elif edit.tag == "insert":
            insertions += 1
        else:  # Defensive: a library update must not silently change accounting.
            raise RuntimeError(f"unexpected RapidFuzz edit operation {edit.tag!r}")
    return substitutions, deletions, insertions


def segment_records(run: dict[str, Any], duration: float) -> tuple[list[dict[str, Any]], dict[str, int]]:
    records: list[dict[str, Any]] = []
    metric = {
        "segments": 0,
        "segments_with_tokens": 0,
        "timed_segments": 0,
        "segments_with_usable_timing": 0,
        "text_segments_with_usable_timing": 0,
        "timing_missing_or_non_numeric_values": 0,
        "timing_non_finite_values": 0,
        "timing_out_of_bounds": 0,
        "timing_non_monotonic": 0,
    }
    prior_start = -math.inf
    segments = run.get("segments")
    if not isinstance(segments, list):
        return records, metric
    for segment in segments:
        if not isinstance(segment, dict):
            continue
        metric["segments"] += 1
        segment_tokens = tokens(segment.get("text"))
        if segment_tokens:
            metric["segments_with_tokens"] += 1
        start, end = segment.get("start"), segment.get("end")
        invalid_type = sum(not isinstance(value, (int, float)) or isinstance(value, bool) for value in (start, end))
        if invalid_type:
            metric["timing_missing_or_non_numeric_values"] += invalid_type
            continue
        invalid_finite = sum(not math.isfinite(float(value)) for value in (start, end))
        if invalid_finite:
            metric["timing_non_finite_values"] += invalid_finite
            continue
        start, end = float(start), float(end)
        metric["timed_segments"] += 1
        if start < 0 or end < start or end > duration + 0.05:
            metric["timing_out_of_bounds"] += 1
            continue
        metric["segments_with_usable_timing"] += 1
        if segment_tokens:
            metric["text_segments_with_usable_timing"] += 1
        if start + 1e-6 < prior_start:
            metric["timing_non_monotonic"] += 1
        prior_start = max(prior_start, start)
        records.append({"start": start, "end": end, "tokens": segment_tokens})
    return records, metric


def exact_manual_serialization_distance(reference: dict[str, Any], observed_tokens: list[str]) -> dict[str, Any]:
    reference_tokens = [token for word in reference["words"] for token in tokens(word.get("text"))]
    substitutions, deletions, insertions = edit_counts(reference_tokens, observed_tokens)
    errors = substitutions + deletions + insertions
    return {
        "method": "rapidfuzz-exact-whole-transcript-token-levenshtein",
        "rapidfuzz_version": RAPIDFUZZ_VERSION,
        "manual_wer_status": "unsupported",
        "manual_wer": None,
        "diagnostic_limit": "manual words are temporally sorted mixed-channel annotations, not a verified single-channel reference serialization",
        "reference_tokens": len(reference_tokens),
        "substitutions": substitutions,
        "deletions": deletions,
        "insertions": insertions,
        "normalized_edit_distance": errors / len(reference_tokens) if reference_tokens else None,
    }


def locally_observed_tokens(records: list[dict[str, Any]], start: float, end: float) -> set[str]:
    low, high = start - LOCAL_TOLERANCE_SECONDS, end + LOCAL_TOLERANCE_SECONDS
    observed: set[str] = set()
    for record in records:
        if record["end"] >= low and record["start"] <= high:
            observed.update(record["tokens"])
    return observed


def local_span_survival(spans: Any, records: list[dict[str, Any]]) -> dict[str, Any]:
    if not isinstance(spans, list):
        return {"spans": 0, "terms": 0, "survived_terms": 0, "complete_spans": 0, "by_span": []}
    rows = []
    expected = survived = complete = 0
    for span in spans:
        if not isinstance(span, dict) or not finite_time(span.get("start")) or not finite_time(span.get("end")):
            continue
        wanted = set(tokens(" ".join(str(term) for term in span.get("terms", []))))
        # Reference terms are already normalized, but tokenize again to make a
        # malformed reference unable to create substring matching.
        observed = locally_observed_tokens(records, float(span["start"]), float(span["end"]))
        present = wanted & observed
        expected += len(wanted)
        survived += len(present)
        complete += bool(wanted) and len(present) == len(wanted)
        rows.append({"id": str(span.get("id", "")), "kind": str(span.get("kind", "")), "terms": len(wanted), "survived_terms": len(present), "complete": bool(wanted) and len(present) == len(wanted)})
    return {"spans": len(rows), "terms": expected, "survived_terms": survived, "complete_spans": complete, "window_tolerance_seconds": LOCAL_TOLERANCE_SECONDS, "by_span": rows}


def lexical_timing_alignment(reference: dict[str, Any], records: list[dict[str, Any]]) -> dict[str, Any]:
    reference_occurrences: dict[str, list[float]] = defaultdict(list)
    for word in reference["words"]:
        for token in tokens(word.get("text")):
            reference_occurrences[token].append(float(word["start"]))
    output_occurrences: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for record in records:
        for token in record["tokens"]:
            output_occurrences[token].append(record)
    candidates = aligned = outside = 0
    for token, starts in reference_occurrences.items():
        entries = output_occurrences.get(token, [])
        if len(starts) != 1 or len(entries) != 1:
            continue
        candidates += 1
        record = entries[0]
        if record["start"] <= starts[0] <= record["end"]:
            aligned += 1
        else:
            outside += 1
    return {"method": "globally-unique-exact-token-within-enclosing-segment", "word_timestamps_used": False, "candidates": candidates, "aligned": aligned, "outside_segment": outside}


def tail_retention(reference: dict[str, Any], records: list[dict[str, Any]], duration: float) -> dict[str, Any]:
    start = duration * 0.9
    wanted = {token for word in reference["words"] if float(word["start"]) >= start for token in tokens(word.get("text"))}
    observed = locally_observed_tokens(records, start, duration)
    return {"tail_start_seconds": start, "terms": len(wanted), "survived_terms": len(wanted & observed), "retention": len(wanted & observed) / len(wanted) if wanted else None}


def annotation_gap_output(reference: dict[str, Any], records: list[dict[str, Any]], duration: float) -> dict[str, Any]:
    bounds = sorted((float(word["start"]), float(word["end"])) for word in reference["words"])
    gaps = []
    prior = 0.0
    for start, end in bounds:
        if start - prior >= ANNOTATION_GAP_MINIMUM_SECONDS:
            gaps.append((prior, start))
        prior = max(prior, end)
    if duration - prior >= ANNOTATION_GAP_MINIMUM_SECONDS:
        gaps.append((prior, duration))
    false_tokens = sum(len(record["tokens"]) for record in records if any(record["start"] >= start and record["end"] <= end for start, end in gaps))
    return {
        "manual_annotation_gaps_at_least_10_seconds": len(gaps),
        "output_tokens_wholly_within_manual_annotation_gaps": false_tokens,
        "limit": "manual annotation gaps are not verified acoustic silence",
    }


def run_text_coverage(run: dict[str, Any]) -> tuple[list[str] | None, dict[str, int], str | None]:
    text = run.get("text")
    if not isinstance(text, str):
        return None, {"run_text_tokens": 0, "segment_text_tokens": 0}, "run text must be a string"
    segments = run.get("segments")
    if not isinstance(segments, list):
        return None, {"run_text_tokens": len(tokens(text)), "segment_text_tokens": 0}, "run segments must be a list"
    run_tokens = tokens(text)
    segment_tokens = [token for segment in segments if isinstance(segment, dict) for token in tokens(segment.get("text"))]
    metric = {"run_text_tokens": len(run_tokens), "segment_text_tokens": len(segment_tokens)}
    if run_tokens != segment_tokens:
        return None, metric, "run text tokens do not match the aggregate segment text"
    return run_tokens, metric, None


def score_run(reference: dict[str, Any], run: Any, duration: float, index: int) -> dict[str, Any]:
    if not isinstance(run, dict):
        return {"index": index, "status": "error", "errors": ["run must be an object"]}
    if run.get("status") == "error":
        return {"index": index, "status": "error", "errors": ["input run status is error"]}
    run_tokens, coverage, coverage_error = run_text_coverage(run)
    if coverage_error:
        return {"index": index, "status": "error", "errors": [coverage_error], "text_coverage": coverage}
    records, timing = segment_records(run, duration)
    if timing["text_segments_with_usable_timing"] != timing["segments_with_tokens"]:
        return {"index": index, "status": "error", "errors": ["one or more output segments with text lack usable timings"], "text_coverage": coverage, "timing": timing}
    observed_tokens = run_tokens
    repeated = sum(left == right for left, right in zip(observed_tokens, observed_tokens[1:]))
    return {
        "index": index,
        "status": "scored",
        "timing": timing,
        "text_coverage": coverage,
        "output_shape": {"tokens": len(observed_tokens), "adjacent_repeated_tokens": repeated},
        "exact_manual_serialization_distance": exact_manual_serialization_distance(reference, observed_tokens),
        "lexical_timing_alignment": lexical_timing_alignment(reference, records),
        "important_span_term_survival": local_span_survival(reference.get("important_spans"), records),
        "annotated_entity_term_survival": local_span_survival(reference.get("entity_spans"), records),
        "tail_retention": tail_retention(reference, records, duration),
        "annotation_gap_output": annotation_gap_output(reference, records, duration),
    }


def score(reference: dict[str, Any], result: dict[str, Any], duration: float) -> dict[str, Any]:
    reference_error = valid_reference(reference)
    if reference_error:
        return {"schema": SCHEMA, "status": "error", "errors": [reference_error]}
    if not finite_time(duration) or float(duration) <= 0:
        return {"schema": SCHEMA, "status": "error", "errors": ["duration must be a positive finite number"]}
    if not isinstance(result, dict):
        return {"schema": SCHEMA, "status": "error", "errors": ["result must be an object"]}
    if result.get("status") == "error":
        return {"schema": SCHEMA, "status": "error", "errors": ["input result status is error"]}
    runs = result.get("runs")
    if not isinstance(runs, list) or not runs:
        return {"schema": SCHEMA, "status": "error", "errors": ["result has no runs"]}
    dependency_error = rapidfuzz_error()
    if dependency_error:
        return {"schema": SCHEMA, "status": "error", "errors": [dependency_error]}
    return {
        "schema": SCHEMA,
        "status": "scored",
        "duration_seconds": float(duration),
        "runs": [score_run(reference, run, float(duration), index) for index, run in enumerate(runs)],
        "limits": [
            "No engine transcript text is returned.",
            "Exact whole-transcript edit counts are computed with RapidFuzz; manual WER remains unsupported for the temporally sorted mixed-channel reference.",
            "Important-span and entity results measure exact annotated local token survival, not semantic commitment or name understanding.",
            "Lexical timing only checks globally unique exact reference tokens against enclosing engine segments; no word timestamps are interpolated.",
            "Manual annotation gaps are not acoustic-silence labels.",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--result", type=Path, required=True)
    parser.add_argument("--duration", type=float, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    try:
        report = score(json.loads(args.reference.read_text()), json.loads(args.result.read_text()), args.duration)
        args.out.write_text(json.dumps(report, sort_keys=True, separators=(",", ":")) + "\n")
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        parser.error(str(exc))
    return 0 if report["status"] == "scored" else 2


if __name__ == "__main__":
    raise SystemExit(main())
