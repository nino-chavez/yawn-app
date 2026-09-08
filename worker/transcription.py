"""Offline transcript creation for one validated dual-leg acquisition."""

from __future__ import annotations

import json
import os
import uuid
import wave
from pathlib import Path
from typing import Callable

import numpy as np

from .storage import StorageRefused, durable_create_new, private_directory


class TranscriptionRefused(ValueError):
    """The capture or fixed offline model cannot produce a canonical transcript."""


def _read_leg(path: Path) -> np.ndarray:
    if path.is_symlink() or not path.is_file():
        raise TranscriptionRefused(f"{path.name} is missing or unsafe")
    try:
        with wave.open(str(path), "rb") as audio:
            if (
                audio.getnchannels() != 1
                or audio.getsampwidth() != 2
                or audio.getframerate() != 16_000
                or audio.getcomptype() != "NONE"
            ):
                raise TranscriptionRefused(
                    f"{path.name} is not mono 16 kHz 16-bit PCM"
                )
            frames = audio.getnframes()
            encoded = audio.readframes(frames)
    except (EOFError, OSError, wave.Error) as exc:
        raise TranscriptionRefused(f"{path.name} is not a readable WAV ({exc})") from None
    if len(encoded) != frames * 2:
        raise TranscriptionRefused(f"{path.name} is truncated")
    return np.frombuffer(encoded, dtype="<i2").astype(np.float32) / 32768.0


def _transcript_health(acquisition_health: dict) -> dict:
    from capture_health import build

    try:
        legs = acquisition_health["legs"]
        return build(
            mic_samples=legs["mic"]["samples"],
            system_samples=legs["system"]["samples"],
            capture_elapsed_samples=acquisition_health["timing"][
                "capture_elapsed_samples"
            ],
            dropouts={
                "mic": legs["mic"]["dropouts"],
                "system": legs["system"]["dropouts"],
            },
            tap_errors=legs["system"]["tap_errors"],
            transcription_requested=True,
            transcript_written=True,
        )
    except (KeyError, TypeError, ValueError) as exc:
        raise TranscriptionRefused(f"capture health cannot be carried forward ({exc})") from None


def require_whisper_model(model_dir: Path) -> Path:
    supplied = model_dir.expanduser()
    if supplied.is_symlink():
        raise TranscriptionRefused("offline transcript model may not be a symlink")
    try:
        resolved = supplied.resolve(strict=True)
    except OSError as exc:
        raise TranscriptionRefused(f"offline transcript model is missing ({exc})") from None
    if not resolved.is_dir():
        raise TranscriptionRefused("offline transcript model is not a directory")
    config = resolved / "config.json"
    if config.is_symlink() or not config.is_file():
        raise TranscriptionRefused("offline transcript model lacks safe config.json")
    weights = [resolved / "weights.safetensors", resolved / "weights.npz"]
    present = [path for path in weights if path.exists() or path.is_symlink()]
    if len(present) != 1 or present[0].is_symlink() or not present[0].is_file():
        raise TranscriptionRefused(
            "offline transcript model needs exactly one safe weights file"
        )
    return resolved


def _release_mlx_whisper_runtime() -> None:
    """Drop mlx-whisper's process-global model and reclaim its Metal cache.

    The application worker serves more than one request, while mlx-whisper's
    ``ModelHolder`` intentionally keeps the last model alive for reuse. Yawn
    has no transcript request to reuse it for once both capture legs finish,
    so retaining that cache would make a completed transcript keep several
    gigabytes of unified memory until the whole application exits.
    """
    import gc

    try:
        from mlx_whisper.transcribe import ModelHolder
    except ImportError:
        return

    ModelHolder.model = None
    ModelHolder.model_path = None
    gc.collect()
    try:
        import mlx.core as mx

        mx.clear_cache()
    except (AttributeError, ImportError, RuntimeError):
        # The model reference is the important release. Cache cleanup is an
        # optimization and must not turn a valid transcript into a refusal.
        pass


def _verify_capture(capture_dir: Path) -> None:
    """Refuse an unverified acquisition before any model or audio work."""
    from verify_capture import verify_acquisition

    verify_acquisition(capture_dir)


