"""Thin app-safe adapters around the repository's canonical validators."""

from __future__ import annotations

import contextlib
import hashlib
import json
import os
import stat
import sys
import tempfile
import uuid
import wave
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Callable

from .product_contracts import (
    ProductContractRefused,
    transcript_view_digest,
    validate_note_create_arguments,
    validate_note_create_join,
    validate_transcript_retry_arguments,
    validate_transcript_retry_join,
    validate_transcript_restore_arguments,
    validate_transcript_restore_join,
    validate_transcript_view,
)
from .storage import (
    StorageRefused,
    durable_create_or_verify_identical,
    durable_create_new,
    opaque_id,
    private_directory,
    read_private_file,
    require_private_root,
    resolve_below,
)

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "spike"))
sys.path.insert(0, str(REPO / "notes"))


class AdapterRefused(ValueError):
    pass


MAX_MEETING_RECEIPT_BYTES = 256 * 1024
# A long meeting transcript is normally well below one MiB. Sixteen MiB leaves
# ample room for timestamps and gate metadata while refusing attacker-sized JSON
# before schema parsing or any canonical loader gets a path to it.
MAX_TRANSCRIPT_REVISION_BYTES = 16 * 1024 * 1024
# A profile is owner-only derived enrollment material, not capture audio. Four
# MiB accommodates a substantial private score receipt while refusing a file
# large enough to make the strict JSON loader an unbounded parsing surface.
MAX_PROFILE_BYTES = 4 * 1024 * 1024


@dataclass(frozen=True)
class _ResolvedTranscript:
    """One immutable transcript revision, resolved back to its capture source."""

    base_path: Path
    base_sha256: str
    base_document: dict
    base_transcript: object
    restored_source_turn_indices: tuple[int, ...]


NoteGenerator = Callable[[object], dict]


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def content_digest_id(value: object, label: str) -> str:
    if (
        not isinstance(value, str)
        or len(value) != 64
        or any(character not in "0123456789abcdef" for character in value)
    ):
        raise AdapterRefused(f"{label} must be a lowercase SHA-256 digest")
    return value


def _exact_arguments(arguments: object, names: set[str]) -> dict:
    if not isinstance(arguments, dict) or set(arguments) != names:
        raise AdapterRefused("operation arguments do not match the closed schema")
    return arguments


def _private_regular_file(path: Path, label: str) -> None:
    if path.is_symlink() or not path.is_file():
        raise AdapterRefused(f"{label} is missing or unsafe")
    if stat.S_IMODE(path.stat().st_mode) != 0o600:
        raise AdapterRefused(f"{label} is not private")


def _transcript_path(root: Path, meeting_id: str, transcript_sha256: str) -> Path:
    return resolve_below(
        root, "meetings", meeting_id, "transcript", f"{transcript_sha256}.json"
    )


