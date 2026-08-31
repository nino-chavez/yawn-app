#!/usr/bin/env python3
"""Private, one-shot note bridge: read-only inspection, projection, and generation."""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import importlib.abc
import importlib.util
import io
import json
import os
import re
import select
import stat
import subprocess
import sys
import threading
import time
import unicodedata
import uuid
import zipfile
from dataclasses import dataclass
from pathlib import Path

MAX_FRAME_BYTES = 64 * 1024
# One generated response, read whole. Larger than any admissible note by a wide
# margin, and bounded so a runaway decode cannot be absorbed instead of refused.
MAX_GENERATOR_RESPONSE_BYTES = 1024 * 1024
# The registered whole-run time budget: `candidate_first.PRODUCT_RUN["gates"]
# ["maximum_elapsed_seconds"]`, re-derived for batch size 1 from the measured
# 6–11 s per single-candidate call (notes/EVAL.md, "Amendment — product elapsed
# budget re-derived for batch size 1"). It is the *harness* budget — the
# ceiling a measured run may occupy — and explicitly not a deadline a person
# waiting on a note would accept. The app-side UX deadline is a separate
# product number owned by the caller and still to be set; the caller may always
# ask for less and can never ask for more.
#
# Written here rather than imported because the bridge is exec'd from verified
# bytes before any validator module exists on the import path, and the value is
# needed while parsing a command. `_require_registered_deadline` binds the two
# at startup instead, so a registration that moves this number refuses the
# bridge rather than leaving the ceiling quietly behind the registration.
GENERATOR_DEADLINE_S = 3600
# Written by `model_store.rs` beside the model files it installs. It is install
# provenance, not model bytes, so it is excluded from the pinned digest set.
# Tolerated, not required: `model_store::verify_model_directory` requires it and
# reads it, and this bridge deliberately does not parse install provenance — the
# manifest's digests are the authority here, so a directory with no receipt
# passes the bridge and still fails the Rust check that owns that question.
MODEL_INSTALL_RECEIPT = "model-install.json"
_SAFE_IDENTIFIER = re.compile(r"^[A-Za-z0-9._/-]+$")
_SAFE_DIGEST = re.compile(r"^[0-9a-f]{64}$")
_SAFE_MEETING_ID = re.compile(r"^[A-Za-z0-9_-]{1,128}$")
_MANIFEST_FIELDS = (
    "schema",
    "role",
    "runtime",
    "bridge",
    "validator",
    "generator",
    "models",
)
_VALIDATOR_MODULES = {
    "note_validator": "note_validator.py",
    "summarize": "summarize.py",
    "transcript": "transcript.py",
    "capture_health": "capture_health.py",
    # The candidate enumerator the measured arm ran. Pinned rather than
    # reimplemented: candidate identity is derived from fragment ids, and a
    # second copy of that derivation is a second owner of what the model is
    # allowed to be asked. Its own imports are `summarize` and `transcript`,
    # both already here, so the closure adds exactly one module.
    "candidate_first": "candidate_first.py",
}
_REQUIRED_FLAGS = {
    "isolated": 1,
    "no_site": 1,
    "ignore_environment": 1,
    "no_user_site": 1,
    "safe_path": True,
    "dont_write_bytecode": 1,
}
# The generator child keeps every isolation flag the bridge can keep and drops
# exactly the two it cannot. `-I` and `-S` suppress site initialization, and
# site initialization is what puts the shared runtime's site-packages on the
# import path; a generator that kept them could not import anything at all.
# `-E` (ignore environment) and `-s` (no user site) survive, the environment is
# emptied rather than filtered, and the child never sees the bridge's own
# confined import path.
_GENERATOR_FLAGS = ("-E", "-s", "-B")
# The child runs the manifest-pinned generator bytes read from an inherited
# descriptor, never a pathname. The pathname was already verified, but a name
# can be replaced between verification and exec while a descriptor cannot, so
# the verified bytes are the ones that run.
#
# mlx-lm's own mlx pin runs ahead of the transcription runtime's shared mlx
# (docs/note-runtime-decision.md, "mlx-lm packaging options"), so the
# generator gets a private site-packages directory inserted ahead of the
# shared one on sys.path -- generate-site-packages/, a sibling of the regular
# site-packages under the same interpreter. mlx_whisper keeps resolving the
# shared, already-verified mlx from the regular site-packages unchanged; only
# this bootstrap's own interpreter sees the isolated one, and only after it
# has already imported nothing else. The directory is derived from
# `sys.executable`, never an inherited environment variable -- the generator
# child's environment is emptied deliberately, and this must not reopen that.
_GENERATOR_BOOTSTRAP = (
    "import os,sys\n"
    "sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(sys.executable)),"
    " 'lib', 'python%d.%d' % sys.version_info[:2], 'generate-site-packages'))\n"
    "c=[]\n"
    "while True:\n"
    " b=os.read({fd},1048576)\n"
    " if not b:\n"
    "  break\n"
    " c.append(b)\n"
    "os.close({fd})\n"
    "exec(compile(b''.join(c),'<note-generator>','exec'),{{'__name__':'__main__'}})\n"
)
# Set while a generator child is running so the parent-liveness watchers can
# reap it. Without this a bridge that exits on parent EOF leaves a model process
# holding several gigabytes of unified memory after the app has quit. It is
# assigned after `Popen` returns, so a parent EOF inside that window still
# orphans the child; closing it needs a pre-fork registration this transport
# does not have.
_RUNNING_GENERATOR: subprocess.Popen | None = None


class BridgeRefused(ValueError):
    pass


class InvalidArguments(ValueError):
    pass


@dataclass
class _DirectoryLink:
    parent_fd: int
    name: str
    child_fd: int
    identity: os.stat_result


@dataclass
class _RetainedFile:
    parent_fd: int
    name: str
    file_fd: int
    identity: os.stat_result
    digest: str
    bytes: bytes
    directories: list[_DirectoryLink]

    def close(self) -> None:
        os.close(self.file_fd)
        os.close(self.parent_fd)
        for link in reversed(self.directories):
            os.close(link.child_fd)
            os.close(link.parent_fd)


@dataclass
class _VerifiedRuntime:
    root_fd: int
    manifest: _RetainedFile
    resources: dict[str, _RetainedFile]
    role: str
    models: tuple[dict, ...] = ()

    def require_unchanged(self) -> None:
        _require_link(self.manifest)
        for resource in self.resources.values():
            _require_link(resource)
            current = hashlib.sha256(
                _read_retained(resource.file_fd, resource.identity)
            ).hexdigest()
            if current != resource.digest:
                raise BridgeRefused("verified resource bytes changed")

    def close(self) -> None:
        for resource in self.resources.values():
            resource.close()
        self.manifest.close()
        os.close(self.root_fd)


@dataclass
class _DescriptorResource:
    file_fd: int
    identity: os.stat_result
    digest: str
    bytes: bytes

    def require_unchanged(self) -> None:
        if not _same_identity(self.identity, os.fstat(self.file_fd)):
            raise BridgeRefused("inherited resource identity changed")
        current = hashlib.sha256(_read_retained(self.file_fd, self.identity)).hexdigest()
        if current != self.digest:
            raise BridgeRefused("inherited resource bytes changed")

    def close(self) -> None:
        os.close(self.file_fd)


