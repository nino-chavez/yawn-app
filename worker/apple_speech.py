"""Bounded local bridge to the signed Apple Speech helper.

The helper is the only process that imports Apple's macOS 26 SpeechTranscriber
API.  This module keeps the Python worker deployable on macOS 14.4 while making
the newer path an explicit, source-bound choice.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
import tempfile
import wave
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .speech_results import SpeechResultRefused, validated_native_segments
from .storage import StorageRefused, durable_create_or_verify_identical, read_private_file

_LOCALE = re.compile(r"^[A-Za-z]{2,3}(?:-[A-Za-z0-9]{2,8})*$")
_SHA256 = re.compile(r"^[0-9a-f]{64}$")
_MAX_RESULT_BYTES = 16 * 1024 * 1024


class AppleSpeechRefused(ValueError):
    """The explicitly selected local Apple engine could not run safely."""


@dataclass(frozen=True)
class AppleSpeechProducer:
    locale: str
    helper: Path
    helper_sha256: str
    os_version: str

    @property
    def identity(self) -> dict[str, str]:
        return {
            "engine": "apple-native",
            "locale": self.locale,
            "helper_sha256": self.helper_sha256,
            "asset_identity": "os-managed",
            "os_version": self.os_version,
        }


def make_producer(*, helper: Path | None, helper_sha256: str | None, locale: str, os_version: str) -> AppleSpeechProducer:
    if not isinstance(locale, str) or not _LOCALE.fullmatch(locale):
        raise AppleSpeechRefused("Apple speech locale is not a BCP-47 identifier")
    if helper is None or not helper.is_absolute() or helper.is_symlink() or not helper.is_file():
        raise AppleSpeechRefused("signed Apple speech helper is unavailable")
    if not isinstance(helper_sha256, str) or not _SHA256.fullmatch(helper_sha256):
        raise AppleSpeechRefused("signed Apple speech helper digest is invalid")
    if not isinstance(os_version, str) or not os_version or len(os_version) > 1024:
        raise AppleSpeechRefused("Apple speech OS version is invalid")
    return AppleSpeechProducer(locale=locale, helper=helper, helper_sha256=helper_sha256, os_version=os_version)


def read_capability(producer_helper: Path, *, locale: str, temporary_parent: Path) -> dict[str, Any]:
    """Read the helper's non-downloading capability receipt before startup."""
    with tempfile.TemporaryDirectory(dir=temporary_parent, prefix=".apple-speech-capability-") as name:
        temporary = Path(name); os.chmod(temporary, 0o700); output = temporary / "capability.json"
        try:
            completed = subprocess.run(
                [str(producer_helper), "--capabilities", "--locale", locale, "--out", str(output)],
                stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                timeout=30, check=False,
            )
        except subprocess.TimeoutExpired as exc:
            raise AppleSpeechRefused("Apple speech capability check timed out") from exc
        if completed.returncode != 0:
            raise AppleSpeechRefused("Apple speech capability check failed")
        try:
            document = json.loads(read_private_file(output, max_bytes=64 * 1024, label="Apple speech capability"))
        except (StorageRefused, UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise AppleSpeechRefused("Apple speech capability receipt was invalid") from exc
    required = {"schema", "state", "reason", "locale", "os_version", "asset_identity"}
    if not isinstance(document, dict) or set(document) - {"reason"} != required - {"reason"} or document.get("schema") != "apple-speech-capability/1" or document.get("state") not in {"ready", "assets-required", "unavailable"} or document.get("asset_identity") != "os-managed" or document.get("locale") != locale or not isinstance(document.get("reason"), (str, type(None))) or not isinstance(document.get("os_version"), str) or not document["os_version"]:
        raise AppleSpeechRefused("Apple speech capability receipt has the wrong shape")
    document.setdefault("reason", None)
    return document


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _duration_seconds(audio: Path) -> float:
    try:
        with wave.open(str(audio), "rb") as source:
            if source.getnchannels() != 1 or source.getsampwidth() != 2 or source.getframerate() != 16_000 or source.getcomptype() != "NONE":
                raise AppleSpeechRefused(f"{audio.name} is not mono 16 kHz PCM16 WAV")
            frames = source.getnframes()
    except (OSError, EOFError, wave.Error) as exc:
        raise AppleSpeechRefused(f"{audio.name} is not a readable WAV") from exc
    if frames <= 0:
        raise AppleSpeechRefused(f"{audio.name} has no audio frames")
    return frames / 16_000.0


def transcribe_leg(
    producer: AppleSpeechProducer,
    audio: Path,
    *,
    temporary_parent: Path,
    timeout_seconds: float = 600.0,
) -> tuple[list[dict[str, Any]], dict[str, str]]:
    """Transcribe one unchanged capture leg and return validator-owned turns.

    The helper inherits the worker process group, so desktop cancellation kills
    it with the worker.  stdin is closed and both text streams are discarded:
    private transcript text exists only in the owner-private result file.
    """
    if audio.is_symlink() or not audio.is_file():
        raise AppleSpeechRefused(f"{audio.name} is missing or unsafe")
    if _sha256(producer.helper) != producer.helper_sha256:
        raise AppleSpeechRefused("Apple speech helper changed after verification")
    before = _sha256(audio)
    duration = _duration_seconds(audio)
    with tempfile.TemporaryDirectory(dir=temporary_parent, prefix=".apple-speech-") as temporary_name:
        temporary = Path(temporary_name)
        os.chmod(temporary, 0o700)
        output = temporary / "result.json"
        try:
            completed = subprocess.run(
                [str(producer.helper), "--transcribe", "--audio", str(audio), "--locale", producer.locale, "--out", str(output)],
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                timeout=timeout_seconds,
                check=False,
            )
        except subprocess.TimeoutExpired as exc:
            raise AppleSpeechRefused("Apple speech transcription timed out") from exc
        if completed.returncode != 0:
            raise AppleSpeechRefused("Apple speech transcription helper failed")
        try:
            raw = read_private_file(output, max_bytes=_MAX_RESULT_BYTES, label="Apple speech result")
            result = json.loads(raw)
            segments = validated_native_segments(result, engine="apple", duration_seconds=duration)
        except (StorageRefused, UnicodeDecodeError, json.JSONDecodeError, SpeechResultRefused) as exc:
            raise AppleSpeechRefused("Apple speech result was invalid") from exc
    after = _sha256(audio)
    if after != before:
        raise AppleSpeechRefused(f"{audio.name} changed while Apple speech read it")
    return segments, {"audio_sha256": before, "result_sha256": hashlib.sha256(raw).hexdigest()}


def provenance_path(transcript_dir: Path, transcript_sha256: str) -> Path:
    return transcript_dir / f"{transcript_sha256}.apple-speech.json"


def write_provenance(
    transcript_dir: Path,
    transcript_sha256: str,
    producer: AppleSpeechProducer,
    *,
    mic: dict[str, str],
    system: dict[str, str],
) -> str:
    """Attach immutable engine provenance without widening transcript/2."""
    document = {
        "schema": "apple-speech-transcript-provenance/1",
        "transcript_sha256": transcript_sha256,
        "producer": producer.identity,
        "legs": {"mic": mic, "system": system},
    }
    data = (json.dumps(document, sort_keys=True, separators=(",", ":")) + "\n").encode()
    try:
        durable_create_or_verify_identical(provenance_path(transcript_dir, transcript_sha256), data)
    except StorageRefused as exc:
        raise AppleSpeechRefused("Apple speech provenance could not be stored") from exc
    return hashlib.sha256(data).hexdigest()


def require_matching_provenance(transcript_dir: Path, transcript_sha256: str, producer: AppleSpeechProducer) -> None:
    path = provenance_path(transcript_dir, transcript_sha256)
    try:
        document = json.loads(read_private_file(path, max_bytes=64 * 1024, label="Apple speech provenance"))
    except (StorageRefused, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise AppleSpeechRefused("retry transcript is not bound to Apple speech") from exc
    if not isinstance(document, dict) or document.get("schema") != "apple-speech-transcript-provenance/1" or document.get("transcript_sha256") != transcript_sha256 or document.get("producer") != producer.identity:
        raise AppleSpeechRefused("retry transcript Apple speech producer does not match this worker")