def _current_transcript_sha256(root: Path, meeting_id: str) -> str:
    """Read only the immutable-current binding the worker needs to enforce."""
    meeting_path = resolve_below(root, "meetings", meeting_id, "meeting.json")
    try:
        meeting = json.loads(read_private_file(
            meeting_path,
            max_bytes=MAX_MEETING_RECEIPT_BYTES,
            label="meeting receipt",
        ))
    except (StorageRefused, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise AdapterRefused("meeting receipt is not valid UTF-8 JSON") from exc
    if (
        not isinstance(meeting, dict)
        or meeting.get("schema") != "meeting/2"
        or meeting.get("meeting_id") != meeting_id
        or not isinstance(meeting.get("artifacts"), dict)
    ):
        raise AdapterRefused("meeting receipt does not bind this meeting")
    current = meeting["artifacts"].get("current_transcript")
    if not isinstance(current, dict) or set(current) != {"relative_path", "sha256"}:
        raise AdapterRefused("meeting receipt has no exact current transcript")
    digest = content_digest_id(current["sha256"], "current transcript digest")
    if current["relative_path"] != f"transcript/{digest}.json":
        raise AdapterRefused("meeting receipt current transcript path is not canonical")
    return digest


def _load_bounded_base_transcript(data: bytes):
    """Give the canonical path-based loader only a private bounded byte copy."""
    from transcript import load

    with tempfile.TemporaryDirectory() as temporary_name:
        temporary = Path(temporary_name)
        private_directory(temporary)
        path = temporary / "transcript.json"
        durable_create_new(path, data)
        return load(path)


def _base_gated_turn_indices(document: dict) -> set[int]:
    turns = document.get("turns")
    if not isinstance(turns, list):
        raise AdapterRefused("base transcript has no turn list")
    gated: set[int] = set()
    for index, turn in enumerate(turns):
        if not isinstance(turn, dict):
            raise AdapterRefused("base transcript has a malformed turn")
        if "gated" in turn and type(turn["gated"]) is not bool:
            raise AdapterRefused("base transcript turn gate is not boolean")
        if turn.get("gated") is True:
            gated.add(index)
    return gated


MAX_TRANSCRIPT_VIEW_DEPTH = 20_000
MAX_TRANSCRIPT_VIEW_CHAIN_BYTES = 4 * 1024 * 1024


def _resolve_transcript_revision(
    root: Path,
    meeting_id: str,
    transcript_sha256: str,
) -> _ResolvedTranscript:
    """Resolve a base transcript or a closed transcript-view/1 chain.

    Iterative on purpose: recursion would hit CPython's recursion limit far
    below the contract's depth cap and kill the worker with an exception the
    protocol loop does not catch. The walk also bounds cumulative view bytes,
    matching the Rust walker, so a hostile chain cannot amplify one meeting
    into unbounded reads.
    """
    current = content_digest_id(transcript_sha256, "transcript digest")
    seen: set[str] = set()
    views: list[dict] = []
    view_bytes_total = 0
    while True:
        if current in seen:
            raise AdapterRefused("transcript view chain is cyclic")
        seen.add(current)
        if len(views) >= MAX_TRANSCRIPT_VIEW_DEPTH:
            raise AdapterRefused("transcript view chain is too deep")
        path = _transcript_path(root, meeting_id, current)
        try:
            revision_bytes = read_private_file(
                path,
                max_bytes=MAX_TRANSCRIPT_REVISION_BYTES,
                label="transcript revision",
            )
        except StorageRefused as exc:
            raise AdapterRefused(str(exc)) from None
        if hashlib.sha256(revision_bytes).hexdigest() != current:
            raise AdapterRefused("transcript revision changed from its content address")
        try:
            document = json.loads(revision_bytes)
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise AdapterRefused("transcript revision is not valid UTF-8 JSON") from exc

        if isinstance(document, dict) and document.get("schema") == "transcript-view/1":
            try:
                view = validate_transcript_view(document)
            except ProductContractRefused as exc:
                raise AdapterRefused(str(exc)) from None
            if view["meeting_id"] != meeting_id:
                raise AdapterRefused("transcript view belongs to another meeting")
            if transcript_view_digest(view) != current:
                raise AdapterRefused("transcript view bytes disagree with its content address")
            view_bytes_total += len(revision_bytes)
            if view_bytes_total > MAX_TRANSCRIPT_VIEW_CHAIN_BYTES:
                raise AdapterRefused("transcript view chain is too deep")
            views.append(view)
            current = content_digest_id(
                view["parent_transcript_sha256"], "transcript digest"
            )
            continue

        # `load` remains the canonical capture-transcript parser. It is
        # deliberately invoked only for base bytes; a view stores no meeting
        # words of its own.
        try:
            base_transcript = _load_bounded_base_transcript(revision_bytes)
        except (KeyError, ValueError, TypeError, OSError) as exc:
            raise AdapterRefused(f"base transcript is invalid: {exc}") from None
        withheld = _base_gated_turn_indices(document)
        base = _ResolvedTranscript(
            base_path=path,
            base_sha256=current,
            base_document=document,
            base_transcript=base_transcript,
            restored_source_turn_indices=(),
        )
        break

    restored_current: tuple[int, ...] = ()
    parent_restored: set[int] = set()
    for view in reversed(views):
        if view["base_transcript_sha256"] != base.base_sha256:
            raise AdapterRefused("transcript view base differs from its parent chain")
        restored = tuple(view["restored_source_turn_indices"])
        if (
            not parent_restored < set(restored)
            or len(set(restored) - parent_restored) != 1
            or not set(restored).issuperset(parent_restored)
        ):
            raise AdapterRefused("transcript view is not a one-turn successor")
        if not set(restored).issubset(withheld):
            raise AdapterRefused("transcript view restores a source turn that was not withheld")
        parent_restored = set(restored)
        restored_current = restored

    if not views:
        return base
    return _ResolvedTranscript(
        base_path=base.base_path,
        base_sha256=base.base_sha256,
        base_document=base.base_document,
        base_transcript=base.base_transcript,
        restored_source_turn_indices=restored_current,
    )


def resolve_transcript(root: Path, meeting_id: str, transcript_sha256: str):
    """Materialize a view for validators without copying text into the view file."""
    root = require_private_root(root)
    meeting_id = opaque_id(meeting_id, "meeting_id")
    resolved = _resolve_transcript_revision(root, meeting_id, transcript_sha256)
    from transcript import Transcript, Turn

    base = resolved.base_transcript
    restored = set(resolved.restored_source_turn_indices)
    visible: list[Turn] = []
    still_withheld: list[Turn] = []
    for index, turn in enumerate(resolved.base_document["turns"]):
        rendered = Turn(
            text=turn["text"], speaker=turn.get("speaker"), start=turn.get("start")
        )
        if turn.get("gated") is True and index not in restored:
            still_withheld.append(rendered)
        else:
            visible.append(rendered)
    return Transcript(
        source=base.source,
        attribution=base.attribution,
        turns=visible,
        gated_turns=still_withheld,
        gate=base.gate,
        capture_health=base.capture_health,
        capture_integrity_unknown=base.capture_integrity_unknown,
    )


def transcript_restore(root: Path, arguments: object) -> dict[str, str]:
    """Create one immutable transcript-view/1 successor for a withheld turn."""
    root = require_private_root(root)
    try:
        values = validate_transcript_restore_arguments(arguments)
    except ProductContractRefused as exc:
        raise AdapterRefused(str(exc)) from None
    meeting_id = values["meeting_id"]
    source_sha256 = values["source_transcript_sha256"]
    if _current_transcript_sha256(root, meeting_id) != source_sha256:
        raise AdapterRefused("requested transcript is not this meeting's current revision")
    parent = _resolve_transcript_revision(root, meeting_id, source_sha256)
    turn_index = values["source_turn_index"]
    if turn_index not in _base_gated_turn_indices(parent.base_document):
        raise AdapterRefused("requested source turn was not withheld")
    if turn_index in parent.restored_source_turn_indices:
        raise AdapterRefused("requested source turn is already restored")
    view = {
        "schema": "transcript-view/1",
        "meeting_id": meeting_id,
        "base_transcript_sha256": parent.base_sha256,
        "parent_transcript_sha256": source_sha256,
        "restored_source_turn_indices": sorted(
            (*parent.restored_source_turn_indices, turn_index)
        ),
    }
    view_sha256 = transcript_view_digest(view)
    target = _transcript_path(root, meeting_id, view_sha256)
    try:
        durable_create_or_verify_identical(
            target, json.dumps(view, ensure_ascii=False, indent=2).encode("utf-8")
        )
    except StorageRefused as exc:
        raise AdapterRefused(str(exc)) from None
    digests = {
        "base-transcript": parent.base_sha256,
        "parent-transcript": source_sha256,
        "transcript": view_sha256,
    }
    try:
        validate_transcript_restore_join(values, view, digests)
    except ProductContractRefused as exc:
        raise AdapterRefused(str(exc)) from None
    return digests


def _meeting_capture(root: Path, meeting_id: str) -> Path:
    meeting_id = opaque_id(meeting_id, "meeting_id")
    return resolve_below(root, "meetings", meeting_id, "capture")


def capture_inspect(root: Path, arguments: object) -> dict[str, str]:
    values = _exact_arguments(arguments, {"meeting_id"})
    capture_dir = _meeting_capture(root, values["meeting_id"])
    from verify_capture import verify_acquisition

    verify_acquisition(capture_dir)
    return {
        "capture-session": sha256(capture_dir / "session.json"),
        "capture-mic": sha256(capture_dir / "mic.wav"),
        "capture-system": sha256(capture_dir / "system.wav"),
    }


def _wav_samples(path: Path) -> int:
    if path.is_symlink() or not path.is_file():
        raise AdapterRefused(f"{path.name} is missing or unsafe")
    if stat.S_IMODE(path.stat().st_mode) != 0o600:
        raise AdapterRefused(f"{path.name} is not private")
    try:
        with wave.open(str(path), "rb") as audio:
            if (
                audio.getnchannels() != 1
                or audio.getsampwidth() != 2
                or audio.getframerate() != 16_000
                or audio.getcomptype() != "NONE"
            ):
                raise AdapterRefused(
                    f"{path.name} is not mono 16 kHz 16-bit PCM"
                )
            frames = audio.getnframes()
            encoded = audio.readframes(frames)
    except (EOFError, OSError, wave.Error) as exc:
        raise AdapterRefused(f"{path.name} is not a readable WAV ({exc})") from None
    if len(encoded) != frames * 2:
        raise AdapterRefused(f"{path.name} is truncated")
    return frames


def capture_finalize(root: Path, arguments: object) -> dict[str, str]:
    values = _exact_arguments(
        arguments,
        {
            "meeting_id",
            "started_at_epoch_seconds",
            "capture_elapsed_samples",
            "pauses",
        },
    )
    meeting_id = opaque_id(values["meeting_id"], "meeting_id")
    started_at = values["started_at_epoch_seconds"]
    # Net of any paused time: the application stops sending audio while paused,
    # so wall-clock elapsed would read as both legs having ended early and would
    # fail the capture-health integrity floor.
    elapsed = values["capture_elapsed_samples"]
    pauses = values["pauses"]
    if (
        isinstance(started_at, bool)
        or not isinstance(started_at, int)
        or started_at <= 0
        or isinstance(elapsed, bool)
        or not isinstance(elapsed, int)
        or elapsed <= 0
        or elapsed > 16_000 * 60 * 60 * 24
    ):
        raise AdapterRefused("capture timing is outside the closed schema")

    from dual_capture import validate_pause_evidence

    if pauses is not None:
        try:
            validate_pause_evidence(pauses, capture_elapsed_samples=elapsed)
        except ValueError as exc:
            raise AdapterRefused(f"capture pause evidence is invalid ({exc})") from None

    capture_dir = _meeting_capture(root, meeting_id)
    names = {path.name for path in capture_dir.iterdir()}
    if names != {"mic.wav", "system.wav"}:
        raise AdapterRefused("capture finalization requires exactly two WAV legs")
    mic_samples = _wav_samples(capture_dir / "mic.wav")
    system_samples = _wav_samples(capture_dir / "system.wav")

    from capture_health import build as build_capture_health
    from dual_capture import finalize_session
    from verify_capture import verify_acquisition

    health = build_capture_health(
        mic_samples=mic_samples,
        system_samples=system_samples,
        capture_elapsed_samples=elapsed,
        dropouts={"mic": [], "system": []},
        tap_errors=[],
        transcription_requested=False,
        transcript_written=False,
    )
    if not health["usable"]:
        raise AdapterRefused("capture did not pass its integrity floor")
    timestamp = datetime.fromtimestamp(started_at, tz=timezone.utc).strftime(
        "%Y-%m-%dT%H:%M:%S%z"
    )
    finalize_session(
        capture_dir,
        timestamp,
        health,
        pauses=pauses,
        no_overwrite=True,
    )
    verify_acquisition(capture_dir)
    return {
        "capture-session": sha256(capture_dir / "session.json"),
        "capture-mic": sha256(capture_dir / "mic.wav"),
        "capture-system": sha256(capture_dir / "system.wav"),
    }


def transcript_create(
    root: Path,
    arguments: object,
    *,
    admission: str,
    model_dir: Path | None,
    encoder_digest: str,
    encoder_path: Path | None = None,
    transcription_engine: str = "whisper",
    speech_locale: str = "en-US",
    apple_speech_helper: Path | None = None,
    apple_speech_helper_sha256: str | None = None,
    apple_speech_os_version: str | None = None,
) -> dict[str, str]:
    values = _exact_arguments(arguments, {"meeting_id"})
    meeting_id = opaque_id(values["meeting_id"], "meeting_id")
    capture_dir = _meeting_capture(root, meeting_id)
    target_dir = resolve_below(root, "meetings", meeting_id, "transcript")
    private_directory(target_dir)
    if admission == "boundary-test":
        source = capture_dir / "transcript.json"
        from verify_capture import verify_capture

        verify_capture(capture_dir)

        from transcript import load

        load(source)
        transcript_digest = sha256(source)
        target = resolve_below(
            root, "meetings", meeting_id, "transcript", f"{transcript_digest}.json"
        )
        if target.exists():
            if not target.is_file() or sha256(target) != transcript_digest:
                raise AdapterRefused(
                    "existing transcript revision disagrees with its name"
                )
        else:
            durable_create_new(target, source.read_bytes())
        return {"transcript": transcript_digest}

    if transcription_engine == "whisper":
        if admission not in {"internal-alpha", "product"} or model_dir is None:
            raise AdapterRefused("runtime admission lacks the fixed transcript model")
        from .transcription import create_transcript_revision
        transcript_digest, _ = create_transcript_revision(
            capture_dir, target_dir, model_dir,
            gate_filter=_installed_voiceprint_gate(root, encoder_digest, encoder_path),
        )
    elif transcription_engine == "apple-native":
        transcript_digest = _create_apple_native_transcript(
            root, capture_dir, target_dir, encoder_digest, encoder_path,
            speech_locale, apple_speech_helper, apple_speech_helper_sha256,
            apple_speech_os_version,
        )
    else:
        raise AdapterRefused("transcription engine is unsupported")
    return {"transcript": transcript_digest}


def _create_apple_native_transcript(
    root: Path, capture_dir: Path, target_dir: Path, encoder_digest: str,
    encoder_path: Path | None, locale: str, helper: Path | None, helper_sha256: str | None, os_version: str | None,
) -> str:
    """Run both retained legs through the explicit signed Apple helper."""
    from .apple_speech import AppleSpeechRefused, make_producer, transcribe_leg, write_provenance
    from .transcription import create_transcript_revision_from_segments
    try:
        producer = make_producer(helper=helper, helper_sha256=helper_sha256, locale=locale, os_version=os_version or "")
        # This capture verifier precedes any native process, mirroring Whisper.
        from verify_capture import verify_acquisition
        verify_acquisition(capture_dir)
        mic, mic_receipt = transcribe_leg(producer, capture_dir / "mic.wav", temporary_parent=target_dir)
        system, system_receipt = transcribe_leg(producer, capture_dir / "system.wav", temporary_parent=target_dir)
        digest, _ = create_transcript_revision_from_segments(
            capture_dir, target_dir, mic_segments=mic, system_segments=system,
            gate_filter=_installed_voiceprint_gate(root, encoder_digest, encoder_path),
        )
        write_provenance(target_dir, digest, producer, mic=mic_receipt, system=system_receipt)
        return digest
    except (AppleSpeechRefused, OSError, ValueError) as exc:
        raise AdapterRefused(str(exc)) from None


def _retry_artifact_reference(
    artifacts: dict,
    name: str,
    relative_path: str,
) -> str:
    reference = artifacts.get(name)
    if not isinstance(reference, dict) or set(reference) != {"relative_path", "sha256"}:
        raise AdapterRefused(f"meeting receipt has no exact {name} reference")
    digest = content_digest_id(reference["sha256"], f"{name} digest")
    if reference["relative_path"] != relative_path:
        raise AdapterRefused(f"meeting receipt {name} path is not canonical")
    return digest


def _retry_source_bindings(root: Path, values: dict) -> dict[str, tuple[Path, str]]:
    """Resolve the exact retained sources a retry is allowed to read.

    This is intentionally separate from transcript.create. A create operation
    may work from a newly completed capture; a retry may only re-run immutable
    transcription against a still-retained meeting receipt and its current
    transcript pointer.
    """
    meeting_id = values["meeting_id"]
    meeting_path = resolve_below(root, "meetings", meeting_id, "meeting.json")
    try:
        meeting = json.loads(
            read_private_file(
                meeting_path,
                max_bytes=MAX_MEETING_RECEIPT_BYTES,
                label="meeting receipt",
            )
        )
    except (StorageRefused, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise AdapterRefused("meeting receipt is not valid UTF-8 JSON") from exc
    if (
        not isinstance(meeting, dict)
        or set(meeting)
        != {
            "schema",
            "meeting_id",
            "lifecycle",
            "retention",
            "artifacts",
            "pending_storage_operation",
        }
        or meeting.get("schema") != "meeting/2"
        or meeting.get("meeting_id") != meeting_id
        or not isinstance(meeting.get("retention"), dict)
        or set(meeting["retention"])
        != {
            "rule",
            "policy_sha256",
            "next_deletion_at_epoch_seconds",
            "state",
            "deletion_receipt",
        }
        or meeting["retention"].get("state") != "retained"
        or meeting.get("pending_storage_operation") is not None
        or not isinstance(meeting.get("artifacts"), dict)
        or set(meeting["artifacts"])
        != {
            "attempt",
            "ownership",
            "capture_session",
            "microphone_audio",
            "system_audio",
            "current_transcript",
            "current_note",
        }
    ):
        raise AdapterRefused("meeting receipt does not retain an exact retry source set")

    artifacts = meeting["artifacts"]
    bindings = {
        "source-transcript": (
            _transcript_path(root, meeting_id, values["source_transcript_sha256"]),
            _retry_artifact_reference(
                artifacts,
                "current_transcript",
                f"transcript/{values['source_transcript_sha256']}.json",
            ),
        ),
        "capture-session": (
            resolve_below(root, "meetings", meeting_id, "capture", "session.json"),
            _retry_artifact_reference(
                artifacts, "capture_session", "capture/session.json"
            ),
        ),
        "capture-mic": (
            resolve_below(root, "meetings", meeting_id, "capture", "mic.wav"),
            _retry_artifact_reference(
                artifacts, "microphone_audio", "capture/mic.wav"
            ),
        ),
        "capture-system": (
            resolve_below(root, "meetings", meeting_id, "capture", "system.wav"),
            _retry_artifact_reference(
                artifacts, "system_audio", "capture/system.wav"
            ),
        ),
    }
    requested = {
        "source-transcript": values["source_transcript_sha256"],
        "capture-session": values["capture_session_sha256"],
        "capture-mic": values["microphone_audio_sha256"],
        "capture-system": values["system_audio_sha256"],
    }
    for name, (_, digest) in bindings.items():
        if digest != requested[name]:
            raise AdapterRefused(f"meeting receipt {name} differs from retry request")
    return bindings


def _stable_private_digest(path: Path, expected: str, label: str) -> str:
    """Hash a retained source without accepting replacement during the read."""
    _private_regular_file(path, label)
    try:
        before = path.stat()
        digest = sha256(path)
        after = path.stat()
    except OSError as exc:
        raise AdapterRefused(f"{label} is missing or unsafe ({exc})") from None
    if (
        before.st_dev != after.st_dev
        or before.st_ino != after.st_ino
        or before.st_mode != after.st_mode
        or before.st_uid != after.st_uid
        or before.st_size != after.st_size
        or before.st_mtime_ns != after.st_mtime_ns
    ):
        raise AdapterRefused(f"{label} changed while being read")
    if digest != expected:
        raise AdapterRefused(f"{label} changed from its retained digest")
    return digest


def _read_retry_sources(bindings: dict[str, tuple[Path, str]]) -> dict[str, str]:
    snapshots: dict[str, str] = {}
    for name, (path, expected) in bindings.items():
        _private_regular_file(path, name)
        try:
            size = path.stat().st_size
        except OSError as exc:
            raise AdapterRefused(f"{name} is missing or unsafe ({exc})") from None
        if name == "source-transcript" and size > MAX_TRANSCRIPT_REVISION_BYTES:
            raise AdapterRefused("source-transcript exceeds its bounded read limit")
        if name == "capture-session" and size > MAX_MEETING_RECEIPT_BYTES:
            raise AdapterRefused("capture-session exceeds its bounded read limit")
        snapshots[name] = _stable_private_digest(path, expected, name)
    return snapshots


def transcript_retry(
    root: Path,
    arguments: object,
    *,
    admission: str,
    model_dir: Path | None,
    encoder_digest: str,
    encoder_path: Path | None = None,
    transcription_engine: str = "whisper",
    speech_locale: str = "en-US",
    apple_speech_helper: Path | None = None,
    apple_speech_helper_sha256: str | None = None,
    apple_speech_os_version: str | None = None,
) -> dict[str, str]:
    """Create an uncommitted immutable transcript candidate from retained audio."""
    root = require_private_root(root)
    try:
        values = validate_transcript_retry_arguments(arguments)
    except ProductContractRefused as exc:
        raise AdapterRefused(str(exc)) from None
    if transcription_engine == "whisper" and (admission not in {"internal-alpha", "product"} or model_dir is None):
        raise AdapterRefused("runtime admission lacks the fixed transcript model")
    if transcription_engine not in {"whisper", "apple-native"}:
        raise AdapterRefused("transcription engine is unsupported")

    bindings = _retry_source_bindings(root, values)
    before = _read_retry_sources(bindings)
    capture_dir = _meeting_capture(root, values["meeting_id"])
    target_dir = resolve_below(root, "meetings", values["meeting_id"], "transcript")
    private_directory(target_dir)
    from verify_capture import verify_acquisition
    try:
        verify_acquisition(capture_dir)
    except (OSError, ValueError, SystemExit) as exc:
        raise AdapterRefused(f"retained capture is invalid ({exc})") from None
    if transcription_engine == "whisper":
        from .apple_speech import provenance_path
        if provenance_path(target_dir, values["source_transcript_sha256"]).exists():
            raise AdapterRefused("This transcript used Apple speech. Select Apple speech in Settings before retrying.")
        from .transcription import create_transcript_revision
        candidate_digest, candidate_path = create_transcript_revision(
            capture_dir, target_dir, model_dir,
            gate_filter=_installed_voiceprint_gate(root, encoder_digest, encoder_path),
        )
    else:
        from .apple_speech import AppleSpeechRefused, make_producer, require_matching_provenance
        try:
            producer = make_producer(helper=apple_speech_helper, helper_sha256=apple_speech_helper_sha256, locale=speech_locale, os_version=apple_speech_os_version or "")
            require_matching_provenance(target_dir, values["source_transcript_sha256"], producer)
            candidate_digest = _create_apple_native_transcript(
                root, capture_dir, target_dir, encoder_digest, encoder_path,
                speech_locale, apple_speech_helper, apple_speech_helper_sha256,
                apple_speech_os_version,
            )
            candidate_path = _transcript_path(root, values["meeting_id"], candidate_digest)
        except AppleSpeechRefused as exc:
            raise AdapterRefused(str(exc)) from None
    candidate_digest = content_digest_id(
        candidate_digest, "candidate transcript digest"
    )
    expected_candidate_path = _transcript_path(root, values["meeting_id"], candidate_digest)
    if candidate_path != expected_candidate_path:
        raise AdapterRefused("candidate transcript path is not canonical")
    after = _read_retry_sources(bindings)
    if after != before:
        raise AdapterRefused("retry changed or raced a retained source")
    try:
        candidate = read_private_file(
            expected_candidate_path,
            max_bytes=MAX_TRANSCRIPT_REVISION_BYTES,
            label="candidate transcript",
        )
    except StorageRefused as exc:
        raise AdapterRefused(str(exc)) from None
    if hashlib.sha256(candidate).hexdigest() != candidate_digest:
        raise AdapterRefused("candidate transcript changed from its content address")
    digests = {
        "candidate-transcript": candidate_digest,
        "source-transcript": after["source-transcript"],
        "capture-session": after["capture-session"],
        "capture-mic": after["capture-mic"],
        "capture-system": after["capture-system"],
    }
    try:
        validate_transcript_retry_join(values, digests)
    except ProductContractRefused as exc:
        raise AdapterRefused(str(exc)) from None
    return digests


def _canonical_sitting_id(value: object) -> str:
    if not isinstance(value, str):
        raise AdapterRefused("sitting_id must be a canonical lowercase UUID")
    try:
        parsed = uuid.UUID(value)
    except ValueError:
        raise AdapterRefused("sitting_id must be a canonical lowercase UUID") from None
    if str(parsed) != value:
        raise AdapterRefused("sitting_id must be a canonical lowercase UUID")
    return value


def _onnx_sitting_embedder(encoder_path: Path):
    """The deployable embedding chain, or the refusal that no encoder is admitted.

    The default runtime records a text placeholder as its encoder and does not
    package onnxruntime, so both the import and the session load refuse on that
    lane; only an encoder-candidate build reaches inference. Nothing here
    admits an encoder — the manifest already digest-verified the artifact, and
    admission remains the operator's recorded decision.
    """
    try:
        import numpy as np
        import onnxruntime as ort

        from .fbank import fbank_features
    except ImportError:
        raise AdapterRefused("runtime has no admitted speaker encoder") from None
    try:
        session = ort.InferenceSession(
            str(encoder_path), providers=["CPUExecutionProvider"]
        )
    except Exception:
        raise AdapterRefused("runtime has no admitted speaker encoder") from None
    lengths = np.ones(1, dtype=np.float32)

    def embed(clip):
        features = fbank_features(clip)
        return np.squeeze(
            session.run(None, {"features": features[None, ...], "lengths": lengths})[0]
        )

    return embed


def _installed_voiceprint_gate(
    root: Path, encoder_digest: str, encoder_path: Path | None, *, embedder=None
):
    """The operator's voice gate for this transcript, or nothing to apply.

    Three states, and the third is the reason this returns rather than assumes.

    *No profile installed* — no gate, unchanged behaviour. The operator has not
    asked for isolation, so a transcript holding every audible voice is honest
    and its `voiceprint` field is null, which is exactly what null means.

    *Profile installed and this runtime can run it* — the gate is built and the
    artifact records what it did.

    *Profile installed and this runtime cannot run it* — refuse the transcript.
    Producing one would write `voiceprint: null` beside words the operator
    believes were checked, and null already means "no profile was supplied".
    That is a different claim from "a profile was supplied and could not be
    applied", and the artifact has no way to say the second one. Only the
    encoder-carrying lane packages onnxruntime and the admitted ONNX model, so
    on a placeholder build installing a profile stops transcription until the
    operator resets the profile. That cost is deliberate: an unchecked
    transcript that reads as checked is the failure this product exists to
    avoid.

    **Absent is a size, not a missing path.** Rust owns this file's lifecycle
    and never unlinks it: `initialize_or_open` creates it at zero bytes on every
    macOS startup before any capture, and a reset swaps the live profile for a
    zero-length file rather than removing it. `profile_present` in
    `profile_lifecycle.rs` is `profile_size != 0`, and this must read the same
    sentinel. Testing existence instead refuses transcription for every operator
    who has never enrolled — the whole product, on a fresh install, on both
    lanes. That is what the first version of this function did.

    `embedder` replaces only the ONNX session, and only for tests. Every other
    check — the encoder file, its digest against the manifest, and the profile's
    fingerprint against that same digest — stays live, so the substituted
    embedder does not weaken what this function refuses. It exists because the
    real one needs onnxruntime and a 20 MB model that no development machine
    packages, and `drop_offprint` already carries the argument for exactly this:
    a test that needs the real model is a test that stops being run. Without it
    the closure below has no coverage at all, and it would first execute on a
    cohort machine after a real meeting.
    """
    try:
        installed = resolve_below(root, "profile", "voiceprint.json")
    except (OSError, StorageRefused):
        return None
    if installed.is_symlink() or not installed.is_file():
        return None
    try:
        if installed.stat().st_size == 0:
            return None
    except OSError:
        return None
    if encoder_path is None or encoder_path.is_symlink() or not encoder_path.is_file():
        raise AdapterRefused("runtime has no admitted speaker encoder")
    if sha256(encoder_path) != encoder_digest:
        raise AdapterRefused("packaged encoder disagrees with its manifest")
    embed = embedder if embedder is not None else _onnx_sitting_embedder(encoder_path)

    from speaker_gate import load_profile

    try:
        # The fingerprint argument is the packaged encoder's own verified digest,
        # so this binds the profile to the exact bytes that will score it. The
        # spike's separate dimension probe exists because there the fingerprint
        # comes from a model directory that may not be the one loaded; here they
        # are the same artifact, checked above.
        profile, threshold, document = load_profile(
            installed, expected_encoder_fingerprint=encoder_digest
        )
    except (
        AttributeError,
        KeyError,
        OSError,
        OverflowError,
        SystemExit,
        TypeError,
        ValueError,
    ):
        raise AdapterRefused("installed voice profile cannot be loaded") from None

    def gate(segments, mic, acoustic, label):
        from dual_capture import Voiceprint, drop_offprint, voiceprint_provenance

        voiceprint = Voiceprint(profile, threshold, document, embed)
        marked, outcome = drop_offprint(segments, mic, voiceprint, acoustic, label)
        return marked, voiceprint_provenance(voiceprint, outcome)

    return gate


def sitting_derive(
    root: Path,
    arguments: object,
    *,
    admission: str,
    model_dir: Path | None,
    encoder_digest: str,
    encoder_path: Path | None,
) -> dict[str, str]:
    """Derive segment evidence and embeddings for one recorded sitting.

    Deliberately absent from the packaged internal-alpha operation set; it is
    announced only on the boundary lane until the operator widens the frozen
    admission. Outputs land in the sitting's work directory for the Rust
    authority to re-verify — see worker/sitting_derivation.py for the artifact
    contract and the content boundary.
    """
    root = require_private_root(root)
    values = _exact_arguments(arguments, {"sitting_id"})
    sitting_id = _canonical_sitting_id(values["sitting_id"])
    # resolve_below resolves strictly, so an absent enrollment tree surfaces as
    # OSError; both absences mean the same thing to the caller.
    try:
        identity_row = resolve_below(
            root, "enrollment", "sittings", sitting_id, "sitting.json"
        )
        if identity_row.is_symlink() or not identity_row.is_file():
            raise AdapterRefused("sitting identity row is missing")
    except OSError:
        raise AdapterRefused("sitting identity row is missing") from None
    try:
        work_dir = resolve_below(root, "enrollment", "work", sitting_id)
        if work_dir.is_symlink() or not work_dir.is_dir():
            raise AdapterRefused("sitting work directory is missing")
    except OSError:
        raise AdapterRefused("sitting work directory is missing") from None
    if model_dir is None:
        raise AdapterRefused("runtime admission lacks the fixed transcript model")
    if encoder_path is None:
        raise AdapterRefused("runtime has no admitted speaker encoder")
    if encoder_path.is_symlink() or not encoder_path.is_file():
        raise AdapterRefused("runtime has no admitted speaker encoder")
    if sha256(encoder_path) != encoder_digest:
        raise AdapterRefused("packaged encoder disagrees with its manifest")
    embed_segment = _onnx_sitting_embedder(encoder_path)

    from .transcription import require_whisper_model

    model = require_whisper_model(model_dir)
    from dual_capture import drop_unvoiced, transcribe

    def transcribe_audio(samples):
        return drop_unvoiced(transcribe(samples, str(model), "en"), samples, "mic")

    from .sitting_derivation import derive_sitting_material

    return derive_sitting_material(
        work_dir,
        sitting_id,
        encoder_sha256=encoder_digest,
        onnx_artifact_sha256=encoder_digest,
        embed_segment=embed_segment,
        transcribe_audio=transcribe_audio,
    )


@dataclass(frozen=True)
class _ProfileQuarantine:
    root_fd: int
    candidates_fd: int
    candidate_fd: int


@dataclass
class _InstalledProfileDirectory:
    root_fd: int
    profile_fd: int | None


def _owned_by_effective_user(metadata: os.stat_result) -> bool:
    effective_user = getattr(os, "geteuid", None)
    owner = getattr(metadata, "st_uid", None)
    return effective_user is None or owner is None or owner == effective_user()


def _require_private_directory_fd(descriptor: int, label: str) -> os.stat_result:
    metadata = os.fstat(descriptor)
    if (
        not stat.S_ISDIR(metadata.st_mode)
        or stat.S_IMODE(metadata.st_mode) != 0o700
        or not _owned_by_effective_user(metadata)
    ):
        raise AdapterRefused(f"{label} is not an owner-private directory")
    return metadata


def _require_private_file_fd(descriptor: int, label: str) -> os.stat_result:
    metadata = os.fstat(descriptor)
    if (
        not stat.S_ISREG(metadata.st_mode)
        or stat.S_IMODE(metadata.st_mode) != 0o600
        or not _owned_by_effective_user(metadata)
    ):
        raise AdapterRefused(f"{label} is not an owner-private regular file")
    return metadata


@contextlib.contextmanager
def _open_profile_quarantine(root: Path, profile_id: str):
    """Open an existing private quarantine chain without following symlinks."""
    no_follow = getattr(os, "O_NOFOLLOW", None)
    directory = getattr(os, "O_DIRECTORY", None)
    if no_follow is None or directory is None:
        raise AdapterRefused("profile quarantine requires no-follow directory support")
    descriptors: list[int] = []
    try:
        root_fd = os.open(root, os.O_RDONLY | directory | no_follow)
        descriptors.append(root_fd)
        _require_private_directory_fd(root_fd, "app data root")
        candidates_fd = os.open(
            "profile-candidates",
            os.O_RDONLY | directory | no_follow,
            dir_fd=root_fd,
        )
        descriptors.append(candidates_fd)
        _require_private_directory_fd(candidates_fd, "profile quarantine root")
        candidate_fd = os.open(
            profile_id,
            os.O_RDONLY | directory | no_follow,
            dir_fd=candidates_fd,
        )
        descriptors.append(candidate_fd)
        _require_private_directory_fd(candidate_fd, "profile quarantine")
        yield _ProfileQuarantine(root_fd, candidates_fd, candidate_fd)
    except AdapterRefused:
        raise
    except OSError as exc:
        raise AdapterRefused("profile quarantine is missing or unsafe") from exc
    finally:
        for descriptor in reversed(descriptors):
            os.close(descriptor)


@contextlib.contextmanager
def _open_profile_candidate(quarantine: _ProfileQuarantine):
    """Open a valid leaf, or surface a symlink that adopt may safely unlink."""
    no_follow = getattr(os, "O_NOFOLLOW", None)
    if no_follow is None:
        raise AdapterRefused("profile candidate requires no-follow support")
    try:
        leaf = os.stat(
            "voiceprint.json",
            dir_fd=quarantine.candidate_fd,
            follow_symlinks=False,
        )
    except OSError as exc:
        raise AdapterRefused("profile candidate is missing or unsafe") from exc
    if stat.S_ISLNK(leaf.st_mode):
        # The parent chain is already pinned and private. Yielding a sentinel lets
        # adopt remove this directory entry without opening or following its target.
        yield None
        return
    if (
        not stat.S_ISREG(leaf.st_mode)
        or stat.S_IMODE(leaf.st_mode) != 0o600
        or not _owned_by_effective_user(leaf)
    ):
        raise AdapterRefused("profile candidate is not an owner-private regular file")
    try:
        descriptor = os.open(
            "voiceprint.json",
            os.O_RDONLY | no_follow,
            dir_fd=quarantine.candidate_fd,
        )
    except OSError as exc:
        raise AdapterRefused("profile candidate changed before it was opened") from exc
    try:
        opened = _require_private_file_fd(descriptor, "profile candidate")
        if opened.st_dev != leaf.st_dev or opened.st_ino != leaf.st_ino:
            raise AdapterRefused("profile candidate changed before it was opened")
        yield descriptor
    finally:
        os.close(descriptor)


def _read_profile_candidate(descriptor: int | None) -> bytes:
    if descriptor is None:
        raise AdapterRefused("profile candidate may not be a symlink")
    before = _require_private_file_fd(descriptor, "profile candidate")
    if before.st_size > MAX_PROFILE_BYTES:
        raise AdapterRefused("profile candidate exceeds its bounded read limit")
    chunks: list[bytes] = []
    remaining = before.st_size
    while remaining:
        chunk = os.read(descriptor, min(remaining, 1024 * 1024))
        if not chunk:
            raise AdapterRefused("profile candidate changed while being read")
        chunks.append(chunk)
        remaining -= len(chunk)
    after = os.fstat(descriptor)
    if (
        before.st_dev != after.st_dev
        or before.st_ino != after.st_ino
        or before.st_mode != after.st_mode
        or before.st_uid != after.st_uid
        or before.st_size != after.st_size
        or not _owned_by_effective_user(after)
    ):
        raise AdapterRefused("profile candidate changed while being read")
    return b"".join(chunks)


def _require_directory_link(
    parent_fd: int,
    name: str,
    expected: os.stat_result,
    label: str,
) -> None:
    try:
        current = os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
    except OSError as exc:
        raise AdapterRefused(f"{label} changed before cleanup") from exc
    if (
        not stat.S_ISDIR(current.st_mode)
        or current.st_dev != expected.st_dev
        or current.st_ino != expected.st_ino
        or stat.S_IMODE(current.st_mode) != 0o700
        or not _owned_by_effective_user(current)
    ):
        raise AdapterRefused(f"{label} changed before cleanup")


def _remove_profile_quarantine(
    quarantine: _ProfileQuarantine, profile_id: str
) -> None:
    """Remove only the exact private directory opened during preflight."""
    _require_private_directory_fd(quarantine.root_fd, "app data root")
    candidates_metadata = _require_private_directory_fd(
        quarantine.candidates_fd, "profile quarantine root"
    )
    candidate_metadata = _require_private_directory_fd(
        quarantine.candidate_fd, "profile quarantine"
    )
    _require_directory_link(
        quarantine.root_fd,
        "profile-candidates",
        candidates_metadata,
        "profile quarantine root",
    )
    _require_directory_link(
        quarantine.candidates_fd,
        profile_id,
        candidate_metadata,
        "profile quarantine",
    )
    # Preflight every entry before deleting any of them. A nested directory is
    # outside the native picker's closed shape and must remain untouched.
    entries = os.listdir(quarantine.candidate_fd)
    for entry in entries:
        metadata = os.stat(
            entry, dir_fd=quarantine.candidate_fd, follow_symlinks=False
        )
        if stat.S_ISDIR(metadata.st_mode) or not _owned_by_effective_user(metadata):
            raise AdapterRefused("profile quarantine has an unsafe directory")
    for entry in entries:
        os.unlink(entry, dir_fd=quarantine.candidate_fd)
    os.fsync(quarantine.candidate_fd)
    _require_directory_link(
        quarantine.root_fd,
        "profile-candidates",
        candidates_metadata,
        "profile quarantine root",
    )
    _require_directory_link(
        quarantine.candidates_fd,
        profile_id,
        candidate_metadata,
        "profile quarantine",
    )
    os.rmdir(profile_id, dir_fd=quarantine.candidates_fd)
    os.fsync(quarantine.candidates_fd)
    # Keep the root descriptor live through the final parent sync. This also
    # makes the root metadata check above part of the same anchored chain.
    os.fsync(quarantine.root_fd)


@contextlib.contextmanager
def _open_installed_profile_directory(root: Path):
    """Pin and validate a pre-existing profile directory before adoption."""
    no_follow = getattr(os, "O_NOFOLLOW", None)
    directory = getattr(os, "O_DIRECTORY", None)
    if no_follow is None or directory is None:
        raise AdapterRefused("installed profile requires no-follow directory support")
    descriptors: list[int] = []
    target: _InstalledProfileDirectory | None = None
    try:
        root_fd = os.open(root, os.O_RDONLY | directory | no_follow)
        descriptors.append(root_fd)
        _require_private_directory_fd(root_fd, "app data root")
        try:
            profile_fd = os.open(
                "profile",
                os.O_RDONLY | directory | no_follow,
                dir_fd=root_fd,
            )
        except FileNotFoundError:
            profile_fd = None
        if profile_fd is not None:
            descriptors.append(profile_fd)
            metadata = _require_private_directory_fd(
                profile_fd, "installed profile directory"
            )
            _require_directory_link(
                root_fd,
                "profile",
                metadata,
                "installed profile directory",
            )
        target = _InstalledProfileDirectory(root_fd, profile_fd)
        yield target
    except AdapterRefused:
        raise
    except OSError as exc:
        raise AdapterRefused("installed profile directory is unsafe") from exc
    finally:
        if target is not None and target.profile_fd is not None:
            descriptors.append(target.profile_fd)
        for descriptor in reversed(list(dict.fromkeys(descriptors))):
            os.close(descriptor)


def _ensure_installed_profile_directory(target: _InstalledProfileDirectory) -> int:
    if target.profile_fd is not None:
        metadata = _require_private_directory_fd(
            target.profile_fd, "installed profile directory"
        )
        _require_directory_link(
            target.root_fd,
            "profile",
            metadata,
            "installed profile directory",
        )
        return target.profile_fd
    try:
        os.mkdir("profile", mode=0o700, dir_fd=target.root_fd)
        os.chmod(
            "profile",
            0o700,
            dir_fd=target.root_fd,
            follow_symlinks=False,
        )
        os.fsync(target.root_fd)
        profile_fd = os.open(
            "profile",
            os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
            dir_fd=target.root_fd,
        )
    except OSError as exc:
        raise AdapterRefused("installed profile directory could not be created") from exc
    try:
        metadata = _require_private_directory_fd(
            profile_fd, "installed profile directory"
        )
        _require_directory_link(
            target.root_fd,
            "profile",
            metadata,
            "installed profile directory",
        )
    except BaseException:
        os.close(profile_fd)
        raise
    target.profile_fd = profile_fd
    return profile_fd


def _read_private_file_at(
    directory_fd: int, name: str, *, max_bytes: int, label: str
) -> bytes:
    try:
        descriptor = os.open(
            name,
            os.O_RDONLY | os.O_NOFOLLOW,
            dir_fd=directory_fd,
        )
    except OSError as exc:
        raise AdapterRefused(f"{label} is missing or unsafe") from exc
    try:
        before = _require_private_file_fd(descriptor, label)
        if before.st_size > max_bytes:
            raise AdapterRefused(f"{label} exceeds its bounded read limit")
        chunks: list[bytes] = []
        remaining = before.st_size
        while remaining:
            chunk = os.read(descriptor, min(remaining, 1024 * 1024))
            if not chunk:
                raise AdapterRefused(f"{label} changed while being read")
            chunks.append(chunk)
            remaining -= len(chunk)
        after = os.fstat(descriptor)
        if (
            before.st_dev != after.st_dev
            or before.st_ino != after.st_ino
            or before.st_mode != after.st_mode
            or before.st_uid != after.st_uid
            or before.st_size != after.st_size
            or not _owned_by_effective_user(after)
        ):
            raise AdapterRefused(f"{label} changed while being read")
        return b"".join(chunks)
    finally:
        os.close(descriptor)


def _install_profile_bytes(
    target: _InstalledProfileDirectory, profile_bytes: bytes
) -> bytes:
    profile_fd = _ensure_installed_profile_directory(target)
    try:
        os.stat("voiceprint.json", dir_fd=profile_fd, follow_symlinks=False)
    except FileNotFoundError:
        pass
    except OSError as exc:
        raise AdapterRefused("installed profile is unsafe") from exc
    else:
        existing = _read_private_file_at(
            profile_fd,
            "voiceprint.json",
            max_bytes=len(profile_bytes),
            label="installed profile",
        )
        if existing != profile_bytes:
            raise AdapterRefused(
                "canonical artifact differs from its requested bytes"
            )
        _ensure_installed_profile_directory(target)
        return existing
    temporary = f".voiceprint.json.{uuid.uuid4()}.tmp"
    descriptor = -1
    try:
        descriptor = os.open(
            temporary,
            os.O_WRONLY | os.O_CREAT | os.O_EXCL,
            0o600,
            dir_fd=profile_fd,
        )
        os.fchmod(descriptor, 0o600)
        view = memoryview(profile_bytes)
        while view:
            written = os.write(descriptor, view)
            if written <= 0:
                raise AdapterRefused("installed profile write did not advance")
            view = view[written:]
        os.fsync(descriptor)
        os.close(descriptor)
        descriptor = -1
        try:
            os.link(
                temporary,
                "voiceprint.json",
                src_dir_fd=profile_fd,
                dst_dir_fd=profile_fd,
                follow_symlinks=False,
            )
            os.fsync(profile_fd)
        except FileExistsError:
            existing = _read_private_file_at(
                profile_fd,
                "voiceprint.json",
                max_bytes=len(profile_bytes),
                label="installed profile",
            )
            if existing != profile_bytes:
                raise AdapterRefused(
                    "canonical artifact differs from its requested bytes"
                )
        installed = _read_private_file_at(
            profile_fd,
            "voiceprint.json",
            max_bytes=len(profile_bytes),
            label="installed profile",
        )
        _ensure_installed_profile_directory(target)
        return installed
    except AdapterRefused:
        raise
    except OSError as exc:
        raise AdapterRefused("installed profile could not be written safely") from exc
    finally:
        if descriptor >= 0:
            os.close(descriptor)
        try:
            os.unlink(temporary, dir_fd=profile_fd)
            os.fsync(profile_fd)
        except FileNotFoundError:
            pass


def _validate_profile_bytes(
    root: Path, profile_bytes: bytes, encoder_digest: str
) -> None:
    """Give the canonical loader only a private, immutable byte snapshot.

    `load_profile` remains the semantic authority for the voiceprint schema,
    provenance, experimental status, and encoder fingerprint.  This adapter
    deliberately does not reproduce any of those checks.  Its responsibility is
    to ensure the loader sees the same bounded bytes that can later be installed.
    """
    from speaker_gate import load_profile

    with tempfile.TemporaryDirectory(dir=root) as temporary_name:
        temporary = Path(temporary_name)
        private_directory(temporary)
        snapshot = temporary / "voiceprint.json"
        durable_create_new(snapshot, profile_bytes)
        try:
            _, _, document = load_profile(
                snapshot, expected_encoder_fingerprint=encoder_digest
            )
            # `load_profile` decides whether the receipt is structurally valid.
            # Admission decides whether that valid receipt is a supported profile:
            # experimental calibration is deliberately available to the research CLI
            # but never enables the product capture path.
            if document["operating_point"]["experimental"]:
                raise AdapterRefused(
                    "experimental profile cannot enable application capture"
                )
        except AdapterRefused:
            raise
        except (
            AttributeError,
            KeyError,
            OSError,
            OverflowError,
            SystemExit,
            TypeError,
            ValueError,
        ) as exc:
            raise AdapterRefused(str(exc)) from None


def _read_valid_profile_candidate(
    root: Path,
    descriptor: int | None,
    encoder_digest: str,
) -> tuple[bytes, str]:
    profile_bytes = _read_profile_candidate(descriptor)
    _validate_profile_bytes(root, profile_bytes, encoder_digest)
    return profile_bytes, hashlib.sha256(profile_bytes).hexdigest()


def _private_profile_root(root: Path) -> Path:
    try:
        return require_private_root(root)
    except StorageRefused as exc:
        raise AdapterRefused(str(exc)) from None


def profile_inspect(root: Path, arguments: object, encoder_digest: str) -> dict[str, str]:
    root = _private_profile_root(root)
    values = _exact_arguments(arguments, {"profile_id"})
    profile_id = opaque_id(values["profile_id"], "profile_id")
    with _open_profile_quarantine(root, profile_id) as quarantine:
        with _open_profile_candidate(quarantine) as descriptor:
            _, digest = _read_valid_profile_candidate(root, descriptor, encoder_digest)
    return {"profile": digest}


def _profile_candidate_is_absent(root: Path, profile_id: str) -> bool:
    no_follow = getattr(os, "O_NOFOLLOW", None)
    directory = getattr(os, "O_DIRECTORY", None)
    if no_follow is None or directory is None:
        raise AdapterRefused("profile cleanup requires no-follow directory support")
    root_fd = os.open(root, os.O_RDONLY | directory | no_follow)
    candidates_fd: int | None = None
    candidate_fd: int | None = None
    try:
        _require_private_directory_fd(root_fd, "app data root")
        try:
            candidates_fd = os.open(
                "profile-candidates",
                os.O_RDONLY | directory | no_follow,
                dir_fd=root_fd,
            )
        except FileNotFoundError:
            return True
        _require_private_directory_fd(candidates_fd, "profile quarantine root")
        try:
            candidate_fd = os.open(
                profile_id,
                os.O_RDONLY | directory | no_follow,
                dir_fd=candidates_fd,
            )
        except FileNotFoundError:
            return True
        _require_private_directory_fd(candidate_fd, "profile quarantine")
        return False
    except AdapterRefused:
        raise
    except OSError as exc:
        raise AdapterRefused("profile cleanup path is unsafe") from exc
    finally:
        if candidate_fd is not None:
            os.close(candidate_fd)
        if candidates_fd is not None:
            os.close(candidates_fd)
        os.close(root_fd)


def profile_discard(root: Path, arguments: object) -> dict[str, str]:
    """Idempotently removes one digest-bound private enrollment candidate."""
    root = _private_profile_root(root)
    values = _exact_arguments(arguments, {"profile_id", "profile_sha256"})
    profile_id = opaque_id(values["profile_id"], "profile_id")
    profile_sha256 = content_digest_id(values["profile_sha256"], "profile_sha256")
    if _profile_candidate_is_absent(root, profile_id):
        return {"profile": profile_sha256}
    with _open_profile_quarantine(root, profile_id) as quarantine:
        with _open_profile_candidate(quarantine) as descriptor:
            profile_bytes = _read_profile_candidate(descriptor)
            if hashlib.sha256(profile_bytes).hexdigest() != profile_sha256:
                raise AdapterRefused("profile cleanup candidate digest changed")
        _remove_profile_quarantine(quarantine, profile_id)
    return {"profile": profile_sha256}


def profile_choices(root: Path, arguments: object, encoder_digest: str) -> dict[str, str]:
    """Measured operating-point choices from stored evidence, as a relay file.

    The document body is deterministic over the evidence, so its digest is
    the identity the later build verifies a selection against. Registered in
    the packaged alpha with the 2026-08-05 profile-build decision.
    """
    root = _private_profile_root(root)
    values = _exact_arguments(arguments, {"operation_id"})
    operation_id = opaque_id(values["operation_id"], "operation_id")
    from .profile_build import ProfileBuildRefused, write_choices

    try:
        return {"choices": write_choices(root, operation_id, encoder_digest)}
    except ProfileBuildRefused as error:
        raise AdapterRefused(str(error)) from None


def profile_build(root: Path, arguments: object, encoder_digest: str) -> dict[str, str]:
    """Build one candidate profile for an explicitly selected measured target.

    The caller supplies a target, never a threshold or a rate; the canonical
    `save_profile` boundary recomputes the deterministic choices and refuses
    anything the evidence cannot support. The candidate lands in the profile
    quarantine, where `profile.inspect` and the Rust lifecycle take over.
    """
    root = _private_profile_root(root)
    values = _exact_arguments(arguments, {"profile_id", "selected_target"})
    profile_id = opaque_id(values["profile_id"], "profile_id")
    selected_target = values["selected_target"]
    if isinstance(selected_target, bool) or not isinstance(selected_target, (int, float)):
        raise AdapterRefused("selected_target must be a number")
    selected_target = float(selected_target)
    if not (0.0 < selected_target < 1.0):
        raise AdapterRefused("selected_target must lie strictly between 0 and 1")
    from .profile_build import ProfileBuildRefused, build_candidate

    try:
        return {
            "profile": build_candidate(root, profile_id, selected_target, encoder_digest)
        }
    except ProfileBuildRefused as error:
        raise AdapterRefused(str(error)) from None


def profile_adopt(root: Path, arguments: object, encoder_digest: str) -> dict[str, str]:
    root = _private_profile_root(root)
    values = _exact_arguments(arguments, {"profile_id"})
    profile_id = opaque_id(values["profile_id"], "profile_id")
    with _open_installed_profile_directory(root) as installed:
        with _open_profile_quarantine(root, profile_id) as quarantine:
            # Once the full parent chain is pinned and verified private, consume
            # its exact quarantine on every leaf verdict. Unsafe parent chains
            # fail before entering this scope and remain untouched.
            try:
                with _open_profile_candidate(quarantine) as descriptor:
                    profile_bytes, digest = _read_valid_profile_candidate(
                        root, descriptor, encoder_digest
                    )
            finally:
                _remove_profile_quarantine(quarantine, profile_id)
        installed_bytes = _install_profile_bytes(installed, profile_bytes)
        if (
            installed_bytes != profile_bytes
            or hashlib.sha256(installed_bytes).hexdigest() != digest
        ):
            raise AdapterRefused(
                "installed profile differs from the validated candidate"
            )
        # The worker result binds only these installed bytes. Re-run the
        # canonical semantic loader after the durable write rather than treating
        # the earlier candidate verdict as authority for a different pathname.
        _validate_profile_bytes(root, installed_bytes, encoder_digest)
        return {"profile": digest}


def note_inspect(root: Path, arguments: object) -> dict[str, str]:
    values = _exact_arguments(arguments, {"meeting_id", "note_id", "transcript_id"})
    meeting_id = opaque_id(values["meeting_id"], "meeting_id")
    note_id = content_digest_id(values["note_id"], "note_id")
    transcript_id = content_digest_id(values["transcript_id"], "transcript_id")
    artifact = resolve_below(root, "meetings", meeting_id, "notes", f"{note_id}.json")
    transcript_path = resolve_below(
        root, "meetings", meeting_id, "transcript", f"{transcript_id}.json"
    )
    if not artifact.is_file() or not transcript_path.is_file():
        raise AdapterRefused("note pair or retained transcript is missing")

    from summarize import structured_artifact_citations, validate_artifact_pair

    document = json.loads(artifact.read_text(encoding="utf-8"))
    if document.get("schema") != "note/2":
        raise AdapterRefused("product note inspection requires note/2")
    if document.get("passed") is not True:
        raise AdapterRefused("rejected note output has no product authority")
    if document.get("meeting", {}).get("id") != meeting_id:
        raise AdapterRefused("note belongs to another meeting")
    if document.get("transcript") != f"../transcript/{transcript_id}.json":
        raise AdapterRefused("note does not name the requested retained transcript")
    if sha256(artifact) != note_id or sha256(transcript_path) != transcript_id:
        raise AdapterRefused("content-addressed note or transcript changed")
    transcript = resolve_transcript(root, meeting_id, transcript_id)
    markdown = validate_artifact_pair(document, artifact, transcript)
    if markdown is None:
        raise AdapterRefused("note/2 is missing its Markdown rendering")
    citations = structured_artifact_citations(document, transcript)
    if document.get("checks", {}).get("citations") != citations:
        raise AdapterRefused("note claim locators disagree with retained checks")
    markdown_digest = sha256(markdown)
    if markdown.name != f"{markdown_digest}.md":
        raise AdapterRefused("note Markdown is not content-addressed")
    return {
        "note": note_id,
        "note-markdown": markdown_digest,
        "transcript": transcript_id,
    }


def _candidate_note_generator(arguments: object) -> NoteGenerator | None:
    """Build the deterministic assembler for a generation-bearing request.

    Returns None for the bare argument shape or any malformed one, preserving
    `note.create`'s original refusal — no admitted generator — and leaving the
    precise refusal message to `note_create`'s own validation. The assembler
    runs no model: the sandboxed generate child already chose the kept
    candidates, and everything here is re-derived from the transcript and the
    registered product contract by `summarize.candidate_note_document`.
    """
    try:
        values = validate_note_create_arguments(arguments)
    except ProductContractRefused:
        return None
    generation = values.get("generation")
    if generation is None:
        return None

    def generator(transcript) -> dict:
        import candidate_first
        from summarize import candidate_note_document

        registered = candidate_first.PRODUCT_RUN["classifier"]
        content_field = "claims" if generation["schema"] == "note-generation/2" else "points"
        return candidate_note_document(
            transcript,
            {
                key: generation[key]
                for key in (
                    "schema", "transcript_sha256", "manifest_sha256",
                    "candidates", content_field,
                )
            },
            meeting_id=values["meeting_id"],
            transcript_sha256=values["source_transcript_sha256"],
            model=registered["model"],
            model_identity={
                "requested": registered["model"],
                "name": registered["model"],
                "digest": registered["model_tree_sha256"],
            },
            elapsed_s=float(generation["receipt"]["elapsed_s"]),
        )

    return generator


def note_create(
    root: Path,
    arguments: object,
    *,
    generator: NoteGenerator | None,
) -> dict[str, str]:
    """Publish one already-passing product note from an injected generator.

    This adapter intentionally has no model default. Choosing and admitting an
    automatic generator remains a product decision; the bounded fixture seam is
    sufficient to exercise the immutable publication and validation path.
    """
    root = require_private_root(root)
    try:
        values = validate_note_create_arguments(arguments)
    except ProductContractRefused as exc:
        raise AdapterRefused(str(exc)) from None
    if generator is None or not callable(generator):
        raise AdapterRefused("note.create has no admitted note generator")
    meeting_id = values["meeting_id"]
    transcript_id = values["source_transcript_sha256"]
    if _current_transcript_sha256(root, meeting_id) != transcript_id:
        raise AdapterRefused("requested transcript is not this meeting's current revision")
    transcript = resolve_transcript(root, meeting_id, transcript_id)
    try:
        document = generator(transcript)
    except AdapterRefused:
        raise
    except Exception as exc:
        raise AdapterRefused("injected note generator did not produce a candidate") from exc
    if not isinstance(document, dict):
        raise AdapterRefused("injected note generator did not produce a JSON object")
    if document.get("schema") != "note/2" or document.get("passed") is not True:
        raise AdapterRefused("only passing note/2 output has product authority")
    if document.get("meeting", {}).get("id") != meeting_id:
        raise AdapterRefused("note candidate belongs to another meeting")
    if document.get("transcript") != f"../transcript/{transcript_id}.json":
        raise AdapterRefused("note candidate does not bind the current transcript")

    from summarize import (
        StructuredOutputError,
        reconcile_capture_provenance,
        structured_artifact_citations,
        validate_artifact_pair,
        validate_note_render,
        validate_stored_verdict,
    )

    try:
        # Preflight the canonical validator's semantic gates before committing
        # durable bytes. The pair validator below then rechecks the exact files.
        validate_stored_verdict(document.get("checks"), document.get("passed"), "note candidate")
        reconcile_capture_provenance(document, transcript, where="note candidate")
        canonical_markdown = validate_note_render(document)
        citations = structured_artifact_citations(document, transcript)
    except StructuredOutputError as exc:
        raise AdapterRefused(str(exc)) from None
    if document.get("checks", {}).get("citations") != citations:
        raise AdapterRefused("note claim locators disagree with retained checks")
    markdown_sha256 = hashlib.sha256(canonical_markdown.encode("utf-8")).hexdigest()
    if document.get("render", {}).get("path") != f"{markdown_sha256}.md":
        raise AdapterRefused("note Markdown filename is not its content digest")
    notes_dir = resolve_below(root, "meetings", meeting_id, "notes")
    private_directory(notes_dir)
    markdown_path = resolve_below(root, "meetings", meeting_id, "notes", f"{markdown_sha256}.md")
    document_bytes = (
        json.dumps(document, ensure_ascii=False, indent=2) + "\n"
    ).encode("utf-8")
    note_sha256 = hashlib.sha256(document_bytes).hexdigest()
    note_path = resolve_below(root, "meetings", meeting_id, "notes", f"{note_sha256}.json")
    # The independent Markdown identity is known before the JSON references it.
    # Each final name is exclusive: a retry may inspect an existing pair but may
    # never replace it.
    try:
        durable_create_or_verify_identical(markdown_path, canonical_markdown.encode("utf-8"))
        durable_create_or_verify_identical(note_path, document_bytes)
    except StorageRefused as exc:
        raise AdapterRefused(str(exc)) from None
    try:
        markdown = validate_artifact_pair(document, note_path, transcript)
    except StructuredOutputError as exc:
        raise AdapterRefused(str(exc)) from None
    if markdown != markdown_path or sha256(note_path) != note_sha256 or sha256(markdown_path) != markdown_sha256:
        raise AdapterRefused("published note pair changed during validation")
    digests = {
        "note": note_sha256,
        "note-markdown": markdown_sha256,
        "transcript": transcript_id,
    }
    try:
        validate_note_create_join(
            values,
            digests,
            note_sha256=note_sha256,
            markdown_sha256=markdown_sha256,
        )
    except ProductContractRefused as exc:
        raise AdapterRefused(str(exc)) from None
    return digests


def corpus_embed(arguments: object, embedding_dir: Path | None) -> dict[str, str]:
    """Window text to vector. Reads no file and writes none.

    The odd one out in this module: every other adapter names artifacts under the
    storage root and returns their digests. This is a pure function of its
    arguments, because windowing is pinned in `corpus_window.rs` against the
    boundaries the measurement produced and re-deriving it here would be a third
    implementation of the same cut.

    The reply is `{text_sha256: base64 little-endian float32}` — the byte layout
    `corpus_window_vector` stores, so the store writes what the model produced
    rather than a re-encoding of it.
    """
    values = _exact_arguments(arguments, {"windows"})
    if embedding_dir is None:
        raise AdapterRefused("no embedding model is packaged in this runtime")
    from worker.embedding import EmbeddingRefused, embed_windows, load

    try:
        encoder, tokenizer = load(embedding_dir)
        return embed_windows(encoder, tokenizer, values["windows"])
    except EmbeddingRefused as exc:
        raise AdapterRefused(str(exc)) from None


def dispatch(
    root: Path,
    operation: str,
    arguments: object,
    *,
    encoder_digest: str,
    admission: str = "boundary-test",
    model_dir: Path | None = None,
    encoder_path: Path | None = None,
    embedding_dir: Path | None = None,
    transcription_engine: str = "whisper",
    speech_locale: str = "en-US",
    apple_speech_helper: Path | None = None,
    apple_speech_helper_sha256: str | None = None,
    apple_speech_os_version: str | None = None,
) -> dict[str, str]:
    adapters = {
        "profile.inspect": lambda: profile_inspect(root, arguments, encoder_digest),
        "profile.choices": lambda: profile_choices(root, arguments, encoder_digest),
        "profile.build": lambda: profile_build(root, arguments, encoder_digest),
        "profile.adopt": lambda: profile_adopt(root, arguments, encoder_digest),
        "profile.discard": lambda: profile_discard(root, arguments),
        "capture.inspect": lambda: capture_inspect(root, arguments),
        "transcript.create": lambda: transcript_create(
            root,
            arguments,
            admission=admission,
            model_dir=model_dir,
            encoder_digest=encoder_digest,
            encoder_path=encoder_path,
            transcription_engine=transcription_engine, speech_locale=speech_locale,
            apple_speech_helper=apple_speech_helper, apple_speech_helper_sha256=apple_speech_helper_sha256,
            apple_speech_os_version=apple_speech_os_version,
        ),
        "transcript.retry": lambda: transcript_retry(
            root,
            arguments,
            admission=admission,
            model_dir=model_dir,
            encoder_digest=encoder_digest,
            encoder_path=encoder_path,
            transcription_engine=transcription_engine, speech_locale=speech_locale,
            apple_speech_helper=apple_speech_helper, apple_speech_helper_sha256=apple_speech_helper_sha256,
            apple_speech_os_version=apple_speech_os_version,
        ),
        "sitting.derive": lambda: sitting_derive(
            root,
            arguments,
            admission=admission,
            model_dir=model_dir,
            encoder_digest=encoder_digest,
            encoder_path=encoder_path,
        ),
        "note.inspect": lambda: note_inspect(root, arguments),
        "note.create": lambda: note_create(
            root, arguments, generator=_candidate_note_generator(arguments)
        ),
        "corpus.embed": lambda: corpus_embed(arguments, embedding_dir),
        "capture.finalize": lambda: capture_finalize(root, arguments),
        "transcript.restore": lambda: transcript_restore(root, arguments),
    }
    try:
        return adapters[operation]()
    except KeyError:
        raise AdapterRefused("operation is outside the fixed registry") from None
    except (OSError, ValueError, SystemExit, StorageRefused) as exc:
        raise AdapterRefused(str(exc)) from None