@dataclass
class _DescriptorRuntime:
    manifest: _DescriptorResource
    resources: dict[str, _DescriptorResource]
    role: str
    models: tuple[dict, ...] = ()

    def require_unchanged(self) -> None:
        self.manifest.require_unchanged()
        for resource in self.resources.values():
            resource.require_unchanged()

    def close(self) -> None:
        for resource in self.resources.values():
            resource.close()
        self.manifest.close()


def _same_identity(left: os.stat_result, right: os.stat_result) -> bool:
    return (
        left.st_dev == right.st_dev
        and left.st_ino == right.st_ino
        and left.st_mode == right.st_mode
        and left.st_uid == right.st_uid
        and left.st_size == right.st_size
        and left.st_nlink == right.st_nlink
    )


def _read_retained(descriptor: int, identity: os.stat_result) -> bytes:
    os.lseek(descriptor, 0, os.SEEK_SET)
    remaining = identity.st_size
    chunks: list[bytes] = []
    while remaining:
        chunk = os.read(descriptor, min(remaining, 1024 * 1024))
        if not chunk:
            raise BridgeRefused("verified resource changed while being read")
        chunks.append(chunk)
        remaining -= len(chunk)
    if not _same_identity(identity, os.fstat(descriptor)):
        raise BridgeRefused("verified resource changed while being read")
    return b"".join(chunks)


def _open_absolute_directory(path: Path, *, private: bool) -> int:
    if not path.is_absolute():
        raise BridgeRefused("approved root must be absolute")
    descriptor = os.open("/", os.O_RDONLY | os.O_DIRECTORY)
    try:
        for component in path.parts[1:]:
            child = os.open(
                component,
                os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                dir_fd=descriptor,
            )
            os.close(descriptor)
            descriptor = child
        identity = os.fstat(descriptor)
        if not stat.S_ISDIR(identity.st_mode) or identity.st_uid != os.geteuid():
            raise BridgeRefused("approved root is not an owned directory")
        if private and stat.S_IMODE(identity.st_mode) != 0o700:
            raise BridgeRefused("temporary root is not owner-private")
        return descriptor
    except Exception:
        os.close(descriptor)
        raise


def _confine_runtime_imports() -> tuple[Path, ...]:
    for name, expected in _REQUIRED_FLAGS.items():
        if getattr(sys.flags, name, None) != expected:
            raise BridgeRefused("note bridge did not start in isolated no-site mode")
    if "site" in sys.modules or "sitecustomize" in sys.modules or "usercustomize" in sys.modules:
        raise BridgeRefused("site initialization ran before note bridge verification")
    base = Path(sys.base_prefix).resolve(strict=True)
    approved: list[Path] = []
    for entry in sys.path:
        if not entry:
            continue
        candidate = Path(entry)
        if not candidate.exists():
            continue
        resolved = candidate.resolve(strict=True)
        if base != resolved and base not in resolved.parents:
            raise BridgeRefused("runtime import path escapes the interpreter prefix")
        if "site-packages" in resolved.parts:
            raise BridgeRefused("site packages are outside the note runtime")
        approved.append(resolved)
    if not approved:
        raise BridgeRefused("attested runtime exposes no standard-library path")
    sys.path[:] = [str(path) for path in approved]
    return tuple(approved)


def _is_below(path: Path, roots: tuple[Path, ...]) -> bool:
    return any(path == root or root in path.parents for root in roots)


def _require_loaded_code_confined(
    runtime: _VerifiedRuntime,
    standard_library: tuple[Path, ...],
    bundle_loader: object,
) -> None:
    bridge = runtime.resources["bridge"].identity
    for module in tuple(sys.modules.values()):
        if module is None:
            continue
        if getattr(module, "__loader__", None) is bundle_loader:
            continue
        origin = getattr(getattr(module, "__spec__", None), "origin", None)
        if origin in {None, "built-in", "frozen"}:
            filename = getattr(module, "__file__", None)
            if filename is None:
                continue
            origin = filename
        try:
            resolved = Path(origin).resolve(strict=True)
        except (OSError, TypeError, ValueError) as exc:
            raise BridgeRefused("loaded module has no confined code identity") from exc
        if _is_below(resolved, standard_library):
            continue
        try:
            identity = os.stat(resolved, follow_symlinks=False)
        except OSError as exc:
            raise BridgeRefused("loaded module identity is unavailable") from exc
        if not _same_identity(identity, bridge):
            raise BridgeRefused("unmanifested code loaded into the note bridge")


def _open_beneath(root_fd: int, relative_path: str, expected_digest: str) -> _RetainedFile:
    parts = relative_path.split("/")
    parent = os.dup(root_fd)
    directories: list[_DirectoryLink] = []
    try:
        for component in parts[:-1]:
            child = os.open(
                component,
                os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                dir_fd=parent,
            )
            identity = os.fstat(child)
            if not stat.S_ISDIR(identity.st_mode) or identity.st_uid != os.geteuid():
                os.close(child)
                raise BridgeRefused("resource path has an unsafe directory")
            directories.append(_DirectoryLink(parent, component, child, identity))
            parent = os.dup(child)
        descriptor = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW, dir_fd=parent)
        identity = os.fstat(descriptor)
        if not stat.S_ISREG(identity.st_mode) or identity.st_uid != os.geteuid():
            os.close(descriptor)
            raise BridgeRefused("resource is not a bundle-owned regular file")
        data = _read_retained(descriptor, identity)
        digest = hashlib.sha256(data).hexdigest()
        if digest != expected_digest:
            os.close(descriptor)
            raise BridgeRefused("resource digest differs from the manifest")
        return _RetainedFile(
            parent,
            parts[-1],
            descriptor,
            identity,
            digest,
            data,
            directories,
        )
    except Exception:
        os.close(parent)
        for link in reversed(directories):
            os.close(link.child_fd)
            os.close(link.parent_fd)
        raise


def _require_link(resource: _RetainedFile) -> None:
    for link in resource.directories:
        try:
            current = os.stat(link.name, dir_fd=link.parent_fd, follow_symlinks=False)
        except OSError as exc:
            raise BridgeRefused("verified resource directory changed") from exc
        if not _same_identity(link.identity, current):
            raise BridgeRefused("verified resource directory changed")
    try:
        current = os.stat(resource.name, dir_fd=resource.parent_fd, follow_symlinks=False)
    except OSError as exc:
        raise BridgeRefused("verified resource pathname changed") from exc
    if not _same_identity(resource.identity, current):
        raise BridgeRefused("verified resource pathname changed")


def _reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict:
    result: dict = {}
    for key, value in pairs:
        if key in result:
            raise BridgeRefused("JSON object contains a duplicate field")
        result[key] = value
    return result


def _safe_component(value: object, label: str) -> str:
    if not isinstance(value, str) or not value or not _SAFE_IDENTIFIER.fullmatch(value):
        raise BridgeRefused(f"{label} is not ASCII-safe")
    if any(part in {"", ".", ".."} for part in value.split("/")):
        raise BridgeRefused(f"{label} has an unsafe path component")
    return value


