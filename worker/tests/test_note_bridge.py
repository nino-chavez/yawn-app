from __future__ import annotations

import hashlib
import io
import json
import os
import select
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import types
import unicodedata
import unittest
from unittest import mock
import uuid
import zipfile
from pathlib import Path

from worker.note_validator import ArtifactFailure, GenerationRefused
from worker.note_validator import _decode_synthesis, locate_kept_candidates, validate_locators
from worker.note_validator import TRANSPORT_NUM_CTX
from worker.note_validator import forbidden_in_claim, validate_claim_rows
from worker.note_validator import inspect as inspect_snapshot
from worker.note_validator import project as project_snapshot
from worker.note_bridge import (
    GENERATOR_DEADLINE_S,
    MAX_FRAME_BYTES,
    BridgeRefused,
    _REQUIRED_FLAGS,
    _confine_runtime_imports,
    InvalidArguments,
    _emit,
    _parse_command,
    _require_registered_deadline,
    _watch_parent_pid,
    verify_descriptor_runtime,
)

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "notes"))
sys.path.insert(0, str(REPO / "spike"))

# The registered classifier offer stride; generate-lane fixtures scale their
# turn counts by it so the OFFERED candidate counts — what the scenarios are
# actually about — stay constant whatever the registration says.
from candidate_first import PRODUCT_RUN as _NOTE_PRODUCT_RUN  # noqa: E402

OFFER_STRIDE = _NOTE_PRODUCT_RUN["classifier"]["offer_stride"]


def private_file(path: Path, data: bytes) -> None:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(data)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class NoteBridgeProcess:
    def __init__(
        self,
        root: Path,
        manifest: Path,
        *,
        executable: Path,
        bridge: Path,
        isolated: bool = True,
        parent_environment: dict[str, str] | None = None,
    ):
        read_fd, self._write_fd = os.pipe()
        command = [str(executable)]
        if isolated:
            command.extend(("-I", "-S", "-E", "-s", "-B"))
        command.extend(
            (
                str(bridge),
                "--temporary-private-root",
                str(root),
                "--note-runtime-manifest",
                str(manifest),
                "--parent-liveness-fd",
                str(read_fd),
            )
        )
        self._process = subprocess.Popen(
            command,
            cwd=manifest.parent,
            env=self._scrubbed_environment(parent_environment or dict(os.environ)),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            pass_fds=(read_fd,),
        )
        os.close(read_fd)
        ready = self._process.stdout.readline()
        self.ready = json.loads(ready) if ready else None

    @staticmethod
    def _scrubbed_environment(_parent: dict[str, str]) -> dict[str, str]:
        return {}

    def send(self, frame: bytes) -> tuple[dict | None, int, bytes]:
        assert self._process.stdin is not None
        assert self._process.stdout is not None
        assert self._process.stderr is not None
        self._process.stdin.write(frame)
        self._process.stdin.close()
        line = self._process.stdout.readline()
        self._process.wait(timeout=3)
        result = json.loads(line) if line else None
        error = self._process.stderr.read()
        return result, self._process.returncode, error

    def close(self) -> None:
        if self._write_fd >= 0:
            os.close(self._write_fd)
            self._write_fd = -1
        self._process.wait(timeout=3)
        if self._process.stdin and not self._process.stdin.closed:
            self._process.stdin.close()
        if self._process.stdout:
            self._process.stdout.close()
        if self._process.stderr:
            self._process.stderr.close()


class NoteBridgeHarnessTests(unittest.TestCase):
    def setUp(self) -> None:
        if os.environ.get("LMN_PACKAGED_RUNTIME_ROOT"):
            # This harness rebinds a copied interpreter, and the packaged
            # standalone python cannot start outside its runtime tree (its
            # install prefix resolves relative to the real layout). Note-bridge
            # behavior stays covered by the development-interpreter run, and
            # release bundles exclude the note runtime entirely.
            self.skipTest(
                "packaged interpreter cannot start from the harness's copied "
                "binary; covered by the development-interpreter run"
            )
        self.role = "inspect"
        self.temporary = tempfile.TemporaryDirectory()
        self.base = Path(self.temporary.name).resolve()
        self.root = self.base / "app"
        self.root.mkdir(mode=0o700)
        self.resources = self.base / "resources"
        self.resources.mkdir(mode=0o700)
        self.runtime = self.resources / "python-runtime"
        shutil.copyfile(sys.executable, self.runtime)
        self.runtime.chmod(0o700)
        self.bridge = self.resources / "note_bridge.py"
        shutil.copyfile(REPO / "worker/note_bridge.py", self.bridge)
        self.bridge.chmod(0o600)
        self.validator = self.resources / "note-validator.zip"
        self._write_validator_bundle(self.validator)
        self.manifest = self.resources / "note-runtime.json"
        self._write_manifest()
        self.meeting_id, self.note_id, self.transcript_id = self._write_note_pair()

    def tearDown(self) -> None:
        self.temporary.cleanup()

    @staticmethod
    def _write_validator_bundle(
        path: Path,
        note_validator_suffix: bytes = b"",
        candidate_first_suffix: bytes = b"",
    ) -> None:
        sources = {
            "note_validator.py": REPO / "worker/note_validator.py",
            "summarize.py": REPO / "notes/summarize.py",
            "transcript.py": REPO / "notes/transcript.py",
            "capture_health.py": REPO / "spike/capture_health.py",
            "candidate_first.py": REPO / "notes/candidate_first.py",
        }
        suffixes = {
            "note_validator.py": note_validator_suffix,
            "candidate_first.py": candidate_first_suffix,
        }
        with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_STORED) as archive:
            for name, source in sources.items():
                archive.writestr(name, source.read_bytes() + suffixes.get(name, b""))
        path.chmod(0o600)

    def _replace_validator(self, note_validator_suffix: str) -> None:
        self.validator.unlink()
        self._write_validator_bundle(self.validator, note_validator_suffix.encode())
        self._write_manifest()

    def _manifest_document(self) -> dict:
        return {
            "schema": "note-runtime/1",
            "role": self.role,
            "runtime": {
                "relative_path": self.runtime.name,
                "sha256": digest(self.runtime),
            },
            "bridge": {
                "relative_path": self.bridge.name,
                "sha256": digest(self.bridge),
            },
            "validator": {
                "relative_path": self.validator.name,
                "sha256": digest(self.validator),
            },
            "generator": None,
            "models": [],
        }

    def _write_manifest(self, document: dict | None = None) -> None:
        self.manifest.unlink(missing_ok=True)
        manifest = document or self._manifest_document()
        private_file(
            self.manifest,
            json.dumps(manifest, ensure_ascii=False, indent=2).encode(),
        )

    def _write_note_pair(self) -> tuple[str, str, str]:
        meeting_id = str(uuid.uuid4())
        pack = json.loads((REPO / "docs/prototype/fixtures/accepted-note2.fixture").read_text())
        transcript_bytes = (json.dumps(pack["transcript"], indent=2) + "\n").encode()
        transcript_id = hashlib.sha256(transcript_bytes).hexdigest()
        transcript = self.root / "meetings" / meeting_id / "transcript" / f"{transcript_id}.json"
        private_file(transcript, transcript_bytes)
        markdown_bytes = pack["markdown"].encode()
        markdown_id = hashlib.sha256(markdown_bytes).hexdigest()
        note = pack["note"]
        note["meeting"]["id"] = meeting_id
        note["transcript"] = f"../transcript/{transcript_id}.json"
        note["render"]["path"] = f"{markdown_id}.md"
        note_bytes = (json.dumps(note, ensure_ascii=False, indent=2) + "\n").encode()
        note_id = hashlib.sha256(note_bytes).hexdigest()
        notes = self.root / "meetings" / meeting_id / "notes"
        private_file(notes / f"{markdown_id}.md", markdown_bytes)
        private_file(notes / f"{note_id}.json", note_bytes)
        for directory in (
            self.root / "meetings",
            self.root / "meetings" / meeting_id,
            self.root / "meetings" / meeting_id / "transcript",
            notes,
        ):
            directory.chmod(0o700)
        return meeting_id, note_id, transcript_id

    def _start(
        self,
        *,
        executable: Path | None = None,
        isolated: bool = True,
        parent_environment: dict[str, str] | None = None,
    ) -> NoteBridgeProcess:
        return NoteBridgeProcess(
            self.root,
            self.manifest,
            executable=executable or self.runtime,
            bridge=self.bridge,
            isolated=isolated,
            parent_environment=parent_environment,
        )

    def _arguments(self) -> dict:
        return {
            "meeting_id": self.meeting_id,
            "note_id": self.note_id,
            "transcript_id": self.transcript_id,
        }

    def _command(self, arguments: dict | None = None) -> tuple[str, bytes]:
        request_id = str(uuid.uuid4())
        command = {
            "schema": "note-bridge-command/1",
            "request_id": request_id,
            "operation": f"note.{self.role}",
            "arguments": arguments or self._arguments(),
        }
        return request_id, json.dumps(command).encode() + b"\n"

    def _tree(self) -> dict[str, str]:
        return {
            str(path.relative_to(self.root)): digest(path)
            for path in sorted(self.root.rglob("*"))
            if path.is_file()
        }

    def test_attested_runtime_bridge_and_validator_inspect_without_app_data_writes(self) -> None:
        before = self._tree()
        request_id, command = self._command()
        bridge = self._start()
        try:
            self.assertEqual(
                bridge.ready,
                {
                    "schema": "note-bridge-event/1",
                    "event": "ready",
                    "protocol": 1,
                    "role": "inspect",
                    "manifest_sha256": digest(self.manifest),
                    "operations": ["note.inspect"],
                },
            )
            result, returncode, error = bridge.send(command)
        finally:
            bridge.close()
        self.assertEqual(returncode, 0)
        self.assertEqual(error, b"")
        self.assertEqual(result["request_id"], request_id)
        self.assertEqual(result["outcome"], "succeeded")
        self.assertEqual(result["artifact_digests"]["note"], self.note_id)
        self.assertEqual(result["artifact_digests"]["transcript"], self.transcript_id)
        self.assertEqual(self._tree(), before)
        self.assertFalse(list(self.root.rglob("operations")))
        self.assertFalse(list(self.root.rglob("children")))

    def test_unattested_interpreter_never_reaches_ready(self) -> None:
        bridge = self._start(executable=Path(sys.executable))
        try:
            self.assertIsNone(bridge.ready)
        finally:
            bridge.close()
        self.assertEqual(bridge._process.returncode, 2)

    def test_unisolated_launcher_never_reaches_ready(self) -> None:
        bridge = self._start(isolated=False)
        try:
            self.assertIsNone(bridge.ready)
        finally:
            bridge.close()
        self.assertEqual(bridge._process.returncode, 2)

    def test_parent_pythonpath_sitecustomize_is_scrubbed_before_startup(self) -> None:
        hostile = self.base / "hostile-startup"
        hostile.mkdir(mode=0o700)
        marker = self.base / "sitecustomize-executed"
        private_file(
            hostile / "sitecustomize.py",
            f"from pathlib import Path\nPath({str(marker)!r}).write_text('executed')\n".encode(),
        )
        request_id, command = self._command()
        bridge = self._start(parent_environment={"PYTHONPATH": str(hostile)})
        try:
            result, returncode, error = bridge.send(command)
        finally:
            bridge.close()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["request_id"], request_id)
        self.assertEqual(result["outcome"], "succeeded")
        self.assertFalse(marker.exists())

    def test_resource_sibling_urllib_cannot_shadow_runtime_standard_library(self) -> None:
        marker = self.base / "urllib-shadow-executed"
        private_file(
            self.resources / "urllib" / "__init__.py",
            f"from pathlib import Path\nPath({str(marker)!r}).write_text('executed')\n".encode(),
        )
        request_id, command = self._command()
        bridge = self._start()
        try:
            result, returncode, error = bridge.send(command)
        finally:
            bridge.close()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["request_id"], request_id)
        self.assertEqual(result["outcome"], "succeeded")
        self.assertFalse(marker.exists())

    def test_intermediate_resource_symlink_never_reaches_ready(self) -> None:
        document = self._manifest_document()
        real = self.resources / "real"
        real.mkdir(mode=0o700)
        moved = real / self.validator.name
        self.validator.rename(moved)
        (self.resources / "linked").symlink_to(real, target_is_directory=True)
        document["validator"] = {
            "relative_path": f"linked/{moved.name}",
            "sha256": digest(moved),
        }
        self._write_manifest(document)
        bridge = self._start()
        try:
            self.assertIsNone(bridge.ready)
        finally:
            bridge.close()
        self.assertEqual(bridge._process.returncode, 2)

    def test_json_array_artifact_returns_closed_invalid_refusal_without_traceback(self) -> None:
        note = self.root / "meetings" / self.meeting_id / "notes" / f"{self.note_id}.json"
        note.write_bytes(b"[]")
        request_id, command = self._command()
        bridge = self._start()
        try:
            result, returncode, error = bridge.send(command)
        finally:
            bridge.close()
        self.assertEqual(returncode, 0)
        self.assertEqual(error, b"")
        self.assertEqual(result["request_id"], request_id)
        self.assertEqual(result["outcome"], "refused")
        self.assertEqual(result["failure"], {"code": "artifact-invalid", "recoverable": False})

    def test_missing_artifact_keeps_its_frozen_refusal_code(self) -> None:
        note = self.root / "meetings" / self.meeting_id / "notes" / f"{self.note_id}.json"
        note.unlink()
        _, command = self._command()
        bridge = self._start()
        try:
            result, returncode, error = bridge.send(command)
        finally:
            bridge.close()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "refused")
        self.assertEqual(result["failure"], {"code": "artifact-missing", "recoverable": True})

    def test_changed_artifact_keeps_its_frozen_refusal_code(self) -> None:
        note = self.root / "meetings" / self.meeting_id / "notes" / f"{self.note_id}.json"
        note.write_bytes(note.read_bytes() + b" ")
        _, command = self._command()
        bridge = self._start()
        try:
            result, returncode, error = bridge.send(command)
        finally:
            bridge.close()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "refused")
        self.assertEqual(result["failure"], {"code": "artifact-changed", "recoverable": False})

    def test_intermediate_artifact_symlink_returns_closed_invalid_refusal(self) -> None:
        real_meetings = self.base / "real-meetings"
        (self.root / "meetings").rename(real_meetings)
        (self.root / "meetings").symlink_to(real_meetings, target_is_directory=True)
        _, command = self._command()
        bridge = self._start()
        try:
            result, returncode, error = bridge.send(command)
        finally:
            bridge.close()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "refused")
        self.assertEqual(result["failure"], {"code": "artifact-invalid", "recoverable": False})

    def test_retained_snapshot_refuses_same_bytes_rename_race(self) -> None:
        note = self.root / "meetings" / self.meeting_id / "notes" / f"{self.note_id}.json"
        original = note.read_bytes()

        def replace_after_open() -> None:
            note.rename(note.with_suffix(".old"))
            private_file(note, original)

        root_fd = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            with self.assertRaises(ArtifactFailure) as failure:
                inspect_snapshot(root_fd, self._arguments(), after_open=replace_after_open)
        finally:
            os.close(root_fd)
        self.assertEqual(
            (failure.exception.code, failure.exception.recoverable),
            (
                "artifact-changed",
                False,
            ),
        )

    def test_retained_snapshot_refuses_same_inode_same_size_mutation(self) -> None:
        note = self.root / "meetings" / self.meeting_id / "notes" / f"{self.note_id}.json"
        original = note.read_bytes()
        replacement = bytearray(original)
        replacement[0] = ord("[") if replacement[0] != ord("[") else ord("{")
        original_identity = note.stat()

        def overwrite_after_open() -> None:
            with note.open("r+b") as handle:
                handle.write(replacement)
                handle.flush()
                os.fsync(handle.fileno())
            current_identity = note.stat()
            self.assertEqual(current_identity.st_ino, original_identity.st_ino)
            self.assertEqual(current_identity.st_size, original_identity.st_size)

        root_fd = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            with self.assertRaises(ArtifactFailure) as failure:
                inspect_snapshot(root_fd, self._arguments(), after_open=overwrite_after_open)
        finally:
            os.close(root_fd)
        self.assertEqual(
            (failure.exception.code, failure.exception.recoverable),
            ("artifact-changed", False),
        )

    def test_identity_drift_during_artifact_refusal_emits_no_result(self) -> None:
        self._replace_validator(
            "\n"
            "def inspect(*args, **kwargs):\n"
            "    from pathlib import Path\n"
            f"    target = Path({str(self.bridge)!r})\n"
            "    with target.open('r+b') as handle:\n"
            "        first = handle.read(1)\n"
            "        handle.seek(0)\n"
            "        handle.write(b'#' if first != b'#' else b'!')\n"
            "        handle.flush()\n"
            "    raise ArtifactFailure('artifact-invalid', False)\n"
        )
        _, command = self._command()
        bridge = self._start()
        try:
            result, returncode, error = bridge.send(command)
        finally:
            bridge.close()
        self.assertIsNone(result)
        self.assertEqual((returncode, error), (2, b""))

    def test_internal_validator_failure_emits_no_artifact_result(self) -> None:
        self._replace_validator(
            "\ndef inspect(*args, **kwargs):\n    raise RuntimeError('injected internal failure')\n"
        )
        _, command = self._command()
        bridge = self._start()
        try:
            result, returncode, error = bridge.send(command)
        finally:
            bridge.close()
        self.assertIsNone(result)
        self.assertEqual((returncode, error), (2, b""))

    def test_temporary_storage_failure_emits_no_artifact_result(self) -> None:
        self._replace_validator(
            "\n"
            "class _UnavailableTemporaryDirectory:\n"
            "    def __init__(self, *args, **kwargs):\n"
            "        pass\n"
            "    def __enter__(self):\n"
            "        raise OSError('injected temporary storage failure')\n"
            "    def __exit__(self, *args):\n"
            "        return False\n"
            "tempfile.TemporaryDirectory = _UnavailableTemporaryDirectory\n"
        )
        _, command = self._command()
        bridge = self._start()
        try:
            result, returncode, error = bridge.send(command)
        finally:
            bridge.close()
        self.assertIsNone(result)
        self.assertEqual((returncode, error), (2, b""))

    def test_invalid_arguments_return_only_invalid_request(self) -> None:
        invalid = self._arguments()
        invalid["note_id"] = 7
        request_id, command = self._command(invalid)
        bridge = self._start()
        try:
            result, returncode, error = bridge.send(command)
        finally:
            bridge.close()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["request_id"], request_id)
        self.assertEqual(result["failure"], {"code": "invalid-request", "recoverable": False})

    def test_duplicate_keys_and_second_frames_are_protocol_failures(self) -> None:
        request_id = str(uuid.uuid4())
        duplicate = (
            '{"schema":"note-bridge-command/1","request_id":"'
            + request_id
            + '","operation":"note.inspect","arguments":'
            + '{"meeting_id":"'
            + self.meeting_id
            + '","note_id":"'
            + self.note_id
            + '","note_id":"'
            + self.note_id
            + '","transcript_id":"'
            + self.transcript_id
            + '"}}\n'
        ).encode()
        for frame in (duplicate, self._command()[1] + self._command()[1]):
            bridge = self._start()
            try:
                result, returncode, error = bridge.send(frame)
            finally:
                bridge.close()
            self.assertIsNone(result)
            self.assertEqual(returncode, 2)
            self.assertEqual(error, b"")


