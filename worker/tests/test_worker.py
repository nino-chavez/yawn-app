from __future__ import annotations

import hashlib
import io
import json
import os
import stat
import subprocess
import sys
import tempfile
import threading
import unittest
import uuid
import wave
from pathlib import Path
from unittest import mock

import numpy as np

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "spike"))
sys.path.insert(0, str(REPO / "notes"))

from capture_health import TRANSCRIPT_SCHEMA, build as build_capture_health
from dual_capture import finalize_session, open_private_binary, sha256, write_private_text
from verify_capture import verify_acquisition, verify_capture
from summarize import validate_artifact_pair
from speaker_gate import Profile, load_profile, save_profile
from transcript import load
from worker.product_contracts import (
    MAX_SOURCE_TURN_INDEX,
    ProductContractRefused,
    transcript_view_digest,
    validate_note_create_arguments,
    validate_note_create_digests,
    validate_note_create_error,
    validate_note_create_join,
    validate_transcript_restore_arguments,
    validate_transcript_restore_digests,
    validate_transcript_restore_join,
    validate_transcript_view,
)
import worker.adapters as adapters
import worker.storage as worker_storage
from worker.adapters import (
    MAX_MEETING_RECEIPT_BYTES,
    MAX_PROFILE_BYTES,
    MAX_TRANSCRIPT_REVISION_BYTES,
    AdapterRefused,
    note_create,
    profile_adopt,
    profile_inspect,
    resolve_transcript,
    transcript_restore,
)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def private_file(path: Path, data: bytes) -> None:
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(data)


def build_capture(capture_dir: Path) -> None:
    capture_dir.mkdir(mode=0o700, parents=True)
    # Cross the model adapter's half-second floor so packaged-runtime tests
    # exercise real transcription and its data-dependent filter output.
    samples = 16_000
    for leg in ("mic", "system"):
        with open_private_binary(capture_dir / f"{leg}.wav") as handle:
            with wave.open(handle, "wb") as wav:
                wav.setnchannels(1)
                wav.setsampwidth(2)
                wav.setframerate(16_000)
                wav.writeframes(b"\x01\0" * samples)
    health = build_capture_health(
        mic_samples=samples,
        system_samples=samples,
        capture_elapsed_samples=samples,
        dropouts={"mic": [], "system": []},
        tap_errors=[],
        transcription_requested=True,
        transcript_written=True,
    )
    transcript = {
        "schema": TRANSCRIPT_SCHEMA,
        "source": "mechanical worker fixture",
        "attribution": "channel",
        "bleed": None,
        "voiceprint": None,
        "capture_health": health,
        "turns": [
            {"speaker": "Me", "start": 0.0, "end": 0.08, "text": "fixture one"},
            {"speaker": "Them", "start": 0.1, "end": 0.18, "text": "fixture two"},
        ],
    }
    write_private_text(
        capture_dir / "transcript.json", json.dumps(transcript, indent=2) + "\n"
    )
    for leg in ("mic", "system"):
        segments = {
            "schema": "mic-segments/1",
            "timeline": f"{leg}-local",
            "leg": leg,
            "duration_s": samples / 16_000,
            "filtered": ["voicing"],
            "labels": None,
            "audio_sha256": sha256(capture_dir / f"{leg}.wav"),
            "audio_samples": samples,
            "captured_at": "2000-01-01T00:00:00+0000",
            "segments": [],
        }
        write_private_text(
            capture_dir / f"{leg}-segments.json",
            json.dumps(segments, indent=2) + "\n",
        )
    finalize_session(capture_dir, "2000-01-01T00:00:00+0000", health)


class WorkerProcess:
    def __init__(self, root: Path, manifest: Path):
        read_fd, self.write_fd = os.pipe()
        packaged_root_value = os.environ.get("LMN_PACKAGED_RUNTIME_ROOT")
        model_arguments = []
        if packaged_root_value:
            packaged_root = Path(packaged_root_value)
            executable = packaged_root / "python-runtime/bin/python3.12"
            prefix = [str(executable), "-E", "-s", "-B", "-m", "worker.main"]
            cwd = packaged_root
            packaged_manifest = json.loads(manifest.read_text())
            if packaged_manifest["schema"] == "app-runtime/2":
                source_value = os.environ.get("LMN_TRANSCRIPT_MODEL_DIR")
                if not source_value:
                    raise AssertionError(
                        "LMN_TRANSCRIPT_MODEL_DIR is required for app-runtime/2 tests"
                    )
                catalog_path = packaged_root / packaged_manifest["model_catalog"]["path"]
                entry = json.loads(catalog_path.read_text())["models"][0]
                model_dir = root / "models" / entry["id"] / entry["revision"]
                model_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
                source = Path(source_value)
                for file in entry["files"]:
                    target = model_dir / file["name"]
                    if not target.exists():
                        os.link((source / file["name"]).resolve(strict=True), target)
                receipt = {
                    "schema": "yawn-model-install/1",
                    "id": entry["id"],
                    "revision": entry["revision"],
                    "files": [
                        {
                            key: file[key]
                            for key in ("role", "name", "bytes", "sha256")
                        }
                        for file in entry["files"]
                    ],
                }
                receipt_path = model_dir / "model-install.json"
                if not receipt_path.exists():
                    private_file(receipt_path, (json.dumps(receipt) + "\n").encode())
                model_arguments = ["--transcript-model-receipt", str(receipt_path)]
        else:
            prefix = [sys.executable, "-m", "worker.main"]
            cwd = REPO
        self.process = subprocess.Popen(
            prefix
            + [
                "--app-data-root",
                str(root),
                "--runtime-manifest",
                str(manifest),
                *model_arguments,
                "--parent-liveness-fd",
                str(read_fd),
            ],
            cwd=cwd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            pass_fds=(read_fd,),
            text=True,
        )
        os.close(read_fd)
        self.closed = False
        self.ready = json.loads(self.process.stdout.readline())

    def request(self, operation: str, arguments: dict) -> dict:
        request_id = str(uuid.uuid4())
        command = {
            "schema": "worker-command/2",
            "request_id": request_id,
            "operation": operation,
            "arguments": arguments,
        }
        self.process.stdin.write(json.dumps(command) + "\n")
        self.process.stdin.flush()
        while True:
            result = json.loads(self.process.stdout.readline())
            if result.get("schema") == "worker-result/2":
                self.assert_result_id(result, request_id)
                return result
            if (
                result.get("schema") != "worker-event/2"
                or result.get("event") != "capture.state"
                or result.get("request_id") != request_id
                or result.get("meeting_id") != arguments.get("meeting_id")
                or result.get("state") not in {"recording", "transcribing"}
            ):
                raise AssertionError("worker emitted an unexpected protocol frame")

    @staticmethod
    def assert_result_id(result: dict, request_id: str) -> None:
        if result["request_id"] != request_id:
            raise AssertionError("worker result request ID mismatch")

    def close(self) -> None:
        if not self.closed:
            os.close(self.write_fd)
            self.closed = True
        self.process.wait(timeout=30)
        error = self.process.stderr.read()
        self.process.stdin.close()
        self.process.stdout.close()
        self.process.stderr.close()
        if self.process.returncode != 0:
            raise AssertionError(error)


class WorkerProtocolTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.base = Path(self.temporary.name)
        self.root = self.base / "app"
        self.root.mkdir(mode=0o700)
        resources = self.base / "resources"
        resources.mkdir(mode=0o700)
        self.resources = resources
        for name in ("runtime", "worker", "tap", "encoder", "permission-probe"):
            private_file(resources / name, name.encode())
        manifest = {
            "schema": "app-runtime/1",
            "admission": "boundary-test",
            "runtime": {"path": "runtime", "sha256": digest(resources / "runtime")},
            "worker": {"path": "worker", "sha256": digest(resources / "worker")},
            "tap": {"path": "tap", "sha256": digest(resources / "tap")},
            "encoder": {"path": "encoder", "sha256": digest(resources / "encoder")},
            "permission_probe": {
                "path": "permission-probe",
                "sha256": digest(resources / "permission-probe"),
            },
            "models": [],
        }
        self.base_manifest = manifest
        self.admission = "boundary-test"
        packaged_root = os.environ.get("LMN_PACKAGED_RUNTIME_ROOT")
        if packaged_root:
            self.manifest = Path(packaged_root) / "app-runtime.json"
            packaged_manifest = json.loads(self.manifest.read_text())
            self.admission = packaged_manifest["admission"]
            self.encoder_digest = packaged_manifest["encoder"]["sha256"]
        else:
            self.manifest = resources / "manifest.json"
            private_file(self.manifest, (json.dumps(manifest) + "\n").encode())
            self.encoder_digest = manifest["encoder"]["sha256"]

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def require_worker_operation(self, operation: str) -> None:
        """Skip when the packaged runtime's admission does not serve the operation.

        The packaged internal-alpha worker serves only the capture/transcript
        set; profile and note operations are boundary-test/product surface and
        stay covered by the fixture-manifest run. A silent drift in the sets
        themselves still fails the ready-frame operations pin below.
        """
        from worker.main import operations_for

        if operation not in operations_for(self.admission):
            self.skipTest(
                f"packaged admission {self.admission} does not serve {operation}; "
                "the boundary-test fixture-manifest run covers it"
            )

    def test_capture_and_transcript_match_direct_validators(self) -> None:
        meeting_id = str(uuid.uuid4())
        capture = self.root / "meetings" / meeting_id / "capture"
        build_capture(capture)
        direct = verify_capture(capture)
        acquisition = verify_acquisition(capture)
        session_digest = digest(capture / "session.json")
        mic_digest = digest(capture / "mic.wav")
        system_digest = digest(capture / "system.wav")
        transcript_digest = digest(capture / "transcript.json")

        worker = WorkerProcess(self.root, self.manifest)
        try:
            self.assertEqual(worker.ready["schema"], "worker-event/2")
            self.assertEqual(worker.ready["protocol"], 2)
            self.assertEqual(worker.ready["admission"], self.admission)
            expected_operations = {
                "capture.finalize",
                "capture.inspect",
                "transcript.create",
                "transcript.retry",
                "sitting.derive",
                "transcript.restore",
                "profile.choices",
                "profile.build",
                "profile.inspect",
                "profile.discard",
                # In every lane since 2026-08-08, when build-alpha began staging
                # the embedding model. It spent one change boundary-only so the
                # shipped worker would not advertise what it could only refuse.
                "corpus.embed",
                # In every lane since 2026-08-16, with the generation-invocation
                # decision: the deterministic candidate-point assembler needs no
                # model in this worker, so there is nothing it can only refuse.
                "note.create",
                # The coordinator re-inspects every published note before the
                # meeting record advances in every admitted lane.
                "note.inspect",
            }
            if self.admission != "internal-alpha":
                expected_operations |= {
                    "profile.adopt",
                }
            self.assertEqual(set(worker.ready["operations"]), expected_operations)
            inspected = worker.request("capture.inspect", {"meeting_id": meeting_id})
            self.assertTrue(inspected["ok"])
            self.assertEqual(
                inspected["artifact_digests"],
                {
                    "capture-session": session_digest,
                    "capture-mic": mic_digest,
                    "capture-system": system_digest,
                },
            )
            created = worker.request("transcript.create", {"meeting_id": meeting_id})
            self.assertTrue(created["ok"])
            created_digest = created["artifact_digests"]["transcript"]
            if self.admission == "boundary-test":
                self.assertEqual(created_digest, transcript_digest)
            retained = self.root / "meetings" / meeting_id / "transcript" / (
                created_digest + ".json"
            )
            self.assertEqual(digest(retained), created_digest)
            load(retained)
            self.assertEqual(direct["status"], "complete")
            self.assertEqual(acquisition["status"], "complete")
        finally:
            worker.close()

    def test_internal_alpha_manifest_selects_the_fixed_model_directory(self) -> None:
        from worker.main import load_manifest, transcript_model_dir

        model_dir = self.resources / "models" / "whisper-large-v3-turbo"
        model_dir.mkdir(mode=0o700, parents=True)
        private_file(model_dir / "config.json", b"config")
        private_file(model_dir / "weights.safetensors", b"weights")
        document = json.loads(json.dumps(self.base_manifest))
        document["admission"] = "internal-alpha"
        document["models"] = [
            {
                "id": "whisper-large-v3-turbo-config",
                "path": "models/whisper-large-v3-turbo/config.json",
                "sha256": digest(model_dir / "config.json"),
            },
            {
                "id": "whisper-large-v3-turbo-weights",
                "path": "models/whisper-large-v3-turbo/weights.safetensors",
                "sha256": digest(model_dir / "weights.safetensors"),
            },
        ]
        alpha_manifest = self.resources / "internal-alpha.json"
        private_file(alpha_manifest, (json.dumps(document) + "\n").encode())

        loaded = load_manifest(alpha_manifest)
        self.assertEqual(transcript_model_dir(alpha_manifest, loaded), model_dir.resolve())

    def test_external_model_receipt_is_bound_to_the_signed_catalog(self) -> None:
        from worker.main import external_transcript_model, load_manifest

        model_id = "whisper-large-v3-turbo-q4"
        revision = "660c343bbf4e52ac257f0b7d952e5388e6f93bef"
        model_dir = self.root / "models" / model_id / revision
        model_dir.mkdir(mode=0o700, parents=True)
        files = []
        for role, name, contents in (
            ("config", "config.json", b"config"),
            ("weights", "weights.npz", b"weights"),
        ):
            private_file(model_dir / name, contents)
            files.append(
                {
                    "role": role,
                    "name": name,
                    "url": f"https://models.example/{revision}/{name}",
                    "bytes": len(contents),
                    "sha256": digest(model_dir / name),
                }
            )
        receipt = {
            "schema": "yawn-model-install/1",
            "id": model_id,
            "revision": revision,
            "files": [
                {key: file[key] for key in ("role", "name", "bytes", "sha256")}
                for file in files
            ],
        }
        receipt_path = model_dir / "model-install.json"
        private_file(receipt_path, (json.dumps(receipt) + "\n").encode())
        catalog = {
            "schema": "yawn-model-catalog/1",
            "models": [
                {
                    "id": model_id,
                    "revision": revision,
                    "title": "Smaller download",
                    "detail": "A fixture model.",
                    "downloadBytes": sum(file["bytes"] for file in files),
                    "installedBytes": sum(file["bytes"] for file in files),
                    "files": files,
                }
            ],
            # The shipped catalog carries note-model entries alongside the
            # transcript entries; the transcript reader must admit the key
            # (regression: the packaged worker refused the whole catalog the
            # first time `note_models` appeared in it).
            "note_models": [],
        }
        catalog_path = self.resources / "model-catalog.json"
        private_file(catalog_path, (json.dumps(catalog) + "\n").encode())
        document = json.loads(json.dumps(self.base_manifest))
        document["schema"] = "app-runtime/2"
        document["admission"] = "internal-alpha"
        document["model_catalog"] = {
            "path": "model-catalog.json",
            "sha256": digest(catalog_path),
        }
        manifest_path = self.resources / "external-alpha.json"
        private_file(manifest_path, (json.dumps(document) + "\n").encode())

        loaded = load_manifest(manifest_path)
        selected, ready = external_transcript_model(
            self.root, manifest_path, loaded, receipt_path
        )
        self.assertEqual(selected, model_dir.resolve())
        self.assertEqual(
            {entry["id"] for entry in ready},
            {"whisper-transcript-config", "whisper-transcript-weights"},
        )

        (model_dir / "weights.npz").write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "digest mismatch"):
            external_transcript_model(self.root, manifest_path, loaded, receipt_path)

    def test_capture_finalize_creates_exact_receipt_without_overwrite(self) -> None:
        meeting_id = str(uuid.uuid4())
        capture = self.root / "meetings" / meeting_id / "capture"
        capture.mkdir(mode=0o700, parents=True)
        samples = 3_200
        for leg in ("mic", "system"):
            with open_private_binary(capture / f"{leg}.wav") as handle:
                with wave.open(handle, "wb") as wav:
                    wav.setnchannels(1)
                    wav.setsampwidth(2)
                    wav.setframerate(16_000)
                    wav.writeframes(b"\x01\0" * samples)

        worker = WorkerProcess(self.root, self.manifest)
        try:
            finalized = worker.request(
                "capture.finalize",
                {
                    "meeting_id": meeting_id,
                    "started_at_epoch_seconds": 946684800,
                    "capture_elapsed_samples": samples,
                    "pauses": None,
                },
            )
            self.assertTrue(finalized["ok"])
            self.assertEqual(
                set(finalized["artifact_digests"]),
                {"capture-session", "capture-mic", "capture-system"},
            )
            verify_acquisition(capture)
            original = (capture / "session.json").read_bytes()

            repeated = worker.request(
                "capture.finalize",
                {
                    "meeting_id": meeting_id,
                    "started_at_epoch_seconds": 946684800,
                    "capture_elapsed_samples": samples,
                    "pauses": None,
                },
            )
            self.assertFalse(repeated["ok"])
            self.assertEqual((capture / "session.json").read_bytes(), original)
        finally:
            worker.close()

    def _finalizable_capture(self, samples: int = 3_200) -> tuple[str, Path]:
        meeting_id = str(uuid.uuid4())
        capture = self.root / "meetings" / meeting_id / "capture"
        capture.mkdir(mode=0o700, parents=True)
        for leg in ("mic", "system"):
            with (
                open_private_binary(capture / f"{leg}.wav") as handle,
                wave.open(handle, "wb") as wav,
            ):
                wav.setnchannels(1)
                wav.setsampwidth(2)
                wav.setframerate(16_000)
                wav.writeframes(b"\x01\0" * samples)
        return meeting_id, capture

    def test_capture_finalize_records_pause_spans_and_leaves_unpaused_receipts_unchanged(
        self,
    ) -> None:
        samples = 3_200
        absent_id, absent_capture = self._finalizable_capture(samples)
        empty_id, empty_capture = self._finalizable_capture(samples)
        paused_id, paused_capture = self._finalizable_capture(samples)

        worker = WorkerProcess(self.root, self.manifest)
        try:
            def finalize(meeting_id: str, pauses: object) -> dict:
                return worker.request(
                    "capture.finalize",
                    {
                        "meeting_id": meeting_id,
                        "started_at_epoch_seconds": 946684800,
                        "capture_elapsed_samples": samples,
                        "pauses": pauses,
                    },
                )

            self.assertTrue(finalize(absent_id, None)["ok"])
            self.assertTrue(
                finalize(empty_id, {"schema": "capture-pauses/1", "spans": []})["ok"]
            )

            # A capture that was never paused writes the receipt it wrote before
            # pause existed: no key claiming an absence, and the same shape
            # whether the caller says None or says "no spans".
            absent = json.loads((absent_capture / "session.json").read_text())
            empty = json.loads((empty_capture / "session.json").read_text())
            self.assertNotIn("pauses", absent)
            self.assertNotIn("pauses", empty)
            self.assertEqual(list(absent), list(empty))
            self.assertEqual(
                list(absent),
                [
                    "schema",
                    "status",
                    "started_at",
                    "finalized_at",
                    "health",
                    "quality",
                    "microphone",
                    "reconciliation",
                    "artifacts",
                ],
            )
            # finalized_at is written from the clock, so the rest of the receipt
            # is what has to agree.
            absent.pop("finalized_at")
            empty.pop("finalized_at")
            self.assertEqual(absent, empty)

            recorded = {
                "schema": "capture-pauses/1",
                "spans": [{"paused_at_samples": 1_600, "resumed_at_samples": 1_600 + 16_000 * 30}],
            }
            self.assertTrue(finalize(paused_id, recorded)["ok"])
            paused = json.loads((paused_capture / "session.json").read_text())
            self.assertEqual(paused["pauses"], recorded)
            verify_acquisition(paused_capture)
        finally:
            worker.close()

    def test_capture_finalize_refuses_malformed_pause_evidence(self) -> None:
        samples = 3_200
        refused = [
            {"schema": "capture-pauses/2", "spans": []},
            {"schema": "capture-pauses/1"},
            {"schema": "capture-pauses/1", "spans": [], "extra": True},
            {
                "schema": "capture-pauses/1",
                "spans": [{"paused_at_samples": 0, "resumed_at_samples": 0}],
            },
            {
                "schema": "capture-pauses/1",
                "spans": [{"paused_at_samples": -1, "resumed_at_samples": 16_000}],
            },
            # Ordered and non-overlapping on the wall clock.
            {
                "schema": "capture-pauses/1",
                "spans": [
                    {"paused_at_samples": 1_600, "resumed_at_samples": 17_600},
                    {"paused_at_samples": 1_599, "resumed_at_samples": 17_600},
                ],
            },
            # A pause cannot begin after the meeting stopped. The wall-clock
            # span is the recorded span plus the total paused time, so a start
            # past the recorded span puts the end past the meeting.
            {
                "schema": "capture-pauses/1",
                "spans": [
                    {"paused_at_samples": samples + 1, "resumed_at_samples": samples + 2}
                ],
            },
            {
                "schema": "capture-pauses/1",
                "spans": [{"paused_at_samples": 0, "resumed_at_samples": True}],
            },
            {
                "schema": "capture-pauses/1",
                "spans": [
                    {"paused_at_samples": index * 4, "resumed_at_samples": index * 4 + 2}
                    for index in range(65)
                ],
            },
            [],
            "none",
        ]

        worker = WorkerProcess(self.root, self.manifest)
        try:
            for pauses in refused:
                meeting_id, capture = self._finalizable_capture(samples)
                result = worker.request(
                    "capture.finalize",
                    {
                        "meeting_id": meeting_id,
                        "started_at_epoch_seconds": 946684800,
                        "capture_elapsed_samples": samples,
                        "pauses": pauses,
                    },
                )
                self.assertFalse(result["ok"], msg=repr(pauses))
                # A refused pause record refuses the whole finalization; it is
                # never normalized into a receipt that omits the gap.
                self.assertFalse((capture / "session.json").exists(), msg=repr(pauses))
        finally:
            worker.close()

    def test_command_paths_and_research_flags_are_refused(self) -> None:
        worker = WorkerProcess(self.root, self.manifest)
        try:
            result = worker.request(
                "capture.inspect",
                {"meeting_id": "../escape", "capture_dir": "/tmp/private"},
            )
            self.assertFalse(result["ok"])
            self.assertEqual(result["code"], "protocol_failure")
            self.assertEqual(result["artifact_digests"], {})
        finally:
            worker.close()

    def test_unavailable_capture_start_cannot_report_success(self) -> None:
        worker = WorkerProcess(self.root, self.manifest)
        try:
            result = worker.request(
                "capture.start",
                {"meeting_id": str(uuid.uuid4())},
            )
            self.assertFalse(result["ok"])
            self.assertEqual(result["code"], "protocol_failure")
            self.assertFalse(result["recoverable"])
            self.assertEqual(result["artifact_digests"], {})
        finally:
            worker.close()

    def test_note_pair_matches_direct_validator(self) -> None:
        self.require_worker_operation("note.inspect")
        pack = json.loads(
            (REPO / "docs/prototype/fixtures/accepted-note2.fixture").read_text()
        )
        meeting_id = str(uuid.uuid4())
        transcript_bytes = (json.dumps(pack["transcript"], indent=2) + "\n").encode()
        transcript_id = hashlib.sha256(transcript_bytes).hexdigest()
        markdown_bytes = pack["markdown"].encode()
        markdown_id = hashlib.sha256(markdown_bytes).hexdigest()
        note = json.loads(json.dumps(pack["note"]))
        note["meeting"]["id"] = meeting_id
        note["transcript"] = f"../transcript/{transcript_id}.json"
        note["render"]["path"] = f"{markdown_id}.md"
        note_bytes = (json.dumps(note, ensure_ascii=False, indent=2) + "\n").encode()
        note_id = hashlib.sha256(note_bytes).hexdigest()
        transcript_dir = self.root / "meetings" / meeting_id / "transcript"
        notes_dir = self.root / "meetings" / meeting_id / "notes"
        transcript_dir.mkdir(mode=0o700, parents=True)
        notes_dir.mkdir(mode=0o700)
        transcript_path = transcript_dir / f"{transcript_id}.json"
        note_path = notes_dir / f"{note_id}.json"
        markdown_path = notes_dir / f"{markdown_id}.md"
        private_file(transcript_path, transcript_bytes)
        private_file(note_path, note_bytes)
        private_file(markdown_path, markdown_bytes)
        validate_artifact_pair(note, note_path, load(transcript_path))

        worker = WorkerProcess(self.root, self.manifest)
        try:
            result = worker.request(
                "note.inspect",
                {
                    "meeting_id": meeting_id,
                    "note_id": note_id,
                    "transcript_id": transcript_id,
                },
            )
            self.assertTrue(result["ok"])
            self.assertEqual(result["artifact_digests"]["note"], digest(note_path))
            self.assertEqual(
                result["artifact_digests"]["note-markdown"], digest(markdown_path)
            )
            self.assertEqual(result["artifact_digests"]["transcript"], transcript_id)

            refused = worker.request(
                "note.inspect",
                {
                    "meeting_id": meeting_id,
                    "note_id": str(uuid.uuid4()),
                    "transcript_id": transcript_id,
                },
            )
            self.assertFalse(refused["ok"])
            self.assertEqual(refused["code"], "protocol_failure")
            self.assertEqual(refused["artifact_digests"], {})

            wrong_meeting = json.loads(json.dumps(note))
            wrong_meeting["meeting"]["id"] = str(uuid.uuid4())
            wrong_meeting_bytes = (
                json.dumps(wrong_meeting, ensure_ascii=False, indent=2) + "\n"
            ).encode()
            wrong_meeting_id = hashlib.sha256(wrong_meeting_bytes).hexdigest()
            private_file(notes_dir / f"{wrong_meeting_id}.json", wrong_meeting_bytes)
            refused = worker.request(
                "note.inspect",
                {
                    "meeting_id": meeting_id,
                    "note_id": wrong_meeting_id,
                    "transcript_id": transcript_id,
                },
            )
            self.assertFalse(refused["ok"])
            self.assertEqual(refused["code"], "protocol_failure")
            self.assertEqual(refused["artifact_digests"], {})

            rejected = json.loads(json.dumps(note))
            rejected["checks"]["context"]["ok"] = False
            rejected["passed"] = False
            rejected_bytes = (
                json.dumps(rejected, ensure_ascii=False, indent=2) + "\n"
            ).encode()
            rejected_id = hashlib.sha256(rejected_bytes).hexdigest()
            private_file(notes_dir / f"{rejected_id}.json", rejected_bytes)
            refused = worker.request(
                "note.inspect",
                {
                    "meeting_id": meeting_id,
                    "note_id": rejected_id,
                    "transcript_id": transcript_id,
                },
            )
            self.assertFalse(refused["ok"])
            self.assertEqual(refused["code"], "protocol_failure")
            self.assertEqual(refused["artifact_digests"], {})

            changed_locator = json.loads(json.dumps(note))
            changed_locator["claims"][0]["turn"] += 1
            changed_bytes = (
                json.dumps(changed_locator, ensure_ascii=False, indent=2) + "\n"
            ).encode()
            changed_id = hashlib.sha256(changed_bytes).hexdigest()
            private_file(notes_dir / f"{changed_id}.json", changed_bytes)
            refused = worker.request(
                "note.inspect",
                {
                    "meeting_id": meeting_id,
                    "note_id": changed_id,
                    "transcript_id": transcript_id,
                },
            )
            self.assertFalse(refused["ok"])
            self.assertEqual(refused["code"], "protocol_failure")
            self.assertEqual(refused["artifact_digests"], {})
        finally:
            worker.close()

    def test_profile_adoption_matches_strict_loader(self) -> None:
        self.require_worker_operation("profile.adopt")
        profile_id = str(uuid.uuid4())
        candidate_dir = self.root / "profile-candidates" / profile_id
        candidate_dir.mkdir(mode=0o700, parents=True)
        candidate_dir.parent.chmod(0o700)
        profile_path = candidate_dir / "voiceprint.json"
        centroid = np.zeros(192)
        centroid[0] = 1.0
        save_profile(
            profile_path,
            Profile(
                centroid=centroid,
                n_enrolled=100,
                n_excluded=0,
                seconds=400.0,
                cohesion=0.8,
                spread=0.1,
            ),
            selected_target=0.05,
            operator_scores=np.linspace(0.60, 0.90, 100).tolist(),
            negative_scores=np.linspace(0.20, 0.50, 20).tolist(),
            held_out="leave-one-sitting-out",
            sittings=[
                {
                    "audio": "a.wav",
                    "audio_sha256": "a" * 64,
                    "captured_at": "2026-07-20T09:00:00+0000",
                },
                {
                    "audio": "b.wav",
                    "audio_sha256": "b" * 64,
                    "captured_at": "2026-07-22T14:00:00+0000",
                },
            ],
            negative_sources=[
                {
                    "source_class": "public-or-licensed",
                    "audio": "negative.wav",
                    "segments": "negative-segments.json",
                    "audio_sha256": "c" * 64,
                    "audio_samples": 1_280_000,
                    "segments_sha256": "d" * 64,
                    "segments_schema": "mic-segments/1",
                    "captured_at": "2026-07-22T15:00:00+0000",
                    "scorable_segments": 20,
                    "scorable_seconds": 80.0,
                }
            ],
            encoder_fingerprint_value=self.encoder_digest,
        )
        load_profile(profile_path, expected_encoder_fingerprint=self.encoder_digest)
        expected_digest = digest(profile_path)

        worker = WorkerProcess(self.root, self.manifest)
        try:
            unsafe_mode_id = str(uuid.uuid4())
            unsafe_mode_dir = candidate_dir.parent / unsafe_mode_id
            unsafe_mode_dir.mkdir(mode=0o700)
            unsafe_mode_path = unsafe_mode_dir / "voiceprint.json"
            private_file(unsafe_mode_path, profile_path.read_bytes())
            unsafe_mode_path.chmod(0o644)
            refused = worker.request(
                "profile.adopt", {"profile_id": unsafe_mode_id}
            )
            self.assertFalse(refused["ok"])
            self.assertEqual(refused["artifact_digests"], {})
            self.assertFalse(unsafe_mode_dir.exists())
            self.assertFalse((self.root / "profile" / "voiceprint.json").exists())

            overflow_id = str(uuid.uuid4())
            overflow_dir = candidate_dir.parent / overflow_id
            overflow_dir.mkdir(mode=0o700)
            overflow_bytes = profile_path.read_bytes().replace(
                b'"n_enrolled": 100', b'"n_enrolled": 1e10000', 1
            )
            self.assertNotEqual(overflow_bytes, profile_path.read_bytes())
            private_file(overflow_dir / "voiceprint.json", overflow_bytes)
            refused = worker.request(
                "profile.inspect", {"profile_id": overflow_id}
            )
            self.assertFalse(refused["ok"])
            self.assertEqual(refused["artifact_digests"], {})

            # The same worker remains available after load_profile raises an
            # OverflowError while converting the hostile numeric count.
            inspected = worker.request("profile.inspect", {"profile_id": profile_id})
            self.assertTrue(inspected["ok"])
            self.assertEqual(inspected["artifact_digests"]["profile"], expected_digest)
            adopted = worker.request("profile.adopt", {"profile_id": profile_id})
            self.assertTrue(adopted["ok"])
            self.assertEqual(adopted["artifact_digests"]["profile"], expected_digest)
            adopted_path = self.root / "profile" / "voiceprint.json"
            self.assertEqual(digest(adopted_path), expected_digest)
            load_profile(adopted_path, expected_encoder_fingerprint=self.encoder_digest)
            self.assertFalse(candidate_dir.exists())
        finally:
            worker.close()

    def test_profile_adoption_refuses_experimental_calibration(self) -> None:
        self.require_worker_operation("profile.adopt")
        profile_id = str(uuid.uuid4())
        candidate_dir = self.root / "profile-candidates" / profile_id
        candidate_dir.mkdir(mode=0o700, parents=True)
        candidate_dir.parent.chmod(0o700)
        profile_path = candidate_dir / "voiceprint.json"
        centroid = np.zeros(192)
        centroid[0] = 1.0
        save_profile(
            profile_path,
            Profile(
                centroid=centroid,
                n_enrolled=100,
                n_excluded=0,
                seconds=400.0,
                cohesion=0.8,
                spread=0.1,
            ),
            selected_target=0.05,
            operator_scores=np.linspace(0.60, 0.90, 100).tolist(),
            negative_scores=np.linspace(0.20, 0.50, 20).tolist(),
            held_out="leave-one-sitting-out",
            sittings=[
                {
                    "audio": "a.wav",
                    "audio_sha256": "a" * 64,
                    "captured_at": "2026-07-20T09:00:00+0000",
                },
                {
                    "audio": "b.wav",
                    "audio_sha256": "b" * 64,
                    "captured_at": "2026-07-22T14:00:00+0000",
                },
            ],
            negative_sources=[
                {
                    "source_class": "public-or-licensed",
                    "audio": "negative.wav",
                    "segments": "negative-segments.json",
                    "audio_sha256": "c" * 64,
                    "audio_samples": 1_280_000,
                    "segments_sha256": "d" * 64,
                    "segments_schema": "mic-segments/1",
                    "captured_at": "2026-07-22T15:00:00+0000",
                    "scorable_segments": 20,
                    "scorable_seconds": 80.0,
                }
            ],
            experimental=True,
            encoder_fingerprint_value=self.encoder_digest,
        )
        worker = WorkerProcess(self.root, self.manifest)
        try:
            result = worker.request("profile.adopt", {"profile_id": profile_id})
            self.assertFalse(result["ok"])
            self.assertEqual(result["artifact_digests"], {})
            self.assertFalse((self.root / "profile" / "voiceprint.json").exists())
            self.assertFalse(candidate_dir.exists())
        finally:
            worker.close()

    def test_profile_discard_is_digest_bound_and_idempotent(self) -> None:
        self.require_worker_operation("profile.discard")
        profile_id = str(uuid.uuid4())
        candidate_dir = self.root / "profile-candidates" / profile_id
        candidate_dir.mkdir(mode=0o700, parents=True)
        candidate_dir.parent.chmod(0o700)
        profile_path = candidate_dir / "voiceprint.json"
        profile_bytes = b"synthetic-profile-candidate"
        private_file(profile_path, profile_bytes)
        expected_digest = hashlib.sha256(profile_bytes).hexdigest()

        worker = WorkerProcess(self.root, self.manifest)
        try:
            refused = worker.request(
                "profile.discard",
                {"profile_id": profile_id, "profile_sha256": "0" * 64},
            )
            self.assertFalse(refused["ok"])
            self.assertEqual(profile_path.read_bytes(), profile_bytes)

            discarded = worker.request(
                "profile.discard",
                {"profile_id": profile_id, "profile_sha256": expected_digest},
            )
            self.assertTrue(discarded["ok"])
            self.assertEqual(discarded["artifact_digests"], {"profile": expected_digest})
            self.assertFalse(candidate_dir.exists())

            repeated = worker.request(
                "profile.discard",
                {"profile_id": profile_id, "profile_sha256": expected_digest},
            )
            self.assertTrue(repeated["ok"])
            self.assertEqual(repeated["artifact_digests"], {"profile": expected_digest})
        finally:
            worker.close()

    def test_malformed_profile_top_levels_do_not_crash_worker(self) -> None:
        candidates = self.root / "profile-candidates"
        candidates.mkdir(mode=0o700)
        meeting_id = str(uuid.uuid4())
        build_capture(self.root / "meetings" / meeting_id / "capture")
        worker = WorkerProcess(self.root, self.manifest)
        try:
            for payload in (b"[]\n", b"null\n", b"1\n", b'"text"\n'):
                profile_id = str(uuid.uuid4())
                candidate_dir = candidates / profile_id
                candidate_dir.mkdir(mode=0o700)
                private_file(candidate_dir / "voiceprint.json", payload)
                refused = worker.request(
                    "profile.inspect", {"profile_id": profile_id}
                )
                self.assertFalse(refused["ok"])
                self.assertEqual(refused["artifact_digests"], {})

            # A subsequent successful request proves the singleton process did
            # not die on AttributeError/TypeError from the canonical loader.
            healthy = worker.request("capture.inspect", {"meeting_id": meeting_id})
            self.assertTrue(healthy["ok"])
        finally:
            worker.close()

    def test_parent_liveness_eof_stops_worker(self) -> None:
        worker = WorkerProcess(self.root, self.manifest)
        os.close(worker.write_fd)
        worker.closed = True
        worker.close()

    def test_parent_liveness_watchdog_interrupts_blocked_work(self) -> None:
        read_fd, write_fd = os.pipe()
        script = """
import sys
import time
from worker.main import start_parent_liveness_watchdog
start_parent_liveness_watchdog(int(sys.argv[1]))
while True:
    time.sleep(60)
"""
        process = subprocess.Popen(
            [sys.executable, "-c", script, str(read_fd)],
            cwd=REPO,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            pass_fds=(read_fd,),
        )
        os.close(read_fd)
        os.close(write_fd)
        process.wait(timeout=30)
        error = process.stderr.read().decode()
        process.stderr.close()
        self.assertEqual(process.returncode, 0, error)

    def test_progress_helper_emits_the_closed_shape(self) -> None:
        from worker.main import emit_progress

        request_id = str(uuid.uuid4())
        meeting_id = str(uuid.uuid4())
        output = io.StringIO()
        with mock.patch("worker.main.sys.stdout", output):
            emit_progress(request_id, meeting_id, "recording")
        self.assertEqual(
            json.loads(output.getvalue()),
            {
                "schema": "worker-event/2",
                "request_id": request_id,
                "event": "capture.state",
                "state": "recording",
                "meeting_id": meeting_id,
            },
        )
        output = io.StringIO()
        with mock.patch("worker.main.sys.stdout", output):
            emit_progress(request_id, meeting_id, "transcribing")
        self.assertEqual(json.loads(output.getvalue())["state"], "transcribing")
        output = io.StringIO()
        with mock.patch("worker.main.sys.stdout", output):
            emit_progress(request_id, meeting_id, "finishing")
        self.assertEqual(json.loads(output.getvalue())["state"], "finishing")
        with self.assertRaises(ValueError):
            emit_progress(request_id, meeting_id, "arming")

    def test_transcription_heartbeat_uses_the_protocol_stream(self) -> None:
        from worker.main import start_transcription_heartbeat

        request_id = str(uuid.uuid4())
        meeting_id = str(uuid.uuid4())
        protocol = io.StringIO()
        with mock.patch("worker.main.TRANSCRIPTION_HEARTBEAT_SECONDS", 0.01):
            heartbeat = start_transcription_heartbeat(
                request_id,
                "transcript.create",
                {"meeting_id": meeting_id},
                protocol_output=protocol,
            )
            self.assertIsNotNone(heartbeat)
            stopped, thread = heartbeat
            try:
                for _ in range(1000):
                    if protocol.getvalue():
                        break
                    threading.Event().wait(0.01)
                self.assertTrue(protocol.getvalue())
            finally:
                stopped.set()
                thread.join()

        self.assertEqual(
            json.loads(protocol.getvalue().splitlines()[0]),
            {
                "schema": "worker-event/2",
                "request_id": request_id,
                "event": "capture.state",
                "state": "transcribing",
                "meeting_id": meeting_id,
            },
        )

    def test_transcription_heartbeat_is_not_started_for_note_create(self) -> None:
        from worker.main import start_transcription_heartbeat

        heartbeat = start_transcription_heartbeat(
            str(uuid.uuid4()),
            "note.create",
            {"meeting_id": str(uuid.uuid4())},
            protocol_output=io.StringIO(),
        )
        self.assertIsNone(heartbeat)

    def test_transcription_heartbeat_is_started_for_transcript_retry(self) -> None:
        from worker.main import start_transcription_heartbeat

        heartbeat = start_transcription_heartbeat(
            str(uuid.uuid4()),
            "transcript.retry",
            {"meeting_id": str(uuid.uuid4())},
            protocol_output=io.StringIO(),
        )
        self.assertIsNotNone(heartbeat)
        stopped, thread = heartbeat
        stopped.set()
        thread.join()

    def test_note_generation_heartbeat_uses_the_protocol_stream(self) -> None:
        from worker.main import start_note_generation_heartbeat

        request_id = str(uuid.uuid4())
        meeting_id = str(uuid.uuid4())
        protocol = io.StringIO()
        with mock.patch("worker.main.NOTE_GENERATION_HEARTBEAT_SECONDS", 0.01):
            heartbeat = start_note_generation_heartbeat(
                request_id,
                "note.create",
                {"meeting_id": meeting_id},
                protocol_output=protocol,
            )
            self.assertIsNotNone(heartbeat)
            stopped, thread = heartbeat
            try:
                for _ in range(1000):
                    if protocol.getvalue():
                        break
                    threading.Event().wait(0.01)
                self.assertTrue(protocol.getvalue())
            finally:
                stopped.set()
                thread.join()

        self.assertEqual(
            json.loads(protocol.getvalue().splitlines()[0]),
            {
                "schema": "worker-event/2",
                "request_id": request_id,
                "event": "capture.state",
                "state": "finishing",
                "meeting_id": meeting_id,
            },
        )

    def test_note_generation_heartbeat_is_not_started_for_transcript_create(self) -> None:
        from worker.main import start_note_generation_heartbeat

        heartbeat = start_note_generation_heartbeat(
            str(uuid.uuid4()),
            "transcript.create",
            {"meeting_id": str(uuid.uuid4())},
            protocol_output=io.StringIO(),
        )
        self.assertIsNone(heartbeat)

    def test_operation_stdout_cannot_enter_protocol(self) -> None:
        from worker.main import dispatch_without_protocol_output

        def noisy_dispatch(*_args, **_kwargs):
            print("not a protocol frame")
            return {"transcript": "a" * 64}

        protocol = io.StringIO()
        with mock.patch("worker.main.dispatch", side_effect=noisy_dispatch):
            with mock.patch("worker.main.sys.stdout", protocol):
                result = dispatch_without_protocol_output(
                    self.root,
                    "transcript.create",
                    {"meeting_id": str(uuid.uuid4())},
                    encoder_digest=self.encoder_digest,
                    admission="internal-alpha",
                    model_dir=None,
                )

        self.assertEqual(result, {"transcript": "a" * 64})
        self.assertEqual(protocol.getvalue(), "")


class ProductOperationContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.fixture = json.loads(
            (REPO / "tests/fixtures/product-operations-v1.json").read_text(
                encoding="utf-8"
            )
        )

    def test_shared_fixture_matches_closed_worker_schemas(self) -> None:
        restoration = self.fixture["restoration"]
        restore_arguments, view, restore_digests = validate_transcript_restore_join(
            restoration["worker_arguments"],
            restoration["view"],
            restoration["worker_artifact_digests"],
        )
        self.assertEqual(
            transcript_view_digest(view), restore_digests["transcript"]
        )
        self.assertEqual(
            transcript_view_digest(dict(reversed(list(view.items())))),
            restore_digests["transcript"],
        )
        self.assertEqual(
            view["parent_transcript_sha256"],
            restore_arguments["source_transcript_sha256"],
        )
        self.assertEqual(
            view["restored_source_turn_indices"],
            [restore_arguments["source_turn_index"]],
        )

        accepted = self.fixture["accepted_note"]
        note = accepted["result"]["note"]
        validate_note_create_join(
            accepted["worker_arguments"],
            accepted["worker_artifact_digests"],
            note_sha256=note["json"]["sha256"],
            markdown_sha256=note["markdown"]["sha256"],
        )
        rejected = self.fixture["rejected_note"]
        validate_note_create_arguments(rejected["worker_arguments"])
        self.assertEqual(
            validate_note_create_error(rejected["worker_error"]), "note-rejected"
        )

    def test_worker_schemas_refuse_unknown_fields_and_drift(self) -> None:
        cases = [
            (
                validate_transcript_restore_arguments,
                self.fixture["restoration"]["worker_arguments"],
            ),
            (validate_transcript_view, self.fixture["restoration"]["view"]),
            (
                validate_note_create_arguments,
                self.fixture["accepted_note"]["worker_arguments"],
            ),
        ]
        for validator, source in cases:
            changed = dict(source)
            changed["unexpected"] = True
            with self.subTest(validator=validator.__name__):
                with self.assertRaises(ProductContractRefused):
                    validator(changed)

        changed_turn = dict(self.fixture["restoration"]["worker_arguments"])
        changed_turn["source_turn_index"] = True
        with self.assertRaises(ProductContractRefused):
            validate_transcript_restore_arguments(changed_turn)

        changed_turn["source_turn_index"] = MAX_SOURCE_TURN_INDEX
        validate_transcript_restore_arguments(changed_turn)
        changed_turn["source_turn_index"] = MAX_SOURCE_TURN_INDEX + 1
        with self.assertRaises(ProductContractRefused):
            validate_transcript_restore_arguments(changed_turn)

        changed_view = dict(self.fixture["restoration"]["view"])
        changed_view["restored_source_turn_indices"] = [7, 7]
        with self.assertRaises(ProductContractRefused):
            validate_transcript_view(changed_view)
        changed_view["restored_source_turn_indices"] = [MAX_SOURCE_TURN_INDEX]
        validate_transcript_view(changed_view)
        changed_view["restored_source_turn_indices"] = [MAX_SOURCE_TURN_INDEX + 1]
        with self.assertRaises(ProductContractRefused):
            validate_transcript_view(changed_view)

        extra_restore_digest = dict(
            self.fixture["restoration"]["worker_artifact_digests"]
        )
        extra_restore_digest["unexpected"] = "a" * 64
        with self.assertRaises(ProductContractRefused):
            validate_transcript_restore_digests(extra_restore_digest, "a" * 64)

        changed_note_digest = dict(
            self.fixture["accepted_note"]["worker_artifact_digests"]
        )
        changed_note_digest["transcript"] = "b" * 64
        with self.assertRaises(ProductContractRefused):
            validate_note_create_digests(
                changed_note_digest,
                self.fixture["accepted_note"]["worker_arguments"][
                    "source_transcript_sha256"
                ],
            )

        restoration = self.fixture["restoration"]
        for key in ("base-transcript", "parent-transcript", "transcript"):
            changed = dict(restoration["worker_artifact_digests"])
            changed[key] = "b" * 64
            with self.subTest(restore_digest=key):
                with self.assertRaises(ProductContractRefused):
                    validate_transcript_restore_join(
                        restoration["worker_arguments"],
                        restoration["view"],
                        changed,
                    )

        accepted = self.fixture["accepted_note"]
        note = accepted["result"]["note"]
        for key in ("note", "note-markdown", "transcript"):
            changed = dict(accepted["worker_artifact_digests"])
            changed[key] = "b" * 64
            with self.subTest(note_digest=key):
                with self.assertRaises(ProductContractRefused):
                    validate_note_create_join(
                        accepted["worker_arguments"],
                        changed,
                        note_sha256=note["json"]["sha256"],
                        markdown_sha256=note["markdown"]["sha256"],
                    )

        for field, value in (
            ("code", "protocol_failure"),
            ("recoverable", False),
            ("artifact_digests", {"note": "a" * 64}),
            ("unexpected", True),
        ):
            changed = dict(self.fixture["rejected_note"]["worker_error"])
            changed[field] = value
            with self.subTest(rejected_field=field):
                with self.assertRaises(ProductContractRefused):
                    validate_note_create_error(changed)

    def test_note_create_overlay_is_exact_and_bounded(self) -> None:
        arguments = dict(self.fixture["accepted_note"]["worker_arguments"])
        arguments["speaker_label_overrides"] = [
            {"source_speaker": "Them", "replacement": "Alex"}
        ]
        validate_note_create_arguments(arguments)

        for override in (
            [{"source_speaker": "Other", "replacement": "Alex"}],
            [{"source_speaker": "Them", "replacement": " Alex "}],
            [
                {"source_speaker": "Them", "replacement": "Alex"},
                {"source_speaker": "Me", "replacement": "Nino"},
            ],
        ):
            changed = dict(arguments)
            changed["speaker_label_overrides"] = override
            with self.subTest(override=override), self.assertRaises(ProductContractRefused):
                validate_note_create_arguments(changed)


class ProductArtifactAdapterTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name) / "app"
        self.root.mkdir(mode=0o700)
        self.root = self.root.resolve()

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def _bind_current(self, meeting_id: str, transcript_id: str) -> None:
        meeting_dir = self.root / "meetings" / meeting_id
        meeting_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        meeting = {
            "schema": "meeting/2",
            "meeting_id": meeting_id,
            "artifacts": {
                "current_transcript": {
                    "relative_path": f"transcript/{transcript_id}.json",
                    "sha256": transcript_id,
                }
            },
        }
        private_file(
            meeting_dir / "meeting.json",
            (json.dumps(meeting, indent=2) + "\n").encode(),
        )

    def _write_gated_transcript(self, meeting_id: str) -> tuple[Path, str]:
        capture = self.root / "meetings" / meeting_id / "capture"
        build_capture(capture)
        source = json.loads((capture / "transcript.json").read_text())
        source["turns"][1]["gated"] = True
        data = (json.dumps(source, indent=2) + "\n").encode()
        transcript_id = hashlib.sha256(data).hexdigest()
        target = self.root / "meetings" / meeting_id / "transcript" / (
            transcript_id + ".json"
        )
        target.parent.mkdir(mode=0o700)
        private_file(target, data)
        self._bind_current(meeting_id, transcript_id)
        return target, transcript_id

    def _write_note_transcript(self, meeting_id: str) -> tuple[Path, str, dict]:
        pack = json.loads(
            (REPO / "docs/prototype/fixtures/accepted-note2.fixture").read_text()
        )
        data = (json.dumps(pack["transcript"], indent=2) + "\n").encode()
        transcript_id = hashlib.sha256(data).hexdigest()
        target = self.root / "meetings" / meeting_id / "transcript" / (
            transcript_id + ".json"
        )
        target.parent.mkdir(mode=0o700, parents=True)
        private_file(target, data)
        self._bind_current(meeting_id, transcript_id)
        return target, transcript_id, pack

    def test_restore_creates_a_text_free_one_turn_view_and_resolves_it(self) -> None:
        meeting_id = str(uuid.uuid4())
        base, source_id = self._write_gated_transcript(meeting_id)
        before = base.read_bytes()
        result = transcript_restore(
            self.root,
            {
                "meeting_id": meeting_id,
                "source_transcript_sha256": source_id,
                "source_turn_index": 1,
            },
        )
        view_path = self.root / "meetings" / meeting_id / "transcript" / (
            result["transcript"] + ".json"
        )
        view = json.loads(view_path.read_text())
        self.assertEqual(set(view), {
            "schema",
            "meeting_id",
            "base_transcript_sha256",
            "parent_transcript_sha256",
            "restored_source_turn_indices",
        })
        self.assertNotIn("text", view_path.read_text())
        self.assertEqual(base.read_bytes(), before)
        resolved = resolve_transcript(self.root, meeting_id, result["transcript"])
        self.assertEqual([turn.text for turn in resolved.turns], ["fixture one", "fixture two"])
        self.assertEqual(resolved.gated_turns, [])
        view_before = view_path.read_bytes()
        retried = transcript_restore(
            self.root,
            {
                "meeting_id": meeting_id,
                "source_transcript_sha256": source_id,
                "source_turn_index": 1,
            },
        )
        self.assertEqual(retried, result)
        self.assertEqual(view_path.read_bytes(), view_before)

    def test_restore_repairs_an_identical_orphan_but_refuses_collision_bytes(self) -> None:
        meeting_id = str(uuid.uuid4())
        _, source_id = self._write_gated_transcript(meeting_id)
        view = {
            "schema": "transcript-view/1",
            "meeting_id": meeting_id,
            "base_transcript_sha256": source_id,
            "parent_transcript_sha256": source_id,
            "restored_source_turn_indices": [1],
        }
        view_bytes = json.dumps(view, indent=2).encode()
        view_id = transcript_view_digest(view)
        view_path = self.root / "meetings" / meeting_id / "transcript" / (view_id + ".json")
        private_file(view_path, view_bytes)
        result = transcript_restore(
            self.root,
            {
                "meeting_id": meeting_id,
                "source_transcript_sha256": source_id,
                "source_turn_index": 1,
            },
        )
        self.assertEqual(result["transcript"], view_id)
        self.assertEqual(view_path.read_bytes(), view_bytes)

        collision_meeting = str(uuid.uuid4())
        _, collision_source = self._write_gated_transcript(collision_meeting)
        collision_view = {
            **view,
            "meeting_id": collision_meeting,
            "base_transcript_sha256": collision_source,
            "parent_transcript_sha256": collision_source,
        }
        collision_id = transcript_view_digest(collision_view)
        collision_path = self.root / "meetings" / collision_meeting / "transcript" / (
            collision_id + ".json"
        )
        private_file(collision_path, b"different bytes")
        with self.assertRaises(AdapterRefused):
            transcript_restore(
                self.root,
                {
                    "meeting_id": collision_meeting,
                    "source_transcript_sha256": collision_source,
                    "source_turn_index": 1,
                },
            )
        self.assertEqual(collision_path.read_bytes(), b"different bytes")

    def test_restore_refuses_stale_or_non_withheld_sources_without_mutation(self) -> None:
        meeting_id = str(uuid.uuid4())
        base, source_id = self._write_gated_transcript(meeting_id)
        before = base.read_bytes()
        for arguments in (
            {
                "meeting_id": meeting_id,
                "source_transcript_sha256": "a" * 64,
                "source_turn_index": 1,
            },
            {
                "meeting_id": meeting_id,
                "source_transcript_sha256": source_id,
                "source_turn_index": 0,
            },
        ):
            with self.subTest(arguments=arguments), self.assertRaises(AdapterRefused):
                transcript_restore(self.root, arguments)
        self.assertEqual(base.read_bytes(), before)
        self.assertEqual(list(base.parent.glob("*.json")), [base])

    def test_note_publication_accepts_only_exact_passing_pair_without_overwrite(self) -> None:
        meeting_id = str(uuid.uuid4())
        _, transcript_id, pack = self._write_note_transcript(meeting_id)

        def accepted(_transcript):
            note = json.loads(json.dumps(pack["note"]))
            note["meeting"]["id"] = meeting_id
            note["transcript"] = f"../transcript/{transcript_id}.json"
            markdown_id = hashlib.sha256(pack["markdown"].encode()).hexdigest()
            note["render"]["path"] = f"{markdown_id}.md"
            return note

        result = note_create(
            self.root,
            {
                "meeting_id": meeting_id,
                "source_transcript_sha256": transcript_id,
            },
            generator=accepted,
        )
        notes = self.root / "meetings" / meeting_id / "notes"
        note_path = notes / (result["note"] + ".json")
        markdown_path = notes / (result["note-markdown"] + ".md")
        self.assertTrue(note_path.is_file())
        self.assertTrue(markdown_path.is_file())
        self.assertEqual(stat.S_IMODE(note_path.stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(markdown_path.stat().st_mode), 0o600)
        self.assertNotEqual(result["note"], result["note-markdown"])
        self.assertEqual(result["transcript"], transcript_id)
        note_before, markdown_before = note_path.read_bytes(), markdown_path.read_bytes()
        retried = note_create(
            self.root,
            {
                "meeting_id": meeting_id,
                "source_transcript_sha256": transcript_id,
            },
            generator=accepted,
        )
        self.assertEqual(retried, result)
        self.assertEqual(note_path.read_bytes(), note_before)
        self.assertEqual(markdown_path.read_bytes(), markdown_before)

    def test_note_create_dispatch_assembles_and_publishes_from_generation_points(self) -> None:
        """The product path end-to-end: generation points in, published pair out.

        The dispatched arguments carry the sandboxed generate child's payload;
        the registry builds the deterministic assembler from it, and the
        published note/2 must replay through the artifact validator with every
        claim typed `point`. The bare argument shape keeps its original
        refusal in the same registry entry.
        """
        import candidate_first
        from summarize import build_fragment_map, structured_artifact_citations

        meeting_id = str(uuid.uuid4())
        _, transcript_id, _pack = self._write_note_transcript(meeting_id)
        transcript = resolve_transcript(self.root, meeting_id, transcript_id)
        manifest = candidate_first.generate_manifest(
            transcript,
            candidate_first.STRATEGY_BROAD,
            contract=candidate_first.PRODUCT_CONTRACT,
        )
        fragments = {
            fragment["source_fragment_id"]: fragment
            for fragment in build_fragment_map(transcript)["fragments"]
        }
        kept = manifest["candidates"][:2]
        self.assertEqual(len(kept), 2, "fixture transcript must offer candidates")
        points = []
        for index, candidate in enumerate(kept):
            anchor = fragments[candidate["anchor_fragment_id"]]
            points.append({
                "point_ordinal": index,
                "candidate_id": candidate["candidate_id"],
                "evidence_state": "located",
                "locators": [{
                    "turn": anchor["turn"],
                    "start": anchor["char_start"],
                    "end": anchor["char_end"],
                    "text_sha256": anchor["text_sha256"],
                }],
            })
        arguments = {
            "meeting_id": meeting_id,
            "source_transcript_sha256": transcript_id,
            "generation": {
                "schema": "note-generation/1",
                "transcript_sha256": transcript_id,
                "manifest_sha256": manifest["manifest_sha256"],
                "candidates": len(manifest["candidates"]),
                "points": points,
                "receipt": {
                    "responses": len(kept),
                    "response_bytes": 64,
                    "last_response_sha256": "0" * 64,
                    "elapsed_s": 12.5,
                },
            },
        }
        result = adapters.dispatch(
            self.root, "note.create", arguments, encoder_digest="0" * 64
        )
        notes = self.root / "meetings" / meeting_id / "notes"
        document = json.loads((notes / (result["note"] + ".json")).read_text())
        self.assertEqual(document["schema"], "note/2")
        self.assertIs(document["passed"], True)
        self.assertEqual(document["claim_evidence_contract"], "candidate-evidence/1")
        self.assertEqual(document["meeting"]["id"], meeting_id)
        citations = structured_artifact_citations(document, transcript)
        self.assertEqual(document["checks"]["citations"], citations)
        self.assertEqual(
            [row["type"] for row in citations["cited"]], ["point"] * len(kept)
        )
        for row, point in zip(citations["cited"], points, strict=True):
            locator = point["locators"][0]
            self.assertEqual(
                row["evidence_refs"][0]["text_sha256"], locator["text_sha256"]
            )
            self.assertEqual(
                row["claim"],
                transcript.turns[locator["turn"]].text[
                    locator["start"]:locator["end"]
                ],
            )
        with self.assertRaises(AdapterRefused):
            adapters.dispatch(
                self.root,
                "note.create",
                {
                    "meeting_id": meeting_id,
                    "source_transcript_sha256": transcript_id,
                },
                encoder_digest="0" * 64,
            )

    def test_note_create_dispatch_publishes_structured_generation_claims(self) -> None:
        import candidate_first
        from summarize import build_fragment_map, structured_artifact_citations

        meeting_id = str(uuid.uuid4())
        _, transcript_id, _pack = self._write_note_transcript(meeting_id)
        transcript = resolve_transcript(self.root, meeting_id, transcript_id)
        manifest = candidate_first.generate_manifest(
            transcript,
            candidate_first.STRATEGY_BROAD,
            contract=candidate_first.PRODUCT_CONTRACT,
        )
        fragments = {
            fragment["source_fragment_id"]: fragment
            for fragment in build_fragment_map(transcript)["fragments"]
        }
        kept = manifest["candidates"][:2]
        claims = []
        for index, (claim_type, candidate) in enumerate(
            zip(("summary", "proposal"), kept, strict=True)
        ):
            anchor = fragments[candidate["anchor_fragment_id"]]
            locator = {
                "turn": anchor["turn"],
                "start": anchor["char_start"],
                "end": anchor["char_end"],
                "text_sha256": anchor["text_sha256"],
            }
            claims.append({
                "claim_ordinal": index,
                "claim_type": claim_type,
                "claim": transcript.turns[locator["turn"]].text[
                    locator["start"]:locator["end"]
                ],
                "candidate_ids": [candidate["candidate_id"]],
                "evidence_state": "located",
                "locators": [locator],
            })
        arguments = {
            "meeting_id": meeting_id,
            "source_transcript_sha256": transcript_id,
            "generation": {
                "schema": "note-generation/2",
                "transcript_sha256": transcript_id,
                "manifest_sha256": manifest["manifest_sha256"],
                "candidates": len(manifest["candidates"]),
                "claims": claims,
                "receipt": {
                    "responses": len(kept) + 1,
                    "response_bytes": 128,
                    "last_response_sha256": "0" * 64,
                    "elapsed_s": 12.5,
                },
            },
        }
        result = adapters.dispatch(
            self.root, "note.create", arguments, encoder_digest="0" * 64
        )
        document = json.loads(
            (
                self.root
                / "meetings"
                / meeting_id
                / "notes"
                / (result["note"] + ".json")
            ).read_text()
        )
        self.assertIs(document["passed"], True)
        self.assertEqual(
            document["claim_evidence_contract"],
            "candidate-synthesis-evidence/1",
        )
        citations = structured_artifact_citations(document, transcript)
        self.assertEqual(
            [row["type"] for row in citations["cited"]],
            ["summary", "proposal"],
        )

    def test_note_publication_repairs_orphan_markdown_but_refuses_collision_bytes(self) -> None:
        meeting_id = str(uuid.uuid4())
        _, transcript_id, pack = self._write_note_transcript(meeting_id)

        def accepted(_transcript):
            note = json.loads(json.dumps(pack["note"]))
            note["meeting"]["id"] = meeting_id
            note["transcript"] = f"../transcript/{transcript_id}.json"
            markdown_id = hashlib.sha256(pack["markdown"].encode()).hexdigest()
            note["render"]["path"] = f"{markdown_id}.md"
            return note

        markdown_id = hashlib.sha256(pack["markdown"].encode()).hexdigest()
        notes = self.root / "meetings" / meeting_id / "notes"
        notes.mkdir(mode=0o700)
        markdown_path = notes / (markdown_id + ".md")
        private_file(markdown_path, pack["markdown"].encode())
        repaired = note_create(
            self.root,
            {
                "meeting_id": meeting_id,
                "source_transcript_sha256": transcript_id,
            },
            generator=accepted,
        )
        self.assertTrue((notes / (repaired["note"] + ".json")).is_file())
        self.assertEqual(markdown_path.read_bytes(), pack["markdown"].encode())

        collision_meeting = str(uuid.uuid4())
        _, collision_transcript, collision_pack = self._write_note_transcript(collision_meeting)

        def collision_candidate(_transcript):
            note = json.loads(json.dumps(collision_pack["note"]))
            note["meeting"]["id"] = collision_meeting
            note["transcript"] = f"../transcript/{collision_transcript}.json"
            note["render"]["path"] = f"{markdown_id}.md"
            return note

        collision_notes = self.root / "meetings" / collision_meeting / "notes"
        collision_notes.mkdir(mode=0o700)
        collision_markdown = collision_notes / (markdown_id + ".md")
        private_file(collision_markdown, b"different bytes")
        with self.assertRaises(AdapterRefused):
            note_create(
                self.root,
                {
                    "meeting_id": collision_meeting,
                    "source_transcript_sha256": collision_transcript,
                },
                generator=collision_candidate,
            )
        self.assertEqual(collision_markdown.read_bytes(), b"different bytes")
        self.assertFalse(list(collision_notes.glob("*.json")))

    def test_bounded_private_read_refuses_oversized_meeting_and_transcript(self) -> None:
        meeting_id = str(uuid.uuid4())
        meeting_dir = self.root / "meetings" / meeting_id
        meeting_dir.mkdir(mode=0o700, parents=True)
        private_file(
            meeting_dir / "meeting.json", b"x" * (MAX_MEETING_RECEIPT_BYTES + 1)
        )
        with self.assertRaises(AdapterRefused):
            transcript_restore(
                self.root,
                {
                    "meeting_id": meeting_id,
                    "source_transcript_sha256": "a" * 64,
                    "source_turn_index": 0,
                },
            )

        transcript_id = "a" * 64
        oversized_meeting = str(uuid.uuid4())
        transcript_dir = self.root / "meetings" / oversized_meeting / "transcript"
        transcript_dir.mkdir(mode=0o700, parents=True)
        private_file(
            transcript_dir / (transcript_id + ".json"),
            b"x" * (MAX_TRANSCRIPT_REVISION_BYTES + 1),
        )
        self._bind_current(oversized_meeting, transcript_id)
        with self.assertRaises(AdapterRefused):
            resolve_transcript(self.root, oversized_meeting, transcript_id)

    def test_note_publication_refuses_rejected_or_misbound_candidates_before_write(self) -> None:
        meeting_id = str(uuid.uuid4())
        _, transcript_id, pack = self._write_note_transcript(meeting_id)

        def rejected(_transcript):
            note = json.loads(json.dumps(pack["note"]))
            note["passed"] = False
            return note

        def wrong_source(_transcript):
            note = json.loads(json.dumps(pack["note"]))
            note["meeting"]["id"] = meeting_id
            note["transcript"] = "../transcript/" + "a" * 64 + ".json"
            return note

        for generator in (None, rejected, wrong_source):
            with self.subTest(generator=generator), self.assertRaises(AdapterRefused):
                note_create(
                    self.root,
                    {
                        "meeting_id": meeting_id,
                        "source_transcript_sha256": transcript_id,
                    },
                    generator=generator,
                )
        self.assertFalse((self.root / "meetings" / meeting_id / "notes").exists())


class ProfileAdoptionSafetyTests(unittest.TestCase):
    """The bridge accepts one private byte snapshot or changes nothing."""

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name) / "app"
        self.root.mkdir(mode=0o700)
        self.encoder_digest = "e" * 64

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def _candidate(self, profile_id: str, payload: bytes | None = None) -> Path:
        directory = self.root / "profile-candidates" / profile_id
        directory.mkdir(mode=0o700, parents=True)
        directory.parent.chmod(0o700)
        directory.chmod(0o700)
        path = directory / "voiceprint.json"
        if payload is not None:
            private_file(path, payload)
            return path
        centroid = np.zeros(192)
        centroid[0] = 1.0
        save_profile(
            path,
            Profile(
                centroid=centroid,
                n_enrolled=100,
                n_excluded=0,
                seconds=400.0,
                cohesion=0.8,
                spread=0.1,
            ),
            selected_target=0.05,
            operator_scores=np.linspace(0.60, 0.90, 100).tolist(),
            negative_scores=np.linspace(0.20, 0.50, 20).tolist(),
            held_out="leave-one-sitting-out",
            sittings=[
                {
                    "audio": "first.wav",
                    "audio_sha256": "a" * 64,
                    "captured_at": "2026-07-20T09:00:00+0000",
                },
                {
                    "audio": "second.wav",
                    "audio_sha256": "b" * 64,
                    "captured_at": "2026-07-22T14:00:00+0000",
                },
            ],
            negative_sources=[
                {
                    "source_class": "public-or-licensed",
                    "audio": "negative.wav",
                    "segments": "negative-segments.json",
                    "audio_sha256": "c" * 64,
                    "audio_samples": 1_280_000,
                    "segments_sha256": "d" * 64,
                    "segments_schema": "mic-segments/1",
                    "captured_at": "2026-07-22T15:00:00+0000",
                    "scorable_segments": 20,
                    "scorable_seconds": 80.0,
                }
            ],
            encoder_fingerprint_value=self.encoder_digest,
        )
        return path

    def _adopt(self, profile_id: str) -> dict[str, str]:
        return profile_adopt(
            self.root, {"profile_id": profile_id}, self.encoder_digest
        )

    def _valid_profile_bytes(self) -> bytes:
        profile_id = str(uuid.uuid4())
        path = self._candidate(profile_id)
        payload = path.read_bytes()
        path.unlink()
        path.parent.rmdir()
        path.parent.parent.rmdir()
        return payload

    def test_adopt_is_digest_bound_idempotent_and_consumes_quarantine(self) -> None:
        first_id = str(uuid.uuid4())
        first = self._candidate(first_id)
        expected = first.read_bytes()
        result = self._adopt(first_id)
        installed = self.root / "profile" / "voiceprint.json"
        self.assertEqual(result, {"profile": hashlib.sha256(expected).hexdigest()})
        self.assertEqual(installed.read_bytes(), expected)
        self.assertEqual(stat.S_IMODE(installed.stat().st_mode), 0o600)
        self.assertFalse(first.parent.exists())

        # Retrying after a crash uses a new quarantine copy, but it must bind to
        # the already installed exact bytes instead of overwriting them.
        second_id = str(uuid.uuid4())
        self._candidate(second_id, expected)
        self.assertEqual(self._adopt(second_id), result)
        self.assertEqual(installed.read_bytes(), expected)
        self.assertFalse((self.root / "profile-candidates" / second_id).exists())

    def test_adopt_sets_exact_private_modes_under_a_restrictive_umask(self) -> None:
        profile_id = str(uuid.uuid4())
        candidate = self._candidate(profile_id)
        expected = candidate.read_bytes()
        prior_umask = os.umask(0o777)
        try:
            result = self._adopt(profile_id)
        finally:
            os.umask(prior_umask)
        profile_dir = self.root / "profile"
        installed = profile_dir / "voiceprint.json"
        self.assertEqual(result, {"profile": hashlib.sha256(expected).hexdigest()})
        self.assertEqual(installed.read_bytes(), expected)
        self.assertEqual(stat.S_IMODE(profile_dir.stat().st_mode), 0o700)
        self.assertEqual(stat.S_IMODE(installed.stat().st_mode), 0o600)
        self.assertFalse(candidate.parent.exists())

    def test_adopt_refuses_missing_malformed_oversized_and_symlink_candidates(self) -> None:
        missing = str(uuid.uuid4())
        with self.assertRaises(AdapterRefused):
            self._adopt(missing)

        cases = (
            (b"{}\n", "malformed"),
            (b"x" * (MAX_PROFILE_BYTES + 1), "oversized"),
        )
        for payload, label in cases:
            with self.subTest(label=label):
                profile_id = str(uuid.uuid4())
                candidate = self._candidate(profile_id, payload)
                with self.assertRaises(AdapterRefused):
                    self._adopt(profile_id)
                self.assertFalse(candidate.parent.exists())
                self.assertFalse((self.root / "profile" / "voiceprint.json").exists())

        wrong_mode_id = str(uuid.uuid4())
        wrong_mode = self._candidate(wrong_mode_id)
        wrong_mode.chmod(0o644)
        with self.assertRaises(AdapterRefused):
            self._adopt(wrong_mode_id)
        self.assertFalse(wrong_mode.parent.exists())
        self.assertFalse((self.root / "profile" / "voiceprint.json").exists())

        profile_id = str(uuid.uuid4())
        candidate = self._candidate(profile_id, b"placeholder")
        candidate.unlink()
        external = self.root / "outside.json"
        private_file(external, b"{}\n")
        candidate.symlink_to(external)
        with self.assertRaises(AdapterRefused):
            self._adopt(profile_id)
        self.assertFalse(candidate.parent.exists())
        self.assertEqual(external.read_bytes(), b"{}\n")

    @unittest.skipUnless(hasattr(os, "mkfifo"), "FIFO probe requires mkfifo")
    def test_adopt_removes_a_nonregular_leaf_under_safe_parents(self) -> None:
        profile_id = str(uuid.uuid4())
        candidate_dir = self.root / "profile-candidates" / profile_id
        candidate_dir.mkdir(mode=0o700, parents=True)
        candidate_dir.parent.chmod(0o700)
        candidate = candidate_dir / "voiceprint.json"
        os.mkfifo(candidate, mode=0o600)
        candidate.chmod(0o600)
        with self.assertRaises(AdapterRefused):
            self._adopt(profile_id)
        self.assertFalse(candidate_dir.exists())
        self.assertFalse((self.root / "profile" / "voiceprint.json").exists())

    def test_adopt_removes_an_lstat_to_open_replacement(self) -> None:
        profile_id = str(uuid.uuid4())
        candidate = self._candidate(profile_id)
        replacement = candidate.with_name("replacement.json")
        private_file(replacement, candidate.read_bytes())
        original_open = adapters.os.open
        swapped = False

        def replace_before_open(
            path, flags: int, mode: int = 0o777, *, dir_fd: int | None = None
        ) -> int:
            nonlocal swapped
            if not swapped and path == "voiceprint.json" and dir_fd is not None:
                swapped = True
                os.replace(
                    replacement.name,
                    candidate.name,
                    src_dir_fd=dir_fd,
                    dst_dir_fd=dir_fd,
                )
            return original_open(path, flags, mode, dir_fd=dir_fd)

        with mock.patch.object(adapters.os, "open", side_effect=replace_before_open):
            with self.assertRaises(AdapterRefused):
                self._adopt(profile_id)
        self.assertTrue(swapped)
        self.assertFalse(candidate.parent.exists())
        self.assertFalse((self.root / "profile" / "voiceprint.json").exists())

    def test_inspect_and_adopt_refuse_nonprivate_quarantine_chain_pre_read(self) -> None:
        for unsafe_level in ("root", "candidate"):
            with self.subTest(unsafe_level=unsafe_level):
                profile_id = str(uuid.uuid4())
                candidate = self._candidate(profile_id)
                expected = candidate.read_bytes()
                unsafe = (
                    candidate.parent.parent
                    if unsafe_level == "root"
                    else candidate.parent
                )
                unsafe.chmod(0o755)
                with mock.patch.object(adapters, "_read_profile_candidate") as read:
                    with self.assertRaises(AdapterRefused):
                        profile_inspect(
                            self.root,
                            {"profile_id": profile_id},
                            self.encoder_digest,
                        )
                    with self.assertRaises(AdapterRefused):
                        self._adopt(profile_id)
                    read.assert_not_called()
                self.assertEqual(candidate.read_bytes(), expected)
                self.assertTrue(candidate.parent.exists())
                self.assertFalse(
                    (self.root / "profile" / "voiceprint.json").exists()
                )
                candidate.parent.parent.chmod(0o700)
                candidate.parent.chmod(0o700)

    def test_symlinked_quarantine_chain_is_never_read_or_cleaned(self) -> None:
        payload = self._valid_profile_bytes()
        outside_root = Path(self.temporary.name) / "outside-candidates"
        outside_root.mkdir(mode=0o700)
        profile_id = str(uuid.uuid4())
        outside_candidate = outside_root / profile_id
        outside_candidate.mkdir(mode=0o700)
        outside_profile = outside_candidate / "voiceprint.json"
        private_file(outside_profile, payload)

        candidates_link = self.root / "profile-candidates"
        candidates_link.symlink_to(outside_root, target_is_directory=True)
        with mock.patch.object(adapters, "_read_profile_candidate") as read:
            with self.assertRaises(AdapterRefused):
                profile_inspect(
                    self.root,
                    {"profile_id": profile_id},
                    self.encoder_digest,
                )
            with self.assertRaises(AdapterRefused):
                self._adopt(profile_id)
            read.assert_not_called()
        self.assertTrue(candidates_link.is_symlink())
        self.assertEqual(outside_profile.read_bytes(), payload)
        self.assertFalse((self.root / "profile" / "voiceprint.json").exists())

        candidates_link.unlink()
        candidates_link.mkdir(mode=0o700)
        child_link = candidates_link / profile_id
        child_link.symlink_to(outside_candidate, target_is_directory=True)
        with mock.patch.object(adapters, "_read_profile_candidate") as read:
            with self.assertRaises(AdapterRefused):
                profile_inspect(
                    self.root,
                    {"profile_id": profile_id},
                    self.encoder_digest,
                )
            with self.assertRaises(AdapterRefused):
                self._adopt(profile_id)
            read.assert_not_called()
        self.assertTrue(child_link.is_symlink())
        self.assertEqual(outside_profile.read_bytes(), payload)
        self.assertFalse((self.root / "profile" / "voiceprint.json").exists())

    def test_cleanup_refuses_unknown_nested_state_without_partial_removal(self) -> None:
        profile_id = str(uuid.uuid4())
        candidate = self._candidate(profile_id)
        expected = candidate.read_bytes()
        unexpected = candidate.parent / "unexpected"
        unexpected.mkdir(mode=0o700)
        sentinel = unexpected / "sentinel"
        private_file(sentinel, b"outside the closed quarantine shape")
        with self.assertRaises(AdapterRefused):
            self._adopt(profile_id)
        self.assertEqual(candidate.read_bytes(), expected)
        self.assertEqual(sentinel.read_bytes(), b"outside the closed quarantine shape")
        self.assertFalse((self.root / "profile" / "voiceprint.json").exists())

    def test_wrong_owner_seams_refuse_each_profile_boundary_without_mutation(self) -> None:
        profile_id = str(uuid.uuid4())
        candidate = self._candidate(profile_id)
        expected = candidate.read_bytes()
        boundaries = {
            "root": self.root,
            "quarantine-root": candidate.parent.parent,
            "quarantine": candidate.parent,
            "candidate": candidate,
        }
        for label, path in boundaries.items():
            with self.subTest(label=label):
                rejected_identity = (path.stat().st_dev, path.stat().st_ino)

                def owns(metadata: os.stat_result) -> bool:
                    return (metadata.st_dev, metadata.st_ino) != rejected_identity

                with mock.patch.object(
                    adapters, "_owned_by_effective_user", side_effect=owns
                ):
                    with self.assertRaises(AdapterRefused):
                        profile_inspect(
                            self.root,
                            {"profile_id": profile_id},
                            self.encoder_digest,
                        )
                    with self.assertRaises(AdapterRefused):
                        self._adopt(profile_id)
                self.assertEqual(candidate.read_bytes(), expected)
                self.assertFalse(
                    (self.root / "profile" / "voiceprint.json").exists()
                )

        with mock.patch.object(
            worker_storage, "_owned_by_effective_user", return_value=False
        ):
            with self.assertRaises(AdapterRefused):
                self._adopt(profile_id)
        self.assertEqual(candidate.read_bytes(), expected)

    def test_wrong_owner_installed_profile_is_not_reused_or_overwritten(self) -> None:
        installed_bytes = self._valid_profile_bytes()
        profile_dir = self.root / "profile"
        profile_dir.mkdir(mode=0o700)
        installed = profile_dir / "voiceprint.json"
        private_file(installed, installed_bytes)
        profile_id = str(uuid.uuid4())
        candidate = self._candidate(profile_id, installed_bytes)
        rejected_identity = (installed.stat().st_dev, installed.stat().st_ino)

        def owns(metadata: os.stat_result) -> bool:
            return (metadata.st_dev, metadata.st_ino) != rejected_identity

        with mock.patch.object(
            adapters, "_owned_by_effective_user", side_effect=owns
        ):
            with self.assertRaises(AdapterRefused):
                self._adopt(profile_id)
        self.assertEqual(installed.read_bytes(), installed_bytes)
        self.assertFalse(candidate.parent.exists())

    def test_unsafe_installed_profile_directory_refuses_before_candidate_cleanup(self) -> None:
        profile_dir = self.root / "profile"
        profile_dir.mkdir(mode=0o755)
        profile_id = str(uuid.uuid4())
        candidate = self._candidate(profile_id)
        expected = candidate.read_bytes()
        with self.assertRaises(AdapterRefused):
            self._adopt(profile_id)
        self.assertEqual(stat.S_IMODE(profile_dir.stat().st_mode), 0o755)
        self.assertEqual(candidate.read_bytes(), expected)
        self.assertFalse((profile_dir / "voiceprint.json").exists())

        profile_dir.rmdir()
        outside = Path(self.temporary.name) / "outside-profile"
        outside.mkdir(mode=0o700)
        sentinel = outside / "sentinel"
        private_file(sentinel, b"outside profile directory")
        profile_dir.symlink_to(outside, target_is_directory=True)
        second_id = str(uuid.uuid4())
        second = self._candidate(second_id)
        second_expected = second.read_bytes()
        with self.assertRaises(AdapterRefused):
            self._adopt(second_id)
        self.assertTrue(profile_dir.is_symlink())
        self.assertEqual(sentinel.read_bytes(), b"outside profile directory")
        self.assertEqual(second.read_bytes(), second_expected)

        profile_dir.unlink()
        profile_dir.mkdir(mode=0o700)
        third_id = str(uuid.uuid4())
        third = self._candidate(third_id)
        third_expected = third.read_bytes()
        rejected_identity = (profile_dir.stat().st_dev, profile_dir.stat().st_ino)

        def owns(metadata: os.stat_result) -> bool:
            return (metadata.st_dev, metadata.st_ino) != rejected_identity

        with mock.patch.object(
            adapters, "_owned_by_effective_user", side_effect=owns
        ):
            with self.assertRaises(AdapterRefused):
                self._adopt(third_id)
        self.assertEqual(third.read_bytes(), third_expected)
        self.assertFalse((profile_dir / "voiceprint.json").exists())

    def test_adopt_uses_the_validated_snapshot_when_candidate_changes(self) -> None:
        profile_id = str(uuid.uuid4())
        candidate = self._candidate(profile_id)
        expected = candidate.read_bytes()
        original_read = adapters._read_profile_candidate
        mutated = False

        def read_then_replace(quarantine) -> bytes:
            nonlocal mutated
            value = original_read(quarantine)
            if not mutated:
                mutated = True
                replacement = candidate.with_name("replacement.json")
                private_file(replacement, b"{}\n")
                os.replace(replacement, candidate)
            return value

        with mock.patch.object(
            adapters, "_read_profile_candidate", side_effect=read_then_replace
        ):
            result = self._adopt(profile_id)
        self.assertTrue(mutated)
        self.assertEqual(result["profile"], hashlib.sha256(expected).hexdigest())
        self.assertEqual(
            (self.root / "profile" / "voiceprint.json").read_bytes(), expected
        )
        self.assertFalse(candidate.parent.exists())

    def test_candidate_changed_during_read_is_refused_and_quarantine_removed(self) -> None:
        profile_id = str(uuid.uuid4())
        candidate = self._candidate(profile_id)
        original_read = adapters.os.read
        changed = False

        def read_then_change_mode(descriptor: int, count: int) -> bytes:
            nonlocal changed
            chunk = original_read(descriptor, count)
            if not changed:
                changed = True
                os.fchmod(descriptor, 0o400)
            return chunk

        with mock.patch.object(adapters.os, "read", side_effect=read_then_change_mode):
            with self.assertRaises(AdapterRefused):
                self._adopt(profile_id)
        self.assertTrue(changed)
        self.assertFalse(candidate.parent.exists())
        self.assertFalse((self.root / "profile" / "voiceprint.json").exists())

    def test_adopt_refuses_a_profile_from_another_encoder_space(self) -> None:
        profile_id = str(uuid.uuid4())
        candidate = self._candidate(profile_id)
        with self.assertRaises(AdapterRefused):
            profile_adopt(
                self.root, {"profile_id": profile_id}, "f" * 64
            )
        self.assertFalse(candidate.parent.exists())
        self.assertFalse((self.root / "profile" / "voiceprint.json").exists())

    def test_adopt_never_overwrites_a_different_installed_profile(self) -> None:
        initial_id = str(uuid.uuid4())
        initial = self._candidate(initial_id)
        initial_bytes = initial.read_bytes()
        self._adopt(initial_id)

        different_id = str(uuid.uuid4())
        # Whitespace changes preserve the valid JSON/profile semantics while
        # creating a different immutable artifact identity.
        different_bytes = initial_bytes.replace(b"\n", b" \n", 1)
        self._candidate(different_id, different_bytes)
        with self.assertRaises(AdapterRefused):
            self._adopt(different_id)
        installed = self.root / "profile" / "voiceprint.json"
        self.assertEqual(installed.read_bytes(), initial_bytes)
        self.assertFalse((self.root / "profile-candidates" / different_id).exists())


