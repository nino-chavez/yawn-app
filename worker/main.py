#!/usr/bin/env python3
"""Protocol-only application worker; no research CLI options are accepted."""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import os
import select
import sys
import threading
import uuid
from pathlib import Path

from worker.adapters import AdapterRefused, dispatch
from worker.storage import StorageRefused, require_private_root

MAX_FRAME_BYTES = 64 * 1024
TRANSCRIPTION_HEARTBEAT_SECONDS = 3
NOTE_GENERATION_HEARTBEAT_SECONDS = 3
_PROTOCOL_WRITE_LOCK = threading.Lock()
# sitting.derive joined the packaged alpha set on 2026-08-04 by the operator's
# guided-enrollment registration decision, following the same-day encoder
# admission verdict (spike/encoder-packaging/RESULTS.md): the shipped recorder
# needs the derivation the sitting evidence store re-verifies. On the default
# lane the adapter still refuses ("runtime has no admitted speaker encoder"),
# so widening the set opens nothing a placeholder-encoder build can misuse.
# transcript.restore joined with the same day's correction-surface (J4)
# registration: the restoration coordinator re-verifies every artifact the
# worker names before anything is published.
# The profile family joined on 2026-08-05 with the operator's profile-build
# decision: choices/build produce from stored evidence through the canonical
# save_profile boundary, and inspect/discard serve the Rust lifecycle's
# strict-loader bridge. profile.adopt stays boundary-lane only — the
# packaged publication path is Rust's enroll_profile_candidate, which
# publishes the re-verified bytes itself.
# corpus.embed joined this set on 2026-08-08, when `build-alpha` began staging
# the embedding model and its tokenizer. It sat in the boundary lane for one
# change so the shipped worker would not advertise a capability it could only
# refuse; that reason expired the moment the model was in the bundle. Note what
# it does *not* mean: the model is packaged and measured, and whether its
# retrieval is useful is still the operator's judgment.
# note.create joined 2026-08-16 with the generation-invocation decision --
# docs/note-runtime-decision.md, slice 2. Its generator is the deterministic
# candidate-point assembler built from the request's own generation payload;
# the model still runs only in the sandboxed one-shot bridge child, never in
# this worker. The comment sits above the literal because Rust's lockstep
# test parses the frozenset source up to its first closing parenthesis.
# note.inspect joined 2026-08-16 with the operational close-out of the same
# decision: it sat boundary-lane-only while note.create's publication path
# was unproven, the same staging corpus.embed used while its model was
# unpackaged. The first real end-to-end run showed the gap -- a published
# note could never clear the re-inspection slice 3 requires before the
# meeting record advances, because the standing worker refused the operation
# outright under internal-alpha admission. Promoted for the same reason
# corpus.embed was: the packaging it was waiting on is done.
ALPHA_OPERATIONS = frozenset(
    {"capture.finalize", "capture.inspect", "transcript.create", "transcript.retry",
     "sitting.derive", "transcript.restore", "corpus.embed",
     "profile.choices", "profile.build", "profile.inspect", "profile.discard",
     "note.create", "note.inspect"}
)
BOUNDARY_OPERATIONS = ALPHA_OPERATIONS | frozenset(
    {"profile.adopt"}
)


def operations_for(admission: str) -> frozenset[str]:
    if admission == "internal-alpha":
        return ALPHA_OPERATIONS
    if admission in {"boundary-test", "product"}:
        return BOUNDARY_OPERATIONS
    raise ValueError("runtime admission is not current")


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def emit(value: dict, *, protocol_output=None) -> None:
    encoded = json.dumps(value, separators=(",", ":"), sort_keys=True)
    if len(encoded.encode("utf-8")) > MAX_FRAME_BYTES:
        raise RuntimeError("outbound protocol frame exceeds limit")
    output = protocol_output if protocol_output is not None else sys.stdout
    with _PROTOCOL_WRITE_LOCK:
        output.write(encoded + "\n")
        output.flush()