class NoteProjectBridgeTests(unittest.TestCase):
    def setUp(self) -> None:
        self._harness = NoteBridgeHarnessTests()
        self._harness.setUp()
        self._harness.role = "project"
        self._harness._write_manifest()

    def tearDown(self) -> None:
        self._harness.tearDown()

    def __getattr__(self, name):
        harness = self.__dict__.get("_harness")
        if harness is None:
            raise AttributeError(name)
        return getattr(harness, name)

    def _fixture(self) -> dict:
        return json.loads(
            (REPO / "tests/fixtures/note-projection-v1.fixture").read_text(encoding="utf-8")
        )

    def test_project_ready_and_projection_are_read_only_and_use_no_temporary_copy(self) -> None:
        self._replace_validator(
            "\n"
            "class _UnavailableTemporaryDirectory:\n"
            "    def __init__(self, *args, **kwargs): pass\n"
            "    def __enter__(self): raise OSError('temporary storage is unavailable')\n"
            "    def __exit__(self, *args): return False\n"
            "tempfile.TemporaryDirectory = _UnavailableTemporaryDirectory\n"
        )
        before = self._tree()
        request_id, command = self._command()
        bridge = self._start()
        try:
            self.assertEqual(
                bridge.ready,
                {
                    "schema": "note-bridge-event/1",
                    "event": "ready",
                    "protocol": 1,
                    "role": "project",
                    "manifest_sha256": digest(self.manifest),
                    "operations": ["note.project"],
                },
            )
            result, returncode, error = bridge.send(command)
        finally:
            bridge.close()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["request_id"], request_id)
        self.assertEqual(result["outcome"], "succeeded")
        self.assertEqual(result["projection"]["note_json_sha256"], self.note_id)
        self.assertEqual(self._tree(), before)

    def test_shared_fixture_locks_project_command_strictness_and_result_shape(self) -> None:
        fixture = self._fixture()
        valid = fixture["valid_commands"][0]
        request_id, arguments = _parse_command(valid["raw_frame"].encode(), "project")
        self.assertEqual(request_id, "11111111-1111-4111-8111-111111111111")
        self.assertEqual(arguments["meeting_id"], "meeting-a")
        for invalid in fixture["invalid_command_frames"]:
            expected = invalid["expected"]
            failure = BridgeRefused if expected == "protocol-failure" else InvalidArguments
            with self.subTest(invalid["name"]), self.assertRaises(failure):
                _parse_command(invalid["raw_frame"].encode(), "project")
        for expected in fixture["valid_results"] + fixture["refusal_results"]:
            result = expected["result"]
            self.assertEqual(
                list(result),
                ["schema", "request_id", "operation", "outcome", "projection", "failure"],
            )
            if result["outcome"] == "succeeded":
                projection = result["projection"]
                self.assertEqual(
                    list(projection),
                    [
                        "schema",
                        "note_json_sha256",
                        "note_markdown_sha256",
                        "transcript_sha256",
                        "claims",
                    ],
                )
                for ordinal, claim in enumerate(projection["claims"]):
                    self.assertEqual(claim["claim_ordinal"], ordinal)
                    self.assertEqual(
                        list(claim),
                        ["claim_ordinal", "claim_sha256", "claim_type", "evidence_state", "claim", "locators"],
                    )
                    self.assertEqual(claim["evidence_state"], "located")
                    self.assertTrue(1 <= len(claim["locators"]) <= 3)
                    self.assertEqual(
                        claim["locators"],
                        sorted(
                            claim["locators"],
                            key=lambda locator: (
                                locator["turn"], locator["start"], locator["end"], locator["text_sha256"]
                            ),
                        ),
                    )
            else:
                self.assertIsNone(result["projection"])
                self.assertEqual(list(result["failure"]), ["code", "recoverable"])

    def test_shared_fixture_unicode_scalar_locator_is_not_a_utf8_byte_offset(self) -> None:
        fixture = self._fixture()
        claim = fixture["valid_results"][1]["result"]["projection"]["claims"][4]
        locator = claim["locators"][1]
        turn = fixture["transcript_turns"][locator["turn"]]
        self.assertEqual(turn[locator["start"] : locator["end"]], "é🙂")
        self.assertEqual(locator["end"], 3)
        self.assertNotEqual(len("aé🙂".encode("utf-8")), locator["end"])

    def test_descriptor_runtime_uses_retained_bridge_resources_after_path_replacement(self) -> None:
        descriptors = [
            os.open(path, os.O_RDONLY)
            for path in (self.manifest, self.bridge, self.validator)
        ]
        runtime = None
        try:
            with mock.patch("worker.note_bridge.sys.executable", str(self.runtime)):
                runtime = verify_descriptor_runtime(*descriptors)
                original_digest = runtime.resources["bridge"].digest
                self.bridge.rename(self.bridge.with_suffix(".admitted"))
                private_file(self.bridge, b"not the descriptor-admitted bridge")
                runtime.require_unchanged()
            self.assertEqual(runtime.resources["bridge"].digest, original_digest)
        finally:
            if runtime is not None:
                runtime.close()
            for descriptor in descriptors:
                os.close(descriptor)

    def test_descriptor_runtime_refuses_a_manifest_resource_bound_to_the_wrong_fd(self) -> None:
        descriptors = [
            os.open(path, os.O_RDONLY)
            for path in (self.manifest, self.bridge, self.bridge)
        ]
        try:
            with mock.patch("worker.note_bridge.sys.executable", str(self.runtime)):
                with self.assertRaises(BridgeRefused):
                    verify_descriptor_runtime(*descriptors)
        finally:
            for descriptor in descriptors:
                os.close(descriptor)

    def test_descriptor_launch_watches_expected_parent_exit_with_kqueue(self) -> None:
        class Queue:
            def __init__(self) -> None:
                self.registered = []

            def control(self, changes, *_):
                if changes is not None:
                    self.registered.extend(changes)
                    return []
                return [object()]

            def close(self) -> None:
                pass

        queue = Queue()
        exited = threading.Event()
        exits = []

        def fake_exit(code: int) -> None:
            exits.append(code)
            exited.set()
            raise SystemExit(code)

        with (
            mock.patch("worker.note_bridge.os.getppid", return_value=4242),
            mock.patch("worker.note_bridge.select.kqueue", return_value=queue),
            mock.patch("worker.note_bridge.select.kevent", return_value=object()) as kevent,
            mock.patch("worker.note_bridge.os._exit", side_effect=fake_exit),
        ):
            _watch_parent_pid(4242)
            self.assertTrue(exited.wait(1))
        self.assertEqual(exits, [0])
        kevent.assert_called_once_with(
            4242,
            filter=select.KQ_FILTER_PROC,
            flags=select.KQ_EV_ADD | select.KQ_EV_ENABLE,
            fflags=select.KQ_NOTE_EXIT,
        )
        self.assertEqual(len(queue.registered), 1)

    def _assert_project_failure(self, code: str, recoverable: bool) -> None:
        _, command = self._command()
        bridge = self._start()
        try:
            result, returncode, error = bridge.send(command)
        finally:
            bridge.close()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["failure"], {"code": code, "recoverable": recoverable})

    def test_project_maps_missing_artifacts_to_the_closed_refusal(self) -> None:
        note = self.root / "meetings" / self.meeting_id / "notes" / f"{self.note_id}.json"
        note.unlink()
        self._assert_project_failure("artifact-missing", True)

    def test_project_maps_changed_artifacts_to_the_closed_refusal(self) -> None:
        note = self.root / "meetings" / self.meeting_id / "notes" / f"{self.note_id}.json"
        note.write_bytes(note.read_bytes() + b" ")
        self._assert_project_failure("artifact-changed", False)

    def test_project_maps_invalid_artifacts_to_the_closed_refusal(self) -> None:
        note = self.root / "meetings" / self.meeting_id / "notes" / f"{self.note_id}.json"
        note.write_bytes(b"[]")
        self._assert_project_failure("artifact-invalid", False)

    def test_project_rechecks_descriptor_identity_after_derivation(self) -> None:
        note = self.root / "meetings" / self.meeting_id / "notes" / f"{self.note_id}.json"
        original = note.read_bytes()

        def replace_after_open() -> None:
            note.rename(note.with_suffix(".old"))
            private_file(note, original)

        root_fd = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            with self.assertRaises(ArtifactFailure) as failure:
                project_snapshot(root_fd, self._arguments(), after_open=replace_after_open)
        finally:
            os.close(root_fd)
        self.assertEqual((failure.exception.code, failure.exception.recoverable), ("artifact-changed", False))

    def _replace_project_with_capacity_projection(self, count: int) -> None:
        self._replace_validator(
            "\n"
            "def project(root_fd, arguments):\n"
            "    claim_texts = [f'claim-{ordinal:06d}' for ordinal in range(" + str(count) + ")]\n"
            "    claims = [{\n"
            "        'claim_ordinal': ordinal,\n"
            "        'claim_sha256': hashlib.sha256(claim.encode('utf-8')).hexdigest(),\n"
            "        'claim_type': 'action',\n"
            "        'evidence_state': 'located',\n"
            "        'claim': claim,\n"
            "        'locators': [{'turn': 0, 'start': 0, 'end': 5, 'text_sha256': '8ed3f6ad685b959ead7022518e1af76cd816f8e8ec7ccdda1ed4018e8f2223f8'}],\n"
            "    } for ordinal, claim in enumerate(claim_texts)]\n"
            "    return {'schema': 'note-claim-projection/1', 'note_json_sha256': arguments['note_id'], 'note_markdown_sha256': 'b' * 64, 'transcript_sha256': arguments['transcript_id'], 'claims': claims}\n"
        )

    def test_shared_fixture_capacity_is_217_or_refusal_at_218_without_truncation(self) -> None:
        fixture = self._fixture()["capacity_case"]
        self._replace_project_with_capacity_projection(fixture["last_fitting_claim_count"])
        _, command = self._command()
        bridge = self._start()
        try:
            result, returncode, error = bridge.send(command)
        finally:
            bridge.close()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "succeeded")
        self.assertEqual(len(result["projection"]["claims"]), fixture["last_fitting_claim_count"])
        self.assertEqual(
            len(json.dumps(result, separators=(",", ":")).encode() + b"\n"),
            fixture["last_fitting_frame_bytes"],
        )
        self._replace_project_with_capacity_projection(fixture["claim_count"])
        _, command = self._command()
        bridge = self._start()
        try:
            result, returncode, error = bridge.send(command)
        finally:
            bridge.close()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["failure"], {
            "code": fixture["expected_result"],
            "recoverable": False,
        })
        self.assertIsNone(result["projection"])
        # The refusal is what reached the pipe, so the over-large frame was
        # measured and discarded before any write. That ordering is what the
        # Rust transport depends on: an oversized frame arriving instead would
        # refuse as a content-free `Unavailable` and abort the whole library
        # rebuild naming the wrong cause.
        self.assertLess(
            len(json.dumps(result, separators=(",", ":")).encode() + b"\n"),
            MAX_FRAME_BYTES,
        )


