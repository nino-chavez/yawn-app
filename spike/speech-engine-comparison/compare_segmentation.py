#!/usr/bin/env python3
"""Compare fixed transcript grouping rules without ASR or note generation.

Input speech results remain read-only.  The JSON report has hashes, counts and
timing diagnostics only; it never writes recognised text back out.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
TOKEN = re.compile(r"[a-z0-9]+(?:'[a-z0-9]+)?", re.I)
PUNCTUATION = re.compile(r"[.!?][\"\u201d\u2019]*$")
GAP_SECONDS = 1.0
MAX_SECONDS = 15.0
MAX_TOKENS = 40


def sha256_file(path: Path) -> str:
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def lexical_tokens(text: Any) -> list[str]:
    return [match.group(0).casefold() for match in TOKEN.finditer(str(text or ""))]


def require_segment(segment: Any, index: int, duration: float) -> dict[str, Any]:
    if not isinstance(segment, dict) or not isinstance(segment.get("text"), str):
        raise ValueError(f"segment {index} lacks text")
    start, end = segment.get("start"), segment.get("end")
    if any(not isinstance(value, (int, float)) or isinstance(value, bool) for value in (start, end)):
        raise ValueError(f"segment {index} lacks numeric timing")
    start, end = float(start), float(end)
    if not math.isfinite(start) or not math.isfinite(end) or start < 0 or end < start or end > duration + 0.05:
        raise ValueError(f"segment {index} has invalid timing")
    return {"start": start, "end": end, "text": segment["text"], "source_index": index}


def validated_segments(run: dict[str, Any], duration: float) -> tuple[list[dict[str, Any]], float]:
    if not math.isfinite(duration) or duration <= 0:
        raise ValueError("recording duration must be positive and finite")
    segments = run.get("segments")
    if not isinstance(segments, list):
        raise ValueError("run lacks segments")
    prepared = [require_segment(segment, index, float(duration)) for index, segment in enumerate(segments)]
    _require_monotonic(prepared, "segment")
    return prepared, float(duration)


def _require_monotonic(items: list[dict[str, Any]], label: str) -> None:
    if any(
        right["start"] + 1e-6 < left["start"]
        or right["end"] + 1e-6 < left["end"]
        for left, right in zip(items, items[1:])
    ):
        raise ValueError(f"{label} times are nonmonotonic")


def _finish(group: dict[str, Any], groups: list[dict[str, Any]]) -> None:
    groups.append(group)


def native_groups(run: dict[str, Any], fallback: list[dict[str, Any]], duration: float) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    utterances = run.get("utterances")
    if not isinstance(utterances, list) or not utterances:
        return fallback, []
    prepared = [require_segment(item, index, duration) for index, item in enumerate(utterances)]
    _require_monotonic(prepared, "utterance")
    return prepared, []


def punctuation_gap_groups(segments: list[dict[str, Any]]) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    groups: list[dict[str, Any]] = []
    exceptions: list[dict[str, Any]] = []
    current: dict[str, Any] | None = None
    for segment in segments:
        if current is not None and (segment["start"] - current["end"] > GAP_SECONDS or segment["end"] - current["start"] > MAX_SECONDS):
            _finish(current, groups)
            current = None
        if current is None:
            current = dict(segment)
        else:
            current["text"] += " " + segment["text"]
            current["end"] = segment["end"]
            current["source_index"] = (current["source_index"], segment["source_index"])
        if PUNCTUATION.search(segment["text"]):
            _finish(current, groups)
            current = None
    if current is not None:
        _finish(current, groups)
    for index, group in enumerate(groups):
        token_count = len(lexical_tokens(group["text"]))
        if token_count > MAX_TOKENS or group["end"] - group["start"] > MAX_SECONDS:
            exceptions.append({"group_index": index, "kind": "oversized_group", "tokens": token_count, "seconds": group["end"] - group["start"]})
    return groups, exceptions


def bounded_groups(segments: list[dict[str, Any]]) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    groups: list[dict[str, Any]] = []
    exceptions: list[dict[str, Any]] = []
    current: dict[str, Any] | None = None
    current_tokens = 0
    for segment in segments:
        token_count = len(lexical_tokens(segment["text"]))
        segment_seconds = segment["end"] - segment["start"]
        oversize = token_count > MAX_TOKENS or segment_seconds > MAX_SECONDS
        if oversize:
            if current is not None:
                _finish(current, groups)
                current = None
                current_tokens = 0
            groups.append(dict(segment))
            exceptions.append({"group_index": len(groups) - 1, "kind": "oversized_source_segment", "tokens": token_count, "seconds": segment_seconds})
            continue
        would_exceed = current is not None and (current_tokens + token_count > MAX_TOKENS or segment["end"] - current["start"] > MAX_SECONDS)
        if would_exceed:
            _finish(current, groups)
            current = None
            current_tokens = 0
        if current is None:
            current = dict(segment)
            current_tokens = token_count
        else:
            current["text"] += " " + segment["text"]
            current["end"] = segment["end"]
            current["source_index"] = (current["source_index"], segment["source_index"])
            current_tokens += token_count
    if current is not None:
        _finish(current, groups)
    return groups, exceptions


def _shape(groups: list[dict[str, Any]], exceptions: list[dict[str, Any]]) -> dict[str, Any]:
    tokens = [token for group in groups for token in lexical_tokens(group["text"])]
    bounds = [{"start": group["start"], "end": group["end"]} for group in groups]
    return {
        "groups": len(groups),
        "lexical_tokens": len(tokens),
        "token_sha256": hashlib.sha256("\0".join(tokens).encode()).hexdigest(),
        "timing_bounds": bounds,
        "exceptions": exceptions,
    }


def candidate_workload(groups: list[dict[str, Any]]) -> dict[str, int]:
    """Measure the current classifier request path without calling a model."""
    import sys
    sys.path[:0] = [str(ROOT), str(ROOT / "notes"), str(ROOT / "spike")]
    import candidate_first
    from transcript import PromptOverlay, Transcript, Turn
    from worker.note_validator import _present_classification_user

    transcript = Transcript(source="segmentation-pilot", attribution="channel", turns=[Turn(text=group["text"], speaker="Me", start=group["start"]) for group in groups])
    manifest = candidate_first.generate_manifest(
        transcript,
        candidate_first.STRATEGY_BROAD,
        contract=candidate_first.PRODUCT_CONTRACT,
    )
    candidate_first.validate_manifest(manifest, transcript)
    registered = candidate_first.PRODUCT_RUN["classifier"]
    batch_size = int(registered["batch_size"])
    offer_stride = int(registered["offer_stride"])
    offered = candidate_first.offered_candidates(manifest["candidates"], offer_stride)
    batches = candidate_first.candidate_batches(offered, batch_size)
    overlay = PromptOverlay.from_transport(transcript)
    user_bytes = []
    system_bytes = []
    for batch in batches:
        _schema, system, user = candidate_first.classification_request(
            transcript,
            manifest,
            batch,
            batch_size,
            offer_stride=offer_stride,
        )
        presented = _present_classification_user(user, transcript, overlay)
        user_bytes.append(len(presented.encode("utf-8")))
        system_bytes.append(len(system.encode("utf-8")))
    return {
        "manifest_candidates": len(manifest["candidates"]),
        "offered_candidates": len(offered),
        "model_calls": len(batches),
        "offer_stride": offer_stride,
        "classifier_user_prompt_bytes_total": sum(user_bytes),
        "classifier_user_prompt_bytes_max": max(user_bytes, default=0),
        "classifier_system_prompt_bytes_total": sum(system_bytes),
        "classifier_system_prompt_bytes_max": max(system_bytes, default=0),
        "classifier_prompt_bytes_total": sum(user_bytes) + sum(system_bytes),
        "classifier_prompt_bytes_max": max(
            (user + system for user, system in zip(user_bytes, system_bytes)),
            default=0,
        ),
    }


def _source_indices(value: Any) -> list[int]:
    if isinstance(value, int) and not isinstance(value, bool):
        return [value]
    if isinstance(value, tuple):
        return [index for child in value for index in _source_indices(child)]
    raise ValueError("derived group lacks source provenance")


def _validate_derived_groups(
    source: list[dict[str, Any]], groups: list[dict[str, Any]], name: str,
) -> None:
    """Ensure local grouping only joins whole, ordered source segments."""
    recovered: list[int] = []
    for group in groups:
        indices = _source_indices(group.get("source_index"))
        if not indices or indices != list(range(indices[0], indices[-1] + 1)):
            raise ValueError(f"{name} changed source segment order")
        expected = source[indices[0]:indices[-1] + 1]
        if (
            group["start"] != expected[0]["start"]
            or group["end"] != expected[-1]["end"]
            or group["text"] != " ".join(row["text"] for row in expected)
        ):
            raise ValueError(f"{name} changed source endpoints or text")
        recovered.extend(indices)
    if recovered != list(range(len(source))):
        raise ValueError(f"{name} did not preserve every source segment")


def compare_run(run: dict[str, Any], duration: float, engine: str) -> dict[str, Any]:
    segments, duration = validated_segments(run, duration)
    native, native_exceptions = native_groups(run, segments, duration)
    punctuation, punctuation_exceptions = punctuation_gap_groups(segments)
    bounded, bounded_exceptions = bounded_groups(segments)
    _validate_derived_groups(segments, punctuation, "punctuation_gap_1s_max_15s")
    _validate_derived_groups(segments, bounded, "bounded_40_tokens_max_15s")
    if engine == "apple":
        current, current_exceptions = native, native_exceptions
        current_provenance = "Apple final utterance groups"
        native_provenance = "Apple native utterance API groups"
    elif engine == "parakeet":
        current, current_exceptions = punctuation, punctuation_exceptions
        current_provenance = "Parakeet current pilot punctuation-gap grouping"
        native_provenance = "Parakeet native timed word API groups"
    else:
        raise ValueError(f"unsupported engine for current-pilot mapping: {engine}")
    methods = {
        "native_api": (native, native_exceptions, native_provenance),
        "current_pilot": (current, current_exceptions, current_provenance),
        "punctuation_gap_1s_max_15s": (punctuation, punctuation_exceptions, "fixed cross-engine punctuation and gap rule"),
        "bounded_40_tokens_max_15s": (bounded, bounded_exceptions, "fixed bounded token and duration rule"),
    }
    output = {}
    for name, (groups, exceptions, provenance) in methods.items():
        shaped = _shape(groups, exceptions) | {"provenance": provenance}
        if engine == "parakeet" and name == "native_api":
            shaped.update({
                "candidate_workload": None,
                "workload_status": "unmeasured",
                "workload_reason": (
                    "raw Parakeet timed words are a native API shape, not an "
                    "implemented pilot grouping"
                ),
            })
        else:
            shaped.update({
                "candidate_workload": candidate_workload(groups),
                "workload_status": "measured",
            })
        output[name] = shaped
    original = [token for segment in segments for token in lexical_tokens(segment["text"])]
    for name, (groups, _exceptions, _provenance) in methods.items():
        grouped = [token for group in groups for token in lexical_tokens(group["text"])]
        if grouped != original:
            raise ValueError(f"{name} did not preserve lexical token order")
    return {"duration_seconds": duration, "source_segments": len(segments), "source_lexical_tokens": len(original), "methods": output}


def parse_input(value: str) -> tuple[str, Path]:
    label, separator, raw_path = value.partition("=")
    if not separator or not label or not raw_path:
        raise argparse.ArgumentTypeError("--input must be LABEL=PATH")
    return label, Path(raw_path).expanduser().resolve()


def parse_duration(value: str) -> tuple[str, float]:
    label, separator, raw_duration = value.partition("=")
    try:
        duration = float(raw_duration)
    except ValueError as exc:
        raise argparse.ArgumentTypeError("--duration must be LABEL=SECONDS") from exc
    if not separator or not label or not math.isfinite(duration) or duration <= 0:
        raise argparse.ArgumentTypeError("--duration must be LABEL=positive seconds")
    return label, duration


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", action="append", required=True, type=parse_input)
    parser.add_argument("--duration", action="append", required=True, type=parse_duration,
                        help="recording duration for one input; result process_seconds is not audio duration")
    parser.add_argument("--run", type=int, default=0)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.out.exists():
        parser.error("--out must be new; source results stay immutable")
    durations = dict(args.duration)
    reports = {}
    for label, path in args.input:
        result = json.loads(path.read_text())
        if result.get("schema") != "speech-engine-result/1" or result.get("status") != "ok":
            parser.error(f"{label} is not a successful speech-engine-result/1")
        runs = result.get("runs")
        if not isinstance(runs, list) or not 0 <= args.run < len(runs):
            parser.error(f"{label} has no selected run")
        if label not in durations:
            parser.error(f"{label} needs an explicit --duration; process_seconds is not recording duration")
        engine = result.get("engine")
        reports[label] = {"input_sha256": sha256_file(path), "engine": engine, "run": args.run, "recording_duration_seconds": durations[label], "comparison": compare_run(runs[args.run], durations[label], engine)}
    document = {"schema": "segmentation-comparison/1", "rules": {"gap_seconds": GAP_SECONDS, "max_seconds": MAX_SECONDS, "max_lexical_tokens": MAX_TOKENS, "source_segment_rule": "never split a source segment; report it oversized"}, "inputs": reports, "limits": ["raw per-leg representation and classifier-request diagnostic only; it does not predict merged or filtered note-time workload", "no quality or latency claim", "recognised text is not written to this report", "recording duration is explicit because result process_seconds is not audio duration", "manual annotation gaps are not audio silence"]}
    previous_umask = os.umask(0o077)
    try:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(document, sort_keys=True, separators=(",", ":")) + "\n")
        args.out.chmod(0o600)
    finally:
        os.umask(previous_umask)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
