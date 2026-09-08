import importlib.util
import json
from pathlib import Path
import unittest


MODULE = Path(__file__).with_name("scoring.py")
SPEC = importlib.util.spec_from_file_location("speech_scoring", MODULE)
scoring = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(scoring)

PREPARE_MODULE = Path(__file__).with_name("prepare_ami.py")
PREPARE_SPEC = importlib.util.spec_from_file_location("prepare_ami", PREPARE_MODULE)
prepare_ami = importlib.util.module_from_spec(PREPARE_SPEC)
assert PREPARE_SPEC and PREPARE_SPEC.loader
PREPARE_SPEC.loader.exec_module(prepare_ami)


def reference():
    return {
        "schema": "speech-reference/1",
        "kind": "human-annotated-public-meeting",
        "words": [
            {"text": "Begin", "start": 0.0, "end": 0.2, "speaker": "A"},
            {"text": "kinetic", "start": 12.0, "end": 12.2, "speaker": "B"},
            {"text": "battery", "start": 12.2, "end": 12.4, "speaker": "B"},
            {"text": "LCD", "start": 61.0, "end": 61.2, "speaker": "C"},
        ],
        "important_spans": [
            {
                "id": "decision-1",
                "kind": "decision",
                "start": 12.0,
                "end": 12.4,
                "text": "kinetic battery",
                "terms": ["kinetic", "battery"],
            }
        ],
        "entity_spans": [
            {
                "id": "entity-1",
                "kind": "annotated-entity",
                "entity_type_id": "ne_23",
                "start": 61.0,
                "end": 61.2,
                "text": "LCD",
                "terms": ["lcd"],
            }
        ],
    }


def transcript_run(segments):
    return {"text": " ".join(segment.get("text", "") for segment in segments), "segments": segments}


