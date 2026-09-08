#!/usr/bin/env python3
"""Verify one built Yawn app without printing private content."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import plistlib
import re
import subprocess
import sys
from pathlib import Path
from xml.parsers.expat import ExpatError

EXPECTED_IDENTIFIER = "com.ninochavez.local-meeting-notes"
EXPECTED_PYTHON_IDENTIFIER = f"{EXPECTED_IDENTIFIER}.python-runtime"
EXPECTED_MINIMUM_MACOS = "14.4"
EXPECTED_TEAM_ID = "34VZ63G58M"
EXPECTED_AUTHORITY = f"Developer ID Application: Abelino Chavez ({EXPECTED_TEAM_ID})"
REQUIRED_PURPOSES = (
    "NSMicrophoneUsageDescription",
    "NSAudioCaptureUsageDescription",
)
PYTHON_EXECUTABLE = Path("Contents/Resources/python-runtime/bin/python3.12")
MAIN_EXECUTABLE = Path("Contents/MacOS/local-meeting-notes-desktop")
CAPTURE_EXECUTABLE = Path("Contents/Resources/bin/meeting-capture")
# Requests microphone access on first run, so it is a requesting binary and carries
# the same audio-input entitlement. Listed here because the else-branch below
# requires an empty entitlement set: an entitled binary missing from these sets
# fails verification rather than passing quietly, which is the behaviour that makes
# adding one deliberate.
PROBE_EXECUTABLE = Path("Contents/Resources/bin/permission-probe")
PYTHON_ENTITLEMENTS = {
    "com.apple.security.cs.allow-unsigned-executable-memory": True,
}
CAPTURE_ENTITLEMENTS = {
    "com.apple.security.device.audio-input": True,
}
CODE_SIGNATURE_RUNTIME = 0x00010000
ENCODER_PLACEHOLDER = "encoder-unavailable.identity"
# Deterministic export from the pinned ECAPA checkpoint; two independent
# exports reproduce this digest byte-identically (spike/encoder-packaging).
# A bundle claiming any other encoder is refused: packaging the candidate
# artifact is measurable here, but nothing in this verifier admits it.
EXPECTED_ENCODER_ONNX_SHA256 = (
    "1d5e288b1037410fd0c98f618e94523a6b7ca8a99c7069f076efb40aa95759cd"
)
# Flipped from a forbidden list when the signed catalog gained the note-model
# role: the note runtime now ships, and a bundle missing any piece of it would
# admit nothing while reading as a build defect rather than an honest refusal.
REQUIRED_NOTE_RUNTIME_RESOURCES = (
    Path("note-bridge.py"),
    Path("note-generator-mlx.py"),
    Path("note-runtime-generate.json"),
    Path("note-runtime-project.json"),
    Path("note-validator.zip"),
)


class VerificationError(ValueError):
    pass


def run(*arguments: str, cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        arguments,
        cwd=cwd,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def require(condition: bool, message: str) -> None:
    if not condition:
        raise VerificationError(message)


def verify_note_runtime_resources_present(resources: Path) -> None:
    for relative in REQUIRED_NOTE_RUNTIME_RESOURCES:
        path = resources / relative
        require(
            path.is_file() and not path.is_symlink(),
            f"required note runtime resource is missing or unsafe: {relative}",
        )


def load_plist(path: Path) -> dict:
    try:
        value = plistlib.loads(path.read_bytes())
    except (OSError, plistlib.InvalidFileException) as exc:
        raise VerificationError(f"built Info.plist is unreadable ({exc})") from None
    require(isinstance(value, dict), "built Info.plist is not a dictionary")
    return value


def verify_metadata(app: Path) -> tuple[Path, dict]:
    plist_path = app / "Contents/Info.plist"
    require(plist_path.is_file(), "built app is missing Contents/Info.plist")
    plist = load_plist(plist_path)
    require(
        plist.get("CFBundleIdentifier") == EXPECTED_IDENTIFIER,
        "built app has the wrong bundle identifier",
    )
    require(
        plist.get("LSMinimumSystemVersion") == EXPECTED_MINIMUM_MACOS,
        "built app has the wrong minimum macOS version",
    )
    version = plist.get("CFBundleShortVersionString")
    require(
        isinstance(version, str)
        and re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:[.-][0-9A-Za-z.-]+)?", version)
        is not None,
        "built app version is missing or unsafe",
    )
    for key in REQUIRED_PURPOSES:
        purpose = plist.get(key)
        require(
            isinstance(purpose, str) and len(purpose.strip()) >= 24,
            f"built app is missing a useful {key}",
        )
    resources = app / "Contents/Resources"
    require(resources.is_dir(), "built app is missing Contents/Resources")
    return resources, plist


def verify_runtime(resources: Path, admission: str) -> None:
    manifest_path = resources / "app-runtime.json"
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise VerificationError(f"runtime manifest is unreadable ({exc})") from None
    schema = manifest.get("schema")
    require(schema in {"app-runtime/1", "app-runtime/2", "app-runtime/3"}, "runtime schema is not current")
    # Bound in every admission as the fallback requester. Internal-alpha routes
    # first-run permission setup through meeting-capture itself, while builds
    # without that helper still need a bounded microphone-status surface.
    require(
        (manifest.get("permission_probe") or {}).get("path") == "bin/permission-probe",
        "runtime is not bound to the first-run permission probe",
    )
    if schema == "app-runtime/3":
        require(admission == "internal-alpha", "Apple speech requires internal-alpha")
        require(
            (manifest.get("apple_speech") or {}).get("path") == "bin/apple-speech",
            "runtime is not bound to the Apple speech helper",
        )
        helper = resources / "bin/apple-speech"
        require(
            helper.is_file() and not helper.is_symlink() and os.access(helper, os.X_OK),
            "Apple speech helper is missing or unsafe",
        )
    else:
        require("apple_speech" not in manifest, "legacy runtime cannot bind Apple speech")
    verify_note_runtime_resources_present(resources)
    require(
        manifest.get("admission") == admission,
        f"runtime admission is not {admission}; refusing this distribution candidate",
    )
    if admission == "internal-alpha":
        require(
            (manifest.get("tap") or {}).get("path") == "bin/meeting-capture",
            "internal-alpha runtime is not bound to the product capture helper",
        )
        # A closed allowlist, and deliberately a *second* statement of the set
        # rather than a read of `worker/build_manifest.py`. A verifier that asked
        # the builder what it had built would assert the manifest equals itself.
        #
        # Being a second statement means it can drift, and it did: the four
        # MiniLM entries were added to the builder on 2026-08-08 and not here, so
        # the first build carrying the embedding model was refused at the release
        # lane on 2026-08-08 — correctly, and later than it should have been.
        # `the_release_verifier_expects_the_models_the_builder_stages` now
        # compares the two files and fails in the ordinary test run instead.
        expected_models = {
            # All four or none — `worker.main.embedding_model_dir` requires the
            # whole set before it names a directory, so a bundle carrying three
            # of them would have an embedder that silently does not exist.
            "all-minilm-l6-v2-config": "models/all-MiniLM-L6-v2/config.json",
            "all-minilm-l6-v2-sentence-config": (
                "models/all-MiniLM-L6-v2/sentence_bert_config.json"
            ),
            "all-minilm-l6-v2-tokenizer": "models/all-MiniLM-L6-v2/tokenizer.json",
            "all-minilm-l6-v2-weights": (
                "models/all-MiniLM-L6-v2/model.safetensors"
            ),
        }
        if schema == "app-runtime/1":
            expected_models.update(
                {
                    "whisper-large-v3-turbo-config": (
                        "models/whisper-large-v3-turbo/config.json"
                    ),
                    "whisper-large-v3-turbo-weights": (
                        "models/whisper-large-v3-turbo/weights.safetensors"
                    ),
                }
            )
        actual_models = {
            model.get("id"): model.get("path")
            for model in manifest.get("models", [])
            if isinstance(model, dict)
        }
        require(
            actual_models == expected_models,
            "internal-alpha model inventory is incomplete or unexpected",
        )
        if schema in {"app-runtime/2", "app-runtime/3"}:
            verify_model_catalog(resources, manifest)
    python = resources / "python-runtime/bin/python3.12"
    require(python.is_file() and os.access(python, os.X_OK), "bundled Python is missing")
    exercise = """