# Stub classifiers stand in for the MLX-LM child. They are ordinary
# manifest-pinned generator resources driven over the real session protocol, so
# each runs through the real spawn, batching, deadline, and decode path rather
# than around it. Every stub is a session: it answers one request line per
# batch until stdin closes.
#
# Note what a stub cannot do. The model's whole output surface is one KEEP or
# ABSTAIN per candidate it was offered, so there is no stub that fabricates a
# transcript row — `decode_classification` refuses an unknown, duplicated,
# reordered, or miscounted verdict before anything reaches the transcript. The
# case is replaced by one that answers with an id it was never shown.
STUB_PREAMBLE = """import json, sys, time


def batches():
    while True:
        line = sys.stdin.readline()
        if not line:
            return
        request = json.loads(line)
        if request.get("schema") == "note-synthesis-request/1":
            # Only lines that look like the alias JSON objects, not the header
            # or (when pre-meeting context is present) the labeled context
            # section and its blank-line separator ahead of them.
            aliases = [
                json.loads(source_line)["id"]
                for source_line in request["user"].splitlines()
                if source_line.strip().startswith("{")
            ]
            # Roadmap intake I3: echo the operator's own context text (not the
            # label around it) into the first overview row when a labeled
            # context section is present, bounded well under the claim-text
            # ceiling, so a test can assert on what actually reached the model
            # without a bespoke stub protocol.
            context_marker = ""
            if "OPERATOR-PROVIDED MEETING CONTEXT" in request["user"]:
                after_label = request["user"].split("):\\n", 1)[1]
                context_text = after_label.split("\\n\\n", 1)[0]
                context_marker = " CONTEXT_SEEN:" + context_text[:80]
            proposal = {
                "overview": [
                    {
                        "text": f"Summary grounded in selected excerpt {alias}."
                        + (context_marker if index == 0 else ""),
                        "evidence_ids": [alias],
                    }
                    for index, alias in enumerate(aliases[:4])
                ],
                "items": [],
            }
            answer({"content": json.dumps(proposal)})
            continue
        enum = request["response_format"]["properties"]["items"]["items"]
        yield request, list(enum["properties"]["candidate_id"]["enum"])


def answer(rows):
    envelope = {"items": rows} if isinstance(rows, list) else rows
    sys.stdout.write(json.dumps(envelope) + chr(10))
    sys.stdout.flush()


def verdicts(ids, pattern):
    return [
        {"candidate_id": value, "verdict": pattern(index)}
        for index, value in enumerate(ids)
    ]
"""

# Keeps one candidate every twentieth request and abstains on the rest.
#
# The registered product batch size is 1, so every request offers exactly one
# locator and "keep the first of each batch" would keep the whole meeting — and
# the gap-1 pruner would then collapse the lot into a single point, because the
# fixture's anchors are one per consecutive turn. Spacing the keeps twenty
# requests apart leaves each one its own run, so the point count is a witness
# for the session protocol and for the pruner at the same time.
STUB_WELL_FORMED = STUB_PREAMBLE + """
for index, (request, ids) in enumerate(batches()):
    answer(verdicts(ids, lambda position: "KEEP" if index % 20 == 0 else "ABSTAIN"))
"""

STUB_MALFORMED = STUB_PREAMBLE + """
for request, ids in batches():
    sys.stdout.write("this is not a classifier response at all" + chr(10))
    sys.stdout.flush()
"""

# Names a candidate it was never offered. Under the old generate-and-locate
# shape this was "cites a nonexistent transcript row"; selection makes the
# stronger check structural.
STUB_UNOFFERED_CANDIDATE = STUB_PREAMBLE + """
for request, ids in batches():
    rows = verdicts(ids, lambda index: "ABSTAIN")
    rows[0]["candidate_id"] = "cf-SECRETFABRICATEDIDMARKER"
    answer(rows)
"""

STUB_WRONG_CARDINALITY = STUB_PREAMBLE + """
for request, ids in batches():
    answer(verdicts(ids, lambda index: "ABSTAIN")[:-1])
"""

STUB_INVALID_VERDICT = STUB_PREAMBLE + """
for request, ids in batches():
    answer(verdicts(ids, lambda index: "MAYBE"))
"""

STUB_NEVER_ANSWERS = STUB_PREAMBLE + """
time.sleep(30)
"""

# Answers the first batch and then hangs, so the deadline is proven to cover
# the whole session and not merely the child's startup.
STUB_STALLS_AFTER_FIRST_BATCH = STUB_PREAMBLE + """
for index, (request, ids) in enumerate(batches()):
    if index:
        time.sleep(30)
    answer(verdicts(ids, lambda position: "ABSTAIN"))
"""

STUB_KEEPS_EVERYTHING = STUB_PREAMBLE + """
for request, ids in batches():
    answer(verdicts(ids, lambda index: "KEEP"))
"""

# Keeps every other candidate, so no two keeps are adjacent and the gap-1
# pruner collapses nothing. This is the only shape that can push the *pruned*
# set past the budget: keeping more is not enough, the keeps have to be spread.
STUB_KEEPS_ALTERNATING = STUB_PREAMBLE + """
for index, (request, ids) in enumerate(batches()):
    answer(verdicts(ids, lambda position: "KEEP" if index % 2 == 0 else "ABSTAIN"))
"""

# Keeps candidates isolated beyond the pruner's largest fitting gap (10
# turns), so no (gap, stride) in the registered fit range can merge them.
STUB_KEEPS_EVERY_ELEVENTH = STUB_PREAMBLE + """
for index, (request, ids) in enumerate(batches()):
    answer(verdicts(ids, lambda position: "KEEP" if index % 11 == 0 else "ABSTAIN"))
"""

# Rewrites a pinned model file while the session is still running, so the
# post-generation identity recheck has something to catch.
STUB_REWRITES_THE_MODEL = STUB_PREAMBLE + """
for request, ids in batches():
    target = Path(request["model_directory"]) / "config.json"
    target.write_bytes(b"{}" + b" " * 64 + chr(10).encode())
    answer(verdicts(ids, lambda index: "KEEP" if index == 0 else "ABSTAIN"))
"""

# Answers with more bytes than one response is allowed to occupy.
STUB_FLOODS_STDOUT = STUB_PREAMBLE + """
for request, ids in batches():
    sys.stdout.write("x" * (1024 * 1024 + 64) + chr(10))
    sys.stdout.flush()
"""

# Reports the model directory it was handed. The real generator loads MLX-LM
# from that path, and no other stub reads the field, so without this the one
# protocol value seam 5 depends on would be unexercised.
STUB_REPORTS_MODEL_DIRECTORY = STUB_PREAMBLE + """
for request, ids in batches():
    Path(MARKER).write_text(request.get("model_directory") or "absent")
    answer(verdicts(ids, lambda index: "ABSTAIN"))
"""

# Reports the rest of the request packet: the decoding fields a provider is
# told to honour, and how many candidates one request offers. Nothing else
# reads them, so without this the packet could drift to any values at all.
STUB_REPORTS_REQUEST = STUB_PREAMBLE + """
for request, ids in batches():
    Path(MARKER).write_text(json.dumps({
        "num_ctx": request.get("num_ctx"),
        "temperature": request.get("temperature"),
        "num_predict": request.get("num_predict"),
        "offered": len(ids),
    }))
    answer(verdicts(ids, lambda index: "ABSTAIN"))
"""

STUB_REQUIRES_CORRECTED_LABEL = STUB_PREAMBLE + """
for index, (request, ids) in enumerate(batches()):
    if '"speaker":"Alex"' not in request["user"] or '"speaker":"Them"' in request["user"]:
        Path(MARKER).write_text("wrong-label")
    else:
        Path(MARKER).write_text("corrected-label")
    answer(verdicts(ids, lambda position: "KEEP" if index % 20 == 0 else "ABSTAIN"))
"""

STUB_REQUIRES_CORRECTED_VOCABULARY = STUB_PREAMBLE + """
for index, (request, ids) in enumerate(batches()):
    if index == 0:
        packet = json.loads(request["user"].split("\\n\\n", 1)[1])[0]
        anchor = next(row["text"] for row in packet["fragments"] if row["anchor"])
        if 'packaging option' not in anchor or 'packagging option' in anchor:
            Path(MARKER).write_text("wrong-vocabulary")
        else:
            Path(MARKER).write_text("corrected-vocabulary")
    answer(verdicts(ids, lambda position: "KEEP" if index % 20 == 0 else "ABSTAIN"))
"""

# Roadmap intake I3, decision 5's invariant. This does not use `batches()`'s
# shared synthesis auto-answer at all: exactly one candidate is kept -- real
# transcript evidence exists and is offered to the model as alias E01 -- but
# the one overview row this proposes names no evidence_ids at all, the shape
# a model would produce if it treated the labeled context section as citable
# content instead of framing rather than citing the real evidence it was
# given. Proves the claim cannot be admitted regardless of what the model
# says or what evidence happened to be available, because `_decode_synthesis`
# requires a valid alias on the row itself and this row offers none.
STUB_SYNTHESIZES_A_CONTEXT_ONLY_CLAIM = """import json, sys


def _write(value):
    sys.stdout.write(json.dumps(value) + chr(10))
    sys.stdout.flush()


kept_one = False
while True:
    line = sys.stdin.readline()
    if not line:
        break
    request = json.loads(line)
    if request.get("schema") == "note-synthesis-request/1":
        proposal = {
            "overview": [{
                "text": "The meeting was about renewing the lease.",
                "evidence_ids": [],
            }],
            "items": [],
        }
        _write({"content": json.dumps(proposal)})
        continue
    schema = request["response_format"]["properties"]["items"]["items"]
    enum = schema["properties"]["candidate_id"]["enum"]
    verdicts = []
    for value in enum:
        if not kept_one:
            verdicts.append({"candidate_id": value, "verdict": "KEEP"})
            kept_one = True
        else:
            verdicts.append({"candidate_id": value, "verdict": "ABSTAIN"})
    _write({"items": verdicts})
"""


MODEL_FILES = {"config.json": b"{}\n", "weights.npz": b"weights\n"}


