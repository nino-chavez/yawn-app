from __future__ import annotations

import copy
import math
import unittest

from worker.speech_results import SpeechResultRefused, validated_native_segments


def run(text: str, segments: list[dict], utterances: list[dict] | None = None) -> dict:
    return {
        "text": text,
        "segments": segments,
        "utterances": utterances or [],
        "timing_issue_count": 0,
    }


def result(engine: str, item: dict) -> dict:
    return {
        "schema": "speech-engine-result/1",
        "status": "ok",
        "engine": engine,
        "runs": [item],
    }


class NativeSpeechResultTests(unittest.TestCase):
    def test_apple_retains_native_final_utterances(self) -> None:
        utterances = [
            {"text": "First sentence.", "start": 0.0, "end": 1.0},
            {"text": "Second sentence.", "start": 1.1, "end": 2.0},
        ]
        segments = [
            {"text": "First", "start": 0.0, "end": 0.3},
            {"text": "sentence.", "start": 0.3, "end": 1.0},
            {"text": "Second", "start": 1.1, "end": 1.5},
            {"text": "sentence.", "start": 1.5, "end": 2.0},
        ]
        native = result("apple", run("First sentence. Second sentence.", segments, utterances))
        self.assertEqual(
            validated_native_segments(native, engine="apple", duration_seconds=2.0),
            utterances,
        )

    def test_parakeet_groups_words_without_changing_native_endpoints(self) -> None:
        words = [
            {"text": "Keep", "start": 0.0, "end": 0.2},
            {"text": "this.", "start": 0.3, "end": 0.5},
            {"text": "Then", "start": 2.0, "end": 2.2},
            {"text": "wait", "start": 2.3, "end": 2.5},
        ]
        turns = validated_native_segments(
            result("parakeet", run("Keep this. Then wait", words)),
            engine="parakeet",
            duration_seconds=3.0,
        )
        self.assertEqual(turns, [
            {"text": "Keep this.", "start": 0.0, "end": 0.5},
            {"text": "Then wait", "start": 2.0, "end": 2.5},
        ])

    def test_wrong_engine_and_unsuccessful_result_refuse(self) -> None:
        native = result("apple", run("word", [{"text": "word", "start": 0, "end": 1}], [{"text": "word", "start": 0, "end": 1}]))
        with self.assertRaisesRegex(SpeechResultRefused, "does not match"):
            validated_native_segments(native, engine="parakeet", duration_seconds=1)
        native["status"] = "error"
        with self.assertRaisesRegex(SpeechResultRefused, "not successful"):
            validated_native_segments(native, engine="apple", duration_seconds=1)

    def test_missing_text_coverage_refuses(self) -> None:
        native = result(
            "parakeet",
            run("first second", [{"text": "first", "start": 0, "end": 1}]),
        )
        with self.assertRaisesRegex(SpeechResultRefused, "cover the full"):
            validated_native_segments(native, engine="parakeet", duration_seconds=2)

    def test_invalid_timing_refuses_without_clamping(self) -> None:
        valid = result(
            "parakeet", run("word", [{"text": "word", "start": 0, "end": 1}])
        )
        for mutation, expected in (
            (lambda item: item.update(start=-0.01), "outside"),
            (lambda item: item.update(end=math.inf), "finite"),
            (lambda item: item.update(end=0), "unordered"),
            (lambda item: item.update(end=2.1), "outside"),
        ):
            with self.subTest(expected=expected):
                native = copy.deepcopy(valid)
                mutation(native["runs"][0]["segments"][0])
                with self.assertRaisesRegex(SpeechResultRefused, expected):
                    validated_native_segments(native, engine="parakeet", duration_seconds=2)

    def test_non_monotonic_timing_refuses_without_sorting(self) -> None:
        native = result(
            "parakeet",
            run(
                "first second",
                [
                    {"text": "first", "start": 1.0, "end": 1.2},
                    {"text": "second", "start": 0.5, "end": 0.8},
                ],
            ),
        )
        with self.assertRaisesRegex(SpeechResultRefused, "non-monotonic"):
            validated_native_segments(native, engine="parakeet", duration_seconds=2)

    def test_apple_utterance_must_enclose_its_timed_words(self) -> None:
        native = result(
            "apple",
            run(
                "first second",
                [
                    {"text": "first", "start": 0.0, "end": 0.5},
                    {"text": "second", "start": 0.5, "end": 1.0},
                ],
                [{"text": "first second", "start": 0.1, "end": 1.0}],
            ),
        )
        with self.assertRaisesRegex(SpeechResultRefused, "do not enclose"):
            validated_native_segments(native, engine="apple", duration_seconds=1)

    def test_reported_timing_issue_or_missing_runs_refuse(self) -> None:
        native = result(
            "parakeet", run("word", [{"text": "word", "start": 0, "end": 1}])
        )
        native["runs"][0]["timing_issue_count"] = 1
        with self.assertRaisesRegex(SpeechResultRefused, "reports"):
            validated_native_segments(native, engine="parakeet", duration_seconds=1)
        native["runs"][0]["timing_issue_count"] = False
        with self.assertRaisesRegex(SpeechResultRefused, "reports"):
            validated_native_segments(native, engine="parakeet", duration_seconds=1)
        native["runs"] = []
        with self.assertRaisesRegex(SpeechResultRefused, "no runs"):
            validated_native_segments(native, engine="parakeet", duration_seconds=1)


if __name__ == "__main__":
    unittest.main()