def emit_progress(
    request_id: str,
    meeting_id: str,
    state: str,
    *,
    protocol_output=None,
) -> None:
    uuid.UUID(request_id)
    uuid.UUID(meeting_id)
    if state not in {"recording", "transcribing", "finishing"}:
        raise ValueError("progress state is outside the closed protocol")
    emit(
        {
            "schema": "worker-event/2",
            "request_id": request_id,
            "event": "capture.state",
            "state": state,
            "meeting_id": meeting_id,
        },
        protocol_output=protocol_output,
    )


def start_transcription_heartbeat(
    request_id: str,
    operation: str,
    arguments: object,
    *,
    protocol_output,
) -> tuple[threading.Event, threading.Thread] | None:
    if operation not in {"transcript.create", "transcript.retry"} or not isinstance(arguments, dict):
        return None
    meeting_id = arguments.get("meeting_id")
    if not isinstance(meeting_id, str):
        return None

    stopped = threading.Event()

    def emit_heartbeat() -> None:
        while not stopped.wait(TRANSCRIPTION_HEARTBEAT_SECONDS):
            try:
                emit_progress(
                    request_id,
                    meeting_id,
                    "transcribing",
                    protocol_output=protocol_output,
                )
            except Exception:
                # The terminal result path remains authoritative. A broken
                # protocol stream or malformed local input must not leave a
                # background thread running after its request ends.
                return

    thread = threading.Thread(
        target=emit_heartbeat,
        name="transcription-heartbeat",
        daemon=True,
    )
    thread.start()
    return stopped, thread


def start_note_generation_heartbeat(
    request_id: str,
    operation: str,
    arguments: object,
    *,
    protocol_output,
) -> tuple[threading.Event, threading.Thread] | None:
    if operation != "note.create" or not isinstance(arguments, dict):
        return None
    meeting_id = arguments.get("meeting_id")
    if not isinstance(meeting_id, str):
        return None

    stopped = threading.Event()

    def emit_heartbeat() -> None:
        while not stopped.wait(NOTE_GENERATION_HEARTBEAT_SECONDS):
            try:
                emit_progress(
                    request_id,
                    meeting_id,
                    "finishing",
                    protocol_output=protocol_output,
                )
            except Exception:
                # The terminal result path remains authoritative. A broken
                # protocol stream or malformed local input must not leave a
                # background thread running after its request ends.
                return

    thread = threading.Thread(
        target=emit_heartbeat,
        name="note-generation-heartbeat",
        daemon=True,
    )
    thread.start()
    return stopped, thread


def load_manifest(path: Path) -> dict:
    document = json.loads(path.read_text(encoding="utf-8"))
    required = {
        "schema",
        "admission",
        "runtime",
        "worker",
        "tap",
        "encoder",
        "permission_probe",
        "models",
    }
    if isinstance(document, dict) and document.get("schema") in {"app-runtime/2", "app-runtime/3"}:
        required.add("model_catalog")
    if isinstance(document, dict) and document.get("schema") == "app-runtime/3":
        required.add("apple_speech")
    if not isinstance(document, dict) or set(document) != required:
        raise ValueError("runtime manifest has the wrong shape")
    if document["schema"] not in {"app-runtime/1", "app-runtime/2", "app-runtime/3"}:
        raise ValueError("runtime manifest schema is not current")
    if document["admission"] not in {"boundary-test", "internal-alpha", "product"}:
        raise ValueError("runtime manifest admission is not current")
    if document["schema"] == "app-runtime/3" and document["admission"] != "internal-alpha":
        raise ValueError("Apple speech requires internal-alpha admission")
    resources = ["runtime", "worker", "tap", "encoder", "permission_probe"]
    if document["schema"] in {"app-runtime/2", "app-runtime/3"}:
        resources.append("model_catalog")
    if document["schema"] == "app-runtime/3":
        resources.append("apple_speech")
    for name in resources:
        entry = document[name]
        if not isinstance(entry, dict) or set(entry) != {"path", "sha256"}:
            raise ValueError(f"runtime manifest {name} entry is malformed")
        unresolved = path.parent / entry["path"]
        if unresolved.is_symlink():
            raise ValueError(f"runtime manifest {name} resource may not be a symlink")
        resource = unresolved.resolve(strict=True)
        if path.parent.resolve() not in resource.parents and resource != path.parent.resolve():
            raise ValueError("runtime manifest resource escapes its directory")
        if not resource.is_file() or file_sha256(resource) != entry["sha256"]:
            raise ValueError(f"runtime manifest {name} digest mismatch")
    if not isinstance(document["models"], list):
        raise ValueError("runtime manifest models must be a list")
    seen_models = set()
    for model in document["models"]:
        if not isinstance(model, dict) or set(model) != {"id", "path", "sha256"}:
            raise ValueError("runtime manifest model entry is malformed")
        if (
            not isinstance(model["id"], str)
            or not model["id"]
            or model["id"] in seen_models
        ):
            raise ValueError("runtime manifest model ID is invalid or duplicated")
        seen_models.add(model["id"])
        unresolved = path.parent / model["path"]
        if unresolved.is_symlink():
            raise ValueError("runtime manifest model may not be a symlink")
        resource = unresolved.resolve(strict=True)
        if path.parent.resolve() not in resource.parents and resource != path.parent.resolve():
            raise ValueError("runtime manifest model escapes its directory")
        if not resource.is_file() or file_sha256(resource) != model["sha256"]:
            raise ValueError("runtime manifest model digest mismatch")
    return document