class NoteGenerateBridgeTests(unittest.TestCase):
    """The admitted generator path: one generator, and nothing it says is taken."""

    MODEL_DIRECTORY = "models/note-model/rev-1"

    # One candidate per turn, and the classifier offers every
    # `offer_stride`-th of them at the registered batch size 1, so scaling
    # the turn count by the stride holds the OFFERED count — the number of
    # requests one run makes — at seventy whatever the registered stride is:
    # the session protocol that makes one model load serve a whole meeting is
    # exercised seventy times over. Seventy offered keeps is also past the
    # registered keep budget of 64, so a classifier that keeps everything
    # overruns the raw keep count — which, per `PRODUCT_RUN`, is no longer
    # the number the gate is applied to.
    TRANSCRIPT_TURNS = 70 * OFFER_STRIDE

    # 792 OFFERED candidates with a keep every 11th request: 72 keeps, each
    # isolated past the pruner's largest fitting gap (10 turns — kept anchors
    # sit 11 * OFFER_STRIDE turns apart), so every gap in range leaves more
    # than 64 runs and the budget-fitted collapse cannot fit — the one shape
    # the fitted pruner must still refuse.
    SPREAD_TRANSCRIPT_TURNS = 792 * OFFER_STRIDE

    def setUp(self) -> None:
        self._harness = NoteBridgeHarnessTests()
        self._harness.setUp()
        self._harness.role = "generate"
        self.marker = self._harness.base / "generator-ran"
        self.generator = self._harness.resources / "note-generator.py"
        self._write_generator(STUB_WELL_FORMED)
        self._write_model_tree(MODEL_FILES)
        self._write_generate_manifest()
        self.candidates = self._write_long_transcript(self.TRANSCRIPT_TURNS)
        self.assertGreater(self.candidates, 64, "fixture must span batches and the keep budget")

    def _write_long_transcript(
        self,
        turns: int,
        *,
        speaker: str | None = None,
        vocabulary_typo: bool = False,
    ) -> int:
        """Replace the meeting's transcript with one that spans several batches."""
        sys.path.insert(0, str(REPO / "notes"))
        sys.path.insert(0, str(REPO / "spike"))
        from summarize import build_fragment_map
        from transcript import load_bytes
        from capture_health import build as build_capture_health

        document = {
            "schema": "capture-transcript/1" if speaker else "file-transcript/1",
            "source": "synthetic generate-path control; not product evidence",
            "attribution": "channel" if speaker else "none",
            "turns": [
                {
                    # Multi-byte, and early enough in the turn that a locator
                    # span straddles it. Fragment offsets are character
                    # offsets; with ASCII-only text a byte/character confusion
                    # in `validate_locators` is undetectable, and the whole
                    # suite passed under one until this fixture carried é and
                    # an emoji.
                    "text": (
                        f"Turn {index:02d}: café \U0001f642 — we agreed to review "
                        f"the {'packagging' if vocabulary_typo else 'packaging'} option."
                    ),
                    "start": float(index + 1),
                    **({"speaker": speaker} if speaker else {}),
                }
                for index in range(turns)
            ],
        }
        if speaker:
            document.update({
                "bleed": None,
                "voiceprint": None,
                "capture_health": build_capture_health(
                    mic_samples=16_000,
                    system_samples=16_000,
                    capture_elapsed_samples=16_000,
                    dropouts={"mic": [], "system": []},
                    tap_errors=[],
                    transcription_requested=True,
                    transcript_written=True,
                ),
            })
        # The coverage this fixture buys is invisible if it ever goes ASCII, so
        # it is asserted rather than trusted. Measured: with ASCII-only turns,
        # rewriting `validate_locators` to byte offsets left all 51 tests
        # passing; with these turns it fails three.
        self.assertTrue(
            any(ord(character) > 127 for turn in document["turns"] for character in turn["text"]),
            "generate fixture must carry multi-byte text",
        )
        encoded = (json.dumps(document, indent=2) + "\n").encode()
        transcript_id = hashlib.sha256(encoded).hexdigest()
        directory = self.root / "meetings" / self.meeting_id / "transcript"
        # The one this replaces goes, so a test that rewrites the transcript
        # does not leave a second one in the tree the read-only assertions
        # compare before and after.
        (directory / f"{self._harness.transcript_id}.json").unlink(missing_ok=True)
        private_file(directory / f"{transcript_id}.json", encoded)
        self._harness.transcript_id = transcript_id
        return len(build_fragment_map(load_bytes(encoded, source="fixture"))["fragments"])

    def tearDown(self) -> None:
        self._harness.tearDown()

    def __getattr__(self, name):
        harness = self.__dict__.get("_harness")
        if harness is None:
            raise AttributeError(name)
        return getattr(harness, name)

    def _write_generator(self, source: str) -> None:
        """Pin a stub as the manifest's generator, marking that it actually ran.

        The marker is what makes "refused before the generator ran" a real
        assertion rather than a claim about the failure code.
        """
        self.generator.unlink(missing_ok=True)
        marked = (
            "from pathlib import Path\n"
            f"MARKER = {str(self.marker)!r}\n"
            "Path(MARKER).write_text('ran')\n"
        )
        private_file(self.generator, (marked + source).encode())

    def _replace_validator_registration_budget(self, seconds: int) -> None:
        """Move the pinned registration's elapsed budget inside the bundle."""
        self.validator.unlink()
        self._harness._write_validator_bundle(
            self.validator,
            candidate_first_suffix=(
                f'\nPRODUCT_RUN["gates"]["maximum_elapsed_seconds"] = {seconds}\n'
            ).encode(),
        )

    def _write_model_tree(self, files: dict[str, bytes]) -> None:
        directory = self.root / self.MODEL_DIRECTORY
        if directory.exists():
            shutil.rmtree(self.root / "models")
        for name, data in files.items():
            private_file(directory / name, data)
        for part in (self.root / "models", directory.parent, directory):
            part.chmod(0o700)

    def _model_entries(self, files: dict[str, bytes] | None = None) -> list[dict]:
        return [
            {"id": f"note-generator-{index}", "sha256": hashlib.sha256(data).hexdigest()}
            for index, data in enumerate((files or MODEL_FILES).values())
        ]

    def _write_generate_manifest(
        self,
        *,
        role: str = "generate",
        generator: dict | None = None,
        models: list[dict] | None = None,
    ) -> None:
        document = self._harness._manifest_document()
        document["role"] = role
        document["generator"] = generator if generator is not None else {
            "relative_path": self.generator.name,
            "sha256": digest(self.generator),
        }
        document["models"] = self._model_entries() if models is None else models
        self._harness._write_manifest(document)

    def _generate_command(self, **overrides) -> tuple[str, bytes]:
        speaker_label_overrides = overrides.pop("speaker_label_overrides", None)
        vocabulary_replacements = overrides.pop("vocabulary_replacements", None)
        pre_meeting_context = overrides.pop("pre_meeting_context", None)
        request_id = str(uuid.uuid4())
        arguments = {
            "meeting_id": self.meeting_id,
            "transcript_id": self.transcript_id,
        }
        if speaker_label_overrides is not None:
            arguments["speaker_label_overrides"] = speaker_label_overrides
        if vocabulary_replacements is not None:
            arguments["vocabulary_replacements"] = vocabulary_replacements
        if pre_meeting_context is not None:
            arguments["pre_meeting_context"] = pre_meeting_context
        arguments.update({"model_directory": self.MODEL_DIRECTORY, "deadline_s": 20})
        arguments.update(overrides)
        command = {
            "schema": "note-bridge-command/1",
            "request_id": request_id,
            "operation": "note.generate",
            "arguments": arguments,
        }
        return request_id, json.dumps(command).encode() + b"\n"

    def _run(self, **overrides) -> tuple[dict | None, int, bytes, str]:
        request_id, command = self._generate_command(**overrides)
        bridge = self._start()
        try:
            result, returncode, error = bridge.send(command)
        finally:
            bridge.close()
        return result, returncode, error, request_id

    def _offered_rows(self) -> list[dict]:
        """The registered classifier offer for this fixture's transcript."""
        sys.path.insert(0, str(REPO / "notes"))
        import candidate_first
        from transcript import load_bytes

        raw = (
            self.root / "meetings" / self.meeting_id / "transcript" / f"{self.transcript_id}.json"
        ).read_bytes()
        manifest = candidate_first.generate_manifest(
            load_bytes(raw, source="fixture"), candidate_first.STRATEGY_BROAD,
            contract=candidate_first.PRODUCT_CONTRACT,
        )
        return candidate_first.offered_candidates(
            manifest["candidates"],
            candidate_first.PRODUCT_RUN["classifier"]["offer_stride"])

    def _manifest_digests(self) -> tuple[str, str]:
        """The product manifest's digest, and the research one it must not be.

        The two contracts enumerate the same candidates from this fixture and
        differ only in how wide a view each candidate carries, so the candidate
        count cannot tell them apart and the digest is the only field on the
        result frame that can.
        """
        sys.path.insert(0, str(REPO / "notes"))
        import candidate_first
        from transcript import load_bytes

        raw = (
            self.root / "meetings" / self.meeting_id / "transcript" / f"{self.transcript_id}.json"
        ).read_bytes()
        loaded = load_bytes(raw, source="fixture")
        product = candidate_first.generate_manifest(
            loaded, candidate_first.STRATEGY_BROAD,
            contract=candidate_first.PRODUCT_CONTRACT,
        )
        research = candidate_first.generate_manifest(
            loaded, candidate_first.STRATEGY_BROAD,
        )
        self.assertNotEqual(product["manifest_sha256"], research["manifest_sha256"])
        self.assertEqual(
            product["counts"]["candidates"], research["counts"]["candidates"]
        )
        return product["manifest_sha256"], research["manifest_sha256"]

    def _transcript_turns(self) -> list[dict]:
        raw = (
            self.root / "meetings" / self.meeting_id / "transcript" / f"{self.transcript_id}.json"
        ).read_text(encoding="utf-8")
        return [
            turn for turn in json.loads(raw).get("turns", []) if isinstance(turn, dict)
        ]

    def test_generate_manifest_reaches_ready_and_returns_validated_note(self) -> None:
        before = self._tree()
        request_id, command = self._generate_command()
        bridge = self._start()
        try:
            self.assertEqual(
                bridge.ready,
                {
                    "schema": "note-bridge-event/1",
                    "event": "ready",
                    "protocol": 1,
                    "role": "generate",
                    "manifest_sha256": digest(self.manifest),
                    "operations": ["note.generate"],
                },
            )
            result, returncode, error = bridge.send(command)
        finally:
            bridge.close()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["request_id"], request_id)
        self.assertEqual(result["outcome"], "generated")
        self.assertIsNone(result["failure"])
        self.assertEqual(result["generation"]["transcript_sha256"], self.transcript_id)
        self.assertEqual(result["generation"]["candidates"], self.candidates)
        # The manifest was enumerated under `PRODUCT_CONTRACT`'s ±2 window, not
        # the research contract's single-fragment one.
        product, research = self._manifest_digests()
        self.assertEqual(result["generation"]["manifest_sha256"], product)
        self.assertNotEqual(result["generation"]["manifest_sha256"], research)
        claims = result["generation"]["claims"]
        # One keep every twentieth of seventy single-candidate requests (the
        # strided offer holds the request count at seventy), kept anchors at
        # least twenty turns apart, so the pruner collapses nothing: four
        # points. A session that only served its first request would report one.
        self.assertEqual(len(claims), 4)
        for ordinal, claim in enumerate(claims):
            self.assertEqual(claim["claim_ordinal"], ordinal)
            self.assertEqual(claim["claim_type"], "summary")
            self.assertTrue(claim["candidate_ids"][0].startswith("cf-"))
            self.assertEqual(claim["evidence_state"], "located")
            # One locator: the candidate's anchor. Not the classification
            # window, which is context the model saw and the point does not
            # cite — and which a wider view would grow past note/2's cap.
            self.assertEqual(len(claim["locators"]), 1)
            self.assertIn("Summary grounded in selected excerpt", claim["claim"])
        # Transcript order, which is what `point_ordinal` claims to number.
        # `point_ordinal` alone is just `enumerate` and witnesses nothing.
        turns = [claim["locators"][0]["turn"] for claim in claims]
        self.assertEqual(turns, sorted(turns))
        self.assertEqual(len(set(turns)), len(turns))
        receipt = result["generation"]["receipt"]
        # One response per offered candidate, then one synthesis response.
        self.assertEqual(
            receipt["responses"], -(-self.candidates // OFFER_STRIDE) + 1)
        self.assertGreater(receipt["response_bytes"], 0)
        # Nothing under the private root moved: no note, no markdown, no receipt.
        self.assertEqual(self._tree(), before)
        self.assertTrue(self.marker.exists())

    def test_generate_applies_the_bounded_overlay_without_changing_locator_source(self) -> None:
        self._write_long_transcript(self.TRANSCRIPT_TURNS, speaker="Them")
        self._write_generator(STUB_REQUIRES_CORRECTED_LABEL)
        self._write_generate_manifest()

        result, returncode, error, _ = self._run(
            speaker_label_overrides=[
                {"source_speaker": "Them", "replacement": "Alex"}
            ]
        )

        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "generated")
        self.assertEqual(self.marker.read_text(), "corrected-label")
        self.assertEqual(result["generation"]["transcript_sha256"], self.transcript_id)
        turns = self._transcript_turns()
        for claim in result["generation"]["claims"]:
            for locator in claim["locators"]:
                text = turns[locator["turn"]]["text"][locator["start"]:locator["end"]]
                self.assertEqual(
                    hashlib.sha256(text.encode()).hexdigest(), locator["text_sha256"]
                )

    def test_changed_length_vocabulary_overlay_preserves_original_locator_offsets(self) -> None:
        self._write_long_transcript(self.TRANSCRIPT_TURNS, vocabulary_typo=True)
        self._write_generator(STUB_REQUIRES_CORRECTED_VOCABULARY)
        self._write_generate_manifest()
        original = self._transcript_turns()[0]["text"]
        start = original.index("packagging")
        end = start + len("packagging")
        result, returncode, error, _ = self._run(
            vocabulary_replacements=[{
                "turn": 0,
                "char_start": start,
                "char_end": end,
                "source_sha256": hashlib.sha256(
                    original[start:end].encode("utf-8")
                ).hexdigest(),
                # One character shorter than the retained source phrase.
                "replacement": "packaging",
            }]
        )

        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "generated")
        self.assertEqual(self.marker.read_text(), "corrected-vocabulary")
        turns = self._transcript_turns()
        self.assertIn("packagging", turns[0]["text"])
        for claim in result["generation"]["claims"]:
            for locator in claim["locators"]:
                text = turns[locator["turn"]]["text"][locator["start"]:locator["end"]]
                self.assertEqual(
                    hashlib.sha256(text.encode("utf-8")).hexdigest(),
                    locator["text_sha256"],
                )

    def test_generate_includes_pre_meeting_context_as_a_labeled_prompt_section(self) -> None:
        """Roadmap intake I3. The operator's own framing reaches the model in
        a section clearly labeled as context, not as a transcript excerpt --
        and every claim the run still produces cites real transcript evidence,
        proving context changed what the model saw without becoming a claim
        of its own.
        """
        context_text = "Deciding whether to renew the office lease."
        result, returncode, error, _ = self._run(pre_meeting_context=context_text)
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "generated")
        claims = result["generation"]["claims"]
        self.assertTrue(
            any(f"CONTEXT_SEEN:{context_text}" in claim["claim"] for claim in claims),
            "the labeled context section must reach the synthesis prompt verbatim",
        )
        turns = self._transcript_turns()
        for claim in claims:
            for locator in claim["locators"]:
                text = turns[locator["turn"]]["text"][locator["start"]:locator["end"]]
                self.assertEqual(
                    hashlib.sha256(text.encode("utf-8")).hexdigest(),
                    locator["text_sha256"],
                    "every claim, context or not, still needs transcript-verified evidence",
                )

    def test_absent_pre_meeting_context_leaves_the_synthesis_prompt_unchanged(self) -> None:
        """Decision 6's gate at the worker layer: with no `pre_meeting_context`
        key at all, the labeled section never appears, and generation behaves
        exactly as the baseline happy-path test proves.
        """
        result, returncode, error, _ = self._run()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "generated")
        for claim in result["generation"]["claims"]:
            self.assertNotIn("CONTEXT_SEEN", claim["claim"])

    def test_context_only_assertion_cannot_become_a_claim(self) -> None:
        """Decision 5's invariant, exercised end to end. A synthesis response
        that names claim text but no valid transcript evidence is dropped by
        `_decode_synthesis`'s alias resolution regardless of whether that text
        happens to echo the operator's context, and a run left with no
        evidence-linked overview refuses rather than publishing anything.
        """
        self._write_generator(STUB_SYNTHESIZES_A_CONTEXT_ONLY_CLAIM)
        self._write_generate_manifest()
        result, returncode, error, request_id = self._run(
            pre_meeting_context="The real reason we met: renew the lease.",
        )
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["request_id"], request_id)
        self.assertEqual(result["outcome"], "transcript-only")
        self.assertIsNone(result["generation"])
        self.assertEqual(result["failure"]["code"], "no-model-summary")
        self.assertTrue(result["failure"]["recoverable"])
        # The refusal receipt must not leak the context text either.
        frame = json.dumps(result, ensure_ascii=False)
        self.assertNotIn("renew the lease", frame)

    def test_stale_out_of_range_and_oversized_vocabulary_overlays_refuse_before_model_launch(self) -> None:
        original = self._transcript_turns()[0]["text"]
        start = original.index("packaging")
        end = start + len("packaging")
        valid = {
            "turn": 0,
            "char_start": start,
            "char_end": end,
            "source_sha256": hashlib.sha256(original[start:end].encode("utf-8")).hexdigest(),
            "replacement": "package",
        }
        cases = {
            "stale": [{**valid, "source_sha256": "0" * 64}],
            "out-of-range": [{**valid, "turn": self.TRANSCRIPT_TURNS}],
            "oversized": [valid] * 65,
        }
        for label, replacements in cases.items():
            self.marker.unlink(missing_ok=True)
            result, returncode, error, _ = self._run(
                vocabulary_replacements=replacements
            )
            with self.subTest(label=label):
                self.assertEqual((returncode, error), (0, b""))
                self.assertEqual(result["outcome"], "transcript-only")
                self.assertFalse(self.marker.exists())
                self.assertEqual(
                    result["failure"]["code"],
                    "vocabulary-overlay" if label != "oversized" else "invalid-request",
                )

    def test_malformed_or_oversized_pre_meeting_context_refuses_before_model_launch(self) -> None:
        cases = {
            "empty": "",
            "oversized": "x" * (16 * 1024 + 1),
            "control-character": "line one\x00line two",
        }
        for label, context_text in cases.items():
            self.marker.unlink(missing_ok=True)
            result, returncode, error, _ = self._run(pre_meeting_context=context_text)
            with self.subTest(label=label):
                self.assertEqual((returncode, error), (0, b""))
                self.assertEqual(result["outcome"], "transcript-only")
                self.assertFalse(self.marker.exists())
                self.assertEqual(result["failure"]["code"], "invalid-request")

    def test_generated_locators_resolve_to_the_transcripts_own_bytes(self) -> None:
        result, _, _, _ = self._run()
        turns = self._transcript_turns()
        for claim in result["generation"]["claims"]:
            for locator in claim["locators"]:
                text = turns[locator["turn"]]["text"][locator["start"]:locator["end"]]
                self.assertEqual(
                    hashlib.sha256(text.encode()).hexdigest(), locator["text_sha256"]
                )

    def test_malformed_generator_response_is_transcript_only(self) -> None:
        self._write_generator(STUB_MALFORMED)
        self._write_generate_manifest()
        result, returncode, error, request_id = self._run()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["request_id"], request_id)
        self.assertEqual(result["outcome"], "transcript-only")
        self.assertIsNone(result["generation"])
        self.assertEqual(result["failure"]["code"], "response-json-syntax")

    def test_unoffered_candidate_is_transcript_only_and_leaks_no_content(self) -> None:
        self._write_generator(STUB_UNOFFERED_CANDIDATE)
        self._write_generate_manifest()
        result, returncode, error, _ = self._run()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "transcript-only")
        self.assertIsNone(result["generation"])
        self.assertEqual(result["failure"]["code"], "response-contract")
        # A refusal receipt must say what failed without saying what was said.
        frame = json.dumps(result, ensure_ascii=False)
        self.assertNotIn("SECRETFABRICATEDIDMARKER", frame)
        for turn in self._transcript_turns():
            text = turn.get("text") or ""
            if len(text) > 12:
                self.assertNotIn(text[:12], frame)

    def test_wrong_verdict_cardinality_is_transcript_only(self) -> None:
        self._write_generator(STUB_WRONG_CARDINALITY)
        self._write_generate_manifest()
        result, _, _, _ = self._run()
        self.assertEqual(result["outcome"], "transcript-only")
        self.assertEqual(result["failure"]["code"], "response-contract")

    def test_verdict_outside_the_closed_set_is_transcript_only(self) -> None:
        self._write_generator(STUB_INVALID_VERDICT)
        self._write_generate_manifest()
        result, _, _, _ = self._run()
        self.assertEqual(result["outcome"], "transcript-only")
        self.assertEqual(result["failure"]["code"], "response-contract")

    def test_generator_that_never_answers_is_transcript_only_at_the_deadline(self) -> None:
        self._write_generator(STUB_NEVER_ANSWERS)
        self._write_generate_manifest()
        started = time.monotonic()
        result, returncode, error, _ = self._run(deadline_s=1)
        self.assertLess(time.monotonic() - started, 20)
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "transcript-only")
        self.assertIsNone(result["generation"])
        self.assertEqual(result["failure"]["code"], "timeout")

    def test_deadline_covers_the_whole_session_not_only_model_load(self) -> None:
        self._write_generator(STUB_STALLS_AFTER_FIRST_BATCH)
        self._write_generate_manifest()
        started = time.monotonic()
        result, returncode, error, _ = self._run(deadline_s=2)
        self.assertLess(time.monotonic() - started, 20)
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "transcript-only")
        self.assertEqual(result["failure"]["code"], "timeout")
        # The first batch was answered before the stall, so the deadline fired
        # mid-session rather than at startup.
        self.assertEqual(result["failure"]["receipt"]["responses"], 1)

    def test_raw_keeps_over_the_budget_pass_once_pruning_brings_them_under(self) -> None:
        """The registered gate is on the pruned set, and this is why it must be.

        At batch size 1 the measured cells kept 71–298 of 165–453 candidates:
        raw keeps run past 64 on every real meeting. A budget checked as
        verdicts accumulate would refuse all of them before the pruning stage
        that exists precisely to bring the count down. Here the classifier
        keeps all seventy offered candidates — past the budget — and the
        result must be exactly what the registered budget-fitted coverage
        collapse keeps of that offer, in the same order.
        """
        self._write_generator(STUB_KEEPS_EVERYTHING)
        self._write_generate_manifest()
        result, returncode, error, _ = self._run(deadline_s=120)
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "generated")
        self.assertIsNone(result["failure"])
        offered = self._offered_rows()
        self.assertGreater(len(offered), 64)
        self.assertEqual(result["generation"]["receipt"]["responses"], len(offered) + 1)
        import candidate_first
        pruned = candidate_first.prune_keeps(
            offered,
            [{"candidate_id": row["candidate_id"], "verdict": "KEEP"}
             for row in offered])
        cited = [
            claim["candidate_ids"][0]
            for claim in result["generation"]["claims"]
        ]
        self.assertEqual(cited, pruned["pruned_candidate_ids"][:4])
        self.assertLessEqual(len(pruned["pruned_candidate_ids"]), 64)
        self.assertGreater(len(pruned["pruned_candidate_ids"]), 0)

    def test_more_pruned_points_than_the_keep_budget_are_refused_not_trimmed(self) -> None:
        """Keeps isolated past every fitting gap still force a refusal."""
        candidates = self._write_long_transcript(self.SPREAD_TRANSCRIPT_TURNS)
        self._write_generator(STUB_KEEPS_EVERY_ELEVENTH)
        self._write_generate_manifest()
        result, returncode, error, _ = self._run(deadline_s=240)
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "transcript-only")
        self.assertIsNone(result["generation"])
        self.assertEqual(result["failure"]["code"], "keep-budget-exceeded")
        # Not recoverable: re-running the same model over the same transcript
        # produces the same overrun, so a retry is not the answer.
        self.assertIs(result["failure"]["recoverable"], False)
        # No early stop any more: the gate is applied once, after every
        # offered candidate has a verdict, because the pruner needs all of them.
        offered = -(-candidates // OFFER_STRIDE)
        self.assertEqual(result["failure"]["receipt"]["responses"], offered)
        # More isolated keeps than the budget at every gap the fit may use,
        # which is the one shape the previous case cannot produce.
        self.assertGreater((offered + 10) // 11, 64)

    def test_generator_carrying_project_manifest_is_still_refused(self) -> None:
        self._write_generate_manifest(role="project")
        bridge = self._start()
        try:
            self.assertIsNone(bridge.ready)
        finally:
            bridge.close()
        self.assertEqual(bridge._process.returncode, 2)
        self.assertFalse(self.marker.exists())

    def test_generate_manifest_without_models_never_reaches_ready(self) -> None:
        self._write_generate_manifest(models=[])
        bridge = self._start()
        try:
            self.assertIsNone(bridge.ready)
        finally:
            bridge.close()
        self.assertEqual(bridge._process.returncode, 2)

    def test_descriptor_generate_launch_admits_only_the_four_descriptor_shape(self) -> None:
        """A fourth inherited descriptor is what makes `generate` reachable.

        The descriptor set and the role are one decision: four descriptors
        against the generate manifest admit, and the runtime carries the
        generator resource and the pinned models `_run_generation` needs.
        """
        paths = (self.manifest, self.bridge, self.validator, self.generator)
        descriptors = [os.open(path, os.O_RDONLY) for path in paths]
        runtime = None
        try:
            with mock.patch("worker.note_bridge.sys.executable", str(self.runtime)):
                runtime = verify_descriptor_runtime(
                    *descriptors[:3], generator_fd=descriptors[3]
                )
            self.assertEqual(runtime.role, "generate")
            self.assertEqual(runtime.resources["generator"].digest, digest(self.generator))
            self.assertEqual(runtime.models, tuple(self._model_entries()))
        finally:
            if runtime is not None:
                runtime.close()
            for descriptor in descriptors:
                os.close(descriptor)

    def test_descriptor_generate_manifest_without_a_generator_descriptor_refuses(self) -> None:
        descriptors = [
            os.open(path, os.O_RDONLY)
            for path in (self.manifest, self.bridge, self.validator)
        ]
        try:
            with mock.patch("worker.note_bridge.sys.executable", str(self.runtime)):
                with self.assertRaises(BridgeRefused):
                    verify_descriptor_runtime(*descriptors)
        finally:
            for descriptor in descriptors:
                os.close(descriptor)

    def test_descriptor_generator_against_a_project_manifest_refuses(self) -> None:
        document = self._harness._manifest_document()
        self._harness._write_manifest(document)
        paths = (self.manifest, self.bridge, self.validator, self.generator)
        descriptors = [os.open(path, os.O_RDONLY) for path in paths]
        try:
            with mock.patch("worker.note_bridge.sys.executable", str(self.runtime)):
                with self.assertRaises(BridgeRefused):
                    verify_descriptor_runtime(
                        *descriptors[:3], generator_fd=descriptors[3]
                    )
        finally:
            for descriptor in descriptors:
                os.close(descriptor)

    def test_missing_transcript_refuses_without_loading_the_model(self) -> None:
        """Nothing before the first batch needs a model, so none is loaded."""
        (
            self.root / "meetings" / self.meeting_id / "transcript"
            / f"{self.transcript_id}.json"
        ).unlink()
        result, returncode, error, _ = self._run()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "transcript-only")
        self.assertEqual(result["failure"], {
            "code": "artifact-missing",
            "recoverable": True,
            "receipt": {},
        })
        # A multi-gigabyte model load in front of a refusal the model was never
        # needed for would also have spent the run's whole deadline.
        self.assertFalse(self.marker.exists())

    def test_the_request_packet_carries_the_registered_decoding_fields(self) -> None:
        """One candidate per request, temperature zero, and the advisory width.

        `num_ctx` is the one field the registration does not pin — the MLX
        child sizes its own context — so it is locked to `TRANSPORT_NUM_CTX`
        rather than to a literal restated here: what the module documents is
        what a provider is actually told.
        """
        self._write_generator(STUB_REPORTS_REQUEST)
        self._write_generate_manifest()
        result, _, _, _ = self._run()
        self.assertEqual(result["failure"]["code"], "no-model-candidates")
        reported = json.loads(self.marker.read_text())
        self.assertEqual(reported["num_ctx"], TRANSPORT_NUM_CTX)
        self.assertEqual(reported["temperature"], 0)
        self.assertEqual(reported["offered"], 1)
        self.assertGreater(reported["num_predict"], 0)

    def test_verified_model_directory_reaches_the_child(self) -> None:
        self._write_generator(STUB_REPORTS_MODEL_DIRECTORY)
        self._write_generate_manifest()
        result, _, _, _ = self._run()
        # Every candidate abstained, so there is no note; the point is the path.
        self.assertEqual(result["failure"]["code"], "no-model-candidates")
        self.assertEqual(
            self.marker.read_text(), str(self.root / self.MODEL_DIRECTORY)
        )

    def test_unpinned_model_file_refuses_before_the_generator_runs(self) -> None:
        (self.root / self.MODEL_DIRECTORY / "weights.npz").write_bytes(b"substituted\n")
        result, returncode, error, _ = self._run()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "transcript-only")
        self.assertEqual(result["failure"], {
            "code": "model-unavailable",
            "recoverable": True,
            "receipt": {},
        })
        self.assertFalse(self.marker.exists())

    def test_symlinked_model_directory_refuses_before_the_generator_runs(self) -> None:
        real = self.base / "real-model"
        shutil.move(str(self.root / self.MODEL_DIRECTORY), str(real))
        (self.root / self.MODEL_DIRECTORY).symlink_to(real, target_is_directory=True)
        result, returncode, error, _ = self._run()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "transcript-only")
        self.assertEqual(result["failure"]["code"], "model-unavailable")
        self.assertFalse(self.marker.exists())

    def test_extra_file_beside_the_pinned_model_refuses(self) -> None:
        private_file(self.root / self.MODEL_DIRECTORY / "extra.bin", b"extra\n")
        result, _, _, _ = self._run()
        self.assertEqual(result["failure"]["code"], "model-unavailable")
        self.assertFalse(self.marker.exists())

    def test_install_receipt_is_not_counted_as_a_pinned_model_file(self) -> None:
        private_file(self.root / self.MODEL_DIRECTORY / "model-install.json", b"{}\n")
        result, _, _, _ = self._run()
        self.assertEqual(result["outcome"], "generated")

    def test_deadline_outside_the_registered_bound_is_an_invalid_request(self) -> None:
        """3600 s is the registered harness budget; 3601 is not askable."""
        result, returncode, error, request_id = self._run(deadline_s=3601)
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["request_id"], request_id)
        self.assertEqual(result["outcome"], "transcript-only")
        self.assertEqual(result["failure"], {
            "code": "invalid-request",
            "recoverable": False,
            "receipt": {},
        })
        self.assertFalse(self.marker.exists())

    def test_the_registered_bound_itself_is_askable(self) -> None:
        """The negative alone would pass with the bound set anywhere below 3601.

        Paired with the case above this pins the bound to exactly the
        registered `PRODUCT_RUN` elapsed budget, which the old 900 s literal
        would fail.
        """
        self.assertEqual(GENERATOR_DEADLINE_S, 3600)
        result, returncode, error, _ = self._run(deadline_s=GENERATOR_DEADLINE_S)
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "generated")

    def test_a_bridge_whose_deadline_left_the_registration_never_reaches_ready(self) -> None:
        """The restated constant is bound to the registration, not merely equal to it.

        `GENERATOR_DEADLINE_S` cannot import the registration — the bridge is
        exec'd from verified bytes before any validator module exists — so the
        only thing stopping a second owner from drifting is this check. Moving
        the registration's budget inside the pinned bundle must refuse the
        bridge, not leave the ceiling silently behind.
        """
        self._replace_validator_registration_budget(3599)
        self._write_generate_manifest()
        bridge = self._start()
        try:
            self.assertIsNone(bridge.ready)
        finally:
            bridge.close()
        self.assertEqual(bridge._process.returncode, 2)
        self.assertFalse(self.marker.exists())

    def test_the_deadline_binding_does_not_reach_the_roles_that_never_use_it(self) -> None:
        """Scope, pinned in both directions.

        The generate role refuses the same divergent bundle (above). `project`
        is the shipped projection transport and reads no deadline at all, so a
        registration edit in `notes/` must not stop a note from being
        projected. Without this the check would sit in `load_validator` and
        silently widen to every role.
        """
        self._replace_validator_registration_budget(3599)
        self._harness.role = "project"
        self._harness._write_manifest()
        bridge = self._start()
        try:
            self.assertIsNotNone(bridge.ready)
            self.assertEqual(bridge.ready["role"], "project")
        finally:
            bridge.close()

    # Every test below was written after a blind mutation run: each weakens one
    # check and the whole suite still passed. They cover the checks that had no
    # witness, not the ones that looked risky.

    def test_non_canonical_manifest_bytes_never_reach_ready(self) -> None:
        """Same document, same field order, different indent — still refused."""
        document = self._harness._manifest_document()
        document["role"] = "generate"
        document["generator"] = {
            "relative_path": self.generator.name,
            "sha256": digest(self.generator),
        }
        document["models"] = self._model_entries()
        self.manifest.unlink(missing_ok=True)
        private_file(
            self.manifest, json.dumps(document, ensure_ascii=False, indent=4).encode()
        )
        bridge = self._start()
        try:
            self.assertIsNone(bridge.ready)
        finally:
            bridge.close()
        self.assertEqual(bridge._process.returncode, 2)

    def test_manifest_carrying_a_json_escape_never_reaches_ready(self) -> None:
        """Locks the behaviour; the escape check is not what currently enforces it.

        Measured, because the first version of this test was believed to cover
        the `b"\\\\" in raw` refusal and did not. Deleting that refusal leaves
        this passing, and so does deleting it together with the resource
        charset that looked like the backup — the manifest is refused because
        no file of that name can be opened. Every string field the manifest has
        today is either compared to a closed set or charset-constrained, so no
        backslash can reach anywhere the escape check is the only guard. It is
        defence in depth for a field that does not exist yet, and this test is
        a behavioural lock rather than a witness for it.
        """
        document = self._harness._manifest_document()
        document["role"] = "generate"
        document["generator"] = {
            "relative_path": f"{self.generator.name}\nsecond-line",
            "sha256": digest(self.generator),
        }
        document["models"] = self._model_entries()
        self._harness._write_manifest(document)
        self.assertIn(b"\\", self.manifest.read_bytes())
        bridge = self._start()
        try:
            self.assertIsNone(bridge.ready)
        finally:
            bridge.close()
        self.assertEqual(bridge._process.returncode, 2)

    def test_group_writable_model_directory_refuses(self) -> None:
        (self.root / self.MODEL_DIRECTORY).chmod(0o770)
        result, returncode, error, _ = self._run()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "transcript-only")
        self.assertEqual(result["failure"]["code"], "model-unavailable")
        self.assertFalse(self.marker.exists())

    def test_model_id_outside_the_shared_charset_never_reaches_ready(self) -> None:
        """A dot passes a path check and fails Rust's `valid_model_identifier`."""
        entries = self._model_entries()
        entries[0]["id"] = "note.generator.config"
        self._write_generate_manifest(models=entries)
        bridge = self._start()
        try:
            self.assertIsNone(bridge.ready)
        finally:
            bridge.close()
        self.assertEqual(bridge._process.returncode, 2)

    def test_response_past_the_byte_cap_is_transcript_only(self) -> None:
        self._write_generator(STUB_FLOODS_STDOUT)
        self._write_generate_manifest()
        result, returncode, error, _ = self._run()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "transcript-only")
        self.assertEqual(result["failure"]["code"], "response-length-truncation")

    # Round-two mutation survivors. Round one hardened twelve checks; these are
    # ten sites round one never touched, of which eight survived. A suite is not
    # done being wrong just because the last round of it was fixed.

    def test_a_storage_root_that_is_not_owner_private_never_reaches_ready(self) -> None:
        self.root.chmod(0o755)
        try:
            bridge = self._start()
            try:
                self.assertIsNone(bridge.ready)
            finally:
                bridge.close()
            self.assertEqual(bridge._process.returncode, 2)
        finally:
            self.root.chmod(0o700)

    def test_two_model_entries_pinning_one_digest_never_reach_ready(self) -> None:
        """An ambiguous pin cannot name which file is which."""
        entries = self._model_entries()
        entries[1]["sha256"] = entries[0]["sha256"]
        self._write_generate_manifest(models=entries)
        bridge = self._start()
        try:
            self.assertIsNone(bridge.ready)
        finally:
            bridge.close()
        self.assertEqual(bridge._process.returncode, 2)

    def test_two_model_entries_sharing_one_id_never_reach_ready(self) -> None:
        entries = self._model_entries()
        entries[1]["id"] = entries[0]["id"]
        self._write_generate_manifest(models=entries)
        bridge = self._start()
        try:
            self.assertIsNone(bridge.ready)
        finally:
            bridge.close()
        self.assertEqual(bridge._process.returncode, 2)

    def test_a_manifest_naming_a_different_file_with_the_same_bytes_is_refused(self) -> None:
        """Digest equality is not identity — but measure which check says so.

        Written to witness `_require_runtime_identity`'s bridge comparison, and
        it does not: deleting that comparison leaves this passing, because
        `_require_loaded_code_confined` independently refuses a `__main__` whose
        inode is not the manifested one. Removing both together is caught. So
        the self-identity check is redundancy, and this is a behavioural lock.
        """
        twin = self.resources / "note-bridge-twin.py"
        private_file(twin, self.bridge.read_bytes())
        self.assertEqual(digest(twin), digest(self.bridge))
        document = self._harness._manifest_document()
        document["role"] = "generate"
        document["bridge"] = {"relative_path": twin.name, "sha256": digest(twin)}
        document["generator"] = {
            "relative_path": self.generator.name,
            "sha256": digest(self.generator),
        }
        document["models"] = self._model_entries()
        self._harness._write_manifest(document)
        bridge = self._start()
        try:
            self.assertIsNone(bridge.ready)
        finally:
            bridge.close()
        self.assertEqual(bridge._process.returncode, 2)

    def test_a_command_for_another_role_is_a_protocol_failure(self) -> None:
        command = {
            "schema": "note-bridge-command/1",
            "request_id": str(uuid.uuid4()),
            "operation": "note.inspect",
            "arguments": {
                "meeting_id": self.meeting_id,
                "transcript_id": self.transcript_id,
                "model_directory": self.MODEL_DIRECTORY,
                "deadline_s": 20,
            },
        }
        bridge = self._start()
        try:
            result, returncode, error = bridge.send(json.dumps(command).encode() + b"\n")
        finally:
            bridge.close()
        self.assertIsNone(result)
        self.assertEqual((returncode, error), (2, b""))
        self.assertFalse(self.marker.exists())

    def test_a_model_file_rewritten_mid_run_refuses_after_generation(self) -> None:
        """The pinned model must still be the pinned model when the run ends.

        Witnesses the pathname half of `_ModelTree.require_unchanged`, not the
        descriptor half: deleting the `os.fstat` comparison alone leaves this
        passing, because `_require_link` re-stats the same file by name and an
        in-place rewrite changes its size. Removing both is caught. The two
        halves answer different questions — a replaced pathname versus a
        rewritten inode — and only one of them is exercised here.
        """
        self._write_generator(STUB_REWRITES_THE_MODEL)
        self._write_generate_manifest()
        result, returncode, error, _ = self._run()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "transcript-only")
        self.assertEqual(result["failure"]["code"], "model-unavailable")
        # It ran to completion first, so this is the post-generation recheck and
        # not the pre-flight verification refusing a bad tree.
        self.assertGreaterEqual(result["failure"]["receipt"]["responses"], 1)

    def test_a_transcript_with_no_classifiable_text_refuses_cleanly(self) -> None:
        """Turns that hold no words yield no candidates, and that is not a note.

        I reported this branch unreachable — the empty-transcript guard sits in
        front of it — and that was wrong. A whitespace-only turn passes that
        guard (the transcript does have a turn) and produces zero fragments, so
        zero candidates and zero batches. Reachable, and a real capture case: a
        meeting whose transcription produced only blank rows must refuse rather
        than return an empty note.
        """
        document = {
            "schema": "file-transcript/1",
            "source": "synthetic blank-capture control; not product evidence",
            "attribution": "none",
            "turns": [{"text": "   ", "start": float(index + 1)} for index in range(3)],
        }
        encoded = (json.dumps(document, indent=2) + "\n").encode()
        transcript_id = hashlib.sha256(encoded).hexdigest()
        private_file(
            self.root / "meetings" / self.meeting_id / "transcript" / f"{transcript_id}.json",
            encoded,
        )
        self._harness.transcript_id = transcript_id
        result, returncode, error, _ = self._run()
        self.assertEqual((returncode, error), (0, b""))
        self.assertEqual(result["outcome"], "transcript-only")
        self.assertEqual(result["failure"]["code"], "no-generatable-transcript")
        # Refused before any model was loaded, because no model was needed.
        self.assertFalse(self.marker.exists())

    def test_generate_command_may_not_name_a_note(self) -> None:
        with self.assertRaises(InvalidArguments):
            _parse_command(
                json.dumps({
                    "schema": "note-bridge-command/1",
                    "request_id": str(uuid.uuid4()),
                    "operation": "note.generate",
                    "arguments": {
                        "meeting_id": self.meeting_id,
                        "note_id": self.note_id,
                        "transcript_id": self.transcript_id,
                    },
                }).encode(),
                "generate",
            )

    def test_escaping_model_directory_is_an_invalid_request(self) -> None:
        result, _, _, _ = self._run(model_directory="models/../../etc")
        self.assertEqual(result["failure"]["code"], "invalid-request")
        self.assertFalse(self.marker.exists())



