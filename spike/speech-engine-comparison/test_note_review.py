from __future__ import annotations

import json
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import prepare_note_review as review


class NoteReviewTests(unittest.TestCase):
    def _fixture(self, root: Path) -> None:
        audio_rows = []
        for audio_id in ("private-short-mic", "private-short-system"):
            audio = root / f"{audio_id}.wav"
            audio.write_bytes(audio_id.encode())
            audio_rows.append({"id": audio_id, "audio": str(audio), "audio_sha256": review.sha256(audio), "duration": 1.0})
        (root / "corpus.json").write_text(json.dumps({"schema": "speech-engine-corpus/1", "cases": audio_rows}))
        for variant in review.VARIANTS:
            directory = root / variant
            directory.mkdir()
            transcript = directory / "transcript.json"
            transcript.write_text(json.dumps({"schema": "file-transcript/1", "source": "recording:fixture", "attribution": "none", "turns": [{"text": "source words", "speaker": "Me", "start": 0.0}]}))
            transcript_sha = review.sha256(transcript)
            generated = directory / "generated.json"
            generated.write_text(json.dumps({
                "schema": "note-generation/2",
                "transcript_sha256": transcript_sha,
                "claims": [{"claim_ordinal": 0, "claim_type": "point", "claim": "source words", "locators": [{"turn": 0, "start": 0, "end": 12, "text_sha256": review.hashlib.sha256(b"source words").hexdigest()}]}],
            }))
            receipt = {
                "schema": "speech-note-replay/1", "status": "validated-note-output", "engine": variant.removesuffix("-notes-short"), "claims": 1,
                "generated_sha256": review.sha256(generated), "transcript_id": transcript_sha,
                "transcript_path": str(transcript), "source_unchanged": True,
            }
            (directory / "receipt.json").write_text(json.dumps(receipt))
            (root / f"{variant}.attempt.json").write_text(json.dumps({"engine": variant.removesuffix("-notes-short"), "returncode": 0}))

    def test_wrong_generated_hash_refuses(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); self._fixture(root)
            receipt = json.loads((root / review.VARIANTS[0] / "receipt.json").read_text())
            receipt["generated_sha256"] = "0" * 64
            (root / review.VARIANTS[0] / "receipt.json").write_text(json.dumps(receipt))
            with self.assertRaises(review.ReviewRefused): review.validate_variant(root, review.VARIANTS[0])

    def test_receipt_engine_and_claim_count_are_required(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); self._fixture(root)
            receipt_path = root / review.VARIANTS[0] / "receipt.json"
            receipt = json.loads(receipt_path.read_text())
            receipt["claims"] = 99
            receipt_path.write_text(json.dumps(receipt))
            with self.assertRaises(review.ReviewRefused): review.validate_variant(root, review.VARIANTS[0])

    def test_cross_transcript_locator_refuses(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); self._fixture(root)
            generated_path = root / review.VARIANTS[0] / "generated.json"
            generated = json.loads(generated_path.read_text())
            generated["transcript_sha256"] = "0" * 64
            generated_path.write_text(json.dumps(generated))
            receipt = json.loads((root / review.VARIANTS[0] / "receipt.json").read_text())
            receipt["generated_sha256"] = review.sha256(generated_path)
            (root / review.VARIANTS[0] / "receipt.json").write_text(json.dumps(receipt))
            with self.assertRaises(review.ReviewRefused): review.validate_variant(root, review.VARIANTS[0])

    def test_wrong_locator_hash_after_outer_rebinding_refuses(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); self._fixture(root)
            generated_path = root / review.VARIANTS[0] / "generated.json"
            generated = json.loads(generated_path.read_text())
            generated["claims"][0]["locators"][0]["text_sha256"] = "0" * 64
            generated_path.write_text(json.dumps(generated))
            receipt = json.loads((root / review.VARIANTS[0] / "receipt.json").read_text())
            receipt["generated_sha256"] = review.sha256(generated_path)
            (root / review.VARIANTS[0] / "receipt.json").write_text(json.dumps(receipt))
            with self.assertRaises(review.ReviewRefused): review.validate_variant(root, review.VARIANTS[0])

    def test_empty_or_malformed_locators_refuse(self) -> None:
        for replacement in ([], [{"turn": 0}]):
            with self.subTest(replacement=replacement), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary); self._fixture(root)
                generated_path = root / review.VARIANTS[0] / "generated.json"
                generated = json.loads(generated_path.read_text())
                generated["claims"][0]["locators"] = replacement
                generated_path.write_text(json.dumps(generated))
                receipt = json.loads((root / review.VARIANTS[0] / "receipt.json").read_text())
                receipt["generated_sha256"] = review.sha256(generated_path)
                (root / review.VARIANTS[0] / "receipt.json").write_text(json.dumps(receipt))
                with self.assertRaises(review.ReviewRefused): review.validate_variant(root, review.VARIANTS[0])

    def test_happy_path_keeps_review_pending(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); self._fixture(root)
            out = root / "packet"
            review.prepare(root, out)
            response = json.loads((out / "review.json").read_text())
            self.assertEqual(response["status"], "pending")
            self.assertFalse(response["answered"])
            self.assertEqual([v["label"] for v in response["variants"]], ["Variant A", "Variant B", "Variant C"])

    def test_existing_output_refuses_overwrite(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary) / "review"
            out.mkdir()
            with self.assertRaises(review.ReviewRefused): review.prepare(Path(temporary), out)

    def test_output_guard_rejects_git_checkout_ancestor(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            checkout = Path(temporary) / "checkout"
            (checkout / ".git").mkdir(parents=True)
            with self.assertRaises(review.ReviewRefused): review.prepare(Path(temporary), checkout / "research")

    def _manifest(self, root: Path) -> Path:
        specs = [
            {"directory": "apple-current", "engine": "apple", "grouping": "current", "label": "Variant A"},
            {"directory": "parakeet-bounded", "engine": "parakeet", "grouping": "bounded", "label": "Variant B"},
            {"directory": "parakeet-current", "engine": "parakeet", "grouping": "current", "label": "Variant C"},
            {"directory": "apple-bounded", "engine": "apple", "grouping": "bounded", "label": "Variant D"},
        ]
        for index, spec in enumerate(specs):
            source = root / review.VARIANTS[index % len(review.VARIANTS)]
            shutil.copytree(source, root / spec["directory"])
            attempt = json.loads((root / f"{review.VARIANTS[index % len(review.VARIANTS)]}.attempt.json").read_text())
            attempt.update({"engine": spec["engine"], "grouping": spec["grouping"], "timed_out": False, "source_unchanged": True})
            (root / f"{spec['directory']}.attempt.json").write_text(json.dumps(attempt))
            receipt = json.loads((root / spec["directory"] / "receipt.json").read_text())
            receipt["engine"] = spec["engine"]; receipt["grouping"] = spec["grouping"]
            (root / spec["directory"] / "receipt.json").write_text(json.dumps(receipt))
            attempt = json.loads((root / f"{spec['directory']}.attempt.json").read_text())
            attempt.update({"schema": "speech-note-replay-attempt/1", "directory": spec["directory"], "receipt_sha256": review.sha256(root / spec["directory"] / "receipt.json")})
            (root / f"{spec['directory']}.attempt.json").write_text(json.dumps(attempt))
        path = root / "variants.json"; path.write_text(json.dumps({"schema": review.VARIANT_SCHEMA, "variants": specs}))
        return path

    def test_valid_four_variant_manifest_and_pending_packet(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); self._fixture(root); manifest = self._manifest(root)
            out = root / "packet"
            review.prepare(root, out, manifest)
            packet = json.loads((out / "review.json").read_text())
            self.assertEqual([v["label"] for v in packet["variants"]], ["Variant A", "Variant B", "Variant C", "Variant D"])
            self.assertFalse(packet["answered"])

    def test_manifest_rejects_bad_directory_duplicates_and_identity(self) -> None:
        cases = [
            {"directory": "../escape", "engine": "apple", "grouping": "current", "label": "A"},
            {"directory": "one", "engine": "apple", "grouping": "current", "label": "A"},
            {"directory": "two", "engine": "apple", "grouping": "current", "label": "A"},
            {"directory": "one", "engine": "unknown", "grouping": "current", "label": "A"},
            {"directory": "one", "engine": "apple", "grouping": "current", "label": "Variant A/x"},
        ]
        for index, entry in enumerate(cases):
            with self.subTest(index=index), tempfile.TemporaryDirectory() as temporary:
                path = Path(temporary) / "variants.json"
                second = {"directory": "two", "engine": "parakeet", "grouping": "bounded", "label": "B"}
                if index == 1:
                    second["directory"] = entry["directory"]
                if index == 2:
                    second["label"] = entry["label"]
                entries = [entry, second]
                path.write_text(json.dumps({"schema": review.VARIANT_SCHEMA, "variants": entries}))
                with self.assertRaises(review.ReviewRefused): review.load_variants_manifest(path)

    def test_manifest_missing_condition_is_unavailable(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); self._fixture(root); manifest = self._manifest(root)
            specs = json.loads(manifest.read_text()); specs["variants"][3]["directory"] = "missing"; manifest.write_text(json.dumps(specs))
            out = root / "packet"; review.prepare(root, out, manifest)
            packet = json.loads((out / "review.json").read_text())
            self.assertIn("unavailable", packet["variants"][3])

    def test_manifest_identity_and_hash_binding_refuse(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); self._fixture(root); manifest = self._manifest(root)
            spec = json.loads(manifest.read_text())["variants"][0]
            attempt_path = root / f"{spec['directory']}.attempt.json"
            attempt = json.loads(attempt_path.read_text()); attempt["grouping"] = "bounded"; attempt_path.write_text(json.dumps(attempt))
            with self.assertRaises(review.ReviewRefused): review.validate_variant(root, spec["directory"], spec)
            attempt["grouping"] = "current"; attempt_path.write_text(json.dumps(attempt))
            receipt_path = root / spec["directory"] / "receipt.json"; receipt = json.loads(receipt_path.read_text()); receipt["engine"] = "parakeet"; receipt_path.write_text(json.dumps(receipt))
            with self.assertRaises(review.ReviewRefused): review.validate_variant(root, spec["directory"], spec)

    def test_manifest_attempt_receipt_hash_refuses_swapped_receipt(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); self._fixture(root); manifest = self._manifest(root)
            spec = json.loads(manifest.read_text())["variants"][0]
            attempt_path = root / f"{spec['directory']}.attempt.json"
            attempt = json.loads(attempt_path.read_text()); attempt["receipt_sha256"] = "0" * 64; attempt_path.write_text(json.dumps(attempt))
            with self.assertRaises(review.ReviewRefused): review.validate_variant(root, spec["directory"], spec)

    def test_manifest_malformed_entry_refuses(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "variants.json"
            path.write_text(json.dumps({"schema": review.VARIANT_SCHEMA, "variants": ["bad"]}))
            with self.assertRaises(review.ReviewRefused): review.load_variants_manifest(path)


if __name__ == "__main__":
    unittest.main()