def transcript_model_dir(manifest_path: Path, manifest: dict) -> Path | None:
    if manifest["admission"] == "boundary-test":
        return None
    resources = {model["id"]: model for model in manifest["models"]}
    expected = {
        "whisper-large-v3-turbo-config",
        "whisper-large-v3-turbo-weights",
    }
    if not expected.issubset(resources):
        raise ValueError("runtime manifest lacks the fixed transcript model")
    parents = {
        (manifest_path.parent / resources[model_id]["path"]).resolve(strict=True).parent
        for model_id in expected
    }
    if len(parents) != 1:
        raise ValueError("fixed transcript model resources do not share one directory")
    return parents.pop()


def external_transcript_model(
    root: Path,
    manifest_path: Path,
    manifest: dict,
    receipt_path: Path | None,
) -> tuple[Path, list[dict]]:
    if manifest["schema"] not in {"app-runtime/2", "app-runtime/3"} or receipt_path is None:
        raise ValueError("external transcript model receipt is unavailable")
    root = root.resolve(strict=True)
    expected_parent = root / "models"
    receipt = receipt_path.resolve(strict=True)
    if receipt_path.is_symlink() or receipt.name != "model-install.json":
        raise ValueError("external transcript model receipt is unsafe")
    document = json.loads(receipt.read_text(encoding="utf-8"))
    if not isinstance(document, dict) or set(document) != {"schema", "id", "revision", "files"}:
        raise ValueError("external transcript model receipt is malformed")
    if document["schema"] != "yawn-model-install/1":
        raise ValueError("external transcript model receipt schema is not current")
    model_dir = receipt.parent
    if model_dir.parent.parent != expected_parent:
        raise ValueError("external transcript model is outside private model storage")
    if model_dir.is_symlink() or model_dir.parent.is_symlink() or not model_dir.is_dir():
        raise ValueError("external transcript model directory is unsafe")

    catalog_entry = manifest["model_catalog"]
    catalog_path = manifest_path.parent / catalog_entry["path"]
    catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
    # `note_models` joined the catalog with the note-generation seam; this
    # reader consumes only the transcript entries, so the key is admitted by
    # shape alone and its contents are judged where they are used (the desktop
    # verifies the whole catalog against the manifest digest before any use).
    if not isinstance(catalog, dict) or set(catalog) - {"note_models"} != {"schema", "models"}:
        raise ValueError("transcript model catalog is malformed")
    if catalog["schema"] != "yawn-model-catalog/1" or not isinstance(catalog["models"], list):
        raise ValueError("transcript model catalog schema is not current")
    if "note_models" in catalog and not isinstance(catalog["note_models"], list):
        raise ValueError("transcript model catalog schema is not current")
    matches = [
        model
        for model in catalog["models"]
        if isinstance(model, dict)
        and model.get("id") == document["id"]
        and model.get("revision") == document["revision"]
    ]
    if len(matches) != 1:
        raise ValueError("installed transcript model is not in the signed catalog")
    model = matches[0]
    required_model = {
        "id", "revision", "title", "detail", "downloadBytes", "installedBytes", "files"
    }
    if set(model) != required_model or not isinstance(model["files"], list):
        raise ValueError("signed transcript model entry is malformed")
    if not isinstance(document["files"], list) or len(document["files"]) != len(model["files"]):
        raise ValueError("installed transcript model inventory is incomplete")
    if not all(isinstance(file, dict) for file in document["files"]):
        raise ValueError("installed transcript model file receipt is malformed")

    actual_names = {item.name for item in model_dir.iterdir()}
    expected_names = {"model-install.json"}
    ready_models = []
    roles = set()
    for expected in model["files"]:
        if not isinstance(expected, dict) or set(expected) != {
            "role", "name", "url", "bytes", "sha256"
        }:
            raise ValueError("signed transcript model file is malformed")
        role = expected["role"]
        name = expected["name"]
        if role not in {"config", "weights"} or role in roles:
            raise ValueError("signed transcript model roles are invalid")
        roles.add(role)
        recorded = [file for file in document["files"] if file.get("role") == role]
        if len(recorded) != 1 or set(recorded[0]) != {"role", "name", "bytes", "sha256"}:
            raise ValueError("installed transcript model file receipt is malformed")
        if any(recorded[0].get(key) != expected.get(key) for key in ("role", "name", "bytes", "sha256")):
            raise ValueError("installed transcript model disagrees with the signed catalog")
        path = model_dir / name
        if path.is_symlink() or not path.is_file() or path.stat().st_size != expected["bytes"]:
            raise ValueError("installed transcript model file is unsafe or incomplete")
        if file_sha256(path) != expected["sha256"]:
            raise ValueError("installed transcript model file digest mismatch")
        expected_names.add(name)
        ready_models.append(
            {
                "id": f"whisper-transcript-{role}",
                "digest": expected["sha256"],
                "available": True,
            }
        )
    if roles != {"config", "weights"} or actual_names != expected_names:
        raise ValueError("installed transcript model directory is not exact")
    return model_dir, ready_models