class SittingDerivationTests(unittest.TestCase):
    """The producer for the sitting evidence store's derivation seam.

    The core is exercised with injected transcription and embedding seams —
    synthetic audio only, no model, no encoder. The adapter's refusal ladder
    and the frozen-admission boundary are pinned separately.
    """

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = (Path(self.temporary.name) / "app").resolve()
        self.root.mkdir(mode=0o700)
        self.sitting_id = str(uuid.uuid4())
        self.work_dir = self.root / "enrollment" / "work" / self.sitting_id

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def _scaffold_sitting(self, seconds: float = 10.0) -> bytes:
        row = self.root / "enrollment" / "sittings" / self.sitting_id / "sitting.json"
        row.parent.mkdir(mode=0o700, parents=True)
        private_file(row, b"{}\n")
        self.work_dir.mkdir(mode=0o700, parents=True)
        t = np.arange(int(seconds * 16_000)) / 16_000
        audio = (np.sin(2 * np.pi * 220.0 * t) * 20_000).astype("<i2").tobytes()
        private_file(self.work_dir / "audio.raw", audio)
        return audio

    @staticmethod
    def _fake_embed(clip: np.ndarray) -> np.ndarray:
        return np.array([float(np.abs(clip).sum()), float(len(clip)), 1.0])

    def _derive(self, segments: list[dict]) -> dict[str, str]:
        from worker.sitting_derivation import derive_sitting_material

        return derive_sitting_material(
            self.work_dir,
            self.sitting_id,
            encoder_sha256="e" * 64,
            onnx_artifact_sha256="e" * 64,
            embed_segment=self._fake_embed,
            transcribe_audio=lambda _samples: segments,
        )

    def test_derivation_writes_content_free_spans_and_unit_embeddings(self) -> None:
        audio = self._scaffold_sitting()
        digests = self._derive([
            {"start": 0.5, "end": 3.0, "text": "SECRET-OPERATOR-WORDS"},
            {"start": 3.5, "end": 4.5, "text": "too short to score"},
            {"start": 5.0, "end": 8.0, "text": "MORE-SECRET-WORDS"},
        ])

        segments_bytes = (self.work_dir / "segments.json").read_bytes()
        embeddings_bytes = (self.work_dir / "embeddings.bin").read_bytes()
        self.assertEqual(digests["segments"], hashlib.sha256(segments_bytes).hexdigest())
        self.assertEqual(
            digests["embeddings"], hashlib.sha256(embeddings_bytes).hexdigest()
        )

        # The transcript exists only inside the process; times, counts, and
        # digests are the entire persisted vocabulary.
        self.assertNotIn(b"SECRET", segments_bytes)
        self.assertNotIn(b"text", segments_bytes)

        document = json.loads(segments_bytes)
        self.assertEqual(document["schema"], "sitting-derivation/1")
        self.assertEqual(document["sitting_id"], self.sitting_id)
        self.assertEqual(document["raw_sha256"], hashlib.sha256(audio).hexdigest())
        self.assertEqual(document["samples"], 160_000)
        self.assertEqual(len(document["segments"]), 3)
        self.assertEqual(document["embedding"]["count"], 2)
        self.assertEqual(document["embedding"]["dim"], 3)
        self.assertEqual(document["embedding"]["sha256"], digests["embeddings"])
        self.assertEqual(document["embedding"]["encoder_sha256"], "e" * 64)

        rows = np.frombuffer(embeddings_bytes, dtype="<f4").reshape(2, 3)
        for row in rows:
            self.assertAlmostEqual(float(np.linalg.norm(row)), 1.0, places=5)

    def test_rederivation_is_idempotent_and_disagreement_is_refused(self) -> None:
        from worker.sitting_derivation import DerivationRefused

        self._scaffold_sitting()
        spans = [{"start": 0.0, "end": 4.0}]
        first = self._derive(spans)
        self.assertEqual(self._derive(spans), first)

        (self.work_dir / "embeddings.bin").write_bytes(b"tampered")
        with self.assertRaisesRegex(DerivationRefused, "disagrees"):
            self._derive(spans)

    def test_span_validation_refuses_disorder_and_clamps_overshoot(self) -> None:
        from worker.sitting_derivation import DerivationRefused

        self._scaffold_sitting()
        with self.assertRaisesRegex(DerivationRefused, "overlap"):
            self._derive([{"start": 0.0, "end": 5.0}, {"start": 4.0, "end": 9.0}])
        with self.assertRaisesRegex(DerivationRefused, "outside the recording"):
            self._derive([{"start": 11.0, "end": 12.0}])
        with self.assertRaisesRegex(DerivationRefused, "non-finite"):
            self._derive([{"start": 0.0, "end": float("nan")}])
        with self.assertRaisesRegex(DerivationRefused, "numeric bounds"):
            self._derive([{"start": 0.0}])

        # Transcription may overshoot the final sample; the end clamps to the
        # recording instead of refusing the whole sitting.
        digests = self._derive([{"start": 5.0, "end": 100.0}])
        document = json.loads((self.work_dir / "segments.json").read_bytes())
        self.assertEqual(document["segments"][0]["end_seconds"], 10.0)
        self.assertEqual(document["embedding"]["count"], 1)
        self.assertIn("segments", digests)

    def test_no_scorable_speech_is_refused(self) -> None:
        from worker.sitting_derivation import DerivationRefused

        self._scaffold_sitting()
        with self.assertRaisesRegex(DerivationRefused, "no scorable speech"):
            self._derive([{"start": 0.0, "end": 1.0}])

    def test_raw_audio_refusals(self) -> None:
        from worker.sitting_derivation import DerivationRefused, read_raw_sitting_audio

        self.work_dir.mkdir(mode=0o700, parents=True)
        with self.assertRaisesRegex(DerivationRefused, "missing or unsafe"):
            read_raw_sitting_audio(self.work_dir)
        private_file(self.work_dir / "audio.raw", b"")
        with self.assertRaisesRegex(DerivationRefused, "empty"):
            read_raw_sitting_audio(self.work_dir)
        (self.work_dir / "audio.raw").write_bytes(b"\x00\x01\x02")
        with self.assertRaisesRegex(DerivationRefused, "16-bit mono PCM"):
            read_raw_sitting_audio(self.work_dir)

    def test_adapter_refusal_ladder(self) -> None:
        arguments = {"sitting_id": self.sitting_id}
        with self.assertRaisesRegex(AdapterRefused, "canonical lowercase UUID"):
            adapters.sitting_derive(
                self.root, {"sitting_id": self.sitting_id.upper()},
                admission="boundary-test", model_dir=None,
                encoder_digest="e" * 64, encoder_path=None,
            )
        with self.assertRaisesRegex(AdapterRefused, "identity row is missing"):
            adapters.sitting_derive(
                self.root, arguments,
                admission="boundary-test", model_dir=None,
                encoder_digest="e" * 64, encoder_path=None,
            )

        row = self.root / "enrollment" / "sittings" / self.sitting_id / "sitting.json"
        row.parent.mkdir(mode=0o700, parents=True)
        private_file(row, b"{}\n")
        with self.assertRaisesRegex(AdapterRefused, "work directory is missing"):
            adapters.sitting_derive(
                self.root, arguments,
                admission="boundary-test", model_dir=None,
                encoder_digest="e" * 64, encoder_path=None,
            )

        self.work_dir.mkdir(mode=0o700, parents=True)
        with self.assertRaisesRegex(AdapterRefused, "fixed transcript model"):
            adapters.sitting_derive(
                self.root, arguments,
                admission="boundary-test", model_dir=None,
                encoder_digest="e" * 64, encoder_path=None,
            )

        model_dir = self.root / "model"
        model_dir.mkdir(mode=0o700)
        with self.assertRaisesRegex(AdapterRefused, "no admitted speaker encoder"):
            adapters.sitting_derive(
                self.root, arguments,
                admission="boundary-test", model_dir=model_dir,
                encoder_digest="e" * 64, encoder_path=None,
            )

        placeholder = self.root / "encoder-unavailable.identity"
        private_file(placeholder, b"no encoder is admitted\n")
        with self.assertRaisesRegex(AdapterRefused, "disagrees with its manifest"):
            adapters.sitting_derive(
                self.root, arguments,
                admission="boundary-test", model_dir=model_dir,
                encoder_digest="e" * 64, encoder_path=placeholder,
            )
        with self.assertRaisesRegex(AdapterRefused, "no admitted speaker encoder"):
            adapters.sitting_derive(
                self.root, arguments,
                admission="boundary-test", model_dir=model_dir,
                encoder_digest=digest(placeholder), encoder_path=placeholder,
            )

    def test_dispatch_registers_the_operation(self) -> None:
        with self.assertRaisesRegex(AdapterRefused, "identity row is missing"):
            adapters.dispatch(
                self.root,
                "sitting.derive",
                {"sitting_id": self.sitting_id},
                encoder_digest="e" * 64,
                admission="boundary-test",
            )

    def test_alpha_set_carries_note_reinspection_but_not_profile_adopt(self) -> None:
        # Widened 2026-08-04 (sitting.derive, transcript.restore) and
        # 2026-08-05 (profile choices/build/inspect/discard) by the
        # operator's registration decisions. profile.adopt stays boundary
        # lane — the packaged publication path is Rust's. note.inspect is in
        # alpha because the coordinator must re-verify a published note before
        # advancing the meeting record.
        from worker.main import operations_for

        self.assertIn("sitting.derive", operations_for("internal-alpha"))
        self.assertIn("profile.choices", operations_for("internal-alpha"))
        self.assertIn("profile.build", operations_for("internal-alpha"))
        self.assertIn("profile.inspect", operations_for("internal-alpha"))
        self.assertIn("profile.discard", operations_for("internal-alpha"))
        self.assertNotIn("profile.adopt", operations_for("internal-alpha"))
        self.assertIn("note.inspect", operations_for("internal-alpha"))
        self.assertIn("profile.adopt", operations_for("boundary-test"))