from pathlib import Path
import numpy as np
from worker.main import load_manifest
load_manifest(Path('app-runtime.json'))
np.linalg.svd(np.eye(2))
np.fft.fft(np.ones(4))
"""
    completed = run(str(python), "-E", "-s", "-B", "-c", exercise, cwd=resources)
    require(
        completed.returncode == 0,
        "bundled Python, worker, manifest, or NumPy runtime exercise failed",
    )
    if admission == "internal-alpha":
        alpha_exercise = "import mlx.core, mlx_whisper, worker.transcription"
        completed = run(
            str(python), "-E", "-s", "-B", "-c", alpha_exercise, cwd=resources
        )
        require(
            completed.returncode == 0,
            "internal-alpha offline transcription runtime failed to import",
        )
    verify_encoder_candidate(resources, manifest, python)


def verify_model_catalog(resources: Path, manifest: dict) -> None:
    resource = manifest.get("model_catalog")
    require(
        isinstance(resource, dict)
        and set(resource) == {"path", "sha256"}
        and resource.get("path") == "model-catalog.json",
        "downloadable-model runtime lacks the pinned catalog resource",
    )
    path = resources / "model-catalog.json"
    require(not path.is_symlink() and path.is_file(), "model catalog is missing or unsafe")
    require(
        hashlib.sha256(path.read_bytes()).hexdigest() == resource.get("sha256"),
        "model catalog does not match the runtime manifest",
    )
    try:
        catalog = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise VerificationError(f"model catalog is unreadable ({exc})") from None
    require(
        catalog == {
            "schema": "yawn-model-catalog/1",
            "models": [
                {
                    "id": "whisper-large-v3-turbo-q4",
                    "revision": "660c343bbf4e52ac257f0b7d952e5388e6f93bef",
                    "title": "Smaller download",
                    "detail": "A 4-bit local transcription model that uses about 464 MB.",
                    "downloadBytes": 463665005,
                    "installedBytes": 463665005,
                    "files": [
                        {
                            "role": "config",
                            "name": "config.json",
                            "url": "https://pub-91cec3695eaf486bbfaaa114df6f2268.r2.dev/models/whisper-large-v3-turbo-q4/660c343bbf4e52ac257f0b7d952e5388e6f93bef/config.json",
                            "bytes": 341,
                            "sha256": "538e24557b8f9bc504700add5e7bbe32087c2353001ff563e64772ad4398671a",
                        },
                        {
                            "role": "weights",
                            "name": "weights.npz",
                            "url": "https://pub-91cec3695eaf486bbfaaa114df6f2268.r2.dev/models/whisper-large-v3-turbo-q4/660c343bbf4e52ac257f0b7d952e5388e6f93bef/weights.npz",
                            "bytes": 463664664,
                            "sha256": "862bbc832b05f3f4ec19dd632b701d61a6d3f5c7906360a10d72a79870642a80",
                        },
                    ],
                },
                {
                    "id": "whisper-large-v3-turbo",
                    "revision": "a4aaeec0636e6fef84abdcbe3544cb2bf7e9f6fb",
                    "title": "Full model",
                    "detail": "The full local Turbo transcription model, using about 1.61 GB.",
                    "downloadBytes": 1613977880,
                    "installedBytes": 1613977880,
                    "files": [
                        {
                            "role": "config",
                            "name": "config.json",
                            "url": "https://pub-91cec3695eaf486bbfaaa114df6f2268.r2.dev/models/whisper-large-v3-turbo/a4aaeec0636e6fef84abdcbe3544cb2bf7e9f6fb/config.json",
                            "bytes": 268,
                            "sha256": "b34fc29e4e11e0a25e812775dd67f4dd16fc2c8eb43d28ae25ff7d660ecb6379",
                        },
                        {
                            "role": "weights",
                            "name": "weights.safetensors",
                            "url": "https://pub-91cec3695eaf486bbfaaa114df6f2268.r2.dev/models/whisper-large-v3-turbo/a4aaeec0636e6fef84abdcbe3544cb2bf7e9f6fb/weights.safetensors",
                            "bytes": 1613977612,
                            "sha256": "951ed3fc1203e6a62467abb2144a96ce7eafca8fa77e3704fdb8635ff3e7f8a6",
                        },
                    ],
                },
            ],
            "note_models": [
                {
                    "id": "gemma-3-12b-it-qat-4bit",
                    "revision": "66fc51ef25778c03d33c4c8bc446973d062e73f4",
                    "title": "Note generation model",
                    "detail": "The local note-generation model (Gemma 3 12B, 4-bit), using about 8.06 GB.",
                    "downloadBytes": 8063332687,
                    "installedBytes": 8063332687,
                    "files": [
                        {
                            "role": "config",
                            "name": "config.json",
                            "url": "https://pub-91cec3695eaf486bbfaaa114df6f2268.r2.dev/models/gemma-3-12b-it-qat-4bit/66fc51ef25778c03d33c4c8bc446973d062e73f4/config.json",
                            "bytes": 7267,
                            "sha256": "e1f96cecfbbae53a97fa351376e2ebb9d0e2220d80c0a194452aa427f89b3066",
                        },
                        {
                            "role": "weights",
                            "name": "model-00001-of-00002.safetensors",
                            "url": "https://pub-91cec3695eaf486bbfaaa114df6f2268.r2.dev/models/gemma-3-12b-it-qat-4bit/66fc51ef25778c03d33c4c8bc446973d062e73f4/model-00001-of-00002.safetensors",
                            "bytes": 5367455313,
                            "sha256": "4716bf31a789e3502fc021cb78a12bd8daea87e5d05534e5a01a00780ae05d2d",
                        },
                        {
                            "role": "weights",
                            "name": "model-00002-of-00002.safetensors",
                            "url": "https://pub-91cec3695eaf486bbfaaa114df6f2268.r2.dev/models/gemma-3-12b-it-qat-4bit/66fc51ef25778c03d33c4c8bc446973d062e73f4/model-00002-of-00002.safetensors",
                            "bytes": 2661219935,
                            "sha256": "37301980c27d8c49e87bb323633343b1222ec8131a71c0f41ff9d6a2d77ebee9",
                        },
                        {
                            "role": "weights-index",
                            "name": "model.safetensors.index.json",
                            "url": "https://pub-91cec3695eaf486bbfaaa114df6f2268.r2.dev/models/gemma-3-12b-it-qat-4bit/66fc51ef25778c03d33c4c8bc446973d062e73f4/model.safetensors.index.json",
                            "bytes": 108605,
                            "sha256": "788cc42a1a92835df62d9a3791f47105f63504c7c404637a73288e9b11bc7b82",
                        },
                        {
                            "role": "tokenizer",
                            "name": "tokenizer.json",
                            "url": "https://pub-91cec3695eaf486bbfaaa114df6f2268.r2.dev/models/gemma-3-12b-it-qat-4bit/66fc51ef25778c03d33c4c8bc446973d062e73f4/tokenizer.json",
                            "bytes": 33384568,
                            "sha256": "4667f2089529e8e7657cfb6d1c19910ae71ff5f28aa7ab2ff2763330affad795",
                        },
                        {
                            "role": "tokenizer-config",
                            "name": "tokenizer_config.json",
                            "url": "https://pub-91cec3695eaf486bbfaaa114df6f2268.r2.dev/models/gemma-3-12b-it-qat-4bit/66fc51ef25778c03d33c4c8bc446973d062e73f4/tokenizer_config.json",
                            "bytes": 1156999,
                            "sha256": "bfe25c2735e395407beb78456ea9a6984a1f00d8c16fa04a8b75f2a614cf53e1",
                        },
                    ],
                },
            ],
        },
        "model catalog is not the release verifier's pinned model inventory",
    )
    require(
        not (resources / "models/whisper-large-v3-turbo").exists(),
        "downloadable-model bundle still contains the full transcript model",
    )


def verify_encoder_candidate(resources: Path, manifest: dict, python: Path) -> None:
    encoder = manifest.get("encoder")
    require(isinstance(encoder, dict), "runtime manifest lacks an encoder entry")
    path = encoder.get("path")
    if path == ENCODER_PLACEHOLDER:
        return
    require(
        isinstance(path, str) and path.startswith("models/") and ".." not in path,
        "packaged encoder path is not inside the models directory",
    )
    artifact = resources / path
    require(
        not artifact.is_symlink() and artifact.is_file(),
        "packaged encoder artifact is missing or unsafe",
    )
    digest = hashlib.sha256()
    with artifact.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    require(
        digest.hexdigest() == EXPECTED_ENCODER_ONNX_SHA256,
        "packaged encoder artifact does not match the pinned deterministic export",
    )
    require(
        encoder.get("sha256") == EXPECTED_ENCODER_ONNX_SHA256,
        "runtime manifest does not bind the pinned encoder digest",
    )
    encoder_exercise = f"""