class NoteSynthesisValidationTests(unittest.TestCase):
    def test_synthesis_keeps_useful_rows_and_resolves_at_most_three_sources(self) -> None:
        sources = [
            {"alias": f"E{index:02d}", "candidate_id": f"candidate-{index}", "text": f"source {index}"}
            for index in range(1, 6)
        ]
        points = [
            {
                "candidate_id": source["candidate_id"],
                "locators": [{
                    "turn": index,
                    "start": 0,
                    "end": 8,
                    "text_sha256": f"{index:064x}",
                }],
            }
            for index, source in enumerate(sources)
        ]
        content = """```json
{"overview":[{"text":"The meeting covered the website plan.","evidence_ids":["E01","unknown","E02","E03","E04"]}],"items":[{"type":"proposal","text":"Explore a partner page.","evidence_ids":["E04"]},{"type":"decision","text":"The team is good to go.","evidence_ids":["E05"]}]}
```"""
        claims = _decode_synthesis(
            json.dumps({"content": content}), sources, points
        )
        self.assertEqual([row["claim_type"] for row in claims], ["summary", "proposal"])
        self.assertEqual(
            claims[0]["candidate_ids"],
            ["candidate-1", "candidate-2", "candidate-3"],
        )
        self.assertEqual(len(claims[0]["locators"]), 3)

    def test_synthesis_refuses_when_no_evidence_linked_overview_survives(self) -> None:
        with self.assertRaises(GenerationRefused) as raised:
            _decode_synthesis(
                json.dumps({"content": '{"overview":[],"items":[]}'}),
                [],
                [],
            )
        self.assertEqual(raised.exception.code, "no-model-summary")

    def test_synthesis_drops_decisions_and_actions_without_explicit_evidence(self) -> None:
        sources = [{
            "alias": "E01",
            "candidate_id": "candidate-1",
            "text": "We could explore a partner page and maybe update the roster.",
        }]
        points = [{
            "candidate_id": "candidate-1",
            "locators": [{
                "turn": 0,
                "start": 0,
                "end": 61,
                "text_sha256": "0" * 64,
            }],
        }]
        content = json.dumps({
            "overview": [{
                "text": "The meeting explored possible website changes.",
                "evidence_ids": ["E01"],
            }],
            "items": [
                {
                    "type": "decision",
                    "text": "The team decided to add a partner page.",
                    "evidence_ids": ["E01"],
                },
                {
                    "type": "action",
                    "text": "The team will update the roster.",
                    "evidence_ids": ["E01"],
                },
            ],
        })
        claims = _decode_synthesis(json.dumps({"content": content}), sources, points)
        self.assertEqual([row["claim_type"] for row in claims], ["summary"])