def _safe_digest(value: object, label: str) -> str:
    if not isinstance(value, str) or not _SAFE_DIGEST.fullmatch(value):
        raise BridgeRefused(f"{label} is not a lowercase SHA-256 digest")
    return value


def _resource(value: object, label: str) -> dict:
    if not isinstance(value, dict) or list(value) != ["relative_path", "sha256"]:
        raise BridgeRefused(f"{label} has the wrong shape")
    return {
        "relative_path": _safe_component(value["relative_path"], f"{label} path"),
        "sha256": _safe_digest(value["sha256"], f"{label} digest"),
    }


def _model_entry(value: object, label: str) -> dict:
    """Accept one manifest model entry: an id and the digest it must have.

    Deliberately carries no path. Externally installed models are named by id
    and digest everywhere else in the app (`model_store.rs::runtime_models`,
    `runtime.rs::matches_ready_with_external_models`), and the directory that
    holds them is supplied per request, not frozen into the bundle manifest.
    """
    if not isinstance(value, dict) or list(value) != ["id", "sha256"]:
        raise BridgeRefused(f"{label} has the wrong shape")
    # Narrower than `_safe_component` on purpose, and matched to Rust's
    # `valid_model_identifier`: an id is a name, not a path, so it gets no dot
    # and no slash. Two sides that disagree on the charset would let one admit
    # an id the other refuses.
    if not isinstance(value["id"], str) or not _SAFE_MEETING_ID.fullmatch(value["id"]):
        raise BridgeRefused(f"{label} id is not a plain identifier")
    return {
        "id": value["id"],
        "sha256": _safe_digest(value["sha256"], f"{label} digest"),
    }


def _require_generator_admission(document: dict) -> None:
    """Admit a generator only on the role that runs one, and only complete.

    This is a biconditional, not a relaxation. Every manifest the app ships
    today is `inspect` or `project`, and for those the refusal is unchanged: a
    generator or a model entry is still a hard refusal. A `generate` manifest is
    the one admitted shape, and it must carry both halves — a generator without
    pinned models could load anything on disk, and pinned models without a
    generator name weights nothing will read.
    """
    generator = document["generator"]
    models = document["models"]
    if document["role"] != "generate":
        if generator is not None or models != []:
            raise BridgeRefused("validator-only note bridge may not carry a generator or models")
        return
    if generator is None or not isinstance(models, list) or not models:
        raise BridgeRefused("generate manifest needs a generator and its pinned models")
    document["generator"] = _resource(generator, "generator")
    entries = [_model_entry(model, f"model {ordinal}") for ordinal, model in enumerate(models)]
    identifiers = [entry["id"] for entry in entries]
    digests = [entry["sha256"] for entry in entries]
    if len(set(identifiers)) != len(identifiers) or len(set(digests)) != len(digests):
        raise BridgeRefused("generate manifest repeats a model id or digest")
    document["models"] = entries


def verify_runtime(manifest_path: Path) -> _VerifiedRuntime:
    resource_root = manifest_path.parent
    root_fd = _open_absolute_directory(resource_root, private=False)
    manifest: _RetainedFile | None = None
    manifest_parent: int | None = None
    manifest_fd: int | None = None
    resources: dict[str, _RetainedFile] = {}
    try:
        name = manifest_path.name
        if not name or "/" in name or name in {".", ".."}:
            raise BridgeRefused("manifest name is unsafe")
        manifest_parent = os.dup(root_fd)
        manifest_fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=manifest_parent)
        identity = os.fstat(manifest_fd)
        if not stat.S_ISREG(identity.st_mode) or identity.st_uid != os.geteuid():
            raise BridgeRefused("manifest is not a bundle-owned regular file")
        raw = _read_retained(manifest_fd, identity)
        manifest = _RetainedFile(
            manifest_parent,
            name,
            manifest_fd,
            identity,
            hashlib.sha256(raw).hexdigest(),
            raw,
            [],
        )
        manifest_parent = None
        manifest_fd = None
        if b"\\" in raw:
            raise BridgeRefused("manifest may not use JSON escapes")
        document = json.loads(raw, object_pairs_hook=_reject_duplicate_keys)
        if not isinstance(document, dict) or list(document) != list(_MANIFEST_FIELDS):
            raise BridgeRefused("manifest fields or field order are not current")
        if json.dumps(document, ensure_ascii=False, indent=2).encode("utf-8") != raw:
            raise BridgeRefused("manifest bytes are not canonical")
        if document["schema"] != "note-runtime/1" or document["role"] not in {
            "inspect",
            "project",
            "generate",
        }:
            raise BridgeRefused("note bridge manifest has the wrong role")
        for field in ("runtime", "bridge", "validator"):
            document[field] = _resource(document[field], field)
        _require_generator_admission(document)
        opened = ("runtime", "bridge", "validator")
        if document["role"] == "generate":
            opened += ("generator",)
        for field in opened:
            resource = document[field]
            resources[field] = _open_beneath(root_fd, resource["relative_path"], resource["sha256"])
        verified = _VerifiedRuntime(
            root_fd, manifest, resources, document["role"], tuple(document["models"])
        )
        _require_runtime_identity(verified)
        verified.require_unchanged()
        return verified
    except Exception:
        for resource in resources.values():
            resource.close()
        if manifest is not None:
            manifest.close()
        else:
            if manifest_fd is not None:
                os.close(manifest_fd)
            if manifest_parent is not None:
                os.close(manifest_parent)
        os.close(root_fd)
        raise


def _resource_from_descriptor(descriptor: int) -> _DescriptorResource:
    owned = os.dup(descriptor)
    try:
        identity = os.fstat(owned)
        if (
            not stat.S_ISREG(identity.st_mode)
            or identity.st_uid != os.geteuid()
            or stat.S_IMODE(identity.st_mode) & 0o022
        ):
            raise BridgeRefused("inherited resource is not a bundle-owned regular file")
        data = _read_retained(owned, identity)
        return _DescriptorResource(
            owned,
            identity,
            hashlib.sha256(data).hexdigest(),
            data,
        )
    except Exception:
        os.close(owned)
        raise


