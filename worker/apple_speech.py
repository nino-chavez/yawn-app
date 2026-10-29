"""Bounded local bridge to the signed Apple Speech helper.

The helper is the only process that imports Apple's macOS 26 SpeechTranscriber
API.  This module keeps the Python worker deployable on macOS 14.4 while making
the newer path an explicit, source-bound choice.
"""

from __future__ import annotations

import contextlib
import hashlib
import json
import math
import os
import re
import subprocess
import tempfile
import threading
import time
import wave
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

from .speech_results import SpeechResultRefused, validated_native_segments
from .storage import StorageRefused, durable_create_or_verify_identical, read_private_file

_LOCALE = re.compile(r"^[A-Za-z]{2,3}(?:-[A-Za-z0-9]{2,8})*$")
_SHA256 = re.compile(r"^[0-9a-f]{64}$")
_MAX_RESULT_BYTES = 16 * 1024 * 1024
# The helper writes one "%.3f\n" line per final result. Anything that does not
# match this closed shape is noise and is ignored: no sign, exponent, "inf" or
# "nan", and no digit separators that float() would otherwise accept.
_PROGRESS_NUMBER = re.compile(rb"\d{1,9}(?:\.\d{1,6})?")
_MAX_PROGRESS_LINE = 65
_MIN_LEG_TIMEOUT_SECONDS = 600
_PROGRESS_READER_JOIN_SECONDS = 5.0
_KILLED_HELPER_REAP_SECONDS = 10.0


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


def leg_timeout_seconds(duration_seconds: float) -> float:
    """A leg may take as long as its own audio, and never less than 10 minutes."""
    return float(max(_MIN_LEG_TIMEOUT_SECONDS, math.ceil(duration_seconds)))


def _drain_progress(stream: Any, duration: float, on_progress: Callable[[float], None] | None, closed: threading.Event) -> None:
    """Read the helper's progress lines until EOF, forwarding each new maximum.

    Progress is best effort.  Every malformed line is dropped, values are clamped
    to the leg's duration, and an exception from the callback only silences
    further reports: the pipe is always drained so the helper never blocks on a
    full pipe, and nothing here can change what the helper's result file says.
    """
    best = 0.0
    discarding = False
    reporting = on_progress is not None
    while True:
        try:
            chunk = stream.readline(_MAX_PROGRESS_LINE)
        except (OSError, ValueError):
            # Close our end so the helper's later progress writes fail harmlessly
            # (it ignores SIGPIPE) instead of filling the pipe and stalling the leg.
            with contextlib.suppress(OSError, ValueError):
                stream.close()
            return
        if not chunk:
            return
        complete = chunk.endswith(b"\n")
        if discarding or not complete:
            # The rest of an overlong line (including its numeric-looking tail),
            # or an unterminated last line: neither is a progress value.
            discarding = not complete
            continue
        text = chunk[:-1]
        if _PROGRESS_NUMBER.fullmatch(text) is None:
            continue
        seconds = min(float(text), duration)
        if seconds <= best:
            continue
        best = seconds
        if reporting and not closed.is_set():
            try:
                on_progress(seconds)
            except Exception:
                reporting = False


