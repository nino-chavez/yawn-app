"""Optional, local-only Nemotron speaker-cluster suggestions.

This is deliberately not a transcript writer.  It runs only when called,
keeps the diarizer's clusters anonymous, and writes a content-free sidecar
bound to the existing transcript revision and retained microphone WAV.
"""

from __future__ import annotations

import hashlib
import json
import math
import os
import stat
import subprocess
import tempfile
from collections.abc import Callable, Mapping
from pathlib import Path
from typing import Any

from .storage import (
    StorageRefused,
    durable_create_or_verify_identical,
    opaque_id,
    private_directory,
    read_private_file,
    require_private_root,
    resolve_below,
)

SCHEMA = "speaker-diarization-suggestion/1"
MAX_DURATION_SECONDS = 24 * 60 * 60
MAX_INTERVALS = 50_000
MAX_SPEAKERS = 8
MAX_OUTPUT_BYTES = 8 * 1024 * 1024
DIARIZATION_TIMEOUT_SECONDS = 15 * 60
MAX_TRANSCRIPT_BYTES = 16 * 1024 * 1024
# NeMo-Speech.cpp reports millisecond timestamps. A captured WAV can end
# between those ticks; one retained Yawn run ends 1.937 ms past its WAV.
MAX_END_ROUNDING_SECONDS = 0.01


class DiarizationRefused(ValueError):
    """The optional diarization request cannot produce a safe sidecar."""