def verify_descriptor_runtime(
    manifest_fd: int,
    bridge_fd: int,
    validator_fd: int,
    generator_fd: int | None = None,
) -> _DescriptorRuntime:
    manifest = _resource_from_descriptor(manifest_fd)
    resources: dict[str, _DescriptorResource] = {}
    try:
        raw = manifest.bytes
        if b"\\" in raw:
            raise BridgeRefused("manifest may not use JSON escapes")
        document = json.loads(raw, object_pairs_hook=_reject_duplicate_keys)
        if not isinstance(document, dict) or list(document) != list(_MANIFEST_FIELDS):
            raise BridgeRefused("manifest fields or field order are not current")
        if json.dumps(document, ensure_ascii=False, indent=2).encode("utf-8") != raw:
            raise BridgeRefused("manifest bytes are not canonical")
        # The descriptor set and the role are one decision, made by the
        # launching parent: three descriptors (manifest, bridge, validator)
        # speak `project` and cannot reach generator bytes at all; a fourth
        # descriptor speaks `generate` and is the only way this launch can
        # receive them. The pairing is a biconditional exactly like
        # `_require_generator_admission`: a generate manifest without a
        # generator descriptor and a project manifest with one both refuse.
        expected_role = "project" if generator_fd is None else "generate"
        if document["schema"] != "note-runtime/1" or document["role"] != expected_role:
            raise BridgeRefused("descriptor bridge manifest has the wrong role")
        for field in ("runtime", "bridge", "validator"):
            document[field] = _resource(document[field], field)
        _require_generator_admission(document)
        resources["bridge"] = _resource_from_descriptor(bridge_fd)
        resources["validator"] = _resource_from_descriptor(validator_fd)
        if resources["bridge"].digest != document["bridge"]["sha256"]:
            raise BridgeRefused("inherited bridge differs from the manifest")
        if resources["validator"].digest != document["validator"]["sha256"]:
            raise BridgeRefused("inherited validator differs from the manifest")
        if generator_fd is not None:
            resources["generator"] = _resource_from_descriptor(generator_fd)
            if resources["generator"].digest != document["generator"]["sha256"]:
                raise BridgeRefused("inherited generator differs from the manifest")
        executable_fd = os.open(
            sys.executable,
            os.O_RDONLY | getattr(os, "O_NOFOLLOW_ANY", os.O_NOFOLLOW),
        )
        try:
            executable_identity = os.fstat(executable_fd)
            if not stat.S_ISREG(executable_identity.st_mode):
                raise BridgeRefused("running interpreter path is unsafe")
            executable_bytes = _read_retained(executable_fd, executable_identity)
        finally:
            os.close(executable_fd)
        if hashlib.sha256(executable_bytes).hexdigest() != document["runtime"]["sha256"]:
            raise BridgeRefused("running interpreter differs from the manifest")
        runtime = _DescriptorRuntime(
            manifest, resources, document["role"], tuple(document["models"])
        )
        runtime.require_unchanged()
        return runtime
    except Exception:
        for resource in resources.values():
            resource.close()
        manifest.close()
        raise


def _require_runtime_identity(runtime: _VerifiedRuntime) -> None:
    executable = os.stat(sys.executable, follow_symlinks=False)
    if not _same_identity(runtime.resources["runtime"].identity, executable):
        raise BridgeRefused("running interpreter differs from the verified runtime")
    bridge = os.stat(__file__, follow_symlinks=False)
    if not _same_identity(runtime.resources["bridge"].identity, bridge):
        raise BridgeRefused("running bridge differs from the verified bridge")


class _BundleLoader(importlib.abc.MetaPathFinder, importlib.abc.Loader):
    def __init__(self, sources: dict[str, bytes]):
        self.sources = sources

    def find_spec(self, fullname: str, path=None, target=None):
        if fullname in self.sources:
            return importlib.util.spec_from_loader(fullname, self)
        return None

    def create_module(self, spec):
        return None

    def exec_module(self, module) -> None:
        source = self.sources[module.__name__]
        module.__file__ = f"/verified/{module.__name__}.py"
        code = compile(source, f"<verified-validator:{module.__name__}>", "exec")
        exec(code, module.__dict__)


def load_validator(runtime: _VerifiedRuntime):
    bundle = runtime.resources["validator"].bytes
    try:
        with zipfile.ZipFile(io.BytesIO(bundle)) as archive:
            names = archive.namelist()
            if len(names) != len(set(names)) or set(names) != set(_VALIDATOR_MODULES.values()):
                raise BridgeRefused("validator bundle has the wrong module inventory")
            sources = {
                module: archive.read(filename) for module, filename in _VALIDATOR_MODULES.items()
            }
    except (OSError, zipfile.BadZipFile, KeyError) as exc:
        raise BridgeRefused("validator bundle is malformed") from exc
    finder = _BundleLoader(sources)
    sys.meta_path.insert(0, finder)
    try:
        for name in _VALIDATOR_MODULES:
            sys.modules.pop(name, None)
        for name in ("capture_health", "transcript", "summarize", "candidate_first"):
            importlib.import_module(name)
        module = importlib.import_module("note_validator")
    finally:
        sys.meta_path.remove(finder)
    if getattr(module, "__loader__", None) is not finder:
        raise BridgeRefused("validator did not load from the verified bundle")
    if any(
        getattr(sys.modules[name], "__loader__", None) is not finder for name in _VALIDATOR_MODULES
    ):
        raise BridgeRefused("validator helper did not load from the verified bundle")
    return module, finder


def _require_registered_deadline(candidate_first) -> None:
    """Bind `GENERATOR_DEADLINE_S` to the registration that owns the number.

    The bridge cannot import the registration at module scope, so it restates
    the value — and a restated constant is a second owner unless something
    checks it. This is that check: one registration, one budget, and a
    generate bundle whose two halves disagree never reaches `ready`.

    Scoped to the role that runs a generator, deliberately. `project` and
    `inspect` never read a deadline and never spawn a child, and the `project`
    role is the shipped projection transport — making it refuse over a number
    it does not use would turn a research-lane edit into a broken reader.
    """
    try:
        registered = candidate_first.PRODUCT_RUN["gates"]["maximum_elapsed_seconds"]
    except (AttributeError, KeyError, TypeError) as exc:
        raise BridgeRefused("pinned registration carries no elapsed budget") from exc
    if registered != GENERATOR_DEADLINE_S:
        raise BridgeRefused("bridge deadline does not match the registered elapsed budget")


@dataclass
class _ModelTree:
    """One verified model directory, held open for the generator's lifetime.

    Digests are streamed rather than retained: the note model is several
    gigabytes, and reading it into the bridge to hash it would cost more memory
    than running it. The descriptors stay open so an unlink or a replacement of
    the directory or any pinned file is detected afterwards.
    """

    directory_fd: int
    directories: list[_DirectoryLink]
    files: list[_RetainedFile]
    path: str

    def require_unchanged(self) -> None:
        for link in self.directories:
            try:
                current = os.stat(link.name, dir_fd=link.parent_fd, follow_symlinks=False)
            except OSError as exc:
                raise BridgeRefused("verified model directory changed") from exc
            if not _same_identity(link.identity, current):
                raise BridgeRefused("verified model directory changed")
        for resource in self.files:
            _require_link(resource)
            if not _same_identity(resource.identity, os.fstat(resource.file_fd)):
                raise BridgeRefused("verified model file changed")

    def close(self) -> None:
        for resource in self.files:
            os.close(resource.file_fd)
            os.close(resource.parent_fd)
        os.close(self.directory_fd)
        for link in reversed(self.directories):
            os.close(link.child_fd)
            os.close(link.parent_fd)


def _streamed_digest(descriptor: int, identity: os.stat_result) -> str:
    """Hash a pinned regular file without retaining its bytes."""
    os.lseek(descriptor, 0, os.SEEK_SET)
    digest = hashlib.sha256()
    remaining = identity.st_size
    while remaining:
        chunk = os.read(descriptor, min(remaining, 4 * 1024 * 1024))
        if not chunk:
            raise BridgeRefused("verified model file changed while being read")
        digest.update(chunk)
        remaining -= len(chunk)
    if not _same_identity(identity, os.fstat(descriptor)):
        raise BridgeRefused("verified model file changed while being read")
    return digest.hexdigest()