# The four files a MiniLM directory needs. All four or none: a directory missing
# `tokenizer.json` would load and then embed with whatever tokenizer happened to
# be importable, which is the substitution `packaged_tokenizer_receipt.json`
# exists to have measured rather than assumed.
EMBEDDING_MODEL_IDS = frozenset(
    {
        "all-minilm-l6-v2-config",
        "all-minilm-l6-v2-sentence-config",
        "all-minilm-l6-v2-tokenizer",
        "all-minilm-l6-v2-weights",
    }
)


def embedding_model_dir(manifest_path: Path, manifest: dict) -> Path | None:
    """The packaged embedding model, or `None` when no lane carries one.

    `None` is the current answer in every lane and the adapter refuses on it.
    That is the honest state: returning a directory that does not exist would
    turn a packaging gap into an import error at request time.
    """
    resources = {model["id"]: model for model in manifest["models"]}
    if not EMBEDDING_MODEL_IDS.issubset(resources):
        return None
    parents = {
        (manifest_path.parent / resources[model_id]["path"]).resolve(strict=True).parent
        for model_id in EMBEDDING_MODEL_IDS
    }
    if len(parents) != 1:
        raise ValueError("packaged embedding model resources do not share one directory")
    return parents.pop()


def parse_command(
    frame: bytes, operations: frozenset[str]
) -> tuple[str, str, object]:
    if len(frame) > MAX_FRAME_BYTES:
        raise ValueError("protocol frame exceeds limit")
    command = json.loads(frame)
    if not isinstance(command, dict) or set(command) != {
        "schema",
        "request_id",
        "operation",
        "arguments",
    }:
        raise ValueError("command has the wrong shape")
    if command["schema"] != "worker-command/2" or command["operation"] not in operations:
        raise ValueError("command schema or operation is unsupported")
    uuid.UUID(command["request_id"])
    return command["request_id"], command["operation"], command["arguments"]


