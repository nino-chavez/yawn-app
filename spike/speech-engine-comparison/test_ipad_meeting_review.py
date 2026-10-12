import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
import wave


MODULE = Path(__file__).with_name("ipad_meeting_review.py")
SPEC = importlib.util.spec_from_file_location("ipad_meeting_review", MODULE)
review = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(review)


def write_wav(path, seconds=30):
    with wave.open(str(path), "wb") as audio:
        audio.setnchannels(1)
        audio.setsampwidth(2)
        audio.setframerate(16000)
        audio.writeframes(b"\0\0" * 16000 * seconds)


def meeting(root):
    root.mkdir()
    write_wav(root / "audio.wav")
    (root / "meeting.json").write_text(json.dumps({"schema": "ipad-ink-meeting/1", "created_at": "2026-01-01T00:00:00Z", "consent_everyone_audible": True, "keep_days": 7}))
    (root / "strokes.json").write_text(json.dumps([{"index": 0, "start_seconds": 1, "end_seconds": 2}, {"index": 1, "start_seconds": None, "end_seconds": None}]))
    (root / "transcript.json").write_text(json.dumps([
        {"start": 1.0, "end": 4.0, "text": "private made up words", "final": True},
        {"start": 10.0, "end": 14.0, "text": "more private words", "final": True},
        {"start": 20.0, "end": 25.0, "text": "final made up words", "final": True},
    ]))


def segments(path):
    path.write_text(json.dumps({"segments": [
        {"start": 0, "end": 8, "speaker": "p1"}, {"start": 8, "end": 16, "speaker": "p2"},
        {"start": 16, "end": 22, "speaker": "p1"}, {"start": 22, "end": 30, "speaker": "p2"},
    ]}))