def _open_model_tree(
    root_fd: int,
    root_path: Path,
    relative_path: str,
    pinned: tuple[dict, ...],
) -> _ModelTree:
    """Open the installed model directory beneath the storage root and pin it.

    Every path component is opened with `O_NOFOLLOW`, so a symlink anywhere in
    the chain refuses instead of resolving — the same rule
    `transcription.require_whisper_model` applies to the transcript model, made
    per-component rather than per-directory. The directory's contents must then
    match the manifest's pinned digests exactly, one file per entry: an extra
    file, a missing file, or a digest the manifest does not pin all refuse.
    """
    parent = os.dup(root_fd)
    directories: list[_DirectoryLink] = []
    files: list[_RetainedFile] = []
    directory_fd: int | None = None
    try:
        for component in relative_path.split("/"):
            child = os.open(
                component,
                os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                dir_fd=parent,
            )
            identity = os.fstat(child)
            if not stat.S_ISDIR(identity.st_mode) or identity.st_uid != os.geteuid():
                os.close(child)
                raise BridgeRefused("model path has an unsafe directory")
            if stat.S_IMODE(identity.st_mode) & 0o022:
                os.close(child)
                raise BridgeRefused("model path is group or world writable")
            directories.append(_DirectoryLink(parent, component, child, identity))
            parent = os.dup(child)
        directory_fd = parent
        parent = None
        names = sorted(
            name
            for name in os.listdir(directory_fd)
            if name != MODEL_INSTALL_RECEIPT
        )
        expected = {entry["sha256"] for entry in pinned}
        if len(names) != len(expected):
            raise BridgeRefused("model directory does not hold the pinned file set")
        observed: set[str] = set()
        for name in names:
            file_parent = os.dup(directory_fd)
            try:
                descriptor = os.open(name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=file_parent)
            except OSError as exc:
                os.close(file_parent)
                raise BridgeRefused("model file is missing or unsafe") from exc
            identity = os.fstat(descriptor)
            if (
                not stat.S_ISREG(identity.st_mode)
                or identity.st_uid != os.geteuid()
                or stat.S_IMODE(identity.st_mode) & 0o022
            ):
                os.close(descriptor)
                os.close(file_parent)
                raise BridgeRefused("model file is not an owner-private regular file")
            digest = _streamed_digest(descriptor, identity)
            if digest not in expected or digest in observed:
                os.close(descriptor)
                os.close(file_parent)
                raise BridgeRefused("model file digest is not pinned by the manifest")
            observed.add(digest)
            files.append(_RetainedFile(file_parent, name, descriptor, identity, digest, b"", []))
        if observed != expected:
            raise BridgeRefused("model directory does not hold the pinned file set")
        return _ModelTree(directory_fd, directories, files, str(root_path / relative_path))
    except Exception:
        for resource in files:
            os.close(resource.file_fd)
            os.close(resource.parent_fd)
        if parent is not None:
            os.close(parent)
        if directory_fd is not None:
            os.close(directory_fd)
        for link in reversed(directories):
            os.close(link.child_fd)
            os.close(link.parent_fd)
        raise


class GeneratorRefused(ValueError):
    """The generator child could not produce a readable response."""

    def __init__(self, code: str, recoverable: bool):
        super().__init__(code)
        self.code = code
        self.recoverable = recoverable


def _reap_generator() -> None:
    """Kill the generator's process group, if one is running."""
    child = _RUNNING_GENERATOR
    if child is None or child.poll() is not None:
        return
    with contextlib.suppress(OSError, ProcessLookupError):
        os.killpg(os.getpgid(child.pid), 9)


