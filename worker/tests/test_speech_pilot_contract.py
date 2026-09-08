"""Independent observable-contract checks for the native speech pilot.

All text and audio below are synthetic fixture evidence.  These tests do not
make a note-quality or device claim.
"""
from __future__ import annotations

import hashlib
import json
import os
import sys
import tempfile
import unittest
import wave
from copy import deepcopy
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(REPO / "notes"), str(REPO / "spike")]

from capture_health import build as build_capture_health
from dual_capture import finalize_session, open_private_binary
from transcript import load
from worker import note_validator
from worker.speech_results import SpeechResultRefused, validated_native_segments
from worker.transcription import create_transcript_revision_from_segments


def write_wav(path: Path, value: int) -> None:
    with open_private_binary(path) as handle:
        with wave.open(handle, "wb") as output:
            output.setnchannels(1)
            output.setsampwidth(2)
            output.setframerate(16_000)
            output.writeframes(value.to_bytes(2, "little", signed=True) * 16_000)


def result(engine: str, text: str = "first words second words") -> dict:
    words = [
        {"start": 0.0, "end": 0.3, "text": "first words"},
        {"start": 0.4, "end": 0.7, "text": "second words"},
    ]
    run = {"text": text, "segments": words, "timing_issue_count": 0}
    if engine == "apple":
        run["utterances"] = [{"start": 0.0, "end": 0.7, "text": "first words second words"}]
    return {"schema": "speech-engine-result/1", "status": "ok", "engine": engine, "runs": [run, deepcopy(run)]}


class SpeechPilotContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.capture = self.root / "capture"
        self.capture.mkdir(mode=0o700)
        write_wav(self.capture / "mic.wav", 500)
        write_wav(self.capture / "system.wav", 900)
        health = build_capture_health(
            mic_samples=16_000,
            system_samples=16_000,
            capture_elapsed_samples=16_000,
            dropouts={"mic": [], "system": []},
            tap_errors=[],
            transcription_requested=False,
            transcript_written=False,
        )
        finalize_session(self.capture, "2000-01-01T00:00:00+0000", health)
        self.before = {name: hashlib.sha256((self.capture / name).read_bytes()).hexdigest() for name in ("mic.wav", "system.wav", "session.json")}

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def transcript_directory(self) -> Path:
        """Create the private hierarchy required by the real note validator."""
        current = self.root
        for name in ("meetings", "pilot", "transcript"):
            current = current / name
            current.mkdir(mode=0o700, exist_ok=True)
            current.chmod(0o700)
        return current

    def test_validated_native_segments_refuse_wrong_engine_bad_times_missing_words_and_nonmonotonic_order(self):
        self.assertEqual(len(validated_native_segments(result("apple"), engine="apple", duration_seconds=1.0)), 1)
        for label, document in {
            "wrong-engine": result("apple"),
            "missing-words": result("parakeet", text="first words"),
            "bad-timing": result("parakeet"),
            "nonmonotonic": result("parakeet"),
        }.items():
            with self.subTest(label=label):
                if label == "wrong-engine":
                    engine = "parakeet"
                else:
                    engine = "parakeet"
                if label == "bad-timing":
                    document["runs"][1]["segments"][0]["end"] = 2.0
                if label == "nonmonotonic":
                    for run in document["runs"]:
                        run["segments"].reverse()
                        run["text"] = "second words first words"
                with self.assertRaises(SpeechResultRefused):
                    validated_native_segments(document, engine=engine, duration_seconds=1.0)

    def test_silent_native_results_stay_empty_and_cannot_invent_a_note(self):
        silent = result("parakeet", text="")
        for run in silent["runs"]:
            run["segments"] = []
        self.assertEqual(validated_native_segments(silent, engine="parakeet", duration_seconds=1.0), [])

        digest, _path = create_transcript_revision_from_segments(
            self.capture,
            self.transcript_directory(),
            mic_segments=[],
            system_segments=[],
            voicing_filter=lambda items, *_args: items,
            bleed_filter=lambda items, *_args: items,
        )
        root_fd = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY)
        try:
            with self.assertRaisesRegex(note_validator.GenerationRefused, "no-generatable"):
                note_validator.generate(root_fd, {"meeting_id": "pilot", "transcript_id": digest}, ask=lambda _request: self.fail("silent input reached model transport"))
        finally:
            os.close(root_fd)

    def test_native_revision_preserves_offsets_filters_and_capture_bytes(self):
        mic = validated_native_segments(result("apple"), engine="apple", duration_seconds=1.0)
        system = [{"start": 0.8, "end": 0.95, "text": "system evidence"}]
        seen = []

        def voicing(segments, _audio, label):
            seen.append(("voice", label, len(segments)))
            return segments

        def bleed(segments, *_args):
            seen.append(("bleed", len(segments)))
            return []

        digest, path = create_transcript_revision_from_segments(
            self.capture, self.root / "meetings" / "pilot" / "transcript",
            mic_segments=mic, system_segments=system, voicing_filter=voicing, bleed_filter=bleed,
        )
        document = load(path)
        self.assertEqual(path.name, f"{digest}.json")
        self.assertEqual([(turn.text, turn.start, turn.speaker) for turn in document.turns], [("system evidence", 0.8, None)])
        self.assertEqual(seen, [("voice", "mic", 1), ("bleed", 1), ("voice", "system", 1)])
        self.assertEqual(self.before, {name: hashlib.sha256((self.capture / name).read_bytes()).hexdigest() for name in self.before})
        self.assertFalse((self.capture / "transcript.json").exists())

    def test_changed_length_vocabulary_is_prompt_only_and_locators_keep_source_offsets(self):
        mic = [{"start": 0.0, "end": 0.5, "text": "We chose packagging today."}]
        system = [{"start": 0.6, "end": 0.9, "text": "Please send the revised plan."}]
        transcript_dir = self.transcript_directory()
        digest, path = create_transcript_revision_from_segments(self.capture, transcript_dir, mic_segments=mic, system_segments=system, voicing_filter=lambda items, *_: items, bleed_filter=lambda items, *_: items)
        source = json.loads(path.read_text())["turns"][0]["text"]
        start = source.index("packagging")
        replacement = {"turn": 0, "char_start": start, "char_end": start + len("packagging"), "source_sha256": hashlib.sha256(b"packagging").hexdigest(), "replacement": "packaging"}
        seen_prompt = []

        def ask(request):
            if request["schema"] == "note-synthesis-request/1":
                aliases = [json.loads(line)["id"] for line in request["user"].splitlines() if line.startswith("{")]
                proposal = {"overview": [{"text": "Synthetic fixture summary.", "evidence_ids": [aliases[0]]}], "items": []}
                return json.dumps({"content": json.dumps(proposal)})
            seen_prompt.append(request["user"])
            enum = request["response_format"]["properties"]["items"]["items"]["properties"]["candidate_id"]["enum"]
            return json.dumps({"items": [{"candidate_id": candidate_id, "verdict": "KEEP"} for candidate_id in enum]})

        root_fd = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY)
        try:
            generated = note_validator.generate(root_fd, {"meeting_id": "pilot", "transcript_id": digest, "vocabulary_replacements": [replacement]}, ask=ask)
        finally:
            os.close(root_fd)
        self.assertTrue(any("packaging" in prompt for prompt in seen_prompt))
        self.assertIn("packagging", json.loads(path.read_text())["turns"][0]["text"])
        for claim in generated["claims"]:
            for locator in claim["locators"]:
                original = json.loads(path.read_text())["turns"][locator["turn"]]["text"]
                self.assertEqual(locator["text_sha256"], hashlib.sha256(original[locator["start"]:locator["end"]].encode()).hexdigest())


if __name__ == "__main__":
    unittest.main()