class ProfileBuildFromEvidenceTests(unittest.TestCase):
    """The stored-evidence path to a candidate profile, end to end.

    Synthetic durable rows stand in for real sittings — the store schemas are
    reproduced exactly — and the finish line is the real `profile_inspect`
    validator: if the built candidate passes the canonical loader's own
    provenance re-derivation, the whole receipt chain holds.
    """

    ENCODER = "ab" * 32
    DIM = 192

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = (Path(self.temporary.name) / "app").resolve()
        self.root.mkdir(mode=0o700)
        self.rng = np.random.default_rng(7)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def _write_sitting(
        self,
        kind: str,
        segments: int,
        *,
        captured_at: int,
        source_class: str | None = None,
        jitter: float = 0.05,
    ) -> str:
        sitting_id = str(uuid.uuid4())
        directory = self.root / "enrollment" / "sittings" / sitting_id
        directory.mkdir(mode=0o700, parents=True)
        base = self.rng.normal(size=self.DIM)
        rows = []
        for _ in range(segments):
            vector = base + self.rng.normal(scale=jitter, size=self.DIM)
            rows.append((vector / np.linalg.norm(vector)).astype("<f4"))
        embeddings = np.stack(rows).tobytes()
        raw = b"synthetic raw " + sitting_id.encode()
        raw_sha = hashlib.sha256(raw).hexdigest()
        spans = [
            {"start_seconds": i * 5.0, "end_seconds": i * 5.0 + 3.5}
            for i in range(segments)
        ]
        private_file(directory / "sitting.json", json.dumps({
            "schema": "sitting-evidence/1",
            "sitting_id": sitting_id,
            "kind": kind,
            "source_class": source_class,
            "started_at_epoch_seconds": captured_at - 60,
        }).encode())
        private_file(directory / "capture.json", json.dumps({
            "schema": "sitting-capture/1",
            "sitting_id": sitting_id,
            "raw": {
                "relative_name": "audio.raw",
                "byte_size": len(raw),
                "sha256": raw_sha,
            },
            "captured_at_epoch_seconds": captured_at,
        }).encode())
        private_file(directory / "segments.json", json.dumps({
            "schema": "sitting-segments/1",
            "sitting_id": sitting_id,
            "raw_sha256": raw_sha,
            "segments": spans,
        }).encode())
        private_file(directory / "embeddings.bin", embeddings)
        private_file(directory / "derived.json", json.dumps({
            "schema": "sitting-derived/1",
            "sitting_id": sitting_id,
            "raw_sha256": raw_sha,
            "encoder_sha256": self.ENCODER,
            "onnx_artifact_sha256": self.ENCODER,
            "embeddings_sha256": hashlib.sha256(embeddings).hexdigest(),
            "embedding_count": segments,
            "embedding_dim": self.DIM,
            "derived_at_epoch_seconds": captured_at + 60,
        }).encode())
        return sitting_id

    def _seed_evidence(self) -> None:
        base = 1_700_000_000
        self._write_sitting("operator-sitting", 6, captured_at=base)
        self._write_sitting("operator-sitting", 6, captured_at=base + 2 * 3600)
        self._write_sitting(
            "negative-source",
            22,
            captured_at=base + 3 * 3600,
            source_class="public-or-licensed",
            jitter=2.5,
        )

    def test_choices_are_deterministic_and_the_build_passes_the_real_loader(self) -> None:
        self._seed_evidence()
        operation_id = str(uuid.uuid4())
        first = adapters.profile_choices(
            self.root, {"operation_id": operation_id}, self.ENCODER
        )["choices"]
        relay = self.root / "enrollment-choices" / f"{operation_id}.json"
        body = json.loads(relay.read_text())
        self.assertEqual(body["schema"], "profile-choices/1")
        self.assertGreaterEqual(len(body["choices"]), 2)
        # Identical evidence yields an identical digest under a fresh
        # operation id — the stale-selection check depends on this.
        second = adapters.profile_choices(
            self.root, {"operation_id": str(uuid.uuid4())}, self.ENCODER
        )["choices"]
        self.assertEqual(first, second)

        target = body["choices"][0]["target_frr"]
        profile_id = str(uuid.uuid4())
        built = adapters.profile_build(
            self.root,
            {"profile_id": profile_id, "selected_target": target},
            self.ENCODER,
        )["profile"]
        candidate = self.root / "profile-candidates" / profile_id / "voiceprint.json"
        self.assertEqual(hashlib.sha256(candidate.read_bytes()).hexdigest(), built)
        inspected = adapters.profile_inspect(
            self.root, {"profile_id": profile_id}, self.ENCODER
        )
        self.assertEqual(inspected["profile"], built)

    def test_build_refuses_a_target_outside_the_measured_choices(self) -> None:
        self._seed_evidence()
        with self.assertRaisesRegex(AdapterRefused, "not one of the measured choices"):
            adapters.profile_build(
                self.root,
                {"profile_id": str(uuid.uuid4()), "selected_target": 0.4242},
                self.ENCODER,
            )

    def test_a_corrupt_row_refuses_instead_of_killing_the_worker(self) -> None:
        # An uncaught TypeError from one corrupt sitting row would crash-loop
        # every worker operation, not just the build. Malformed numerics must
        # surface as the adapter refusal every neighbor produces.
        self._seed_evidence()
        victim = next(
            (self.root / "enrollment" / "sittings").iterdir()
        )
        capture_row = victim / "capture.json"
        row = json.loads(capture_row.read_text())
        row["captured_at_epoch_seconds"] = None
        capture_row.write_text(json.dumps(row))
        with self.assertRaisesRegex(AdapterRefused, "capture time"):
            adapters.profile_choices(
                self.root, {"operation_id": str(uuid.uuid4())}, self.ENCODER
            )

    def test_build_refuses_a_symlinked_candidate_quarantine(self) -> None:
        # mkdir(exist_ok=True) passes through a symlinked directory; the
        # voiceprint's score receipts must never be written outside the
        # private root.
        self._seed_evidence()
        operation_id = str(uuid.uuid4())
        adapters.profile_choices(self.root, {"operation_id": operation_id}, self.ENCODER)
        relay = self.root / "enrollment-choices" / f"{operation_id}.json"
        target = json.loads(relay.read_text())["choices"][0]["target_frr"]
        outside = Path(self.temporary.name) / "outside"
        outside.mkdir(mode=0o700)
        (self.root / "profile-candidates").symlink_to(outside)
        with self.assertRaisesRegex(AdapterRefused, "unsafe"):
            adapters.profile_build(
                self.root,
                {"profile_id": str(uuid.uuid4()), "selected_target": target},
                self.ENCODER,
            )
        self.assertFalse(any(outside.iterdir()))

    def test_choices_refuse_mixed_encoder_material_and_thin_evidence(self) -> None:
        self._seed_evidence()
        with self.assertRaisesRegex(AdapterRefused, "another encoder"):
            adapters.profile_choices(
                self.root, {"operation_id": str(uuid.uuid4())}, "cd" * 32
            )
        empty = (Path(self.temporary.name) / "empty").resolve()
        empty.mkdir(mode=0o700)
        with self.assertRaisesRegex(AdapterRefused, "two saved voice sessions"):
            adapters.profile_choices(
                empty, {"operation_id": str(uuid.uuid4())}, self.ENCODER
            )



if __name__ == "__main__":
    unittest.main()