def parent_is_alive(parent_fd: int) -> bool:
    ready, _, _ = select.select([parent_fd], [], [], 0)
    if not ready:
        return True
    return os.read(parent_fd, 1) != b""


def start_parent_liveness_watchdog(parent_fd: int) -> threading.Thread:
    def exit_at_eof() -> None:
        while True:
            try:
                value = os.read(parent_fd, 1)
            except InterruptedError:
                continue
            except OSError:
                os._exit(2)
            if value == b"":
                os._exit(0)

    thread = threading.Thread(
        target=exit_at_eof,
        name="parent-liveness-watchdog",
        daemon=True,
    )
    thread.start()
    return thread


def dispatch_without_protocol_output(
    root: Path,
    operation: str,
    arguments: object,
    *,
    encoder_digest: str,
    admission: str,
    model_dir: Path | None,
    encoder_path: Path | None = None,
    embedding_dir: Path | None = None,
    transcription_engine: str = "whisper",
    speech_locale: str = "en-US",
    apple_speech_helper: Path | None = None,
    apple_speech_helper_sha256: str | None = None,
    apple_speech_os_version: str | None = None,
) -> dict:
    # The worker owns stdout as a newline-delimited JSON protocol. Research
    # adapters and model libraries may print progress on data-dependent paths;
    # letting one such line escape makes a valid operation look like a malformed
    # protocol frame. Discard operation stdout at the boundary. Protocol emits
    # carry the original stream explicitly, so a heartbeat remains visible.
    with open(os.devnull, "w", encoding="utf-8") as discarded:
        with contextlib.redirect_stdout(discarded):
            return dispatch(
                root,
                operation,
                arguments,
                encoder_digest=encoder_digest,
                admission=admission,
                model_dir=model_dir,
                encoder_path=encoder_path,
                embedding_dir=embedding_dir,
                transcription_engine=transcription_engine,
                speech_locale=speech_locale,
                apple_speech_helper=apple_speech_helper,
                apple_speech_helper_sha256=apple_speech_helper_sha256,
                apple_speech_os_version=apple_speech_os_version,
            )


