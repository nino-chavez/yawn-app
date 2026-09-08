"""Validate native speech-engine receipts before they become Yawn transcript turns.

This boundary accepts only the private ``speech-engine-result/1`` files emitted
by the research probe.  It deliberately preserves native time bounds: a bad
or incomplete result is refused instead of being repaired into a transcript.
"""

from __future__ import annotations

import math
import re
from collections.abc import Mapping
from typing import Any


class SpeechResultRefused(ValueError):
    """A native result cannot safely become a source-bound transcript."""


# Core Media times can pass through JSON with a sub-microsecond rounding error.
# This tolerance permits that representation noise while retaining the original
# native endpoint rather than clamping or otherwise inventing a new one.
TIMING_TOLERANCE_SECONDS = 1e-6
_TOKEN = re.compile(r"\S+")


def _number(value: Any, description: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise SpeechResultRefused(f"{description} must be a finite number")
    number = float(value)
    if not math.isfinite(number):
        raise SpeechResultRefused(f"{description} must be a finite number")
    return number


def _tokens(text: str) -> list[str]:
    return _TOKEN.findall(text)


def _timed_items(
    value: Any,
    *,
    description: str,
    duration_seconds: float,
) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        raise SpeechResultRefused(f"{description} must be a list")
    items: list[dict[str, Any]] = []
    previous_start: float | None = None
    previous_end: float | None = None
    for index, item in enumerate(value):
        if not isinstance(item, Mapping):
            raise SpeechResultRefused(f"{description}[{index}] must be an object")
        text = item.get("text")
        if not isinstance(text, str) or not text.strip():
            raise SpeechResultRefused(f"{description}[{index}] has no text")
        start = _number(item.get("start"), f"{description}[{index}].start")
        end = _number(item.get("end"), f"{description}[{index}].end")
        if end <= start:
            raise SpeechResultRefused(f"{description}[{index}] has unordered bounds")
        if start < -TIMING_TOLERANCE_SECONDS or end > duration_seconds + TIMING_TOLERANCE_SECONDS:
            raise SpeechResultRefused(f"{description}[{index}] is outside recording bounds")
        if previous_start is not None and (
            start + TIMING_TOLERANCE_SECONDS < previous_start
            or end + TIMING_TOLERANCE_SECONDS < previous_end
        ):
            raise SpeechResultRefused(
                f"{description}[{index}] is non-monotonic with the preceding item"
            )
        items.append({"start": start, "end": end, "text": text})
        previous_start = start
        previous_end = end
    return items


def _require_full_text_coverage(
    run_text: str,
    items: list[dict[str, Any]],
    *,
    description: str,
) -> None:
    if _tokens(run_text) != [token for item in items for token in _tokens(item["text"])]:
        raise SpeechResultRefused(f"{description} do not cover the full native run text")


def _group_parakeet_words(words: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Keep the experiment's explicit grouping contract for word-only output."""
    groups: list[dict[str, Any]] = []
    current: dict[str, Any] | None = None
    for word in words:
        if current is not None and (
            word["start"] - current["end"] > 1.0
            or word["end"] - current["start"] > 15.0
        ):
            groups.append(current)
            current = None
        if current is None:
            current = dict(word)
        else:
            current["text"] += " " + word["text"]
            current["end"] = word["end"]
        if re.search(r'[.!?]["\u201d\u2019]*$', word["text"]):
            groups.append(current)
            current = None
    if current is not None:
        groups.append(current)
    return groups


def _require_apple_utterance_alignment(
    segments: list[dict[str, Any]], utterances: list[dict[str, Any]]
) -> None:
    """Prove each Apple final utterance contains its sequential timed words."""
    segment_index = 0
    for utterance_index, utterance in enumerate(utterances):
        utterance_tokens = _tokens(utterance["text"])
        matched_tokens: list[str] = []
        while len(matched_tokens) < len(utterance_tokens):
            if segment_index >= len(segments):
                raise SpeechResultRefused(
                    f"runs Apple utterances[{utterance_index}] lack corresponding timed segments"
                )
            segment = segments[segment_index]
            segment_tokens = _tokens(segment["text"])
            token_end = len(matched_tokens) + len(segment_tokens)
            if utterance_tokens[len(matched_tokens):token_end] != segment_tokens:
                raise SpeechResultRefused(
                    f"runs Apple utterances[{utterance_index}] do not align with timed segments"
                )
            if (
                segment["start"] < utterance["start"] - TIMING_TOLERANCE_SECONDS
                or segment["end"] > utterance["end"] + TIMING_TOLERANCE_SECONDS
            ):
                raise SpeechResultRefused(
                    f"runs Apple utterances[{utterance_index}] do not enclose timed segments"
                )
            matched_tokens.extend(segment_tokens)
            segment_index += 1
        if matched_tokens != utterance_tokens:
            raise SpeechResultRefused(
                f"runs Apple utterances[{utterance_index}] split a timed segment"
            )
    if segment_index != len(segments):
        raise SpeechResultRefused("runs Apple timed segments lack a final utterance")


def _validate_run(
    run: Any,
    *,
    engine: str,
    duration_seconds: float,
    index: int,
) -> list[dict[str, Any]]:
    if not isinstance(run, Mapping):
        raise SpeechResultRefused(f"runs[{index}] must be an object")
    timing_issue_count = run.get("timing_issue_count")
    if (
        isinstance(timing_issue_count, bool)
        or not isinstance(timing_issue_count, int)
        or timing_issue_count != 0
    ):
        raise SpeechResultRefused(f"runs[{index}] reports unavailable or invalid timing")
    text = run.get("text")
    if not isinstance(text, str):
        raise SpeechResultRefused(f"runs[{index}] has no text")
    segments = _timed_items(
        run.get("segments"),
        description=f"runs[{index}].segments",
        duration_seconds=duration_seconds,
    )
    _require_full_text_coverage(text, segments, description=f"runs[{index}].segments")

    if engine == "apple":
        utterances = _timed_items(
            run.get("utterances"),
            description=f"runs[{index}].utterances",
            duration_seconds=duration_seconds,
        )
        _require_full_text_coverage(
            text, utterances, description=f"runs[{index}].utterances"
        )
        _require_apple_utterance_alignment(segments, utterances)
        return utterances
    if engine == "parakeet":
        grouped = _group_parakeet_words(segments)
        # Grouping preserves the word endpoints, but check the derived shape
        # explicitly so a future grouping change cannot silently reverse it.
        return _timed_items(
            grouped,
            description=f"runs[{index}].grouped_segments",
            duration_seconds=duration_seconds,
        )
    raise AssertionError(f"unexpected validated engine {engine}")


def validated_native_segments(
    result: Mapping[str, Any],
    *,
    engine: str,
    duration_seconds: float,
) -> list[dict[str, Any]]:
    """Return native-bound Yawn turns or refuse malformed Apple/Parakeet output.

    All repeats are checked before selecting the first, so a partially malformed
    result file cannot look clean simply because its first run was usable.
    """
    duration = _number(duration_seconds, "recording duration")
    if duration <= 0:
        raise SpeechResultRefused("recording duration must be positive")
    if engine not in {"apple", "parakeet"}:
        raise SpeechResultRefused("native engine must be apple or parakeet")
    if not isinstance(result, Mapping):
        raise SpeechResultRefused("native result must be an object")
    if result.get("schema") != "speech-engine-result/1":
        raise SpeechResultRefused("native result schema is not speech-engine-result/1")
    if result.get("status") != "ok":
        raise SpeechResultRefused("native result is not successful")
    if result.get("engine") != engine:
        raise SpeechResultRefused(
            f"native result engine {result.get('engine')!r} does not match {engine!r}"
        )
    runs = result.get("runs")
    if not isinstance(runs, list) or not runs:
        raise SpeechResultRefused("native result has no runs")
    adapted = [
        _validate_run(run, engine=engine, duration_seconds=duration, index=index)
        for index, run in enumerate(runs)
    ]
    return adapted[0]