import numpy as np
import onnxruntime
from worker.fbank import fbank_features
session = onnxruntime.InferenceSession({path!r}, providers=['CPUExecutionProvider'])
features = fbank_features(np.zeros(16000, dtype=np.float32))
embedding = session.run(None, {{'features': features[np.newaxis, ...],
                               'lengths': np.ones(1, dtype=np.float32)}})[0]
assert embedding.shape[-1] == 192, embedding.shape
"""
    completed = run(str(python), "-E", "-s", "-B", "-c", encoder_exercise, cwd=resources)
    require(
        completed.returncode == 0,
        "packaged onnxruntime speaker-encoder exercise failed",
    )


def macho_inventory(app: Path) -> list[tuple[Path, str]]:
    inventory: list[tuple[Path, str]] = []
    for path in sorted(app.rglob("*")):
        if path.is_symlink() or not path.is_file():
            continue
        kind = run("/usr/bin/file", "-b", str(path))
        require(kind.returncode == 0, "file could not inspect a bundled artifact")
        if not kind.stdout.startswith("Mach-O"):
            continue
        arches = run("/usr/bin/lipo", "-archs", str(path))
        require(arches.returncode == 0, "lipo could not inspect a Mach-O")
        architecture_list = arches.stdout.strip().split()
        architecture_set = set(architecture_list)
        require(
            "arm64" in architecture_set,
            f"Mach-O lacks an arm64 slice: {path.relative_to(app)}",
        )
        require(
            architecture_set <= {"arm64", "x86_64"},
            f"Mach-O contains an unexpected architecture: {path.relative_to(app)}",
        )
        if "executable" in kind.stdout:
            require(
                architecture_list == ["arm64"],
                f"bundle executable is not arm64-only: {path.relative_to(app)}",
            )
        inventory.append((path, kind.stdout.strip()))
    require(inventory, "built app contains no Mach-O files")
    return inventory


def entitlements(path: Path) -> dict:
    result = run("/usr/bin/codesign", "-d", "--entitlements", ":-", str(path))
    xml_start = result.stdout.find("<?xml")
    if xml_start < 0:
        return {}
    try:
        value = plistlib.loads(result.stdout[xml_start:].encode())
    except (plistlib.InvalidFileException, ExpatError):
        raise VerificationError("signed entitlement output is malformed") from None
    require(isinstance(value, dict), "signed entitlements are not a dictionary")
    return value


def expected_designated_requirement(identifier: str) -> str:
    return (
        f'identifier "{identifier}" and anchor apple generic and '
        "certificate 1[field.1.2.840.113635.100.6.2.6] exists and "
        "certificate leaf[field.1.2.840.113635.100.6.1.13] exists and "
        f'certificate leaf[subject.OU] = "{EXPECTED_TEAM_ID}"'
    )


def normalized_requirement(value: str) -> str:
    value = re.sub(r"/\*\s*exists\s*\*/", "exists", value)
    return " ".join(value.split())


def signing_identity(path: Path) -> tuple[str, int, str, str]:
    detail = run(
        "/usr/bin/codesign",
        "-d",
        "--verbose=5",
        "--requirements",
        "-",
        str(path),
    )
    require(detail.returncode == 0, f"codesign could not inspect {path}")
    output = detail.stdout + detail.stderr
    identifier = re.search(r"(?m)^Identifier=(\S+)$", output)
    team = re.search(r"(?m)^TeamIdentifier=(\S+)$", output)
    flags = re.search(r"(?m)^CodeDirectory .* flags=0x([0-9a-fA-F]+)", output)
    requirement = re.search(r"(?m)^designated => (.+)$", output)
    require(identifier is not None, f"missing signing identifier: {path}")
    require(team is not None, f"missing signing team: {path}")
    require(flags is not None, f"missing signature flags: {path}")
    require(requirement is not None, f"missing designated requirement: {path}")
    return (
        identifier.group(1),
        int(flags.group(1), 16),
        normalized_requirement(requirement.group(1)),
        team.group(1),
    )


def verify_exact_code_identity(path: Path, identifier: str) -> None:
    actual_identifier, flags, requirement, team = signing_identity(path)
    require(actual_identifier == identifier, f"wrong signing identifier: {path}")
    require(team == EXPECTED_TEAM_ID, f"wrong signing team: {path}")
    require(
        flags & CODE_SIGNATURE_RUNTIME == CODE_SIGNATURE_RUNTIME,
        f"executable lacks the hardened-runtime bit: {path}",
    )
    require(
        requirement == expected_designated_requirement(identifier),
        f"unexpected designated requirement: {path}",
    )


def verify_signatures(app: Path, inventory: list[tuple[Path, str]]) -> None:
    outer = run("/usr/bin/codesign", "--verify", "--deep", "--strict", str(app))
    require(outer.returncode == 0, "outer app signature is not strict and complete")
    verify_exact_code_identity(app, EXPECTED_IDENTIFIER)
    require(entitlements(app) == CAPTURE_ENTITLEMENTS, "unexpected outer entitlement")
    for path, kind in inventory:
        checked = run("/usr/bin/codesign", "--verify", "--strict", str(path))
        require(
            checked.returncode == 0,
            f"invalid nested signature: {path.relative_to(app)}",
        )
        detail = run("/usr/bin/codesign", "-dv", "--verbose=4", str(path))
        signature = detail.stdout + detail.stderr
        require(
            f"Authority={EXPECTED_AUTHORITY}" in signature,
            f"wrong signing authority: {path.relative_to(app)}",
        )
        require(
            f"TeamIdentifier={EXPECTED_TEAM_ID}" in signature,
            f"wrong signing team: {path.relative_to(app)}",
        )
        relative = path.relative_to(app)
        if relative == PYTHON_EXECUTABLE:
            expected_entitlements = PYTHON_ENTITLEMENTS
            verify_exact_code_identity(path, EXPECTED_PYTHON_IDENTIFIER)
        elif relative in {MAIN_EXECUTABLE, CAPTURE_EXECUTABLE, PROBE_EXECUTABLE}:
            expected_entitlements = CAPTURE_ENTITLEMENTS
            if relative == MAIN_EXECUTABLE:
                verify_exact_code_identity(path, EXPECTED_IDENTIFIER)
        else:
            expected_entitlements = {}
            if "executable" in kind:
                _, flags, _, _ = signing_identity(path)
                require(
                    flags & CODE_SIGNATURE_RUNTIME == CODE_SIGNATURE_RUNTIME,
                    f"executable lacks the hardened-runtime bit: {relative}",
                )
        require(
            entitlements(path) == expected_entitlements,
            f"unexpected entitlement: {path.relative_to(app)}",
        )


def verify(app: Path, *, signed: bool, admission: str) -> tuple[str, int]:
    supplied = app.expanduser()
    require(not supplied.is_symlink(), "app path may not be a symlink")
    app = supplied.resolve()
    require(app.is_dir(), f"app bundle does not exist: {app}")
    resources, plist = verify_metadata(app)
    verify_runtime(resources, admission)
    inventory = macho_inventory(app)
    manifest = json.loads((resources / "app-runtime.json").read_text(encoding="utf-8"))
    if manifest.get("schema") == "app-runtime/3":
        require(
            any(path == resources / "bin/apple-speech" and "executable" in kind
                for path, kind in inventory),
            "Apple speech helper is not a bundled Mach-O executable",
        )
    if signed:
        verify_signatures(app, inventory)
    return str(plist["CFBundleShortVersionString"]), len(inventory)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app", type=Path)
    parser.add_argument("--signed", action="store_true")
    parser.add_argument(
        "--admission",
        choices=("product", "internal-alpha"),
        default="product",
    )
    args = parser.parse_args()
    try:
        version, macho_count = verify(
            args.app, signed=args.signed, admission=args.admission
        )
    except VerificationError as exc:
        print(f"release bundle verification: BLOCKED — {exc}", file=sys.stderr)
        return 1
    mode = "signed" if args.signed else "unsigned"
    print(
        f"release bundle verification: PASS ({mode}, version {version}, "
        f"{macho_count} arm64-compatible Mach-O files, {args.admission})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