def _run_transcribe_helper(
    argv: list[str],
    *,
    duration: float,
    timeout_seconds: float,
    on_progress: Callable[[float], None] | None,
) -> int:
    """Run the helper, forwarding progress from its stdout, and return its exit code.

    The deadline is enforced on the process itself, not on stdout: a helper that
    closes stdout but keeps running is still killed when time is up.
    """
    process = subprocess.Popen(
        argv,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    closed = threading.Event()
    reader = threading.Thread(
        target=_drain_progress,
        args=(process.stdout, duration, on_progress, closed),
        name="apple-speech-progress",
        daemon=True,
    )
    reader.start()
    timed_out = False
    returncode = 1
    try:
        try:
            returncode = process.wait(timeout=timeout_seconds)
        except subprocess.TimeoutExpired:
            timed_out = True
    finally:
        if process.poll() is None:
            process.kill()
            try:
                process.wait(timeout=_KILLED_HELPER_REAP_SECONDS)
            except subprocess.TimeoutExpired:
                pass
        reader.join(timeout=_PROGRESS_READER_JOIN_SECONDS)
        closed.set()
        if not reader.is_alive():
            process.stdout.close()
    if timed_out:
        raise AppleSpeechRefused("Apple speech transcription timed out")
    return returncode


def transcribe_leg(
    producer: AppleSpeechProducer,
    audio: Path,
    *,
    temporary_parent: Path,
    timeout_seconds: float | None = None,
    on_progress: Callable[[float], None] | None = None,
) -> tuple[list[dict[str, Any]], dict[str, str]]:
    """Transcribe one unchanged capture leg and return validator-owned turns.

    The helper inherits the worker process group, so desktop cancellation kills
    it with the worker.  stdin and stderr are closed and discarded.  stdout
    carries only the end time of each final result, as numbers; private
    transcript text exists only in the owner-private result file.

    ``on_progress(seconds_done)`` is called from a reader thread with a strictly
    rising value in ``[0, duration]``.  An exception it raises stops further
    reports and never affects the transcription.  Without ``timeout_seconds`` the
    leg may take ``max(600, ceil(duration))`` seconds.
    """
    if audio.is_symlink() or not audio.is_file():
        raise AppleSpeechRefused(f"{audio.name} is missing or unsafe")
    if _sha256(producer.helper) != producer.helper_sha256:
        raise AppleSpeechRefused("Apple speech helper changed after verification")
    before = _sha256(audio)
    duration = _duration_seconds(audio)
    limit = leg_timeout_seconds(duration) if timeout_seconds is None else timeout_seconds
    with tempfile.TemporaryDirectory(dir=temporary_parent, prefix=".apple-speech-") as temporary_name:
        temporary = Path(temporary_name)
        os.chmod(temporary, 0o700)
        output = temporary / "result.json"
        returncode = _run_transcribe_helper(
            [str(producer.helper), "--transcribe", "--audio", str(audio), "--locale", producer.locale, "--out", str(output)],
            duration=duration,
            timeout_seconds=limit,
            on_progress=on_progress,
        )
        if returncode != 0:
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


class TranscriptionProgress:
    """Cumulative audio seconds done across both legs, throttled for the wire.

    ``emit(done, total)`` is called with whole seconds.  ``done`` never goes down,
    ``total`` never changes, and an event is sent at most once per ``interval``
    seconds and only when ``done`` changed.  The first event (done 0) and the
    final one (done == total) are exempt from the throttle.  Progress is best
    effort: the first exception from ``emit`` turns reporting off for good and is
    never raised.
    """

    def __init__(
        self,
        emit: Callable[[int, int], None],
        *,
        total: int,
        clock: Callable[[], float] = time.monotonic,
        sleep: Callable[[float], None] = time.sleep,
        interval: float = 1.0,
    ) -> None:
        if not isinstance(total, int) or isinstance(total, bool) or total <= 0:
            raise ValueError("progress total must be a positive whole number of seconds")
        self.total = total
        self._emit = emit
        self._clock = clock
        self._sleep = sleep
        self._interval = interval
        self._lock = threading.Lock()
        self._high = 0
        self._sent = -1
        self._sent_at: float | None = None
        self._off = False

    def start(self) -> None:
        """Send done == 0 before the first leg begins."""
        self._publish(0, force=True)

    def leg(self, base: int, ceiling: int) -> Callable[[float], None]:
        """Return the ``on_progress`` callback for a leg starting at ``base`` seconds."""
        def report(seconds: float) -> None:
            if not isinstance(seconds, (int, float)) or not math.isfinite(seconds) or seconds < 0:
                return
            self._publish(base + min(math.floor(seconds), ceiling))
        return report

    def leg_complete(self, done: int) -> None:
        """A leg exited cleanly: its whole duration counts as done.

        A leg that reported nothing (silence) would otherwise leave the bar
        short, so this waits out the throttle window instead of dropping the
        update.  The wait is at most ``interval``.
        """
        with self._lock:
            wait = 0.0
            if done > self._sent and self._sent_at is not None:
                wait = self._interval - (self._clock() - self._sent_at)
        if wait > 0:
            self._sleep(wait)
        self._publish(done, force=True)

    def finish(self) -> None:
        """Both legs succeeded: make sure done == total has been sent."""
        self._publish(self.total, force=True)

    def _publish(self, done: int, *, force: bool = False) -> None:
        with self._lock:
            if self._off:
                return
            self._high = max(self._high, min(done, self.total))
            if self._high <= self._sent:
                return
            now = self._clock()
            if not force and self._sent_at is not None and now - self._sent_at < self._interval:
                return
            try:
                self._emit(self._high, self.total)
            except Exception:
                self._off = True
                return
            self._sent, self._sent_at = self._high, now


def _leg_ceiling(audio: Path) -> int:
    if audio.is_symlink() or not audio.is_file():
        raise AppleSpeechRefused(f"{audio.name} is missing or unsafe")
    return math.ceil(_duration_seconds(audio))


def transcribe_legs(
    producer: AppleSpeechProducer,
    mic_audio: Path,
    system_audio: Path,
    *,
    temporary_parent: Path,
    emit_progress: Callable[[int, int], None] | None = None,
    clock: Callable[[], float] = time.monotonic,
    sleep: Callable[[float], None] = time.sleep,
) -> tuple[tuple[list[dict[str, Any]], dict[str, str]], tuple[list[dict[str, Any]], dict[str, str]]]:
    """Run the mic leg, then the system leg, reporting cumulative progress.

    ``total = ceil(mic) + ceil(system)`` is taken from both WAVs up front.  While a
    leg runs, done is the cumulative floor of its reported seconds; a leg that
    exits cleanly counts as its whole ceiling.  If either WAV cannot be measured,
    progress is skipped and ``transcribe_leg`` refuses that file exactly as it
    always did.
    """
    progress: TranscriptionProgress | None = None
    mic_ceiling = system_ceiling = 0
    if emit_progress is not None:
        try:
            mic_ceiling = _leg_ceiling(mic_audio)
            system_ceiling = _leg_ceiling(system_audio)
            progress = TranscriptionProgress(emit_progress, total=mic_ceiling + system_ceiling, clock=clock, sleep=sleep)
        except (AppleSpeechRefused, ValueError):
            progress = None
    if progress is not None:
        progress.start()
    mic = transcribe_leg(
        producer, mic_audio, temporary_parent=temporary_parent,
        on_progress=progress.leg(0, mic_ceiling) if progress is not None else None,
    )
    if progress is not None:
        progress.leg_complete(mic_ceiling)
    system = transcribe_leg(
        producer, system_audio, temporary_parent=temporary_parent,
        on_progress=progress.leg(mic_ceiling, system_ceiling) if progress is not None else None,
    )
    if progress is not None:
        progress.finish()
    return mic, system


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