class ScoringTests(unittest.TestCase):
    def test_nite_range_href_accepts_the_real_second_endpoint_shape(self):
        self.assertEqual(
            prepare_ami.href_ids("ES2004c.B.words.xml#id(ES2004c.B.words0)..id(ES2004c.B.words8)"),
            ("ES2004c.B.words0", "ES2004c.B.words8"),
        )
        self.assertEqual(
            prepare_ami.href_ids("ne-types.xml#id(ne_23)"),
            ("ne_23", None),
        )

    def test_nite_range_resolves_every_word_in_a_multiword_endpoint_range(self):
        by_id = {
            "w0": {"id": "w0", "speaker": "A", "text": "one", "is_word": True},
            "gap": {"id": "gap", "speaker": "A", "text": "", "is_word": False},
            "w1": {"id": "w1", "speaker": "A", "text": "two", "is_word": True},
            "w2": {"id": "w2", "speaker": "A", "text": "three", "is_word": True},
        }
        resolved = prepare_ami.range_words("w0", "w2", by_id, {"A": ["w0", "gap", "w1", "w2"]})
        self.assertEqual([word["text"] for word in resolved], ["one", "two", "three"])

    def test_same_global_words_in_wrong_windows_fail_local_important_span(self):
        result = {"runs": [transcript_run([{"text": "kinetic battery Begin", "start": 0.0, "end": 1.0}, {"text": "LCD", "start": 61.0, "end": 61.3}])]}
        report = scoring.score(reference(), result, 70.0)
        run = report["runs"][0]
        self.assertEqual(run["status"], "scored")
        self.assertEqual(run["important_span_term_survival"]["survived_terms"], 0)
        self.assertFalse(run["important_span_term_survival"]["by_span"][0]["complete"])
        self.assertNotEqual(run["exact_manual_serialization_distance"]["normalized_edit_distance"], 0.0)
        self.assertEqual(run["exact_manual_serialization_distance"]["manual_wer_status"], "unsupported")
        self.assertIsNone(run["exact_manual_serialization_distance"]["manual_wer"])

    def test_invalid_or_missing_times_cannot_be_scored_as_success(self):
        result = {"runs": [transcript_run([{"text": "kinetic battery", "start": None, "end": "later"}])]}
        run = scoring.score(reference(), result, 70.0)["runs"][0]
        self.assertEqual(run["status"], "error")
        self.assertNotIn("exact_manual_serialization_distance", run)
        self.assertEqual(run["timing"]["timing_missing_or_non_numeric_values"], 2)

    def test_empty_output_has_one_word_error_rate_not_zero(self):
        run = scoring.score(reference(), {"runs": [transcript_run([])]}, 70.0)["runs"][0]
        self.assertEqual(run["status"], "scored")
        self.assertEqual(run["exact_manual_serialization_distance"]["normalized_edit_distance"], 1.0)
        self.assertEqual(run["output_shape"]["tokens"], 0)

    def test_normalization_repetitions_and_entity_local_survival(self):
        result = {
            "runs": [
                transcript_run(
                    [
                        {"text": "BEGIN", "start": 0.0, "end": 0.3},
                        {"text": "Kinetic kinetic battery", "start": 12.0, "end": 12.5},
                        {"text": "LCD", "start": 61.0, "end": 61.4},
                    ]
                )
            ]
        }
        run = scoring.score(reference(), result, 70.0)["runs"][0]
        self.assertEqual(run["output_shape"]["adjacent_repeated_tokens"], 1)
        self.assertEqual(run["important_span_term_survival"]["survived_terms"], 2)
        self.assertEqual(run["annotated_entity_term_survival"]["survived_terms"], 1)
        self.assertEqual(run["lexical_timing_alignment"]["aligned"], 3)
        self.assertNotIn("kinetic", json.dumps(run))

    def test_token_normalization_preserves_whole_tokens_and_apostrophes(self):
        self.assertEqual(scoring.tokens("LCD, can't / co-op"), ["lcd", "can't", "co", "op"])

    def test_missing_reference_and_input_error_never_produce_zero_wer(self):
        missing = scoring.score({}, {"runs": [transcript_run([])]}, 70.0)
        failed = scoring.score(reference(), {"status": "error", "runs": [transcript_run([])]}, 70.0)
        self.assertEqual(missing["status"], "error")
        self.assertEqual(failed["status"], "error")
        self.assertNotIn("runs", missing)
        self.assertNotIn("runs", failed)

    def test_nonmonotonic_and_nonfinite_times_are_counted(self):
        result = {"runs": [transcript_run([{"text": "Begin", "start": 5.0, "end": 5.2}, {"text": "LCD", "start": 4.0, "end": 4.2}, {"text": "noise", "start": float("nan"), "end": 8.0}])]}
        run = scoring.score(reference(), result, 70.0)["runs"][0]
        self.assertEqual(run["status"], "error")
        self.assertEqual(run["timing"]["timing_non_monotonic"], 1)
        self.assertEqual(run["timing"]["timing_non_finite_values"], 1)

    def test_exact_edit_counts_keep_insertions_and_deletions_in_the_right_columns(self):
        self.assertEqual(scoring.edit_counts(["a"], ["a", "b"]), (0, 0, 1))
        self.assertEqual(scoring.edit_counts(["a", "b"], ["a"]), (0, 1, 0))
        self.assertEqual(scoring.edit_counts(["a"], ["b"]), (1, 0, 0))

    def test_run_text_must_cover_all_segment_tokens_and_all_text_segments_need_times(self):
        mismatch = {"runs": [{"text": "kinetic Begin", "segments": [{"text": "Begin kinetic", "start": 0.0, "end": 0.2}]}]}
        partial = {"runs": [{"text": "Begin kinetic", "segments": [{"text": "Begin", "start": 0.0, "end": 0.2}, {"text": "kinetic", "start": None, "end": None}]}]}
        mismatch_run = scoring.score(reference(), mismatch, 70.0)["runs"][0]
        partial_run = scoring.score(reference(), partial, 70.0)["runs"][0]
        self.assertEqual(mismatch_run["status"], "error")
        self.assertIn("do not match", mismatch_run["errors"][0])
        self.assertEqual(partial_run["status"], "error")
        self.assertIn("lack usable timings", partial_run["errors"][0])

    def test_annotation_gap_metric_never_claims_silence(self):
        run = scoring.score(reference(), {"runs": [transcript_run([])]}, 70.0)["runs"][0]
        diagnostic = run["annotation_gap_output"]
        self.assertIn("manual_annotation_gaps", next(iter(diagnostic)))
        self.assertIn("not verified acoustic silence", diagnostic["limit"])


if __name__ == "__main__":
    unittest.main()
