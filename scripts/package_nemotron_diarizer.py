#!/usr/bin/env python3
"""Build and stage the optional Apple-Silicon Nemotron diarization runtime.

The source checkout is deliberately external to Yawn.  This script admits only
the pinned, clean NeMo-Speech.cpp revision, builds it into a Yawn-owned build
directory, and copies a closed Mach-O dependency set into the staged runtime.
It never downloads a model and never writes into the source checkout.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path


SOURCE_COMMIT = "97a15afa5caa9bce5baaa86c1184103877af4101"
SCHEMA = "nemotron-diarization-runtime/2"
COMMAND = ("diarize", "INPUT", "--model", "MODEL", "--device", "metal", "--preset", "v3-offline", "--format", "json")
REQUIRED_CACHE = {
    "CMAKE_BUILD_TYPE": "Release",
    "CMAKE_OSX_ARCHITECTURES": "arm64",
    "GGML_METAL": "ON",
    "GGML_METAL_EMBED_LIBRARY": "ON",
    "NEMO_SPEECH_GGML_PATCHED": "OFF",
    "NEMO_SPEECH_BUILD_ASR": "OFF",
    "NEMO_SPEECH_BUILD_CLI": "ON",
    "NEMO_SPEECH_BUILD_DIAR": "ON",
    "NEMO_SPEECH_BUILD_EXAMPLES": "OFF",
    "NEMO_SPEECH_BUILD_GRPC": "OFF",
    "NEMO_SPEECH_BUILD_HTTP": "OFF",
    "NEMO_SPEECH_BUILD_MIC_CAPTURE": "OFF",
    "NEMO_SPEECH_BUILD_NMT": "OFF",
    "NEMO_SPEECH_BUILD_S2S": "OFF",
    "NEMO_SPEECH_BUILD_TESTS": "OFF",
    "NEMO_SPEECH_BUILD_TOOLS": "OFF",
    "NEMO_SPEECH_BUILD_TTS": "OFF",
}

# NVIDIA's static SentencePiece branch deliberately excludes Apple.  The patch
# below is the small, reviewed macOS counterpart: it chooses only the pinned
# archive placed in NEMO_SPEECH_DEPENDENCY_PREFIX and does not fall back to a
# host package.  It is applied only to the disposable checkout below.
MACOS_STATIC_SENTENCEPIECE_PATCH = """\
diff --git a/src/asr/CMakeLists.txt b/src/asr/CMakeLists.txt
--- a/src/asr/CMakeLists.txt
+++ b/src/asr/CMakeLists.txt
@@ -68,8 +68,8 @@
 # phrases with the model's embedded tokenizer (context biasing in
 # rnnt_greedy_decoder.cpp / RnntModel::encode_phrase); the flashlight decoder
 # also encodes OOV boost phrases with it. Prefer a static archive on ELF
 # platforms so its bundled protobuf symbols stay private.