class _GeneratorSession:
    """One generator child, many classification batches, one whole-run deadline.

    A child per batch would reload several gigabytes of model weights twenty-odd
    times for one meeting, so the child is a session: it loads once, then
    answers one request line per batch until stdin closes. The deadline is the
    registered whole-run budget and it covers model load, so a child that spends
    it all on startup times out rather than silently borrowing from generation.

    Reads and writes are interleaved. A batch of 32 candidate packets exceeds
    the pipe buffer, and the child cannot answer before it has the whole line,
    so writing the request to completion before reading would deadlock.
    """

    def __init__(self, source: _RetainedFile, model: _ModelTree, deadline_s: int):
        global _RUNNING_GENERATOR

        os.lseek(source.file_fd, 0, os.SEEK_SET)
        self.started = time.monotonic()
        self.expires_at = self.started + deadline_s
        self.responses = 0
        self.response_bytes = 0
        self.last_response_sha256 = "unavailable"
        self._pending = bytearray()
        try:
            self._child = subprocess.Popen(
                [
                    sys.executable,
                    *_GENERATOR_FLAGS,
                    "-c",
                    _GENERATOR_BOOTSTRAP.format(fd=source.file_fd),
                ],
                cwd="/",
                env={},
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                pass_fds=(source.file_fd,),
                start_new_session=True,
                bufsize=0,
            )
        except OSError as exc:
            raise GeneratorRefused("provider-generation-failure", True) from exc
        _RUNNING_GENERATOR = self._child

    def _remaining(self) -> float:
        remaining = self.expires_at - time.monotonic()
        if remaining <= 0:
            raise GeneratorRefused("timeout", True)
        return remaining

    def ask(self, request: dict) -> str:
        """Send one batch and take exactly one response line back."""
        payload = bytearray(
            json.dumps(request, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
            + b"\n"
        )
        stdin = self._child.stdin
        stdout = self._child.stdout
        while True:
            if b"\n" in self._pending:
                break
            writable = [stdin] if payload else []
            ready, sendable, _ = select.select([stdout], writable, [], self._remaining())
            if sendable:
                try:
                    written = os.write(stdin.fileno(), payload[:65536])
                except BrokenPipeError as exc:
                    raise GeneratorRefused("provider-generation-failure", True) from exc
                del payload[:written]
            if ready:
                chunk = os.read(stdout.fileno(), 65536)
                if not chunk:
                    raise GeneratorRefused("provider-generation-failure", True)
                self._pending.extend(chunk)
                self.response_bytes += len(chunk)
                if self.response_bytes > MAX_GENERATOR_RESPONSE_BYTES:
                    raise GeneratorRefused("response-length-truncation", False)
            if not ready and not sendable:
                raise GeneratorRefused("timeout", True)
        line, _, rest = bytes(self._pending).partition(b"\n")
        self._pending = bytearray(rest)
        self.responses += 1
        self.last_response_sha256 = hashlib.sha256(line).hexdigest()
        try:
            return line.decode("utf-8")
        except UnicodeDecodeError as exc:
            raise GeneratorRefused("response-json-syntax", False) from exc

    def receipt(self) -> dict:
        """Closed, content-free: counts and digests, never a response body."""
        return {
            "responses": self.responses,
            "response_bytes": self.response_bytes,
            "last_response_sha256": self.last_response_sha256,
            "elapsed_s": round(time.monotonic() - self.started, 6),
        }

    def close(self) -> None:
        global _RUNNING_GENERATOR

        try:
            if self._child.stdin and not self._child.stdin.closed:
                self._child.stdin.close()
        except OSError:
            pass
        _reap_generator()
        with contextlib.suppress(OSError, subprocess.TimeoutExpired):
            self._child.wait(timeout=5)
        _RUNNING_GENERATOR = None


def _canonical_uuid(value: object) -> str:
    if not isinstance(value, str):
        raise InvalidArguments("identifier is not a UUID")
    try:
        parsed = uuid.UUID(value)
    except (ValueError, AttributeError) as exc:
        raise InvalidArguments("identifier is not a UUID") from exc
    if str(parsed) != value:
        raise InvalidArguments("identifier is not a canonical lowercase UUID")
    return value


def _opaque_meeting_id(value: object) -> str:
    if not isinstance(value, str) or not _SAFE_MEETING_ID.fullmatch(value):
        raise InvalidArguments("meeting ID is not an opaque meeting ID")
    return value


def _validate_speaker_label_overrides(value: object) -> None:
    """Admit the one bounded overlay the generate role can carry.

    This is transport validation, not sidecar parsing: the desktop has already
    derived the entries from its digest-bound correction history. The bridge
    only ensures a hostile command cannot widen those exact source groups
    before the validator presents them to the model.
    """
    if not isinstance(value, list) or len(value) > 3:
        raise InvalidArguments("speaker label overlay has too many source groups")
    previous = -1
    for override in value:
        if not isinstance(override, dict) or list(override) != ["source_speaker", "replacement"]:
            raise InvalidArguments("speaker label overlay entry has the wrong shape")
        rank = {None: 0, "Me": 1, "Them": 2}.get(override["source_speaker"])
        replacement = override["replacement"]
        if (
            rank is None
            or rank <= previous
            or not isinstance(replacement, str)
            or not replacement
            or len(replacement.encode("utf-8")) > 80
            or replacement != replacement.strip()
            or any(unicodedata.category(character) == "Cc" for character in replacement)
        ):
            raise InvalidArguments("speaker label overlay is invalid")
        previous = rank


def _validate_pre_meeting_context(value: object) -> None:
    """Bound the operator's pre-meeting framing text before it reaches a prompt.

    Free text, not a structured overlay, so only the byte ceiling and control
    characters are policed here; multi-line is legitimate context, so newline
    and tab are the two control characters this allows. The 16 KiB ceiling
    mirrors the desktop sidecar's own cap
    (`apps/desktop/src-tauri/src/meeting_context.rs::MAX_TEXT_BYTES`); this
    bridge does not read that file, so the bound is restated here rather than
    shared.
    """
    if (
        not isinstance(value, str)
        or not value
        or len(value.encode("utf-8")) > 16 * 1024
        or any(
            unicodedata.category(character) == "Cc" and character not in "\n\t"
            for character in value
        )
    ):
        raise InvalidArguments("pre-meeting context is invalid")


def _validate_vocabulary_replacements(value: object) -> None:
    """Close source-span replacements before the validator imports a model.

    The validator owns source-byte re-derivation after it opens the immutable
    transcript. This bridge owns the hostile-frame boundary, so malformed or
    oversized overlays cannot get as far as the generator process.
    """
    if not isinstance(value, list) or len(value) > 64:
        raise InvalidArguments("vocabulary overlay has too many replacements")
    previous = None
    for replacement in value:
        if not isinstance(replacement, dict) or list(replacement) != [
            "turn", "char_start", "char_end", "source_sha256", "replacement"
        ]:
            raise InvalidArguments("vocabulary overlay entry has the wrong shape")
        turn = replacement["turn"]
        start = replacement["char_start"]
        end = replacement["char_end"]
        source_sha256 = replacement["source_sha256"]
        text = replacement["replacement"]
        if (
            type(turn) is not int
            or not 0 <= turn <= 0xFFFFFFFF
            or type(start) is not int
            or start < 0
            or type(end) is not int
            or end <= start
            or not isinstance(text, str)
            or not text
            or len(text.encode("utf-8")) > 512
            or any(unicodedata.category(character) in {"Cc", "Cs"} for character in text)
        ):
            raise InvalidArguments("vocabulary overlay is invalid")
        try:
            _safe_digest(source_sha256, "vocabulary overlay source digest")
        except BridgeRefused as exc:
            raise InvalidArguments(str(exc)) from exc
        position = (turn, start, end)
        if previous is not None and previous >= position:
            raise InvalidArguments("vocabulary overlay replacements are not source ordered")
        previous = position


def _parse_command(frame: bytes, role: str) -> tuple[str, dict]:
    value = json.loads(frame, object_pairs_hook=_reject_duplicate_keys)
    if not isinstance(value, dict) or list(value) != [
        "schema",
        "request_id",
        "operation",
        "arguments",
    ]:
        raise BridgeRefused("command has the wrong outer shape")
    try:
        request_id = _canonical_uuid(value["request_id"])
    except InvalidArguments as exc:
        raise BridgeRefused("command request ID is invalid") from exc
    operation = f"note.{role}"
    if value["schema"] != "note-bridge-command/1" or value["operation"] != operation:
        raise BridgeRefused("command is outside the manifest role")
    arguments = value["arguments"]
    # Generation names no note: the note is what it is being asked to propose.
    # It names the installed model directory instead, because the manifest pins
    # model identity by digest and the caller owns where that model is
    # installed.
    expected = ["meeting_id", "note_id", "transcript_id"]
    if role == "generate":
        expected = ["meeting_id", "transcript_id"]
        if "speaker_label_overrides" in arguments:
            expected.append("speaker_label_overrides")
        if "vocabulary_replacements" in arguments:
            expected.append("vocabulary_replacements")
        if "pre_meeting_context" in arguments:
            expected.append("pre_meeting_context")
        expected.extend(("model_directory", "deadline_s"))
    if not isinstance(arguments, dict) or list(arguments) != expected:
        raise InvalidArguments("note arguments have the wrong shape")
    if role == "inspect":
        _canonical_uuid(arguments["meeting_id"])
    else:
        _opaque_meeting_id(arguments["meeting_id"])
    try:
        if role != "generate":
            _safe_digest(arguments["note_id"], "note ID")
        else:
            _safe_component(arguments["model_directory"], "model directory")
            # The caller owns the budget for the run it started; the bridge owns
            # the ceiling. `GENERATOR_DEADLINE_S` is the registered whole-run
            # bound, so a caller may ask for less and can never ask for more.
            deadline = arguments["deadline_s"]
            if type(deadline) is not int or not 1 <= deadline <= GENERATOR_DEADLINE_S:
                raise InvalidArguments("generation deadline is outside the registered bound")
            if "speaker_label_overrides" in arguments:
                _validate_speaker_label_overrides(arguments["speaker_label_overrides"])
            if "vocabulary_replacements" in arguments:
                _validate_vocabulary_replacements(arguments["vocabulary_replacements"])
            if "pre_meeting_context" in arguments:
                _validate_pre_meeting_context(arguments["pre_meeting_context"])
        _safe_digest(arguments["transcript_id"], "transcript ID")
    except BridgeRefused as exc:
        raise InvalidArguments(str(exc)) from exc
    return request_id, arguments


def _read_frame(parent_fd: int | None) -> bytes:
    buffer = bytearray()
    while b"\n" not in buffer:
        watched = [sys.stdin.fileno()]
        if parent_fd is not None:
            watched.append(parent_fd)
        ready, _, _ = select.select(watched, [], [], 10)
        if parent_fd is not None and parent_fd in ready and os.read(parent_fd, 1) == b"":
            raise BridgeRefused("parent liveness ended")
        if sys.stdin.fileno() not in ready:
            raise BridgeRefused("note command timed out")
        chunk = os.read(sys.stdin.fileno(), MAX_FRAME_BYTES + 2 - len(buffer))
        if not chunk:
            raise BridgeRefused("note command ended before a frame")
        buffer.extend(chunk)
        if len(buffer) > MAX_FRAME_BYTES:
            raise BridgeRefused("note command exceeds the frame limit")
    frame, extra = bytes(buffer).split(b"\n", 1)
    if extra:
        raise BridgeRefused("note bridge received a second input frame")
    return frame


def _emit(value: dict) -> None:
    """Write one outbound frame. `ensure_ascii=False` is contract, not taste.

    Every frame this bridge writes goes through here, and the spelling is load
    bearing rather than cosmetic. A ``backslash-u`` escape costs six bytes per
    non-ASCII scalar against a fixed 64 KiB frame, so the same legal projection
    can fit unescaped and overflow escaped — measured on the shared fixture's
    worst legal case at 65,236 bytes against 95,956. The manifest already pins
    this spelling and Rust refuses a backslash in it outright; the result frame
    has no such parser-side guard, so the guarantee has to live at the writer.
    """
    encoded = json.dumps(value, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    if len(encoded) + 1 > MAX_FRAME_BYTES:
        raise BridgeRefused("bridge frame exceeds its limit")
    sys.stdout.buffer.write(encoded + b"\n")
    sys.stdout.buffer.flush()


def _transcript_only(request_id: str, code: str, recoverable: bool, receipt: dict) -> None:
    """Every generate failure lands here: no note, and the transcript stands.

    One outcome for every failure class — a storage fault, a malformed response,
    a fabricated citation, a timeout — because the product answer is the same in
    all of them and a caller must not be able to tell them apart by shape. The
    receipt is closed and content-free by construction; see
    `_GeneratorSession.receipt`.
    """
    _emit({
        "schema": "note-generation-result/1",
        "request_id": request_id,
        "operation": "note.generate",
        "outcome": "transcript-only",
        "generation": None,
        "failure": {"code": code, "recoverable": recoverable, "receipt": receipt},
    })


def _refused(request_id: str, code: str, recoverable: bool, role: str) -> None:
    if role == "generate":
        _transcript_only(request_id, code, recoverable, {})
        return
    if role == "inspect":
        _emit({
            "schema": "note-bridge-result/1",
            "request_id": request_id,
            "operation": "note.inspect",
            "outcome": "refused",
            "artifact_digests": {},
            "failure": {"code": code, "recoverable": recoverable},
        })
        return
    _emit({
        "schema": "note-projection-result/1",
        "request_id": request_id,
        "operation": "note.project",
        "outcome": "refused",
        "projection": None,
        "failure": {"code": code, "recoverable": recoverable},
    })


def _watch_parent(parent_fd: int) -> None:
    def exit_at_eof() -> None:
        while True:
            try:
                value = os.read(parent_fd, 1)
            except InterruptedError:
                continue
            except OSError:
                _reap_generator()
                os._exit(2)
            if value == b"":
                _reap_generator()
                os._exit(0)

    threading.Thread(target=exit_at_eof, daemon=True).start()


def _watch_parent_pid(expected_parent_pid: int) -> None:
    if expected_parent_pid <= 1 or os.getppid() != expected_parent_pid:
        raise BridgeRefused("parent identity changed before watch registration")
    if not hasattr(select, "kqueue"):
        raise BridgeRefused("parent process watch is unavailable")
    queue = select.kqueue()
    event = select.kevent(
        expected_parent_pid,
        filter=select.KQ_FILTER_PROC,
        flags=select.KQ_EV_ADD | select.KQ_EV_ENABLE,
        fflags=select.KQ_NOTE_EXIT,
    )
    queue.control([event], 0, 0)
    if os.getppid() != expected_parent_pid:
        queue.close()
        raise BridgeRefused("parent identity changed during watch registration")

    def exit_with_parent() -> None:
        try:
            events = queue.control(None, 1, None)
        except Exception:
            _reap_generator()
            os._exit(2)
        # The generator is the one child this bridge can leave behind, and it
        # holds the model's unified memory. Reaped before exit, in its own
        # process group, so the app quitting does not strand several gigabytes.
        _reap_generator()
        os._exit(0 if events else 2)

    threading.Thread(target=exit_with_parent, daemon=True).start()


def _require_execution_identity(
    runtime: _VerifiedRuntime,
    standard_library: tuple[Path, ...],
    bundle_loader: object,
) -> None:
    _require_runtime_identity(runtime)
    runtime.require_unchanged()
    _require_loaded_code_confined(runtime, standard_library, bundle_loader)


def _require_descriptor_identity(
    runtime: _DescriptorRuntime,
    bundle_loader: object,
) -> None:
    runtime.require_unchanged()
    if any(
        getattr(sys.modules[name], "__loader__", None) is not bundle_loader
        for name in _VALIDATOR_MODULES
    ):
        raise BridgeRefused("validator code escaped the inherited bundle")


def _run_generation(
    runtime: "_VerifiedRuntime | _DescriptorRuntime",
    validator,
    root_fd: int,
    root_path: Path,
    arguments: dict,
    request_id: str,
    require_identity,
) -> int:
    """Verify the model, run the generator, and admit only validated points.

    Fail-closed throughout. The model tree is verified before the generator is
    invoked, so an unpinned or symlinked model refuses without any model being
    loaded, and every exit that is not a fully validated projection is the same
    `transcript-only` outcome.
    """
    try:
        model = _open_model_tree(
            root_fd, root_path, arguments["model_directory"], runtime.models
        )
    except (BridgeRefused, OSError):
        require_identity()
        _transcript_only(request_id, "model-unavailable", True, {})
        return 0
    session: _GeneratorSession | None = None
    receipt: dict = {}
    try:
        # Spawned on the first batch, not before it. Everything ahead of that
        # first request — opening the transcript, checking its digest, loading
        # it, enumerating candidates — can refuse, and none of those refusals
        # needs a model. Spawning eagerly would make a missing transcript pay a
        # multi-gigabyte load, and spend the run's deadline on it, before
        # answering a question the model was never required for.
        def ask(request: dict) -> str:
            nonlocal session

            if session is None:
                session = _GeneratorSession(
                    runtime.resources["generator"], model, arguments["deadline_s"]
                )
            # On every request, not just the first: the value is the same
            # verified path each time, and a child that had to remember it from
            # one earlier message is a contract that breaks quietly when the
            # batching changes.
            return session.ask({**request, "model_directory": model.path})

        def session_receipt() -> dict:
            return session.receipt() if session is not None else {}

        try:
            result = validator.generate(root_fd, arguments, ask=ask)
        except (validator.GenerationRefused, GeneratorRefused) as exc:
            require_identity()
            _transcript_only(request_id, exc.code, exc.recoverable, session_receipt())
            return 0
        except validator.ArtifactFailure as exc:
            require_identity()
            _transcript_only(request_id, exc.code, exc.recoverable, session_receipt())
            return 0
        except Exception:
            require_identity()
            _transcript_only(request_id, "note2-validation-failed", False, session_receipt())
            return 0
        receipt = session_receipt()
        # The model may not have changed under the generator that read it.
        try:
            model.require_unchanged()
        except BridgeRefused:
            require_identity()
            _transcript_only(request_id, "model-unavailable", True, receipt)
            return 0
    finally:
        if session is not None:
            session.close()
        model.close()
    require_identity()
    generated = {
        "schema": "note-generation-result/1",
        "request_id": request_id,
        "operation": "note.generate",
        "outcome": "generated",
        "generation": {**result, "receipt": receipt},
        "failure": None,
    }
    encoded = json.dumps(generated, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    if len(encoded) + 1 > MAX_FRAME_BYTES:
        # Unreachable at the current budget, and kept anyway. A maximal
        # projection — 64 claims, each a 160-character claim with three
        # locators — encodes to 46,019 bytes against a 65,536-byte frame, so
        # `MAX_GENERATED_CLAIMS` refuses first and no test can reach this line.
        # It stands because the keep budget is a product number that can move,
        # and if it moves the answer must still be a refusal: a shortened note
        # is indistinguishable from a note the model chose to keep short.
        _transcript_only(request_id, "generation-capacity-exceeded", False, receipt)
        return 0
    _emit(generated)
    return 0


def run(
    root_path: Path,
    manifest_path: Path | None,
    parent_fd: int | None,
    *,
    descriptor_fds: tuple[int, int, int] | None = None,
    generator_fd: int | None = None,
    expected_parent_pid: int | None = None,
) -> int:
    standard_library = _confine_runtime_imports()
    if descriptor_fds is None:
        if manifest_path is None or parent_fd is None or generator_fd is not None:
            raise BridgeRefused("path bridge launch is incomplete")
        runtime = verify_runtime(manifest_path)
    else:
        if manifest_path is not None or parent_fd is not None or expected_parent_pid is None:
            raise BridgeRefused("descriptor bridge launch is incomplete")
        _watch_parent_pid(expected_parent_pid)
        runtime = verify_descriptor_runtime(*descriptor_fds, generator_fd=generator_fd)
    root_fd = _open_absolute_directory(root_path, private=True)
    try:
        validator, bundle_loader = load_validator(runtime)
        if runtime.role == "generate":
            _require_registered_deadline(sys.modules["candidate_first"])

        def require_identity() -> None:
            if isinstance(runtime, _DescriptorRuntime):
                _require_descriptor_identity(runtime, bundle_loader)
            else:
                _require_execution_identity(runtime, standard_library, bundle_loader)

        require_identity()
        _emit(
            {
                "schema": "note-bridge-event/1",
                "event": "ready",
                "protocol": 1,
                "role": runtime.role,
                "manifest_sha256": runtime.manifest.digest,
                "operations": [f"note.{runtime.role}"],
            }
        )
        try:
            frame = _read_frame(parent_fd)
        except Exception:
            require_identity()
            return 2
        request_id: str | None = None
        try:
            parsed = json.loads(frame, object_pairs_hook=_reject_duplicate_keys)
            if isinstance(parsed, dict) and isinstance(parsed.get("request_id"), str):
                request_id = parsed["request_id"]
            request_id, arguments = _parse_command(frame, runtime.role)
        except InvalidArguments:
            if request_id is None:
                require_identity()
                return 2
            try:
                request_id = _canonical_uuid(request_id)
            except InvalidArguments:
                require_identity()
                return 2
            require_identity()
            _refused(request_id, "invalid-request", False, runtime.role)
            return 0
        except (BridgeRefused, UnicodeDecodeError, json.JSONDecodeError):
            require_identity()
            return 2
        require_identity()
        if runtime.role == "generate":
            return _run_generation(
                runtime,
                validator,
                root_fd,
                root_path,
                arguments,
                request_id,
                require_identity,
            )
        try:
            result = (
                validator.inspect(root_fd, arguments)
                if runtime.role == "inspect"
                else validator.project(root_fd, arguments)
            )
        except validator.ArtifactFailure as exc:
            if (exc.code, exc.recoverable) not in {
                ("artifact-invalid", False),
                ("artifact-missing", True),
                ("artifact-changed", False),
            }:
                require_identity()
                return 2
            require_identity()
            _refused(request_id, exc.code, exc.recoverable, runtime.role)
            return 0
        except Exception:
            require_identity()
            return 2
        require_identity()
        if runtime.role == "project":
            projected = {
                "schema": "note-projection-result/1",
                "request_id": request_id,
                "operation": "note.project",
                "outcome": "succeeded",
                "projection": result,
                "failure": None,
            }
            encoded = json.dumps(projected, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
            if len(encoded) + 1 > MAX_FRAME_BYTES:
                _refused(request_id, "projection-capacity-exceeded", False, runtime.role)
                return 0
            _emit(projected)
            return 0
        _emit(
            {
                "schema": "note-bridge-result/1",
                "request_id": request_id,
                "operation": "note.inspect",
                "outcome": "succeeded",
                "artifact_digests": result,
                "failure": None,
            }
        )
        return 0
    finally:
        os.close(root_fd)
        runtime.close()


def main() -> int:
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--temporary-private-root", type=Path, required=True)
    parser.add_argument("--note-runtime-manifest", type=Path, required=True)
    parser.add_argument("--parent-liveness-fd", type=int, required=True)
    arguments = parser.parse_args()
    _watch_parent(arguments.parent_liveness_fd)
    try:
        return run(
            arguments.temporary_private_root,
            arguments.note_runtime_manifest,
            arguments.parent_liveness_fd,
        )
    except Exception:
        return 2


def main_from_fds(
    manifest_fd: int,
    bridge_fd: int,
    validator_fd: int,
    storage_root: str,
    expected_parent_pid: int,
    generator_fd: int | None = None,
) -> int:
    """Entry point used only by the signed Rust-owned descriptor bootstrap.

    Three descriptors launch the `project` role; the fourth, when the parent
    stages one, carries the manifest-pinned generator bytes and launches
    `generate`. The role is derived from the descriptor set, never from the
    manifest alone, so a mislabeled manifest refuses instead of escalating.
    """
    try:
        return run(
            Path(storage_root),
            None,
            None,
            descriptor_fds=(manifest_fd, bridge_fd, validator_fd),
            generator_fd=generator_fd,
            expected_parent_pid=expected_parent_pid,
        )
    except Exception:
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
