import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


MODULE = Path(__file__).with_name("summarize_grouping_pilot.py")
SPEC = importlib.util.spec_from_file_location("summarize_grouping_pilot", MODULE)
pilot = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(pilot)


def digest(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def write_json(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, sort_keys=True), encoding="utf-8")


class GroupingPilotTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name) / "run"
        self.root.mkdir()
        self.snapshot_dir = self.root / "frozen-inputs"
        self.snapshot_dir.mkdir()
        self.snapshot_hashes = {}
        for name in ("mic.wav", "system.wav", "session.json"):
            path = self.snapshot_dir / name
            path.write_bytes(name.encode())
            self.snapshot_hashes[str(path)] = pilot.sha256(path)
        write_json(self.root / "run-plan.json", {
            "schema": "speech-grouping-run-plan/1",
            "human_review": "pending",
        })
        self.variants = [
            {"directory": "apple-current", "engine": "apple", "grouping": "current", "label": "Variant A"},
            {"directory": "apple-bounded", "engine": "apple", "grouping": "bounded", "label": "Variant B"},
            {"directory": "parakeet-current", "engine": "parakeet", "grouping": "current", "label": "Variant C"},
            {"directory": "parakeet-bounded", "engine": "parakeet", "grouping": "bounded", "label": "Variant D"},
        ]
        write_json(self.root / "variants.json", {"schema": "speech-note-review-variants/1", "variants": self.variants})
        for variant in self.variants:
            self.write_success(variant)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def write_success(self, variant: dict, *, model_revision: str = "note-model-r1", source_hashes: dict | None = None) -> None:
        directory = self.root / variant["directory"]
        transcript = {
            "schema": "file-transcript/1",
            "source": "synthetic fixture",
            "attribution": "none",
            "turns": [{"text": "fixture evidence", "start": 0.0}],
        }
        transcript_bytes = (json.dumps(transcript, sort_keys=True) + "\n").encode()
        transcript_id = digest(transcript_bytes)
        transcript_path = directory / "meetings" / "research" / "transcript" / f"{transcript_id}.json"
        transcript_path.parent.mkdir(parents=True, exist_ok=True)
        transcript_path.write_bytes(transcript_bytes)
        evidence_hash = digest(b"fixture evidence")
        generated = {
            "schema": "note-generation/2",
            "transcript_sha256": transcript_id,
            "claims": [{"locators": [{"turn": 0, "start": 0, "end": 16, "text_sha256": evidence_hash}]}],
        }
        generated_path = directory / "generated.json"
        write_json(generated_path, generated)
        hashes = source_hashes or {name: digest(name.encode()) for name in pilot.RELEVANT_SOURCE_HASHES}
        audio = [digest(b"mic"), digest(b"system"), digest(b"session")]
        speech = [digest(b"mic-result"), digest(b"system-result")]
        receipt = {
            "schema": "speech-note-replay/1",
            "engine": variant["engine"],
            "grouping": variant["grouping"],
            "status": "validated-note-output",
            "source_unchanged": True,
            "generated_sha256": pilot.sha256(generated_path),
            "transcript_id": transcript_id,
            "claims": 1,
            "note_model": {"revision": model_revision},
            "note_setup_seconds": 1.0,
            "note_generation_seconds": 2.0,
            "note_requests": [{"kind": "note-classification-request/1", "seconds": 0.5}],
            "process_peak_rss_bytes": 123,
            "source_audio_hashes": audio,
            "speech_source_hashes": speech,
            "source_code_hashes": hashes,
        }
        receipt_path = directory / "receipt.json"
        write_json(receipt_path, receipt)
        native = {}
        for leg in ("mic", "system"):
            result_path = directory / f"native-{leg}-result.json"
            native_receipt_path = directory / f"native-{leg}-receipt.json"
            result_path.write_bytes(b"{}")
            native_receipt_path.write_bytes(b"{}")
            native[leg] = {
                "result": {"path": str(result_path), "sha256": pilot.sha256(result_path), "bytes": result_path.stat().st_size},
                "receipt": {"path": str(native_receipt_path), "sha256": pilot.sha256(native_receipt_path), "bytes": native_receipt_path.stat().st_size},
                "receipt_status": "ok",
                "receipt_source_unchanged": True,
            }
        preflight_path = directory / "preflight.json"
        write_json(preflight_path, {
            "schema": "speech-grouping-preflight/1",
            "note_model": {"revision": model_revision},
            "source_code_hashes": hashes,
            "run_plan_sha256": pilot.sha256(self.root / "run-plan.json"),
            "variants_sha256": pilot.sha256(self.root / "variants.json"),
            "native": {variant["engine"]: native},
        })
        write_json(self.root / f"{variant['directory']}.attempt.json", {
            "schema": "speech-note-replay-attempt/1",
            "status": "completed",
            "returncode": 0,
            "timed_out": False,
            "elapsed_seconds": 4.0,
            "engine": variant["engine"],
            "grouping": variant["grouping"],
            "directory": variant["directory"],
            "receipt_sha256": pilot.sha256(receipt_path),
            "receipt_status": "validated-note-output",
            "preflight_sha256": pilot.sha256(preflight_path),
            "source_unchanged": True,
            "source_hashes_before": self.snapshot_hashes,
            "source_hashes_after": self.snapshot_hashes,
        })

    def test_happy_path_is_content_free_and_has_signed_deltas(self):
        output = self.root / "summary.json"
        summary = pilot.summarize(self.root, output)
        self.assertEqual(output.stat().st_mode & 0o777, 0o600)
        self.assertEqual(len(summary["conditions"]), 4)
        self.assertTrue(all(row["status"] == "available" for row in summary["conditions"]))
        self.assertTrue(all(row["status"] == "available" for row in summary["engine_comparisons"]))
        self.assertNotIn("fixture evidence", output.read_text())
        self.assertNotIn(str(self.root), output.read_text())

    def test_changed_outer_receipt_hash_refuses(self):
        path = self.root / "apple-current.attempt.json"
        document = json.loads(path.read_text())
        document["receipt_sha256"] = digest(b"wrong")
        write_json(path, document)
        with self.assertRaisesRegex(pilot.GroupingPilotRefused, "receipt hash"):
            pilot.summarize(self.root, self.root / "summary.json")

    def test_wrong_engine_or_grouping_refuses(self):
        path = self.root / "apple-current.attempt.json"
        document = json.loads(path.read_text())
        document["engine"] = "parakeet"
        write_json(path, document)
        with self.assertRaisesRegex(pilot.GroupingPilotRefused, "engine/grouping"):
            pilot.summarize(self.root, self.root / "summary.json")

    def test_mismatched_model_or_source_identity_refuses(self):
        self.write_success(self.variants[1], model_revision="other-revision")
        with self.assertRaisesRegex(pilot.GroupingPilotRefused, "matched source identity mismatch"):
            pilot.summarize(self.root, self.root / "summary.json")

    def test_mismatched_asr_identity_within_engine_refuses(self):
        receipt_path = self.root / "apple-bounded" / "receipt.json"
        receipt = json.loads(receipt_path.read_text())
        receipt["speech_source_hashes"][0] = digest(b"different-asr")
        write_json(receipt_path, receipt)
        attempt_path = self.root / "apple-bounded.attempt.json"
        attempt = json.loads(attempt_path.read_text())
        attempt["receipt_sha256"] = pilot.sha256(receipt_path)
        write_json(attempt_path, attempt)
        with self.assertRaisesRegex(pilot.GroupingPilotRefused, "speech_source_hashes"):
            pilot.summarize(self.root, self.root / "summary.json")

    def test_wrong_producer_source_identity_refuses(self):
        directory = self.root / "apple-bounded"
        receipt_path = directory / "receipt.json"
        receipt = json.loads(receipt_path.read_text())
        producer = "spike/speech-engine-comparison/replay_notes.py"
        receipt["source_code_hashes"][producer] = digest(b"wrong-producer")
        write_json(receipt_path, receipt)
        preflight_path = directory / "preflight.json"
        preflight = json.loads(preflight_path.read_text())
        preflight["source_code_hashes"][producer] = digest(b"wrong-producer")
        write_json(preflight_path, preflight)
        attempt_path = self.root / "apple-bounded.attempt.json"
        attempt = json.loads(attempt_path.read_text())
        attempt["receipt_sha256"] = pilot.sha256(receipt_path)
        attempt["preflight_sha256"] = pilot.sha256(preflight_path)
        write_json(attempt_path, attempt)
        with self.assertRaisesRegex(pilot.GroupingPilotRefused, "source_code_hashes"):
            pilot.summarize(self.root, self.root / "summary.json")

    def test_wrong_frozen_plan_binding_refuses(self):
        preflight_path = self.root / "apple-current" / "preflight.json"
        preflight = json.loads(preflight_path.read_text())
        preflight["run_plan_sha256"] = digest(b"other-plan")
        write_json(preflight_path, preflight)
        attempt_path = self.root / "apple-current.attempt.json"
        attempt = json.loads(attempt_path.read_text())
        attempt["preflight_sha256"] = pilot.sha256(preflight_path)
        write_json(attempt_path, attempt)
        with self.assertRaisesRegex(pilot.GroupingPilotRefused, "preflight binding"):
            pilot.summarize(self.root, self.root / "summary.json")

    def test_invalid_locator_refuses(self):
        generated_path = self.root / "apple-current" / "generated.json"
        generated = json.loads(generated_path.read_text())
        generated["claims"][0]["locators"][0]["text_sha256"] = digest(b"wrong")
        write_json(generated_path, generated)
        receipt_path = self.root / "apple-current" / "receipt.json"
        receipt = json.loads(receipt_path.read_text())
        receipt["generated_sha256"] = pilot.sha256(generated_path)
        write_json(receipt_path, receipt)
        attempt_path = self.root / "apple-current.attempt.json"
        attempt = json.loads(attempt_path.read_text())
        attempt["receipt_sha256"] = pilot.sha256(receipt_path)
        write_json(attempt_path, attempt)
        with self.assertRaisesRegex(pilot.GroupingPilotRefused, "locator"):
            pilot.summarize(self.root, self.root / "summary.json")

    def test_missing_or_failed_attempt_is_explicit(self):
        missing = self.root / "apple-current.attempt.json"
        missing.unlink()
        summary = pilot.summarize(self.root, self.root / "summary.json")
        unavailable = next(row for row in summary["conditions"] if row["engine"] == "apple" and row["grouping"] == "current")
        self.assertEqual(unavailable["unavailable_reason"], "missing_attempt")
        self.write_success(self.variants[0])
        failed = self.root / "parakeet-bounded.attempt.json"
        write_json(failed, {
            "schema": "speech-note-replay-attempt/1",
            "status": "timeout",
            "returncode": 124,
            "timed_out": True,
            "elapsed_seconds": 600.0,
            "engine": "parakeet",
            "grouping": "bounded",
            "directory": "parakeet-bounded",
            "receipt_path": None,
            "receipt_sha256": None,
            "receipt_status": None,
            "preflight_sha256": digest(b"preflight"),
            "source_unchanged": True,
            "source_hashes_before": self.snapshot_hashes,
            "source_hashes_after": self.snapshot_hashes,
        })
        summary = pilot.summarize(self.root, self.root / "failed-summary.json")
        unavailable = next(row for row in summary["conditions"] if row["engine"] == "parakeet" and row["grouping"] == "bounded")
        self.assertEqual(unavailable["status"], "unavailable")
        comparison = next(row for row in summary["engine_comparisons"] if row["engine"] == "parakeet")
        self.assertEqual(comparison["status"], "unavailable")


if __name__ == "__main__":
    unittest.main()
