#!/usr/bin/env python3
"""Write one digest-bound application-runtime manifest."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import tempfile
import zipfile
from pathlib import Path


REPO = Path(__file__).resolve().parents[1]
NOTE_MANIFEST = Path("note-runtime-project.json")
# The `generate`-role sibling. `crates/session-core/src/note_projector_process.rs`
# reads it beside the project manifest to learn which generator and which model
# digests the bundle pins, and refuses a generate manifest that names a
# generator without pinning models (or the reverse). `main()` writes it in the
# note-runtime lane, pinned to the `NOTE_MODELS` catalog entry.
NOTE_GENERATE_MANIFEST = Path("note-runtime-generate.json")
NOTE_BRIDGE = Path("note-bridge.py")
NOTE_GENERATOR = Path("note-generator-mlx.py")
NOTE_VALIDATOR = Path("note-validator.zip")
# Insertion order is the zip write order, and `verify_note_runtime` compares
# `archive.namelist()` to this list positionally. Append; do not reorder.
VALIDATOR_SOURCES = {
    "note_validator.py": REPO / "worker/note_validator.py",
    "summarize.py": REPO / "notes/summarize.py",
    "transcript.py": REPO / "notes/transcript.py",
    "capture_health.py": REPO / "spike/capture_health.py",
    "candidate_first.py": REPO / "notes/candidate_first.py",
}

MODEL_BASE_URL = os.environ.get(
    "YAWN_MODEL_BASE_URL",
    "https://pub-91cec3695eaf486bbfaaa114df6f2268.r2.dev/models",
).rstrip("/")
TRANSCRIPT_MODELS = [
    {
        "id": "whisper-large-v3-turbo-q4",
        "revision": "660c343bbf4e52ac257f0b7d952e5388e6f93bef",
        "title": "Smaller download",
        "detail": "A 4-bit local transcription model that uses about 464 MB.",
        "files": [
            {
                "role": "config",
                "name": "config.json",
                "bytes": 341,
                "sha256": "538e24557b8f9bc504700add5e7bbe32087c2353001ff563e64772ad4398671a",
            },
            {
                "role": "weights",
                "name": "weights.npz",
                "bytes": 463_664_664,
                "sha256": "862bbc832b05f3f4ec19dd632b701d61a6d3f5c7906360a10d72a79870642a80",
            },
        ],
    },
    {
        "id": "whisper-large-v3-turbo",
        "revision": "a4aaeec0636e6fef84abdcbe3544cb2bf7e9f6fb",
        "title": "Full model",
        "detail": "The full local Turbo transcription model, using about 1.61 GB.",
        "files": [
            {
                "role": "config",
                "name": "config.json",
                "bytes": 268,
                "sha256": "b34fc29e4e11e0a25e812775dd67f4dd16fc2c8eb43d28ae25ff7d660ecb6379",
            },
            {
                "role": "weights",
                "name": "weights.safetensors",
                "bytes": 1_613_977_612,
                "sha256": "951ed3fc1203e6a62467abb2144a96ce7eafca8fa77e3704fdb8635ff3e7f8a6",
            },
        ],
    },
]

# The one note-generation model: the registration-pinned
# mlx-community/gemma-3-12b-it-qat-4bit snapshot (notes/EVAL.md, product
# registration — snapshot 66fc51ef…). These six files are the complete
# behavioral surface of the snapshot for the product path: the fixed prompt
# rendering never calls the tokenizer's chat template, and a six-file tree
# was measured to load and tokenize byte-identically to the full snapshot
# (same token ids, same greedy decode) before this entry was written.
NOTE_MODELS = [
    {
        "id": "gemma-3-12b-it-qat-4bit",
        "revision": "66fc51ef25778c03d33c4c8bc446973d062e73f4",
        "title": "Note generation model",
        "detail": "The local note-generation model (Gemma 3 12B, 4-bit), using about 8.06 GB.",
        "files": [
            {
                "role": "config",
                "name": "config.json",
                "bytes": 7_267,
                "sha256": "e1f96cecfbbae53a97fa351376e2ebb9d0e2220d80c0a194452aa427f89b3066",
            },
            {
                "role": "weights",
                "name": "model-00001-of-00002.safetensors",
                "bytes": 5_367_455_313,
                "sha256": "4716bf31a789e3502fc021cb78a12bd8daea87e5d05534e5a01a00780ae05d2d",
            },
            {
                "role": "weights",
                "name": "model-00002-of-00002.safetensors",
                "bytes": 2_661_219_935,
                "sha256": "37301980c27d8c49e87bb323633343b1222ec8131a71c0f41ff9d6a2d77ebee9",
            },
            {
                "role": "weights-index",
                "name": "model.safetensors.index.json",
                "bytes": 108_605,
                "sha256": "788cc42a1a92835df62d9a3791f47105f63504c7c404637a73288e9b11bc7b82",
            },
            {
                "role": "tokenizer",
                "name": "tokenizer.json",
                "bytes": 33_384_568,
                "sha256": "4667f2089529e8e7657cfb6d1c19910ae71ff5f28aa7ab2ff2763330affad795",
            },
            {
                "role": "tokenizer-config",
                "name": "tokenizer_config.json",
                "bytes": 1_156_999,
                "sha256": "bfe25c2735e395407beb78456ea9a6984a1f00d8c16fa04a8b75f2a614cf53e1",
            },
        ],
    },
]


def sha256(path: Path) -> str:
    if path.is_symlink() or not path.is_file():
        raise SystemExit(f"runtime resource is missing or unsafe: {path}")
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def atomic_write(target: Path, contents: bytes) -> None:
    descriptor, temporary_name = tempfile.mkstemp(
        dir=target.parent, prefix=f".{target.name}.", suffix=".tmp"
    )
    temporary = Path(temporary_name)
    try:
        os.fchmod(descriptor, 0o644)
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(contents)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, target)
        directory = os.open(target.parent, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        temporary.unlink(missing_ok=True)


def validator_bundle() -> bytes:
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_STORED) as archive:
        for name, source in VALIDATOR_SOURCES.items():
            information = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            information.compress_type = zipfile.ZIP_STORED
            information.create_system = 3
            information.external_attr = 0o100644 << 16
            archive.writestr(information, source.read_bytes())
    return output.getvalue()


def note_manifest(root: Path) -> dict:
    resources = {
        "runtime": Path("python-runtime/bin/python3.12"),
        "bridge": NOTE_BRIDGE,
        "validator": NOTE_VALIDATOR,
    }
    return {
        "schema": "note-runtime/1",
        "role": "project",
        **{
            name: {
                "relative_path": str(relative),
                "sha256": sha256(root / relative),
            }
            for name, relative in resources.items()
        },
        "generator": None,
        "models": [],
    }


def note_generate_manifest(root: Path, models: list[dict]) -> dict:
    """The `generate`-role manifest: the same three resources, plus the generator.

    `worker/note_generator_mlx.py` is the generator, staged as
    `note-generator-mlx.py`, and it is pinned by digest exactly as the bridge
    and validator are — the bridge execs the verified bytes read from an
    inherited descriptor, never the pathname, so this digest is what decides
    which generator can run.

    `models` are the note-model file pins, one `{"id", "sha256"}` per file, in
    the ids `note_runtime_models` derives on the Rust side. It is a parameter
    rather than a constant because those digests belong to a signed catalog
    entry that does not exist yet; the manifest cannot be written until it does,
    and an empty list is refused here rather than producing a manifest that
    Rust and `worker/note_bridge.py` would both refuse later.
    """
    if not models:
        raise SystemExit("generate manifest requires at least one pinned model file")
    resources = {
        "runtime": Path("python-runtime/bin/python3.12"),
        "bridge": NOTE_BRIDGE,
        "validator": NOTE_VALIDATOR,
    }
    return {
        "schema": "note-runtime/1",
        "role": "generate",
        **{
            name: {
                "relative_path": str(relative),
                "sha256": sha256(root / relative),
            }
            for name, relative in resources.items()
        },
        "generator": {
            "relative_path": str(NOTE_GENERATOR),
            "sha256": sha256(root / NOTE_GENERATOR),
        },
        "models": sorted(models, key=lambda model: model["id"]),
    }


def canonical_note_manifest(document: dict) -> bytes:
    return json.dumps(document, ensure_ascii=False, indent=2).encode("utf-8")


def verify_note_runtime(root: Path) -> None:
    raw = (root / NOTE_MANIFEST).read_bytes()
    if b"\\" in raw:
        raise SystemExit("note runtime manifest contains JSON escapes")
    document = json.loads(raw)
    if canonical_note_manifest(document) != raw or document != note_manifest(root):
        raise SystemExit("note runtime manifest is not canonical or digest-bound")
    with zipfile.ZipFile(root / NOTE_VALIDATOR) as archive:
        if archive.namelist() != list(VALIDATOR_SOURCES):
            raise SystemExit("note validator bundle inventory is not exact")
        for name, source in VALIDATOR_SOURCES.items():
            if archive.read(name) != source.read_bytes():
                raise SystemExit(f"note validator source differs: {name}")


def verify_note_runtime_absent(root: Path) -> None:
    for relative in (
        NOTE_BRIDGE,
        NOTE_MANIFEST,
        NOTE_VALIDATOR,
        NOTE_GENERATE_MANIFEST,
        NOTE_GENERATOR,
    ):
        path = root / relative
        if path.exists() or path.is_symlink():
            raise SystemExit(f"test-only note runtime resource is present in bundle root: {relative}")


def _catalog_entries(sources: list[dict]) -> list[dict]:
    entries = []
    for source in sources:
        revision = source["revision"]
        files = [
            {
                **file,
                "url": f"{MODEL_BASE_URL}/{source['id']}/{revision}/{file['name']}",
            }
            for file in source["files"]
        ]
        installed_bytes = sum(file["bytes"] for file in files)
        entries.append(
            {
                "id": source["id"],
                "revision": revision,
                "title": source["title"],
                "detail": source["detail"],
                "downloadBytes": installed_bytes,
                "installedBytes": installed_bytes,
                "files": files,
            }
        )
    return entries


def model_catalog() -> dict:
    return {
        "schema": "yawn-model-catalog/1",
        "models": _catalog_entries(TRANSCRIPT_MODELS),
        "note_models": _catalog_entries(NOTE_MODELS),
    }


def note_runtime_model_id(role: str, name: str) -> str:
    """The note-runtime identifier for one catalog file.

    Must derive exactly what `note_runtime_models` in
    `crates/session-core/src/note_projector_process.rs` derives for the same
    file: admission compares the manifest this module writes against the Rust
    derivation digest for digest, so a drift refuses rather than mis-admits.
    The characterization test
    `note_runtime_model_ids_name_every_file_of_a_sharded_model_distinctly`
    pins the Rust side; `tests/test_distribution_tooling.py` pins this one to
    the same expected list.
    """
    if role == "config":
        return "note-generator-config"
    if role == "weights-index":
        return "note-generator-weights-index"
    if role == "tokenizer":
        return "note-generator-tokenizer"
    if role == "tokenizer-config":
        return "note-generator-tokenizer-config"
    if role == "weights":
        if name.startswith("model-") and name.endswith(".safetensors"):
            shard = name[len("model-") : -len(".safetensors")]
            if shard:
                return "note-generator-weights-" + shard.replace(".", "-")
        return "note-generator-weights"
    raise SystemExit(f"unknown note-model file role: {role}")


def note_model_pins() -> list[dict]:
    """The `{"id", "sha256"}` pins the generate manifest carries, sorted."""
    if len(NOTE_MODELS) != 1:
        raise SystemExit("the generate manifest pins exactly one note model")
    pins = [
        {
            "id": note_runtime_model_id(file["role"], file["name"]),
            "sha256": file["sha256"],
        }
        for file in NOTE_MODELS[0]["files"]
    ]
    if len({pin["id"] for pin in pins}) != len(pins):
        raise SystemExit("note-model file identifiers collide")
    return sorted(pins, key=lambda pin: pin["id"])


def write_model_catalog(root: Path) -> Path:
    path = root / "model-catalog.json"
    atomic_write(path, (json.dumps(model_catalog(), indent=2) + "\n").encode())
    return path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument(
        "--admission",
        choices=("boundary-test", "internal-alpha", "product"),
        default="boundary-test",
    )
    parser.add_argument(
        "--exclude-note-runtime",
        action="store_true",
        help="write only the application runtime manifest for a bundle that excludes test-only note resources",
    )
    parser.add_argument(
        "--encoder",
        type=Path,
        default=Path("encoder-unavailable.identity"),
        help=(
            "relative path of the packaged speaker-encoder artifact; the default records the"
            " placeholder identity, which every consumer reads as encoder-unavailable"
        ),
    )
    parser.add_argument(
        "--external-transcript-models",
        action="store_true",
        help="bind the signed hosted-model catalog instead of bundled Whisper weights",
    )
    parser.add_argument(
        "--apple-speech",
        action="store_true",
        help="bind the Apple speech helper alongside the external-model catalog",
    )
    arguments = parser.parse_args()
    if arguments.apple_speech and not (
        arguments.external_transcript_models and arguments.admission == "internal-alpha"
    ):
        raise SystemExit("Apple speech requires internal-alpha external transcript models")
    root = arguments.root.resolve(strict=True)
    if arguments.encoder.is_absolute() or not (root / arguments.encoder).resolve().is_relative_to(
        root
    ):
        raise SystemExit(f"encoder path escapes the runtime root: {arguments.encoder}")
    if arguments.exclude_note_runtime:
        verify_note_runtime_absent(root)
    else:
        atomic_write(root / NOTE_VALIDATOR, validator_bundle())
        atomic_write(root / NOTE_MANIFEST, canonical_note_manifest(note_manifest(root)))
        atomic_write(
            root / NOTE_GENERATE_MANIFEST,
            canonical_note_manifest(note_generate_manifest(root, note_model_pins())),
        )
    # Tauri's resource map is intentionally identical in every lane. The
    # catalog is signed into all bundles; app-runtime/2 and /3 authorize models
    # installed outside the bundle.
    catalog_path = write_model_catalog(root)
    resources = {
        "runtime": Path("python-runtime/bin/python3.12"),
        "worker": Path("worker/main.py"),
        "tap": Path(
            "bin/meeting-capture"
            if arguments.admission == "internal-alpha"
            else "bin/audiotee"
        ),
        "encoder": arguments.encoder,
        # Fallback requester for admissions without meeting-capture. Carried here
        # because the app can run it, and every child this app runs is
        # digest-verified from this manifest before it is spawned.
        "permission_probe": Path("bin/permission-probe"),
    }
    if arguments.apple_speech:
        resources["apple_speech"] = Path("bin/apple-speech")
    models = []
    if arguments.admission == "internal-alpha":
        if not arguments.external_transcript_models:
            models.extend(
                [
                    {
                        "id": "whisper-large-v3-turbo-config",
                        "path": "models/whisper-large-v3-turbo/config.json",
                    },
                    {
                        "id": "whisper-large-v3-turbo-weights",
                        "path": "models/whisper-large-v3-turbo/weights.safetensors",
                    },
                ]
            )
        # All four or none. `worker.main.embedding_model_dir` requires the
        # whole set before it will name a directory, because a model missing
        # its `tokenizer.json` would load and then embed with whatever
        # tokenizer happened to be importable.
        models.extend([
            {
                "id": "all-minilm-l6-v2-config",
                "path": "models/all-MiniLM-L6-v2/config.json",
            },
            {
                "id": "all-minilm-l6-v2-sentence-config",
                "path": "models/all-MiniLM-L6-v2/sentence_bert_config.json",
            },
            {
                "id": "all-minilm-l6-v2-tokenizer",
                "path": "models/all-MiniLM-L6-v2/tokenizer.json",
            },
            {
                "id": "all-minilm-l6-v2-weights",
                "path": "models/all-MiniLM-L6-v2/model.safetensors",
            },
        ])
    catalog_resource = None
    if arguments.external_transcript_models:
        if arguments.admission != "internal-alpha":
            raise SystemExit("external transcript models require internal-alpha admission")
        catalog_resource = {"path": catalog_path.name, "sha256": sha256(catalog_path)}
    manifest = {
        "schema": ("app-runtime/3" if arguments.apple_speech else
                   "app-runtime/2" if catalog_resource else "app-runtime/1"),
        "admission": arguments.admission,
        **{
            name: {"path": str(relative), "sha256": sha256(root / relative)}
            for name, relative in resources.items()
        },
        **({"model_catalog": catalog_resource} if catalog_resource else {}),
        "models": [
            {**model, "sha256": sha256(root / model["path"])}
            for model in models
        ],
    }
    atomic_write(root / "app-runtime.json", (json.dumps(manifest, indent=2) + "\n").encode())
    if not arguments.exclude_note_runtime:
        verify_note_runtime(root)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