-if(UNIX AND NOT APPLE)
+if(UNIX)
     set(_NEMO_SPEECH_LIBRARY_SUFFIXES "${CMAKE_FIND_LIBRARY_SUFFIXES}")
     set(CMAKE_FIND_LIBRARY_SUFFIXES ".a")
     find_library(SENTENCEPIECE_STATIC_LIB sentencepiece
@@ -85,6 +85,8 @@ if(SENTENCEPIECE_STATIC_LIB)
     endif()
     target_link_libraries(nemo_speech_asr PRIVATE ${SENTENCEPIECE_STATIC_LIB})
     target_include_directories(nemo_speech_asr PRIVATE ${SENTENCEPIECE_INCLUDE_DIR})
-    target_link_options(
-        nemo_speech_asr PRIVATE "LINKER:--exclude-libs,libsentencepiece.a")
+    if(NOT APPLE)
+        target_link_options(
+            nemo_speech_asr PRIVATE "LINKER:--exclude-libs,libsentencepiece.a")
+    endif()
 elseif(NEMO_SPEECH_WITH_NORM AND UNIX AND NOT APPLE)
"""
PATCH_SHA256 = hashlib.sha256(MACOS_STATIC_SENTENCEPIECE_PATCH.encode()).hexdigest()


class AdmissionError(ValueError):
    """The native runtime cannot be safely admitted to Yawn's bundle."""


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
        raise AdmissionError(message)


def sha256(path: Path) -> str:
    require(path.is_file() and not path.is_symlink(), f"runtime file is missing or unsafe: {path}")
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def checked_source(source: Path) -> Path:
    source = source.resolve(strict=True)
    head = run("git", "-C", str(source), "rev-parse", "HEAD")
    require(head.returncode == 0, f"NeMo-Speech.cpp source is not a Git checkout: {source}")
    require(head.stdout.strip() == SOURCE_COMMIT, "NeMo-Speech.cpp source commit does not match the pinned revision")
    status = run("git", "-C", str(source), "status", "--porcelain", "--ignore-submodules=none")
    require(status.returncode == 0, f"could not inspect NeMo-Speech.cpp source state: {source}")
    require(not status.stdout.strip(), "NeMo-Speech.cpp source checkout is dirty; refuse a non-reproducible native runtime")
    return source


def source_tree(source: Path) -> str:
    tree = run("git", "-C", str(source), "rev-parse", f"{SOURCE_COMMIT}^{{tree}}")
    require(tree.returncode == 0 and len(tree.stdout.strip()) == 40, "could not read pinned NeMo-Speech.cpp tree")
    return tree.stdout.strip()


def clean_source_copy(seed: Path, build_dir: Path) -> Path:
    """Make the pinned source and its sole Metal dependency locally.

    `seed` may be the deliberately dirty research checkout.  We read only its
    Git object store, clone from it without a network remote, and never reset
    or alter it.  ggml is the sole submodule required by the Metal diarization
    build; the checked-out child must match the pin and be clean.
    """
    seed = seed.resolve(strict=True)
    head = run("git", "-C", str(seed), "rev-parse", "HEAD")
    require(head.returncode == 0 and head.stdout.strip() == SOURCE_COMMIT, "NeMo-Speech.cpp seed does not contain the pinned revision")
    target = build_dir / "source"
    if target.exists():
        shutil.rmtree(target)
    cloned = run("git", "clone", "--no-checkout", "--shared", str(seed), str(target))
    require(cloned.returncode == 0, f"could not create local NeMo-Speech.cpp source copy: {cloned.stderr.strip()}")
    checkout = run("git", "-C", str(target), "checkout", "--detach", "--quiet", SOURCE_COMMIT)
    require(checkout.returncode == 0, f"could not check out pinned NeMo-Speech.cpp source: {checkout.stderr.strip()}")
    required = run("git", "-C", str(target), "ls-tree", SOURCE_COMMIT, "ggml")
    require(required.returncode == 0 and required.stdout.startswith("160000 commit "), "pinned NeMo-Speech.cpp source has no ggml submodule")
    ggml_commit = required.stdout.split()[2]
    seed_ggml = seed / "ggml"
    ggml_head = run("git", "-C", str(seed_ggml), "rev-parse", "HEAD")
    require(ggml_head.returncode == 0 and ggml_head.stdout.strip() == ggml_commit, "research ggml checkout does not provide the pinned stock revision")
    ggml = target / "ggml"
    copied = run("git", "clone", "--no-checkout", "--shared", str(seed_ggml), str(ggml))
    require(copied.returncode == 0, f"could not create local ggml source copy: {copied.stderr.strip()}")
    checkout = run("git", "-C", str(ggml), "checkout", "--detach", "--quiet", ggml_commit)
    require(checkout.returncode == 0, f"could not check out pinned stock ggml: {checkout.stderr.strip()}")
    # subprocess.run does not accept input through the shared `run` helper.
    applied = subprocess.run(
        ("git", "-C", str(target), "apply", "--whitespace=error-all", "-"),
        input=MACOS_STATIC_SENTENCEPIECE_PATCH,
        text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False,
    )
    require(applied.returncode == 0, f"could not apply Yawn macOS SentencePiece patch: {applied.stderr.strip()}")
    status = run("git", "-C", str(target), "status", "--porcelain", "--ignore-submodules=none")
    require(status.returncode == 0 and status.stdout.strip() == "M src/asr/CMakeLists.txt", "local NeMo-Speech.cpp copy differs from the approved patch")
    checked = run("git", "-C", str(ggml), "status", "--porcelain")
    require(checked.returncode == 0 and not checked.stdout.strip(), "local ggml source copy is not clean")
    return target


def static_dependency_prefix(prefix: Path) -> Path:
    prefix = prefix.resolve()
    require(prefix.is_dir() and not prefix.is_symlink(), f"pinned static dependency prefix is unavailable: {prefix}")
    archive = prefix / "sentencepiece/lib/libsentencepiece.a"
    header = prefix / "sentencepiece/include/sentencepiece_processor.h"
    licenses = prefix / "sentencepiece/share/licenses/nemo-speech/third_party/sentencepiece"
    require(archive.is_file() and not archive.is_symlink(), f"pinned static SentencePiece archive is unavailable: {archive}")
    require(header.is_file() and not header.is_symlink(), f"pinned static SentencePiece header is unavailable: {header}")
    require(licenses.is_dir() and not licenses.is_symlink(), f"pinned static SentencePiece licenses are unavailable: {licenses}")
    require(any(path.is_file() and not path.is_symlink() for path in licenses.rglob("*")), "pinned static SentencePiece license directory is empty")
    return prefix


def cache_values(build_dir: Path) -> dict[str, str]:
    cache = build_dir / "CMakeCache.txt"
    require(cache.is_file(), f"NeMo-Speech.cpp build cache is missing: {cache}")
    values: dict[str, str] = {}
    for line in cache.read_text(encoding="utf-8").splitlines():
        if line.startswith("//") or line.startswith("#") or "=" not in line or ":" not in line:
            continue
        key_and_type, value = line.split("=", 1)
        key, _type = key_and_type.split(":", 1)
        values[key] = value
    return values


def verify_build_configuration(source: Path, build_dir: Path) -> None:
    values = cache_values(build_dir)
    require(values.get("CMAKE_HOME_DIRECTORY") == str(source), "NeMo-Speech.cpp build cache points at a different source checkout")
    for key, expected in REQUIRED_CACHE.items():
        require(values.get(key) == expected, f"NeMo-Speech.cpp build cache must set {key}={expected}")


def macho_architecture(path: Path) -> None:
    inspected = run("lipo", "-archs", str(path))
    require(inspected.returncode == 0 and inspected.stdout.split() == ["arm64"], f"native runtime must be arm64 only: {path.name}")


def linked_dylibs(path: Path) -> list[str]:
    inspected = run("otool", "-L", str(path))
    require(inspected.returncode == 0, f"could not inspect Mach-O dependencies: {path}")
    names = []
    for line in inspected.stdout.splitlines()[1:]:
        name = line.strip().split(" ", 1)[0]
        if name.startswith("@rpath/") and name.endswith(".dylib"):
            names.append(name.removeprefix("@rpath/"))
        elif name.startswith("/") and not name.startswith(("/usr/lib/", "/System/Library/")):
            raise AdmissionError(
                f"native runtime has a non-system absolute dependency: {name}; "
                "provide a pinned static build dependency instead"
            )
    return sorted(set(names))


def rpaths(path: Path) -> list[str]:
    inspected = run("otool", "-l", str(path))
    require(inspected.returncode == 0, f"could not inspect Mach-O load paths: {path}")
    values = []
    lines = iter(inspected.stdout.splitlines())
    for line in lines:
        if line.strip() == "cmd LC_RPATH":
            for detail in lines:
                detail = detail.strip()
                if detail.startswith("path "):
                    values.append(detail.removeprefix("path ").split(" (offset", 1)[0])
                    break
    return values


def set_rpath(path: Path, expected: str) -> None:
    for value in rpaths(path):
        changed = run("install_name_tool", "-delete_rpath", value, str(path))
        require(changed.returncode == 0, f"could not remove native runtime rpath {value}: {path.name}")
    changed = run("install_name_tool", "-add_rpath", expected, str(path))
    require(changed.returncode == 0, f"could not set native runtime rpath: {path.name}")
    require(rpaths(path) == [expected], f"native runtime has unexpected loader paths: {path.name}")


def resource(relative: Path, root: Path) -> dict[str, str]:
    return {"path": str(relative), "sha256": sha256(root / relative)}


def runtime_document(stage: Path, dylibs: list[Path], assets: list[Path], licenses: list[Path], source: dict[str, str]) -> dict:
    return {
        "schema": SCHEMA,
        "source": source,
        "platform": {"os": "macos", "arch": "arm64", "backend": "metal"},
        "command": {**resource(Path("bin/nemo-speech"), stage), "argv": list(COMMAND)},
        "dylibs": [resource(path, stage) for path in dylibs],
        "assets": [resource(path, stage) for path in assets],
        "licenses": [resource(path, stage) for path in licenses],
    }


def write_document(stage: Path, document: dict) -> Path:
    target = stage / "runtime.json"
    target.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
    os.chmod(target, 0o644)
    return target


def verify_staged_runtime(stage: Path, *, check_digests: bool = True) -> dict:
    stage = stage.resolve(strict=True)
    document_path = stage / "runtime.json"
    try:
        document = json.loads(document_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise AdmissionError(f"Nemotron runtime receipt is unreadable: {exc}") from None
    require(set(document) == {"schema", "source", "platform", "command", "dylibs", "assets", "licenses"}, "Nemotron runtime receipt has an unexpected shape")
    require(document["schema"] == SCHEMA, "Nemotron runtime receipt schema is not current")
    source = document["source"]
    require(isinstance(source, dict) and source.get("commit") == SOURCE_COMMIT and isinstance(source.get("tree"), str) and len(source["tree"]) == 40 and source.get("patch_sha256") == PATCH_SHA256, "Nemotron runtime receipt source pin or patch digest is wrong")
    require(document["platform"] == {"os": "macos", "arch": "arm64", "backend": "metal"}, "Nemotron runtime receipt platform is wrong")
    command = document["command"]
    require(isinstance(command, dict) and set(command) == {"path", "sha256", "argv"}, "Nemotron runtime command receipt is invalid")
    require(command.get("path") == "bin/nemo-speech" and command.get("argv") == list(COMMAND), "Nemotron runtime command contract is wrong")
    require(isinstance(command["sha256"], str) and len(command["sha256"]) == 64, "Nemotron runtime command digest is invalid")
    if check_digests:
        require(sha256(stage / command["path"]) == command["sha256"], "Nemotron runtime command digest differs")
    entries = document["dylibs"] if isinstance(document["dylibs"], list) else []
    require(entries, "Nemotron runtime receipt carries no dylibs")
    paths = [command["path"]]
    for entry in entries:
        require(isinstance(entry, dict) and set(entry) == {"path", "sha256"}, "Nemotron runtime resource receipt is invalid")
        relative = entry["path"]
        require(isinstance(relative, str) and relative and not Path(relative).is_absolute() and ".." not in Path(relative).parts, "Nemotron runtime resource path is unsafe")
        require(isinstance(entry["sha256"], str) and len(entry["sha256"]) == 64, "Nemotron runtime resource digest is invalid")
        require(relative not in paths, "Nemotron runtime receipt repeats a resource")
        paths.append(relative)
        if check_digests:
            require(sha256(stage / relative) == entry["sha256"], f"Nemotron runtime resource digest differs: {relative}")
    require(all(path.startswith("lib/") and path.endswith(".dylib") for path in paths[1:]), "Nemotron runtime dylib receipt is invalid")
    assets = document["assets"] if isinstance(document["assets"], list) else []
    require(not assets, "embedded Metal runtime must not declare external assets")
    for entry in assets:
        require(isinstance(entry, dict) and set(entry) == {"path", "sha256"}, "Nemotron runtime Metal asset receipt is invalid")
        require(isinstance(entry["sha256"], str) and len(entry["sha256"]) == 64, "Nemotron runtime Metal asset digest is invalid")
        require((stage / entry["path"]).is_file() and not (stage / entry["path"]).is_symlink(), "Nemotron runtime Metal asset is missing or unsafe")
        if check_digests:
            require(sha256(stage / entry["path"]) == entry["sha256"], "Nemotron runtime Metal asset digest differs")
    licenses = document["licenses"] if isinstance(document["licenses"], list) else []
    require(licenses, "Nemotron runtime receipt has no license accounting")
    for entry in licenses:
        require(isinstance(entry, dict) and set(entry) == {"path", "sha256"}, "Nemotron runtime license receipt is invalid")
        relative = entry["path"]
        require(isinstance(relative, str) and relative.startswith("licenses/") and ".." not in Path(relative).parts, "Nemotron runtime license path is unsafe")
        require(isinstance(entry["sha256"], str) and len(entry["sha256"]) == 64, "Nemotron runtime license digest is invalid")
        if check_digests:
            require(sha256(stage / relative) == entry["sha256"], f"Nemotron runtime license digest differs: {relative}")
    return document


def refresh_staged_runtime(stage: Path) -> None:
    """Re-bind a verified native closure after nested code signing changes bytes.

    The release lane signs nested Mach-O files before it rebuilds manifests.  A
    receipt written before that step would necessarily go stale.  This refresh
    keeps the immutable source/command contract, re-checks the staged loader
    closure, then records the signed bytes for the enclosing app signature.
    """
    document = verify_staged_runtime(stage, check_digests=False)
    dylibs = [Path(entry["path"]) for entry in document["dylibs"]]
    executable = stage / "bin/nemo-speech"
    for path in [executable, *(stage / relative for relative in dylibs)]:
        signature = run("codesign", "--verify", "--strict", str(path))
        require(signature.returncode == 0, f"staged Nemotron binary is not signed: {path.name}")
    macho_architecture(executable)
    require(rpaths(executable) == ["@loader_path/../lib"], "staged Nemotron CLI loader path is wrong")
    names = {path.name for path in dylibs}
    require(set(linked_dylibs(executable)).issubset(names), "staged Nemotron CLI dylib set is incomplete")
    for relative in dylibs:
        dylib = stage / relative
        macho_architecture(dylib)
        require(rpaths(dylib) == ["@loader_path"], f"staged Nemotron dylib loader path is wrong: {relative.name}")
        require(set(linked_dylibs(dylib)).issubset(names), f"staged Nemotron dylib set is incomplete: {relative.name}")
    # Rehashing is reachable only through the release lanes' explicit
    # post-sign flag. `admit()` alone accepts artifacts from a source build.
    write_document(stage, runtime_document(stage, dylibs, [Path(entry["path"]) for entry in document["assets"]], [Path(entry["path"]) for entry in document["licenses"]], document["source"]))


def stage_licenses(source: Path, dependency_prefix: Path, stage: Path) -> list[Path]:
    entries = [source / "LICENSE", source / "NOTICE", source / "THIRD_PARTY_NOTICES.md", source / "ggml/LICENSE"]
    entries.extend(sorted((dependency_prefix / "sentencepiece/share/licenses/nemo-speech/third_party/sentencepiece").rglob("*")))
    copied: list[Path] = []
    for original in entries:
        if not original.is_file() or original.is_symlink():
            continue
        relative = Path("licenses") / original.relative_to(source if original.is_relative_to(source) else dependency_prefix)
        target = stage / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(original, target, follow_symlinks=False)
        copied.append(relative)
    require(copied, "could not stage Nemotron license accounting")
    return copied


def admit(source: Path, build_dir: Path, stage: Path, dependency_prefix: Path) -> None:
    source = source.resolve(strict=True)
    head = run("git", "-C", str(source), "rev-parse", "HEAD")
    require(head.returncode == 0 and head.stdout.strip() == SOURCE_COMMIT, "local NeMo-Speech.cpp source pin is wrong")
    build_dir = build_dir.resolve(strict=True)
    verify_build_configuration(source, build_dir)
    artifact_dir = build_dir / "bin"
    executable = artifact_dir / "nemo-speech"
    require(executable.is_file() and os.access(executable, os.X_OK), "NeMo-Speech.cpp diarization CLI is missing")
    macho_architecture(executable)
    pending = linked_dylibs(executable)
    require(pending, "NeMo-Speech.cpp diarization CLI has no packaged dylib dependencies")
    resolved: dict[str, Path] = {}
    while pending:
        name = pending.pop()
        if name in resolved:
            continue
        artifact = artifact_dir / name
        require(artifact.exists(), f"NeMo-Speech.cpp dependency is missing from build output: {name}")
        resolved[name] = artifact.resolve(strict=True)
        pending.extend(linked_dylibs(resolved[name]))

    if stage.exists():
        shutil.rmtree(stage)
    (stage / "bin").mkdir(parents=True)
    (stage / "lib").mkdir()
    shutil.copy2(executable, stage / "bin/nemo-speech", follow_symlinks=True)
    os.chmod(stage / "bin/nemo-speech", 0o755)
    dylibs: list[Path] = []
    for name, artifact in sorted(resolved.items()):
        target = stage / "lib" / name
        shutil.copy2(artifact, target, follow_symlinks=True)
        os.chmod(target, 0o755)
        macho_architecture(target)
        set_rpath(target, "@loader_path")
        dylibs.append(target.relative_to(stage))
    set_rpath(stage / "bin/nemo-speech", "@loader_path/../lib")
    staged_names = {path.name for path in dylibs}
    require(set(linked_dylibs(stage / "bin/nemo-speech")).issubset(staged_names), "staged CLI dylib set differs from its declared dependencies")
    for path in dylibs:
        require(set(linked_dylibs(stage / path)).issubset(staged_names), f"staged dylib set is incomplete for {path.name}")
    smoke = run(str(stage / "bin/nemo-speech"), "diarize", "--help")
    require(smoke.returncode == 0, "staged Nemotron CLI cannot load its private dylibs")
    for token in ("--device", "--preset", "v3-offline", "--format"):
        require(token in smoke.stdout, f"staged Nemotron CLI does not expose diarization option: {token}")
    licenses = stage_licenses(source, dependency_prefix, stage)
    write_document(stage, runtime_document(stage, dylibs, [], licenses, {"commit": SOURCE_COMMIT, "tree": source_tree(source), "patch_sha256": PATCH_SHA256}))
    verify_staged_runtime(stage)


def build(seed: Path, build_dir: Path, stage: Path, dependency_prefix: Path) -> None:
    build_dir = build_dir.resolve()
    stage = stage.resolve()
    build_dir.parent.mkdir(parents=True, exist_ok=True)
    source = clean_source_copy(seed, build_dir)
    dependency_prefix = static_dependency_prefix(dependency_prefix)
    cmake_dir = build_dir / "cmake"
    configured = run(
        "cmake", "-S", str(source), "-B", str(cmake_dir), "-G", "Ninja",
        "-DCMAKE_BUILD_TYPE=Release", "-DCMAKE_OSX_ARCHITECTURES=arm64",
        f"-DNEMO_SPEECH_DEPENDENCY_PREFIX={dependency_prefix}",
        "-DGGML_METAL=ON", "-DGGML_METAL_EMBED_LIBRARY=ON",
        "-DNEMO_SPEECH_GGML_PATCHED=OFF",
        "-DNEMO_SPEECH_BUILD_ASR=OFF", "-DNEMO_SPEECH_BUILD_CLI=ON",
        "-DNEMO_SPEECH_BUILD_DIAR=ON", "-DNEMO_SPEECH_BUILD_EXAMPLES=OFF",
        "-DNEMO_SPEECH_BUILD_MIC_CAPTURE=OFF",
        "-DNEMO_SPEECH_BUILD_GRPC=OFF", "-DNEMO_SPEECH_BUILD_HTTP=OFF",
        "-DNEMO_SPEECH_BUILD_NMT=OFF", "-DNEMO_SPEECH_BUILD_S2S=OFF",
        "-DNEMO_SPEECH_BUILD_TESTS=OFF", "-DNEMO_SPEECH_BUILD_TOOLS=OFF",
        "-DNEMO_SPEECH_BUILD_TTS=OFF",
    )
    require(configured.returncode == 0, f"NeMo-Speech.cpp configure failed: {configured.stderr.strip()}")
    verify_build_configuration(source, cmake_dir)
    compiled = run("cmake", "--build", str(cmake_dir), "--parallel")
    require(compiled.returncode == 0, f"NeMo-Speech.cpp build failed: {compiled.stderr.strip()}")
    admit(source, cmake_dir, stage, dependency_prefix)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("build", "admit", "verify"))
    parser.add_argument("--source", type=Path)
    parser.add_argument("--build-dir", type=Path)
    parser.add_argument("--dependency-prefix", type=Path)
    parser.add_argument("--stage", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "verify":
            verify_staged_runtime(args.stage)
        else:
            require(args.source is not None and args.build_dir is not None and args.dependency_prefix is not None, "build and admit require --source, --build-dir, and --dependency-prefix")
            if args.command == "build":
                build(args.source, args.build_dir, args.stage, args.dependency_prefix)
            else:
                admit(args.source, args.build_dir, args.stage, static_dependency_prefix(args.dependency_prefix))
    except (AdmissionError, OSError) as exc:
        print(f"package-nemotron-diarizer: BLOCKED — {exc}", file=sys.stderr)
        return 1
    print(f"package-nemotron-diarizer: PASS — {args.stage}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
