"""Focused contract checks for optional local speaker suggestions.

All audio and diarizer responses are synthetic. No private recording, model, or
runtime is needed to exercise this adapter.
"""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
import uuid
import wave
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(REPO / "spike")]

from capture_health import build as build_capture_health
from dual_capture import finalize_session, open_private_binary
from worker import adapters
from worker.diarization import parse_diarizer_output, speaker_suggest


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def private_file(path: Path, data: bytes, mode: int = 0o600) -> None:
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    with os.fdopen(descriptor, "wb") as stream:
        stream.write(data)
    path.chmod(mode)


def write_wav(path: Path) -> None:
    with open_private_binary(path) as stream, wave.open(stream, "wb") as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(16_000)
        output.writeframes((500).to_bytes(2, "little", signed=True) * (16_000 * 6))


class SpeakerSuggestionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name) / "app"
        self.root.mkdir(mode=0o700)
        self.meeting_id = str(uuid.uuid4())
        self.meeting = self.root / "meetings" / self.meeting_id
        self.capture = self.meeting / "capture"
        self.capture.mkdir(mode=0o700, parents=True)
        write_wav(self.capture / "mic.wav")
        write_wav(self.capture / "system.wav")
        health = build_capture_health(
            mic_samples=16_000 * 6,
            system_samples=16_000 * 6,
            capture_elapsed_samples=16_000 * 6,
            dropouts={"mic": [], "system": []},
            tap_errors=[],
            transcription_requested=False,
            transcript_written=False,
        )
        finalize_session(self.capture, "2000-01-01T00:00:00+0000", health)
        self.audio_before = {
            name: digest(self.capture / name)
            for name in ("mic.wav", "system.wav", "session.json")
        }
        transcript_health = build_capture_health(
            mic_samples=16_000 * 6,
            system_samples=16_000 * 6,
            capture_elapsed_samples=16_000 * 6,
            dropouts={"mic": [], "system": []},
            tap_errors=[],
            transcription_requested=True,
            transcript_written=True,
        )
        self.transcript = {
            "schema": "capture-transcript/1",
            "source": "synthetic diarization fixture",
            "attribution": "channel",
            "bleed": None,
            "voiceprint": None,
            "capture_health": transcript_health,
            "turns": [
                {"start": 0.0, "end": 2.0, "speaker": "Me", "text": "first synthetic turn"},
                {"start": 2.0, "end": 4.0, "speaker": "Them", "text": "system synthetic turn"},
                {"start": 4.0, "end": 5.5, "speaker": "Me", "text": "last synthetic turn"},
            ],
        }
        transcript_bytes = (json.dumps(self.transcript, indent=2) + "\n").encode()
        self.transcript_digest = hashlib.sha256(transcript_bytes).hexdigest()
        transcript_dir = self.meeting / "transcript"
        transcript_dir.mkdir(mode=0o700)
        private_file(transcript_dir / f"{self.transcript_digest}.json", transcript_bytes)
        meeting_receipt = {
            "schema": "meeting/2",
            "meeting_id": self.meeting_id,
            "artifacts": {
                "current_transcript": {
                    "relative_path": f"transcript/{self.transcript_digest}.json",
                    "sha256": self.transcript_digest,
                }
            },
        }
        private_file(self.meeting / "meeting.json", (json.dumps(meeting_receipt) + "\n").encode())
        self.executable = self.root / "nemo-speech"
        private_file(self.executable, b"synthetic executable", 0o700)
        self.model = self.root / "Nemotron-3-Diarization.gguf"
        private_file(self.model, b"synthetic GGUF")

    def tearDown(self) -> None:
        self.temporary.cleanup()

    @property
    def arguments(self) -> dict[str, str]:
        return {
            "meeting_id": self.meeting_id,
            "source_transcript_sha256": self.transcript_digest,
            "microphone_audio_sha256": digest(self.capture / "mic.wav"),
        }

    @staticmethod
    def output(segments: list[dict]) -> str:
        # This is the observed standalone NeMo-Speech.cpp shape. The adapter
        # accepts it but does not retain the private source path.
        return json.dumps({"file": "/private/synthetic/mic.wav", "segments": segments})

    def runner(self, output: str, *, returncode: int = 0):
        captured: list[tuple[list[str], dict]] = []

        def run(command, **kwargs):
            captured.append((command, kwargs))
            return subprocess.CompletedProcess(command, returncode, output, "synthetic stderr")

        return run, captured

    def invoke(self, runner):
        return speaker_suggest(
            self.root,
            self.arguments,
            diarizer_executable=self.executable,
            diarizer_model=self.model,
            runner=runner,
        )

    def test_writes_anonymous_content_free_sidecar_and_preserves_overlap(self) -> None:
        runner, captured = self.runner(self.output([
            {"start": 0.5, "end": 1.5, "speaker": 2},
            {"start": 1.0, "end": 2.5, "speaker": 1},
            {"start": 4.0, "end": 5.5, "speaker": 2},
        ]))
        result = self.invoke(runner)
        self.assertEqual(result["status"], "suggestions-ready")
        self.assertEqual(len(captured), 1)
        command, options = captured[0]
        self.assertEqual(command[1], "diarize")
        self.assertEqual(Path(command[2]).resolve(), (self.capture / "mic.wav").resolve())
        self.assertEqual(command[3], "--model")
        self.assertEqual(Path(command[4]).resolve(), self.model.resolve())
        self.assertEqual(command[5:], ["--device", "metal", "--preset", "v3-offline", "--format", "json"])
        self.assertEqual(Path(command[0]).resolve(), self.executable.resolve())
        self.assertFalse(options["shell"])
        self.assertEqual(options["stdin"], subprocess.DEVNULL)
        self.assertEqual(options["env"], {"PATH": "/usr/bin:/bin", "NO_COLOR": "1"})

        sidecar = self.meeting / "speaker-suggestions" / f"{result['speaker-suggestion']}.json"
        document = json.loads(sidecar.read_text())
        self.assertEqual(document["schema"], "speaker-diarization-suggestion/1")
        self.assertEqual(document["status"], "review-required")
        self.assertEqual(document["anonymous_cluster_count"], 2)
        self.assertNotIn("file", document)
        self.assertNotIn("synthetic turn", json.dumps(document))
        self.assertEqual(document["intervals"], [
            {"start": 0.5, "end": 1.5, "cluster": "cluster-2"},
            {"start": 1.0, "end": 2.5, "cluster": "cluster-1"},
            {"start": 4.0, "end": 5.5, "cluster": "cluster-2"},
        ])
        self.assertEqual(document["mic_turn_suggestions"], [
            {"source_turn_index": 0, "speaker_presence": [
                {"cluster": "cluster-1", "seconds": 1.0},
                {"cluster": "cluster-2", "seconds": 1.0},
            ]},
            {"source_turn_index": 2, "speaker_presence": [
                {"cluster": "cluster-2", "seconds": 1.5},
            ]},
        ])
        self.assertEqual(self.audio_before, {
            name: digest(self.capture / name)
            for name in self.audio_before
        })
        self.assertEqual(
            json.loads((self.meeting / "transcript" / f"{self.transcript_digest}.json").read_text()),
            self.transcript,
        )
        self.assertEqual(sidecar.stat().st_mode & 0o777, 0o600)

    def test_identical_request_is_idempotent_and_zero_speakers_is_reviewable(self) -> None:
        runner, _ = self.runner(self.output([]))
        first = self.invoke(runner)
        second = self.invoke(runner)
        self.assertEqual(first, second)
        sidecars = list((self.meeting / "speaker-suggestions").glob("*.json"))
        self.assertEqual(len(sidecars), 1)
        document = json.loads(sidecars[0].read_text())
        self.assertEqual(document["status"], "no-speakers")
        self.assertEqual(document["anonymous_cluster_count"], 0)
        self.assertEqual(
            [item["speaker_presence"] for item in document["mic_turn_suggestions"]],
            [[], []],
        )

    def test_invalid_output_failure_and_missing_runtime_leave_prior_sidecar_untouched(self) -> None:
        good_runner, _ = self.runner(self.output([{"start": 0.0, "end": 1.0, "speaker": 1}]))
        good = self.invoke(good_runner)
        sidecar = self.meeting / "speaker-suggestions" / f"{good['speaker-suggestion']}.json"
        before = sidecar.read_bytes()

        malformed_runner, _ = self.runner(self.output([{"start": 0.0, "end": 7.0, "speaker": 1}]))
        failed = self.invoke(malformed_runner)
        self.assertEqual(failed["status"], "error")
        self.assertEqual(sidecar.read_bytes(), before)
        self.assertEqual(len(list(sidecar.parent.glob("*.json"))), 1)

        unavailable = speaker_suggest(
            self.root,
            self.arguments,
            diarizer_executable=None,
            diarizer_model=self.model,
        )
        self.assertEqual(unavailable, {
            "status": "unavailable", "reason": "diarizer runtime is not configured"
        })
        self.assertEqual(
            adapters.dispatch(
                self.root,
                "speaker.suggest",
                self.arguments,
                encoder_digest="0" * 64,
            ),
            unavailable,
        )

    def test_parser_refuses_malformed_times_and_too_many_clusters(self) -> None:
        for value in (
            b"not json",
            self.output([{"start": 1.0, "end": 1.0, "speaker": 1}]).encode(),
            self.output([{"start": 0.0, "end": 1.0, "speaker": index} for index in range(1, 10)]).encode(),
        ):
            with self.assertRaises(ValueError):
                parse_diarizer_output(value, duration_seconds=6.0)

    def test_parser_clamps_millisecond_end_rounding_only(self) -> None:
        rounded = self.output([{"start": 5.5, "end": 6.002, "speaker": 1}]).encode()
        self.assertEqual(
            parse_diarizer_output(rounded, duration_seconds=6.0),
            [{"start": 5.5, "end": 6.0, "cluster": "cluster-1"}],
        )
        beyond_tolerance = self.output(
            [{"start": 5.5, "end": 6.02, "speaker": 1}]
        ).encode()
        with self.assertRaises(ValueError):
            parse_diarizer_output(beyond_tolerance, duration_seconds=6.0)


if __name__ == "__main__":
    unittest.main()