class NoteGeneratorChildTests(unittest.TestCase):
    """`worker/note_generator_mlx.py`, everywhere it can be checked without MLX.

    The decode itself needs the pinned weights and cannot run here — that is
    what the registered runs measure. Everything around it can: which locators
    the child is allowed to answer about, what it refuses, the bytes it
    assembles, and the session loop. And one thing more, which no runtime test
    could give: that the two blocks it duplicates from
    `notes/product_run.py` have not drifted from it.
    """

    DECIDE = (
        "    def decide(self, system: str, user: str, locators: list[str]) -> list[str]:\n"
        "        import mlx.core as mx\n"
    )
    ASSEMBLE = "def _assemble_contract_response(locators: list[str], verdicts: list[str]) -> str:"
    # The two names the child may spell differently, because it may not import
    # from `notes/`. Everything else in the mirrored blocks must be identical
    # byte for byte.
    RENAMES = (("self._loaded()", "self._load()"), ("_Refused(", "StructuredOutputError("))

    def setUp(self) -> None:
        sys.path.insert(0, str(REPO / "notes"))
        from worker import note_generator_mlx

        self.child = note_generator_mlx

    @staticmethod
    def _block(text: str, start: str, end: str) -> str:
        opened = text.index(start)
        return text[opened:text.index(end, opened)].rstrip()

    def _mirrored(self, start: str, reference_end: str, child_end: str) -> tuple[str, str]:
        reference = self._block(
            (REPO / "notes/product_run.py").read_text(encoding="utf-8"), start, reference_end
        )
        mirror = self._block(
            (REPO / "worker/note_generator_mlx.py").read_text(encoding="utf-8"), start, child_end
        )
        for local, referenced in self.RENAMES:
            mirror = mirror.replace(local, referenced)
        return reference, mirror

    def test_the_decode_has_not_drifted_from_its_reference_implementation(self) -> None:
        """The sync obligation, as a check rather than a comment.

        `notes/product_run.py::MLXVerdictTransport.decide` is what every
        registered measurement ran. This child duplicates it because the
        bridge's confined import path cannot reach `notes/`, so nothing but
        this comparison stops the shipped decode from quietly becoming a
        different configuration from the measured one.
        """
        reference, mirror = self._mirrored(
            self.DECIDE, "\n\ndef _assemble_contract_response", "    # --- end block mirrored"
        )
        self.assertEqual(reference, mirror)
        # Named individually, so a future edit that changes both files in the
        # same wrong direction still has to explain itself here.
        for registered in (
            "<start_of_turn>user\\n",
            "<end_of_turn>\\n",
            "tok.bos_token_id",
            "add_special_tokens=False",
            "make_prompt_cache",
            "chunk: int = 2048",
            'last[keep_first].item() > last[abstain_first].item()',
        ):
            self.assertIn(registered, mirror, registered)

    def test_the_response_assembly_has_not_drifted_from_its_reference(self) -> None:
        reference, mirror = self._mirrored(
            self.ASSEMBLE, "\n\ndef accept_all_decisions", "# --- end block mirrored"
        )
        self.assertEqual(reference, mirror)

    def _schema(self, count: int) -> tuple[dict, list[str]]:
        import candidate_first

        candidate_ids = [f"cf-{index:064x}" for index in range(count)]
        return candidate_first.classification_format(candidate_ids), candidate_ids

    def test_the_offered_locators_are_read_from_the_bridges_own_request_schema(self) -> None:
        """Not a hand-written shape: the schema the validator actually sends."""
        import candidate_first

        schema, candidate_ids = self._schema(1)
        self.assertEqual(
            self.child._offered_locators({"response_format": schema}),
            candidate_first.batch_locators(candidate_ids),
        )

    def test_a_request_that_offers_no_locators_is_refused(self) -> None:
        schema, _ = self._schema(3)
        broken = [
            {},
            {"response_format": {}},
            {"response_format": {"properties": {"items": {}}}},
            {"response_format": "not a schema"},
        ]
        for request in broken:
            with self.assertRaises(self.child._Refused):
                self.child._offered_locators(request)

    def test_a_malformed_locator_set_is_refused_rather_than_answered(self) -> None:
        """Each case must fail on its own defect, not on the wrapper.

        Measured: the first version of this test passed the bare schema where a
        request belongs, so every case refused on the missing `response_format`
        key and three locator checks could be deleted with the suite still
        green. The unmutated control below is what makes the rest of the loop
        mean anything.
        """
        import copy

        schema, _ = self._schema(3)

        def request(enum: object) -> dict:
            broken = copy.deepcopy(schema)
            broken["properties"]["items"]["items"]["properties"]["candidate_id"][
                "enum"
            ] = enum
            return {"response_format": broken}

        offered = schema["properties"]["items"]["items"]["properties"]["candidate_id"]["enum"]
        self.assertEqual(self.child._offered_locators(request(offered)), offered)
        for enum in (
            [],
            "c01",
            [offered[0], offered[0], offered[2]],
            [offered[0], 7, offered[2]],
            [offered[0], "", offered[2]],
        ):
            with self.assertRaises(self.child._Refused):
                self.child._offered_locators(request(enum))

    def test_a_model_directory_that_is_not_there_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            self.assertEqual(
                self.child._model_directory({"model_directory": temporary}), temporary
            )
            for value in (None, "", 7, str(Path(temporary) / "absent")):
                with self.assertRaises(self.child._Refused):
                    self.child._model_directory({"model_directory": value})

    def test_the_assembled_response_decodes_under_the_registered_contract(self) -> None:
        import candidate_first

        _schema, candidate_ids = self._schema(4)
        locators = candidate_first.batch_locators(candidate_ids)
        verdicts = ["KEEP", "ABSTAIN", "ABSTAIN", "KEEP"]
        raw = self.child._assemble_contract_response(locators, verdicts)
        decoded = candidate_first.decode_classification(raw, candidate_ids)
        self.assertEqual([row["verdict"] for row in decoded["items"]], verdicts)
        self.assertEqual(
            [row["candidate_id"] for row in decoded["items"]], candidate_ids
        )
        self.assertEqual(decoded["counts"]["out_of_order_positions"], 0)

    def _stub_session(self, verdict: str = "KEEP"):
        child = self.child

        class _Stub:
            resolved: list[str] = []

            def resolve(self, directory):
                _Stub.resolved.append(directory)

            def decide(self, system, user, locators):
                return [verdict] * len(locators)

            def synthesize(self, system, user, max_tokens):
                return json.dumps({"overview": [], "items": []})

        return _Stub

    def _drive(self, requests: list[dict], session=None) -> tuple[int, list[str]]:
        """Run the child's session loop over prepared lines, without MLX."""
        written: list[str] = []
        stdin = io.StringIO("".join(json.dumps(row) + "\n" for row in requests))
        holder = types.SimpleNamespace(
            write=lambda text: written.append(text), flush=lambda: None
        )
        with mock.patch.object(self.child, "_Session", session or self._stub_session()):
            with mock.patch.object(sys, "stdin", stdin), mock.patch.object(
                sys, "stdout", holder
            ):
                code = self.child.main()
        return code, "".join(written).splitlines()

    def _request(self, count: int = 2, **overrides) -> dict:
        schema, _ = self._schema(count)
        request = {
            "schema": "note-classification-request/1",
            "system": "classify",
            "user": "candidates",
            "response_format": schema,
            "temperature": 0.0,
            "model_directory": "/",
        }
        request.update(overrides)
        return request

    def test_the_session_answers_one_line_per_request_until_stdin_closes(self) -> None:
        session = self._stub_session()
        code, lines = self._drive(
            [self._request(2), self._request(3)], session=session
        )
        self.assertEqual(code, 0)
        self.assertEqual(len(lines), 2)
        self.assertEqual(
            [len(json.loads(line)["items"]) for line in lines], [2, 3]
        )
        # Every request resolves the model directory it names. The counterpart
        # of the refusal case below, which asserts the opposite: without this,
        # a child that never resolved the model at all would still pass.
        self.assertEqual(session.resolved, ["/", "/"])

    def test_a_request_the_child_cannot_answer_writes_one_error_line_and_exits(self) -> None:
        """The bridge reads the line, not the exit code — so there must be a line."""
        code, lines = self._drive([self._request(2, temperature=0.7), self._request(2)])
        self.assertEqual(code, 1)
        self.assertEqual(len(lines), 1)
        self.assertEqual(list(json.loads(lines[0])), ["error"])
        # An error line is not a classifier response, which is exactly how the
        # bridge turns it into a refusal.
        self.assertNotIn("items", lines[0])

    def test_a_request_with_no_prompt_is_refused_before_any_model_is_touched(self) -> None:
        session = self._stub_session()
        code, lines = self._drive([self._request(2, system="")], session=session)
        self.assertEqual(code, 1)
        self.assertEqual(list(json.loads(lines[0])), ["error"])
        self.assertEqual(session.resolved, [])

    def test_the_session_returns_synthesis_as_one_transport_line(self) -> None:
        request = {
            "schema": "note-synthesis-request/1",
            "system": "summarize",
            "user": "selected excerpts",
            "temperature": 0,
            "num_predict": 1800,
            "model_directory": "/",
        }
        code, lines = self._drive([request])
        self.assertEqual(code, 0)
        self.assertEqual(len(lines), 1)
        self.assertEqual(
            json.loads(json.loads(lines[0])["content"]),
            {"overview": [], "items": []},
        )