class IpadMeetingReviewTests(unittest.TestCase):
    def test_valid_meeting_has_content_free_summary(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "meeting"; meeting(root)
            summary, errors = review.validate_meeting(root)
            self.assertEqual(errors, [])
            self.assertEqual(summary["strokes"], {"timed": 1, "untimed": 1})
            self.assertNotIn("private made up words", json.dumps(summary))

    def test_every_validation_fault_is_reported(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "meeting"; root.mkdir()
            (root / "meeting.json").write_text(json.dumps({"schema": "wrong", "consent_everyone_audible": False, "keep_days": 2, "created_at": "no"}))
            (root / "strokes.json").write_text(json.dumps([{"index": 0, "start_seconds": 1, "end_seconds": None}]))
            (root / "transcript.json").write_text(json.dumps([{"start": 0, "end": 1, "text": "hidden", "final": False}]))
            _, errors = review.validate_meeting(root)
            self.assertTrue(any("schema" in error for error in errors))
            self.assertTrue(any("consent" in error for error in errors))
            self.assertTrue(any("keep_days" in error for error in errors))
            self.assertTrue(any("created_at" in error for error in errors))
            self.assertTrue(any("audio.wav is missing" in error for error in errors))
            self.assertTrue(any("strokes" in error for error in errors))
            self.assertTrue(any("transcript" in error for error in errors))

    def test_prediction_threshold_floor_and_percentage_boundaries(self):
        segments_data = [{"start": 0, "end": .5, "speaker": "a"}, {"start": 0, "end": .49, "speaker": "b"}]
        self.assertEqual(review.predicted_set(segments_data, 0, 4), {"a"})
        segments_data = [{"start": 0, "end": 1, "speaker": "a"}, {"start": 0, "end": .99, "speaker": "b"}]
        self.assertEqual(review.predicted_set(segments_data, 0, 10), {"a"})
        overlap = [{"start": 0, "end": 1, "speaker": "a"}, {"start": 0, "end": 1, "speaker": "b"}]
        self.assertEqual(review.predicted_set(overlap, 0, 10), {"a", "b"})

    def test_packet_is_deterministic_and_labels_are_blind(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary); source = base / "meeting"; meeting(source); arm = base / "arm.json"; segments(arm)
            args = type("Args", (), {"meeting_dir": source, "segments": arm, "out": base / "one", "seed": 3, "order_seed": 9})
            review.packet(args)
            args.out = base / "two"; review.packet(args)
            for relative in ("manifest.json", "labels.json"):
                self.assertEqual((base / "one" / relative).read_bytes(), (base / "two" / relative).read_bytes())
            self.assertEqual(sorted(item.read_bytes() for item in (base / "one" / "clips").iterdir()), sorted(item.read_bytes() for item in (base / "two" / "clips").iterdir()))
            labels = (base / "one" / "labels.json").read_text()
            self.assertNotIn("p1", labels); self.assertNotIn("identity", labels); self.assertNotIn("start", labels)

    def test_label_speakers_runs_both_arms_with_fake_cli(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary); source = base / "meeting"; meeting(source)
            fake = base / "fake-cli.py"
            fake.write_text("#!/usr/bin/env python3\nimport sys\nout = sys.argv[sys.argv.index('--output') + 1]\nopen(out, 'w').write('SPEAKER x 1 0.0 1.0 <NA> <NA> p1 <NA>\\n' if sys.argv[1] == 'nemotron3-diarize' else '{\\\"segments\\\":[{\\\"start\\\":0,\\\"end\\\":1,\\\"speaker\\\":\\\"p1\\\"}]}')\n")
            fake.chmod(0o755)
            status = review.label_speakers(type("Args", (), {"meeting_dir": source, "cli": fake, "out": base / "runs"}))
            self.assertEqual(status, 0)
            for arm in ("nemotron3-c128-split-w8a8", "pipeline-offline"):
                self.assertTrue((base / "runs" / arm / "ami-full.json").is_file())

    def _packet(self, base):
        source = base / "meeting"; meeting(source); arm = base / "arm.json"; segments(arm)
        args = type("Args", (), {"meeting_dir": source, "segments": arm, "out": base / "packet", "seed": 1, "order_seed": 1})
        review.packet(args)
        return args.out

    def test_score_maps_identity_and_excludes_unclear_with_baseline(self):
        with tempfile.TemporaryDirectory() as temporary:
            packet = self._packet(Path(temporary)); manifest = json.loads((packet / "manifest.json").read_text())
            labels = json.loads((packet / "labels.json").read_text())
            for clip in manifest["clips"]:
                if clip["kind"] == "identity": labels[clip["label"]]["speaker_set"] = ["me" if clip["predicted_set"] == ["p1"] else "other"]
                elif clip["label"] == "A": labels[clip["label"]]["speaker_set"] = ["me"]
                else: labels[clip["label"]]["speaker_set"] = "unclear"
            path = packet / "filled.json"; path.write_text(json.dumps(labels)); out = packet / "score.json"
            status = review.score(type("Args", (), {"packet_dir": packet, "labels": path, "out": out}))
            result = json.loads(out.read_text())
            self.assertEqual(status, 0); self.assertEqual(result["labelled"], 1); self.assertEqual(result["unclear"], len([x for x in manifest["clips"] if x["kind"] == "balanced"]) - 1)
            self.assertEqual(result["baseline_matches"], 1)
            self.assertNotIn("private made up words", json.dumps(result))

    def test_score_writes_unresolved_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            packet = self._packet(Path(temporary)); labels = json.loads((packet / "labels.json").read_text())
            for label in labels: labels[label]["speaker_set"] = ["me"]
            path = packet / "bad.json"; path.write_text(json.dumps(labels)); out = packet / "score.json"
            self.assertEqual(review.score(type("Args", (), {"packet_dir": packet, "labels": path, "out": out})), 1)
            self.assertEqual(json.loads(out.read_text()), {"status": "identity_unresolved"})

    def test_sweep_removes_expired_and_keeps_unexpired(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name, delete_after in (("expired", "2026-01-01T00:00:00Z"), ("future", "2026-02-01T00:00:00Z")):
                packet = root / name; (packet / "clips").mkdir(parents=True); (packet / "clips" / "A.wav").write_bytes(b"x")
                (packet / "manifest.json").write_text(json.dumps({"schema": "ipad-ink-blind-review/1", "delete_after": delete_after}))
            status = review.sweep(type("Args", (), {"root": root, "now": "2026-01-15T00:00:00Z"}))
            self.assertEqual(status, 0)
            self.assertFalse((root / "expired" / "clips").exists())
            self.assertTrue((root / "future" / "clips").exists())


if __name__ == "__main__":
    unittest.main()