def _digest_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def _digest_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _finite_number(value: object, label: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise DiarizationRefused(f"{label} must be a finite number")
    number = float(value)
    if not math.isfinite(number):
        raise DiarizationRefused(f"{label} must be a finite number")
    return number


def _private_regular_file(path: Path, label: str, *, executable: bool = False) -> Path:
    supplied = path.expanduser()
    if supplied.is_symlink():
        raise DiarizationRefused(f"{label} is missing or unsafe")
    try:
        resolved = supplied.resolve(strict=True)
        metadata = resolved.stat()
    except OSError as exc:
        raise DiarizationRefused(f"{label} is missing or unsafe") from exc
    if not resolved.is_file() or stat.S_ISLNK(metadata.st_mode):
        raise DiarizationRefused(f"{label} is missing or unsafe")
    if executable and not os.access(resolved, os.X_OK):
        raise DiarizationRefused(f"{label} is not executable")
    return resolved


def _validated_wav(path: Path) -> tuple[Path, float]:
    import wave

    if path.is_symlink() or not path.is_file():
        raise DiarizationRefused("microphone audio is missing or unsafe")
    if stat.S_IMODE(path.stat().st_mode) != 0o600:
        raise DiarizationRefused("microphone audio is not private")
    try:
        with wave.open(str(path), "rb") as audio:
            if (
                audio.getnchannels() != 1
                or audio.getsampwidth() != 2
                or audio.getframerate() != 16_000
                or audio.getcomptype() != "NONE"
            ):
                raise DiarizationRefused(
                    "microphone audio is not mono 16 kHz 16-bit PCM"
                )
            frames = audio.getnframes()
            remaining_bytes = frames * 2
            while remaining_bytes:
                chunk = audio.readframes(min(remaining_bytes // 2, 32_768))
                if not chunk:
                    break
                remaining_bytes -= len(chunk)
    except (EOFError, OSError, wave.Error) as exc:
        raise DiarizationRefused(f"microphone audio is not a readable WAV ({exc})") from None
    if remaining_bytes != 0 or frames <= 0:
        raise DiarizationRefused("microphone audio is truncated or empty")
    duration = frames / 16_000
    if duration > MAX_DURATION_SECONDS:
        raise DiarizationRefused("microphone audio exceeds the diarization duration limit")
    return path, duration


def _arguments(value: object) -> dict[str, str]:
    names = {"meeting_id", "source_transcript_sha256", "microphone_audio_sha256"}
    if not isinstance(value, dict) or set(value) != names:
        raise DiarizationRefused("speaker.suggest arguments do not match the closed schema")
    meeting_id = opaque_id(value["meeting_id"], "meeting_id")
    result = {"meeting_id": meeting_id}
    for name in ("source_transcript_sha256", "microphone_audio_sha256"):
        digest = value[name]
        if (
            not isinstance(digest, str)
            or len(digest) != 64
            or any(character not in "0123456789abcdef" for character in digest)
        ):
            raise DiarizationRefused(f"{name} must be a lowercase SHA-256 digest")
        result[name] = digest
    return result


def _read_transcript(root: Path, meeting_id: str, digest: str) -> dict[str, Any]:
    path = resolve_below(root, "meetings", meeting_id, "transcript", f"{digest}.json")
    try:
        data = read_private_file(
            path, max_bytes=MAX_TRANSCRIPT_BYTES, label="transcript revision"
        )
    except StorageRefused as exc:
        raise DiarizationRefused(str(exc)) from None
    if _digest_bytes(data) != digest:
        raise DiarizationRefused("transcript revision changed from its content address")
    try:
        document = json.loads(data)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise DiarizationRefused("transcript revision is not valid UTF-8 JSON") from exc
    if not isinstance(document, dict) or document.get("schema") != "capture-transcript/1":
        raise DiarizationRefused("speaker suggestions require an immutable base transcript")
    if document.get("attribution") != "channel":
        raise DiarizationRefused("transcript does not identify microphone turns")
    if not isinstance(document.get("turns"), list):
        raise DiarizationRefused("transcript has no turn list")
    # Keep the existing transcript parser as the semantic authority. It sees a
    # bounded private snapshot, never a caller-controlled path that could move
    # after the content-address check above.
    try:
        from transcript import load

        with tempfile.TemporaryDirectory(dir=root) as temporary_name:
            temporary = Path(temporary_name)
            private_directory(temporary)
            snapshot = temporary / "transcript.json"
            durable_create_or_verify_identical(snapshot, data)
            load(snapshot)
    except (KeyError, OSError, TypeError, ValueError) as exc:
        raise DiarizationRefused(f"base transcript is invalid: {exc}") from None
    return document


def _cluster_key(value: object) -> str:
    # The observed standalone NeMo-Speech.cpp JSON emits 1-based integer
    # speakers. Keeping this closed avoids treating an unexpected label as an
    # identity-bearing string.
    if isinstance(value, bool) or not isinstance(value, int) or value < 1:
        raise DiarizationRefused("diarizer segment speaker is invalid")
    return f"integer:{value}"


def parse_diarizer_output(output: bytes, *, duration_seconds: float) -> list[dict[str, Any]]:
    """Validate observed NeMo-Speech.cpp JSON without retaining its ``file`` path."""
    if len(output) > MAX_OUTPUT_BYTES:
        raise DiarizationRefused("diarizer output exceeds the bounded read limit")
    try:
        document = json.loads(output)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise DiarizationRefused("diarizer output is not valid UTF-8 JSON") from exc
    if not isinstance(document, dict) or set(document) != {"file", "segments"}:
        raise DiarizationRefused("diarizer output does not match the observed JSON shape")
    if not isinstance(document["file"], str) or not isinstance(document["segments"], list):
        raise DiarizationRefused("diarizer output does not match the observed JSON shape")
    if len(document["segments"]) > MAX_INTERVALS:
        raise DiarizationRefused("diarizer output has too many segments")

    parsed: list[tuple[float, float, str]] = []
    for index, raw in enumerate(document["segments"]):
        if not isinstance(raw, Mapping) or set(raw) != {"start", "end", "speaker"}:
            raise DiarizationRefused(f"diarizer segment {index} has an invalid shape")
        start = _finite_number(raw["start"], f"diarizer segment {index} start")
        end = _finite_number(raw["end"], f"diarizer segment {index} end")
        if (
            start < 0
            or end <= start
            or start >= duration_seconds
            or end > duration_seconds + MAX_END_ROUNDING_SECONDS
        ):
            raise DiarizationRefused(f"diarizer segment {index} is outside microphone bounds")
        parsed.append((start, min(end, duration_seconds), _cluster_key(raw["speaker"])))

    keys = sorted({speaker for _, _, speaker in parsed})
    if len(keys) > MAX_SPEAKERS:
        raise DiarizationRefused("diarizer output has too many anonymous clusters")
    labels = {key: f"cluster-{index + 1}" for index, key in enumerate(keys)}
    # Do not merge, sort, or de-overlap these intervals: overlap is evidence the
    # later reviewer needs, and an interval is not an asserted turn identity.
    return [
        {"start": start, "end": end, "cluster": labels[speaker]}
        for start, end, speaker in parsed
    ]


def _union_seconds(intervals: list[tuple[float, float]]) -> float:
    total = 0.0
    end: float | None = None
    start: float | None = None
    for low, high in sorted(intervals):
        if end is None or low > end:
            if start is not None and end is not None:
                total += end - start
            start, end = low, high
        else:
            end = max(end, high)
    if start is not None and end is not None:
        total += end - start
    return total


def match_mic_turns(
    transcript: Mapping[str, Any], intervals: list[dict[str, Any]], *, duration_seconds: float
) -> list[dict[str, Any]]:
    """Return content-free cluster presence for only source ``Me`` turns."""
    suggestions: list[dict[str, Any]] = []
    for index, turn in enumerate(transcript["turns"]):
        if not isinstance(turn, Mapping):
            raise DiarizationRefused(f"transcript turn {index} is invalid")
        if turn.get("speaker") != "Me":
            continue
        start = _finite_number(turn.get("start"), f"transcript turn {index} start")
        end = _finite_number(turn.get("end"), f"transcript turn {index} end")
        if start < 0 or end <= start:
            raise DiarizationRefused(f"transcript turn {index} has invalid timing")
        low, high = max(0.0, start), min(duration_seconds, end)
        by_cluster: dict[str, list[tuple[float, float]]] = {}
        if high > low:
            for interval in intervals:
                overlap_low = max(low, interval["start"])
                overlap_high = min(high, interval["end"])
                if overlap_high > overlap_low:
                    by_cluster.setdefault(interval["cluster"], []).append(
                        (overlap_low, overlap_high)
                    )
        suggestions.append(
            {
                "source_turn_index": index,
                "speaker_presence": [
                    {
                        "cluster": cluster,
                        "seconds": round(_union_seconds(ranges), 6),
                    }
                    for cluster, ranges in sorted(by_cluster.items())
                ],
            }
        )
    return suggestions


Runner = Callable[..., subprocess.CompletedProcess[str]]


def speaker_suggest(
    root: Path,
    arguments: object,
    *,
    diarizer_executable: Path | None,
    diarizer_model: Path | None,
    runner: Runner = subprocess.run,
) -> dict[str, str]:
    """Create an explicit, private, review-only Nemotron suggestion sidecar.

    Missing optional runtime material is an unavailable result. All other input,
    execution, and parsing failures are explicit errors and leave prior sidecars
    unchanged because writing happens only after the candidate is complete.
    """
    try:
        root = require_private_root(root)
        values = _arguments(arguments)
        if diarizer_executable is None or diarizer_model is None:
            return {"status": "unavailable", "reason": "diarizer runtime is not configured"}
        executable = _private_regular_file(
            diarizer_executable, "diarizer executable", executable=True
        )
        model = _private_regular_file(diarizer_model, "diarizer model")
        if model.suffix.lower() != ".gguf":
            raise DiarizationRefused("diarizer model is not a GGUF file")
        executable_digest = _digest_file(executable)
        model_digest = _digest_file(model)

        capture_dir = resolve_below(root, "meetings", values["meeting_id"], "capture")
        from verify_capture import verify_acquisition

        verified = verify_acquisition(capture_dir)
        microphone = capture_dir / "mic.wav"
        microphone, duration = _validated_wav(microphone)
        microphone_digest = _digest_file(microphone)
        if (
            microphone_digest != values["microphone_audio_sha256"]
            or verified["mic_sha256"] != microphone_digest
        ):
            raise DiarizationRefused("microphone audio changed from its requested capture")
        transcript = _read_transcript(
            root, values["meeting_id"], values["source_transcript_sha256"]
        )

        command = [
            str(executable),
            "diarize",
            str(microphone),
            "--model",
            str(model),
            "--device",
            "metal",
            "--preset",
            "v3-offline",
            "--format",
            "json",
        ]
        completed = runner(
            command,
            check=False,
            shell=False,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=DIARIZATION_TIMEOUT_SECONDS,
            env={"PATH": "/usr/bin:/bin", "NO_COLOR": "1"},
        )
        if completed.returncode != 0:
            return {"status": "error", "reason": "diarizer process failed"}
        if not isinstance(completed.stdout, str):
            raise DiarizationRefused("diarizer process did not return text output")
        output = completed.stdout.encode("utf-8")
        if (
            _digest_file(microphone) != microphone_digest
            or _digest_file(executable) != executable_digest
            or _digest_file(model) != model_digest
        ):
            raise DiarizationRefused("diarizer input or runtime changed during execution")
        intervals = parse_diarizer_output(output, duration_seconds=duration)
        turn_suggestions = match_mic_turns(
            transcript, intervals, duration_seconds=duration
        )
        document = {
            "schema": SCHEMA,
            "status": "review-required" if intervals else "no-speakers",
            "source": {
                "transcript_sha256": values["source_transcript_sha256"],
                "microphone_audio_sha256": microphone_digest,
                "diarizer_executable_sha256": executable_digest,
                "diarizer_model_sha256": model_digest,
                "diarizer_output_sha256": _digest_bytes(output),
            },
            "duration_seconds": duration,
            "anonymous_cluster_count": len({item["cluster"] for item in intervals}),
            "intervals": intervals,
            "mic_turn_suggestions": turn_suggestions,
        }
        encoded = (json.dumps(document, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
        sidecar_digest = _digest_bytes(encoded)
        sidecar_directory = resolve_below(
            root, "meetings", values["meeting_id"], "speaker-suggestions"
        )
        private_directory(sidecar_directory)
        target = resolve_below(
            root,
            "meetings",
            values["meeting_id"],
            "speaker-suggestions",
            f"{sidecar_digest}.json",
        )
        durable_create_or_verify_identical(target, encoded)
        return {
            "status": "suggestions-ready",
            "speaker-suggestion": sidecar_digest,
            "transcript": values["source_transcript_sha256"],
            "capture-mic": microphone_digest,
            "diarizer": executable_digest,
            "diarizer-model": model_digest,
        }
    except (ValueError, StorageRefused, OSError, subprocess.TimeoutExpired) as exc:
        return {"status": "error", "reason": str(exc)}