def run(
    root: Path,
    manifest_path: Path,
    parent_fd: int,
    transcript_model_receipt: Path | None = None,
    transcription_engine: str = "whisper",
    speech_locale: str = "en-US",
) -> int:
    # dispatch_without_protocol_output redirects sys.stdout while model adapters
    # run. Capture the actual protocol stream before that redirect so a
    # background heartbeat cannot be discarded alongside adapter noise.
    protocol_output = sys.stdout
    root = require_private_root(root)
    manifest = load_manifest(manifest_path)
    if transcription_engine not in {"whisper", "apple-native"}:
        raise ValueError("transcription engine is unsupported")
    if manifest["schema"] in {"app-runtime/2", "app-runtime/3"} and (transcription_engine == "whisper" or transcript_model_receipt is not None):
        model_dir, external_models = external_transcript_model(
            root, manifest_path, manifest, transcript_model_receipt
        )
    else:
        model_dir = None if transcription_engine == "apple-native" else transcript_model_dir(manifest_path, manifest)
        external_models = []
    embedding_dir = embedding_model_dir(manifest_path, manifest)
    apple_helper: Path | None = None
    apple_helper_sha256: str | None = None
    apple_os_version: str | None = None
    if transcription_engine == "apple-native":
        if manifest["schema"] != "app-runtime/3":
            raise ValueError("Apple native transcription requires app-runtime/3")
        entry = manifest["apple_speech"]
        apple_helper = (manifest_path.parent / entry["path"]).resolve(strict=True)
        apple_helper_sha256 = entry["sha256"]
        from .apple_speech import AppleSpeechRefused, read_capability
        try:
            capability = read_capability(apple_helper, locale=speech_locale, temporary_parent=root)
        except AppleSpeechRefused as exc:
            raise ValueError(str(exc)) from None
        if capability["state"] != "ready":
            raise ValueError("Apple speech assets are not ready")
        apple_os_version = capability["os_version"]
    operations = operations_for(manifest["admission"])
    emit(
        {
            "schema": "worker-event/2",
            "event": "worker.ready",
            "protocol": 2,
            "admission": manifest["admission"],
            "build": manifest["worker"]["sha256"],
            "runtime": {"kind": "bundled", "digest": manifest["runtime"]["sha256"]},
            "tap": {"build": manifest["tap"]["sha256"], "available": True},
            "models": [
                {
                    "id": model["id"],
                    "digest": model["sha256"],
                    "available": True,
                }
                for model in manifest["models"]
            ] + external_models,
            "operations": sorted(operations),
        },
        protocol_output=protocol_output,
    )
    while parent_is_alive(parent_fd):
        ready, _, _ = select.select([sys.stdin.buffer, parent_fd], [], [])
        if parent_fd in ready and not parent_is_alive(parent_fd):
            return 0
        if sys.stdin.buffer not in ready:
            continue
        frame = sys.stdin.buffer.readline(MAX_FRAME_BYTES + 2)
        if not frame:
            return 0
        try:
            request_id, operation, arguments = parse_command(frame, operations)
            # Each starter is guarded to its own operation and returns None
            # otherwise, so exactly one of these can be active per request.
            heartbeat = start_transcription_heartbeat(
                request_id,
                operation,
                arguments,
                protocol_output=protocol_output,
            ) or start_note_generation_heartbeat(
                request_id,
                operation,
                arguments,
                protocol_output=protocol_output,
            )
            try:
                digests = dispatch_without_protocol_output(
                    root,
                    operation,
                    arguments,
                    encoder_digest=manifest["encoder"]["sha256"],
                    admission=manifest["admission"],
                    model_dir=model_dir,
                    encoder_path=manifest_path.parent / manifest["encoder"]["path"],
                    embedding_dir=embedding_dir,
                    transcription_engine=transcription_engine,
                    speech_locale=speech_locale,
                    apple_speech_helper=apple_helper,
                    apple_speech_helper_sha256=apple_helper_sha256,
                    apple_speech_os_version=apple_os_version,
                )
            finally:
                if heartbeat is not None:
                    stopped, heartbeat_thread = heartbeat
                    stopped.set()
                    heartbeat_thread.join()
            emit(
                {
                    "schema": "worker-result/2",
                    "request_id": request_id,
                    "ok": True,
                    "code": None,
                    "recoverable": None,
                    "artifact_digests": digests,
                },
                protocol_output=protocol_output,
            )
        except (AdapterRefused, UnicodeDecodeError, ValueError, json.JSONDecodeError):
            known_request = None
            try:
                candidate = json.loads(frame)
                uuid.UUID(candidate.get("request_id", ""))
                known_request = candidate["request_id"]
            except Exception:
                return 2
            emit(
                {
                    "schema": "worker-result/2",
                    "request_id": known_request,
                    "ok": False,
                    "code": "protocol_failure",
                    "recoverable": False,
                    "artifact_digests": {},
                },
                protocol_output=protocol_output,
            )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--app-data-root", required=True, type=Path)
    parser.add_argument("--runtime-manifest", required=True, type=Path)
    parser.add_argument("--transcript-model-receipt", type=Path)
    parser.add_argument("--transcription-engine", choices=["whisper", "apple-native"], default="whisper")
    parser.add_argument("--speech-locale", default="en-US")
    parser.add_argument("--parent-liveness-fd", type=int)
    arguments = parser.parse_args()
    parent_fd = arguments.parent_liveness_fd
    if parent_fd is None:
        try:
            parent_fd = int(os.environ["LMN_PARENT_LIVENESS_FD"])
        except (KeyError, ValueError):
            return 2
    start_parent_liveness_watchdog(parent_fd)
    try:
        return run(
            arguments.app_data_root,
            arguments.runtime_manifest,
            parent_fd,
            arguments.transcript_model_receipt,
            arguments.transcription_engine,
            arguments.speech_locale,
        )
    except (OSError, ValueError, StorageRefused, json.JSONDecodeError):
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