class FrameSerializationContractTests(unittest.TestCase):
    """`ensure_ascii=False` on every outbound frame is a size contract."""

    def _emitted(self, value: dict) -> bytes:
        stream = io.BytesIO()
        holder = types.SimpleNamespace(buffer=stream)
        with mock.patch.object(sys, "stdout", holder):
            _emit(value)
        return stream.getvalue()

    def _non_ascii_frame(self) -> tuple[dict, str]:
        """A result frame carrying the fixture's own non-ASCII text.

        Worth stating plainly: the shared fixture has exactly one non-ASCII
        string, `transcript_turns[3]`, and no result frame in it carries any.
        So the cross-language fixture never exercises a non-ASCII *frame* —
        which is why the escaping cost went unnoticed on both sides. This
        borrows that text into a claim to exercise the writer; no validation
        runs on it, because the serializer is what is under test.
        """
        fixture = json.loads(
            (REPO / "tests/fixtures/note-projection-v1.fixture").read_text(encoding="utf-8")
        )
        text = fixture["transcript_turns"][3]
        result = fixture["valid_results"][1]["result"]
        result["projection"]["claims"][4]["claim"] = text
        return result, text

    def test_non_ascii_frames_are_written_raw_and_never_escaped(self) -> None:
        result, text = self._non_ascii_frame()
        self.assertTrue(any(ord(character) > 127 for character in text))
        frame = self._emitted(result)
        # The escape is what costs six bytes per scalar against a fixed frame.
        self.assertNotIn(b"\\u", frame)
        self.assertIn(text.encode("utf-8"), frame)
        self.assertEqual(json.loads(frame.decode("utf-8")), result)

    def test_escaping_the_same_frame_would_inflate_it(self) -> None:
        """The contract is load bearing, not cosmetic — measure the delta."""
        result, _ = self._non_ascii_frame()
        raw = len(self._emitted(result))
        escaped = len(
            json.dumps(result, separators=(",", ":"), ensure_ascii=True).encode() + b"\n"
        )
        self.assertGreater(escaped, raw)

    def test_a_frame_over_the_limit_raises_instead_of_writing_a_partial(self) -> None:
        oversized = {"schema": "note-bridge-result/1", "pad": "x" * (MAX_FRAME_BYTES + 1)}
        stream = io.BytesIO()
        holder = types.SimpleNamespace(buffer=stream)
        with mock.patch.object(sys, "stdout", holder), self.assertRaises(BridgeRefused):
            _emit(oversized)
        self.assertEqual(stream.getvalue(), b"")


