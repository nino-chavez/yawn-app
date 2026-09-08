import importlib.util
import json
from pathlib import Path
import sys
import unittest


MODULE = Path(__file__).with_name("compare_segmentation.py")
SPEC = importlib.util.spec_from_file_location("compare_segmentation", MODULE)
comparison = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(comparison)


def segment(start, end, text, index):
    return {"start": start, "end": end, "text": text, "source_index": index}


class SegmentationComparisonTests(unittest.TestCase):
    def test_punctuation_and_bounded_grouping_preserve_token_order_and_endpoints(self):
        source = [segment(0, 0.3, "First sentence.", 0), segment(0.4, 0.8, "Second words", 1), segment(0.9, 1.2, "continue.", 2)]
        punctuation, _ = comparison.punctuation_gap_groups(source)
        bounded, _ = comparison.bounded_groups(source)
        expected = [token for row in source for token in comparison.lexical_tokens(row["text"])]
        for groups in (punctuation, bounded):
            self.assertEqual([token for row in groups for token in comparison.lexical_tokens(row["text"])], expected)
            self.assertEqual((groups[0]["start"], groups[-1]["end"]), (0, 1.2))

    def test_bounded_grouping_keeps_an_oversized_source_segment_whole(self):
        source = [segment(0, 16, " ".join(f"w{i}" for i in range(41)), 0), segment(17, 17.2, "tail", 1)]
        groups, exceptions = comparison.bounded_groups(source)
        self.assertEqual(len(groups), 2)
        self.assertEqual((groups[0]["start"], groups[0]["end"]), (0, 16))
        self.assertEqual(exceptions[0]["kind"], "oversized_source_segment")
        self.assertEqual(exceptions[0]["tokens"], 41)

    def test_invalid_timing_refuses_instead_of_repairing(self):
        run = {"total_seconds": 2.0, "segments": [{"start": 1.5, "end": 2.1, "text": "out of bounds"}]}
        with self.assertRaisesRegex(ValueError, "invalid timing"):
            comparison.validated_segments(run, 2.0)
        with self.assertRaisesRegex(ValueError, "nonmonotonic"):
            comparison.validated_segments(
                {"segments": [
                    {"start": 0.0, "end": 1.0, "text": "long"},
                    {"start": 0.2, "end": 0.8, "text": "nested"},
                ]},
                2.0,
            )

    def test_native_utterances_keep_order_and_candidate_workload_is_measured(self):
        source = [segment(0, 0.4, "We agreed.", 0), segment(0.5, 0.9, "I will send it.", 1)]
        native, _ = comparison.native_groups(
            {"utterances": source}, source, 1.0,
        )
        workload = comparison.candidate_workload(native)
        sys.path[:0] = [str(Path(__file__).resolve().parents[2] / "notes"), str(Path(__file__).resolve().parents[2] / "spike")]
        import candidate_first
        from transcript import PromptOverlay, Transcript, Turn
        from worker.note_validator import _present_classification_user

        transcript = Transcript(source="fixture", attribution="channel", turns=[Turn(text=row["text"], speaker="Me", start=row["start"]) for row in native])
        manifest = candidate_first.generate_manifest(transcript, candidate_first.STRATEGY_BROAD, contract=candidate_first.PRODUCT_CONTRACT)
        registered = candidate_first.PRODUCT_RUN["classifier"]
        offered = candidate_first.offered_candidates(manifest["candidates"], registered["offer_stride"])
        batches = candidate_first.candidate_batches(offered, registered["batch_size"])
        overlay = PromptOverlay.from_transport(transcript)
        expected_user_bytes = []
        expected_system_bytes = []
        for batch in batches:
            _schema, system, user = candidate_first.classification_request(
                transcript, manifest, batch, registered["batch_size"],
                offer_stride=registered["offer_stride"],
            )
            expected_user_bytes.append(len(_present_classification_user(user, transcript, overlay).encode("utf-8")))
            expected_system_bytes.append(len(system.encode("utf-8")))
        self.assertEqual(workload["manifest_candidates"], len(manifest["candidates"]))
        self.assertEqual(workload["offered_candidates"], len(offered))
        self.assertEqual(workload["model_calls"], len(batches))
        self.assertEqual(workload["classifier_user_prompt_bytes_total"], sum(expected_user_bytes))
        self.assertEqual(workload["classifier_system_prompt_bytes_total"], sum(expected_system_bytes))
        with self.assertRaisesRegex(ValueError, "utterance times are nonmonotonic"):
            comparison.native_groups(
                {"utterances": [source[1], source[0]]}, source, 1.0,
            )

    def test_current_pilot_maps_parakeet_to_punctuation_and_apple_to_utterances(self):
        source = [
            {"start": 0.0, "end": 0.4, "text": "First words"},
            {"start": 0.5, "end": 0.9, "text": "continue."},
        ]
        parakeet = comparison.compare_run({"segments": source}, 1.0, "parakeet")
        self.assertEqual(
            parakeet["methods"]["current_pilot"]["groups"],
            parakeet["methods"]["punctuation_gap_1s_max_15s"]["groups"],
        )
        self.assertIn("Parakeet current pilot", parakeet["methods"]["current_pilot"]["provenance"])
        apple = comparison.compare_run(
            {"segments": source, "utterances": [{"start": 0.0, "end": 0.9, "text": "First words continue."}]},
            1.0,
            "apple",
        )
        self.assertEqual(
            apple["methods"]["current_pilot"]["groups"],
            apple["methods"]["native_api"]["groups"],
        )

    def test_raw_parakeet_words_keep_shape_but_skip_unimplemented_workload(self):
        report = comparison.compare_run(
            {"segments": [
                {"start": 0.0, "end": 0.2, "text": "first"},
                {"start": 0.3, "end": 0.5, "text": "second"},
            ]},
            1.0,
            "parakeet",
        )
        native = report["methods"]["native_api"]
        self.assertEqual(native["groups"], 2)
        self.assertIsNone(native["candidate_workload"])
        self.assertEqual(native["workload_status"], "unmeasured")
        self.assertIn("not an implemented pilot grouping", native["workload_reason"])
        self.assertEqual(report["methods"]["current_pilot"]["workload_status"], "measured")

    def test_report_shape_has_timing_and_hashes_but_no_recognized_text(self):
        source = [segment(0, 0.5, "fixture words must stay private", 0)]
        report = comparison._shape(source, [])
        encoded = json.dumps(report)
        self.assertNotIn("fixture words must stay private", encoded)
        self.assertEqual(report["timing_bounds"], [{"start": 0, "end": 0.5}])
        self.assertEqual(report["lexical_tokens"], 5)


if __name__ == "__main__":
    unittest.main()