def _capture_health(capture_dir: Path) -> dict:
    """Load the already-verified acquisition health into a transcript receipt."""
    try:
        session = json.loads((capture_dir / "session.json").read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise TranscriptionRefused(f"capture session is unreadable ({exc})") from None
    return _transcript_health(session.get("health"))


def _read_capture_audio(
    capture_dir: Path,
) -> tuple[np.ndarray, np.ndarray, dict | None]:
    """Decode both checked capture legs and derive their acoustic overlap."""
    from dual_capture import bleed

    mic = _read_leg(capture_dir / "mic.wav")
    system = _read_leg(capture_dir / "system.wav")
    return mic, system, bleed(mic, system)


def _capture_context(capture_dir: Path) -> tuple[dict, np.ndarray, np.ndarray, dict | None]:
    """Verify one acquisition and load the audio needed by native filters."""
    _verify_capture(capture_dir)
    health = _capture_health(capture_dir)
    mic, system, acoustic = _read_capture_audio(capture_dir)
    return health, mic, system, acoustic


def _filter_mic_segments(
    mic_segments: list[dict],
    *,
    mic: np.ndarray,
    system: np.ndarray,
    acoustic: dict | None,
    voicing_filter: Callable[[list[dict], np.ndarray, str], list[dict]] | None,
    bleed_filter: Callable[
        [list[dict], np.ndarray, np.ndarray, dict | None, str], list[dict]
    ]
    | None,
    gate_filter: Callable[
        [list[dict], np.ndarray, dict | None, str],
        tuple[list[dict], dict | None],
    ]
    | None,
) -> tuple[list[dict], dict | None, bool]:
    """Apply the established mic-only voicing, bleed, and operator gate order."""
    from dual_capture import drop_bled, drop_unvoiced

    voicing_filter = voicing_filter or drop_unvoiced
    bleed_filter = bleed_filter or drop_bled
    voiced = voicing_filter(mic_segments, mic, "mic")
    filtered = bleed_filter(voiced, mic, system, acoustic, "mic")
    partial_bleed_detected = len(filtered) < len(voiced)
    gating = None
    if gate_filter is not None:
        filtered, gating = gate_filter(filtered, mic, acoustic, "mic")
    return filtered, gating, partial_bleed_detected


def _filter_system_segments(
    system_segments: list[dict],
    *,
    system: np.ndarray,
    voicing_filter: Callable[[list[dict], np.ndarray, str], list[dict]] | None,
) -> list[dict]:
    """Apply the established system-leg voicing filter."""
    from dual_capture import drop_unvoiced

    return (voicing_filter or drop_unvoiced)(system_segments, system, "system")


def _write_transcript_revision(
    transcript_dir: Path,
    *,
    health: dict,
    acoustic: dict | None,
    mic_segments: list[dict],
    system_segments: list[dict],
    gating: dict | None,
    partial_bleed_detected: bool,
) -> tuple[str, Path]:
    """Write already-filtered segments using the immutable transcript protocol."""
    from dual_capture import MergedTurn, sha256, write_transcript
    from transcript import load

    merged = [
        MergedTurn(
            segment["start"],
            segment["end"],
            label,
            segment["text"],
            segment.get("gated", False),
            segment.get("gate_score"),
            segment.get("gate_reason"),
        )
        for segments, label in ((mic_segments, "Me"), (system_segments, "Them"))
        for segment in segments
    ]
    merged.sort(key=lambda turn: (turn.start, turn.end, turn.label))

    private_directory(transcript_dir)
    temporary = transcript_dir / f".candidate-{uuid.uuid4()}.json"
    try:
        write_transcript(
            temporary,
            merged,
            acoustic,
            gating=gating,
            capture_health=health,
            attribution_untrusted=partial_bleed_detected,
            quiet=True,
        )
        encoded = temporary.read_bytes()
        digest = sha256(temporary)
        target = transcript_dir / f"{digest}.json"
        if target.exists():
            if target.is_symlink() or not target.is_file() or sha256(target) != digest:
                raise TranscriptionRefused(
                    "existing transcript revision disagrees with its digest"
                )
        else:
            durable_create_new(target, encoded)
        load(target)
        return digest, target
    except (OSError, StorageRefused, ValueError) as exc:
        if isinstance(exc, TranscriptionRefused):
            raise
        raise TranscriptionRefused(str(exc)) from None
    finally:
        try:
            temporary.unlink(missing_ok=True)
        except OSError:
            pass
        try:
            directory = os.open(transcript_dir, os.O_RDONLY)
        except OSError:
            pass
        else:
            try:
                os.fsync(directory)
            finally:
                os.close(directory)


def create_transcript_revision_from_segments(
    capture_dir: Path,
    transcript_dir: Path,
    *,
    mic_segments: list[dict],
    system_segments: list[dict],
    voicing_filter: Callable[[list[dict], np.ndarray, str], list[dict]] | None = None,
    bleed_filter: Callable[
        [list[dict], np.ndarray, np.ndarray, dict | None, str], list[dict]
    ]
    | None = None,
    gate_filter: Callable[
        [list[dict], np.ndarray, dict | None, str],
        tuple[list[dict], dict | None],
    ]
    | None = None,
) -> tuple[str, Path]:
    """Write a filtered immutable revision from externally validated segments.

    This path has no speech-model argument and never imports an ASR runtime.
    Callers must validate their engine-specific result before passing it here.
    """
    health, mic, system, acoustic = _capture_context(capture_dir)
    mic_segments, gating, partial_bleed_detected = _filter_mic_segments(
        mic_segments,
        mic=mic,
        system=system,
        acoustic=acoustic,
        voicing_filter=voicing_filter,
        bleed_filter=bleed_filter,
        gate_filter=gate_filter,
    )
    system_segments = _filter_system_segments(
        system_segments,
        system=system,
        voicing_filter=voicing_filter,
    )
    return _write_transcript_revision(
        transcript_dir,
        health=health,
        acoustic=acoustic,
        mic_segments=mic_segments,
        system_segments=system_segments,
        gating=gating,
        partial_bleed_detected=partial_bleed_detected,
    )


def create_transcript_revision(
    capture_dir: Path,
    transcript_dir: Path,
    model_dir: Path,
    *,
    transcribe_audio: Callable[[np.ndarray, str, str], list[dict]] | None = None,
    voicing_filter: Callable[[list[dict], np.ndarray, str], list[dict]] | None = None,
    bleed_filter: Callable[
        [list[dict], np.ndarray, np.ndarray, dict | None, str], list[dict]
    ]
    | None = None,
    gate_filter: Callable[
        [list[dict], np.ndarray, dict | None, str],
        tuple[list[dict], dict | None],
    ]
    | None = None,
) -> tuple[str, Path]:
    """Create one immutable transcript revision without mutating capture bytes."""
    _verify_capture(capture_dir)
    model_dir = require_whisper_model(model_dir)
    health = _capture_health(capture_dir)
    mic, system, acoustic = _read_capture_audio(capture_dir)
    from dual_capture import transcribe
    owns_mlx_runtime = transcribe_audio is None
    transcribe_audio = transcribe_audio or transcribe
    try:
        mic_segments, gating, partial_bleed_detected = _filter_mic_segments(
            transcribe_audio(mic, str(model_dir), "en"),
            mic=mic,
            system=system,
            acoustic=acoustic,
            voicing_filter=voicing_filter,
            bleed_filter=bleed_filter,
            gate_filter=gate_filter,
        )
        del mic
        system_segments = _filter_system_segments(
            transcribe_audio(system, str(model_dir), "en"),
            system=system,
            voicing_filter=voicing_filter,
        )
        del system
    finally:
        if owns_mlx_runtime:
            _release_mlx_whisper_runtime()
    return _write_transcript_revision(
        transcript_dir,
        health=health,
        acoustic=acoustic,
        mic_segments=mic_segments,
        system_segments=system_segments,
        gating=gating,
        partial_bleed_detected=partial_bleed_detected,
    )