class EvidenceRuleTests(unittest.TestCase):
    """The note/2 locator rule, exercised directly.

    A blind mutation run widened the cap to 1..32 and dropped the sorted and
    duplicate checks, and every test still passed: the generate path emits one
    locator per point and every stored fixture is already well formed, so
    nothing in either lane supplied a locator set the rule was supposed to
    reject. These tests supply them.

    Deliberately unit tests rather than fixture rows. The same rule is enforced
    in `note_projection.rs`, and the shared fixture is the right place for
    cross-language parity — but it is indexed positionally by two suites, so it
    is being widened once, after both branches land.
    """

    TURNS = ("Alpha beta gamma delta.", "Epsilon zeta eta theta.")

    def _transcript(self):
        from transcript import load_bytes

        document = {
            "schema": "file-transcript/1",
            "source": "synthetic evidence-rule control; not product evidence",
            "attribution": "none",
            "turns": [
                {"text": text, "start": float(index + 1)}
                for index, text in enumerate(self.TURNS)
            ],
        }
        return load_bytes((json.dumps(document, indent=2) + "\n").encode(), source="rule")

    def _reference(self, turn: int, start: int, end: int) -> dict:
        text = self.TURNS[turn][start:end]
        return {
            "turn": turn,
            "char_start": start,
            "char_end": end,
            "text_sha256": hashlib.sha256(text.encode("utf-8")).hexdigest(),
        }

    def test_a_well_formed_locator_set_passes(self) -> None:
        transcript = self._transcript()
        locators = validate_locators(
            [self._reference(0, 0, 5), self._reference(1, 0, 7)], transcript
        )
        self.assertEqual(len(locators), 2)

    def test_more_than_three_locators_is_refused(self) -> None:
        transcript = self._transcript()
        references = [
            self._reference(0, 0, 5),
            self._reference(0, 6, 10),
            self._reference(0, 11, 16),
            self._reference(1, 0, 7),
        ]
        with self.assertRaises(ArtifactFailure):
            validate_locators(references, transcript)

    def test_an_empty_locator_set_is_refused(self) -> None:
        with self.assertRaises(ArtifactFailure):
            validate_locators([], self._transcript())

    def test_out_of_order_locators_are_refused(self) -> None:
        transcript = self._transcript()
        references = [self._reference(1, 0, 7), self._reference(0, 0, 5)]
        with self.assertRaises(ArtifactFailure):
            validate_locators(references, transcript)

    def test_duplicate_locators_are_refused(self) -> None:
        transcript = self._transcript()
        references = [self._reference(0, 0, 5), self._reference(0, 0, 5)]
        with self.assertRaises(ArtifactFailure):
            validate_locators(references, transcript)

    def _claim_row(self, claim: str) -> dict:
        span = self.TURNS[0][0:10]
        return {
            "claim": claim,
            "type": "decision",
            # Encoding a lone surrogate raises, so a row carrying one gets a
            # placeholder. The forbidden-character rule short-circuits ahead of
            # the digest comparison, which is exactly what is under test.
            "claim_sha256": (
                hashlib.sha256(claim.encode("utf-8", "surrogatepass")).hexdigest()
            ),
            "evidence_refs": [
                {
                    "turn": 0,
                    "char_start": 0,
                    "char_end": 10,
                    "text_sha256": hashlib.sha256(span.encode("utf-8")).hexdigest(),
                }
            ],
        }

    def test_an_ordinary_claim_passes(self) -> None:
        """Positive control for the refusals below."""
        claims = validate_claim_rows([self._claim_row("we agreed to ship")], self._transcript())
        self.assertEqual(len(claims), 1)

    def test_control_characters_in_claim_text_are_refused(self) -> None:
        """Matches `note_projection.rs`, which refuses these content-free.

        Every one of these was accepted here until measured. A claim this
        validator admits and Rust refuses does not render wrong — it aborts the
        library rebuild for an unrelated reason, because the refusal carries no
        content by design.
        """
        transcript = self._transcript()
        for label, code_point in (
            ("newline", 0x0A),
            ("carriage return", 0x0D),
            ("tab", 0x09),
            ("null", 0x00),
            ("delete", 0x7F),
            ("C1 next-line", 0x85),
            ("line separator", 0x2028),
            ("paragraph separator", 0x2029),
        ):
            with self.subTest(label):
                claim = "we agreed" + chr(code_point) + "to ship"
                with self.assertRaises(ArtifactFailure):
                    validate_claim_rows([self._claim_row(claim)], transcript)

    def test_the_forbidden_set_covers_rusts_and_says_where_it_is_wider(self) -> None:
        """Cc and the two separators mirror `forbidden()`; Cs is deliberately extra.

        Rust's `forbidden()` does not name surrogates because serde_json refuses
        the escape at parse, so a surrogate can never reach that function. Python
        has no such parser guarantee — `json.loads` returns a lone surrogate
        happily — so the set here is wider by exactly Cs. Behaviour matches; the
        two enforcement points do not, and the asymmetry is the point of the
        test.
        """
        for point in range(0x00, 0x20):
            self.assertTrue(forbidden_in_claim(chr(point)), hex(point))
        for point in range(0x7F, 0xA0):
            self.assertTrue(forbidden_in_claim(chr(point)), hex(point))
        self.assertTrue(forbidden_in_claim(chr(0x2028)))
        self.assertTrue(forbidden_in_claim(chr(0x2029)))
        for point in (0xD800, 0xDBFF, 0xDC00, 0xDFFF):
            self.assertTrue(forbidden_in_claim(chr(point)), hex(point))
        for allowed in (" ", "a", "\u00e9", "\U0001f642", "\u00a0", "\u200b"):
            self.assertFalse(forbidden_in_claim(allowed), allowed)

    def test_a_lone_surrogate_is_refused_as_a_rule_not_as_an_encoding_error(self) -> None:
        """It was already refused; it was not refused *by a rule*.

        `json.loads` produces a lone surrogate from a `\\ud800` escape, and the
        old path stopped it only when the claim digest encoded to UTF-8 and
        raised — a refusal that depended on the digest check's position in a
        boolean chain. The outcome was right and the reason was accidental.
        """
        lone = json.loads('"\\ud800"')
        self.assertEqual(unicodedata.category(lone), "Cs")
        row = self._claim_row("we agreed " + lone + " to ship")
        with self.assertRaises(ArtifactFailure) as caught:
            validate_claim_rows([row], self._transcript())
        # The rule refuses it, so the failure is the validator's closed code and
        # not a UnicodeEncodeError escaping from the digest computation.
        self.assertEqual(caught.exception.code, "artifact-invalid")

    def test_a_claim_past_the_former_cap_still_passes(self) -> None:
        """No length cap: candidate-first claims are verbatim transcript
        excerpts, unbounded by `candidate_first.py`'s fragment spans, and
        write time never caps them either. `note_projection.rs`'s matching
        cap was removed the same change; both sides now agree there is none.
        """
        transcript = self._transcript()
        past_former_cap = "a" * 400
        self.assertEqual(
            len(validate_claim_rows([self._claim_row(past_former_cap)], transcript)), 1
        )

    def test_a_long_claim_counts_characters_not_bytes_for_other_rules(self) -> None:
        """160 emoji are 640 bytes and still one legal claim.

        Length carries no rule now, but other checks (the digest, the
        forbidden-character set) must still operate on characters, not bytes
        -- the same divergence class as the control characters, pointing the
        other way.
        """
        claim = "\U0001f642" * 160
        self.assertGreater(len(claim.encode("utf-8")), 160)
        claims = validate_claim_rows([self._claim_row(claim)], self._transcript())
        self.assertEqual(len(claims), 1)

    def test_an_empty_claim_is_refused(self) -> None:
        with self.assertRaises(ArtifactFailure):
            validate_claim_rows([self._claim_row("")], self._transcript())

    def test_an_empty_span_is_refused(self) -> None:
        """`end == start` cites nothing; a locator must point at some text."""
        transcript = self._transcript()
        with self.assertRaises(ArtifactFailure):
            validate_locators([self._reference(0, 3, 3)], transcript)

    def test_a_reversed_span_is_refused(self) -> None:
        transcript = self._transcript()
        reference = self._reference(0, 0, 5)
        reference["char_start"], reference["char_end"] = 5, 0
        with self.assertRaises(ArtifactFailure):
            validate_locators([reference], transcript)

    def test_a_digest_that_does_not_describe_the_span_is_refused(self) -> None:
        transcript = self._transcript()
        reference = self._reference(0, 0, 5)
        reference["text_sha256"] = hashlib.sha256(b"something else").hexdigest()
        with self.assertRaises(ArtifactFailure):
            validate_locators([reference], transcript)

    def test_a_candidate_from_another_transcript_view_is_refused(self) -> None:
        """The view digest is the only thing tying candidates to this transcript."""
        transcript = self._transcript()
        manifest = {"transcript_view_sha256": "0" * 64}
        with self.assertRaises(GenerationRefused) as caught:
            locate_kept_candidates(manifest, [], transcript)
        self.assertEqual(caught.exception.code, "citation-locator")


class RuntimeConfinementTests(unittest.TestCase):
    """`_confine_runtime_imports` refuses an import path it did not sanction.

    Reached as a unit because the subprocess harness cannot construct the state:
    the bridge always starts with `-I -S -E -s`, so site initialization never
    runs and site-packages never lands on `sys.path`. That made the check look
    unreachable, which is a claim about the *gate in front of it*, not about the
    check itself.

    Every refusal here is asserted by message, not merely by type. The first
    version of this class used the real interpreter's site-packages and passed
    with the site-packages check deleted — Homebrew's site-packages resolves
    outside the resolved base prefix, so the *prefix-escape* refusal fired and
    `assertRaises(BridgeRefused)` could not tell the two apart. A synthetic
    prefix puts the directory genuinely inside, so the intended branch is the
    one that runs.
    """

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.prefix = Path(self.temporary.name).resolve()
        self.stdlib = self.prefix / "lib" / f"python3.{sys.version_info.minor}"
        self.stdlib.mkdir(mode=0o700, parents=True)
        self.site_packages = self.stdlib / "site-packages"
        self.site_packages.mkdir(mode=0o700)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def _confine_with(self, entries: list[str]):
        """Call the confiner with sanctioned flags and a chosen import path."""
        flags = types.SimpleNamespace(**dict(_REQUIRED_FLAGS))
        saved_path = list(sys.path)
        saved_modules = {
            name: sys.modules.pop(name)
            for name in ("site", "sitecustomize", "usercustomize")
            if name in sys.modules
        }
        try:
            with (
                mock.patch.object(sys, "flags", flags),
                mock.patch.object(sys, "base_prefix", str(self.prefix)),
            ):
                sys.path[:] = entries
                return _confine_runtime_imports()
        finally:
            sys.path[:] = saved_path
            sys.modules.update(saved_modules)

    def test_a_standard_library_path_is_sanctioned(self) -> None:
        """Positive control: without it, the refusals below prove nothing."""
        approved = self._confine_with([str(self.stdlib)])
        self.assertIn(self.stdlib, approved)

    def test_site_packages_inside_the_prefix_is_refused_by_name(self) -> None:
        with self.assertRaises(BridgeRefused) as caught:
            self._confine_with([str(self.site_packages)])
        self.assertIn("site packages", str(caught.exception))

    def test_a_path_outside_the_interpreter_prefix_is_refused_by_name(self) -> None:
        with self.assertRaises(BridgeRefused) as caught:
            self._confine_with([str(REPO)])
        self.assertIn("escapes the interpreter prefix", str(caught.exception))

    def test_an_attested_runtime_with_no_library_path_is_refused(self) -> None:
        with self.assertRaises(BridgeRefused) as caught:
            self._confine_with([])
        self.assertIn("no standard-library path", str(caught.exception))

    def test_an_unsanctioned_flag_is_refused_before_any_path_is_read(self) -> None:
        flags = types.SimpleNamespace(**{**_REQUIRED_FLAGS, "no_site": 0})
        with mock.patch.object(sys, "flags", flags), self.assertRaises(BridgeRefused) as caught:
            _confine_runtime_imports()
        self.assertIn("isolated no-site mode", str(caught.exception))


if __name__ == "__main__":
    unittest.main()
