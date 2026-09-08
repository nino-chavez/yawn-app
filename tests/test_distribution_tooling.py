from __future__ import annotations

import hashlib
import json
import importlib.util
import os
import plistlib
import re
import subprocess
import tempfile
import unittest
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def source(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def load_build_manifest():
    path = ROOT / "worker/build_manifest.py"
    spec = importlib.util.spec_from_file_location("note_build_manifest", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_release_verifier():
    path = ROOT / "scripts/verify-release-bundle.py"
    spec = importlib.util.spec_from_file_location("release_bundle_verifier", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class DistributionToolingTests(unittest.TestCase):
    def test_dmg_builder_and_layout_are_closed(self) -> None:
        builder = source("scripts/build-dmg.sh")
        for required in (
            "ditto",
            "ln -s /Applications",
            "-format UDZO",
            '[[ "$DMG" == *.dmg ]]',
        ):
            self.assertIn(required, builder)

        layout = source("scripts/verify-dmg-layout.sh")
        for required in (
            "-readonly -nobrowse",
            "Contents/Info.plist",
            '[[ -L "$MOUNT/Applications" ]]',
            'readlink "$MOUNT/Applications"',
            "unexpected top-level DMG content",
        ):
            self.assertIn(required, layout)

    def test_signing_sequence_reuses_the_proven_film_room_trust_path(self) -> None:
        signing = source("scripts/sign-notarize.sh")
        self.assertIn('PROFILE="filmroom-notary"', signing)
        self.assertIn('EXPECTED_TEAM_ID="34VZ63G58M"', signing)
        identity_check = signing.index("Developer ID Application identity:")
        notary_check = signing.index("notary profile:")
        preflight_verdict = signing.index("signing preflight: PASS")
        self.assertLess(identity_check, preflight_verdict)
        self.assertLess(notary_check, preflight_verdict)

        unsigned_verify = signing.index('verify-release-bundle.py" "$APP"')
        first_mutating_sign = signing.index('codesign "${sign_args[@]}" "$path"')
        manifest_refresh = signing.index(
            'build_manifest.py" \\\n  "$APP/Contents/Resources" --admission "$ADMISSION"'
        )
        outer_app_sign = signing.index(
            'codesign --force --options runtime --timestamp \\\n'
            '  --entitlements "$CAPTURE_ENTITLEMENTS" --sign "$IDENTITY" "$APP"'
        )
        app_submit = signing.index('notarytool submit "$STAGE/app.zip"')
        app_staple = signing.index('stapler staple "$APP"')
        dmg_build = signing.index('build-dmg.sh" "$APP" "$DMG"')
        dmg_submit = signing.index('notarytool submit "$DMG"')
        final_verify = signing.index('verify-signed-release.sh" "$APP" "$DMG"')
        self.assertLess(unsigned_verify, first_mutating_sign)
        self.assertLess(first_mutating_sign, manifest_refresh)
        self.assertLess(manifest_refresh, outer_app_sign)
        self.assertLess(app_submit, app_staple)
        self.assertLess(app_staple, dmg_build)
        self.assertLess(dmg_build, dmg_submit)
        self.assertLess(dmg_submit, final_verify)

        self.assertIn('if [[ "$cmd" == "run-alpha" ]]', signing)
        self.assertIn('--entitlements "$PYTHON_ENTITLEMENTS"', signing)
        self.assertIn('EXPECTED_IDENTIFIER="com.ninochavez.local-meeting-notes"', signing)
        self.assertIn('PYTHON_SIGNING_IDENTIFIER="${EXPECTED_IDENTIFIER}.python-runtime"', signing)
        self.assertIn('--identifier "$PYTHON_SIGNING_IDENTIFIER"', signing)
        self.assertIn('--entitlements "$CAPTURE_ENTITLEMENTS"', signing)

    def test_local_signing_mode_requires_admission_and_stops_before_release_steps(self) -> None:
        signing = source("scripts/sign-notarize.sh")
        self.assertIn('if [[ "$cmd" == "local" ]]', signing)
        self.assertIn('[[ "${1:-}" == "--admission" ]]', signing)
        self.assertIn('[[ "$ADMISSION" == "product" || "$ADMISSION" == "internal-alpha" ]]', signing)
        self.assertIn('LOCAL_ONLY=1', signing)

        signed_verify = signing.index('[[ "$verified" == "1" ]] || die "signed app failed release verification"')
        local_exit = signing.index('echo "DONE: locally signed and verified app (not notarized or packaged)"')
        app_submit = signing.index('notarytool submit "$STAGE/app.zip"')
        dmg_build = signing.index('build-dmg.sh" "$APP" "$DMG"')
        self.assertLess(signed_verify, local_exit)
        self.assertLess(local_exit, app_submit)
        self.assertLess(local_exit, dmg_build)

        local_section = signing[signing.index('if [[ "$cmd" == "local" ]]'):local_exit]
        self.assertIn('--options runtime', local_section)
        self.assertIn('build_manifest.py', local_section)
        self.assertIn('verify-release-bundle.py" "$APP" --signed --admission "$ADMISSION"', local_section)
        self.assertIn(
            'if [[ "$LOCAL_ONLY" == "0" ]]; then\n  xcrun notarytool history',
            signing,
        )

    def test_local_signing_mode_exits_before_notarytool_stapler_and_dmg(self) -> None:
        signing = source("scripts/sign-notarize.sh")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bin_dir = root / "bin"
            bin_dir.mkdir()
            log = root / "calls.log"

            def stub(name: str, body: str) -> None:
                path = bin_dir / name
                path.write_text(
                    f"#!/bin/sh\nprintf '%s ' '{name}' >> '{log}'\nprintf '%s ' \"$@\" >> '{log}'\nprintf '\\n' >> '{log}'\n{body}\n"
                )
                path.chmod(0o755)

            # The local lane must never call any of these release-only tools.
            stub("xcrun", "exit 99")
            stub("stapler", "exit 99")
            stub("hdiutil", "exit 99")
            stub("codesign", "exit 0")
            stub("security", "printf '%s\\n' '  1) ABC \"Developer ID Application: Test (34VZ63G58M)\"'")
            stub("file", "printf '%s\\n' 'Mach-O 64-bit executable arm64'")
            stub("python3", "case \"$*\" in *'\\\"schema\\\"'*) printf '%s\\n' app-runtime/1 ;; *'\\\"encoder\\\"'*) printf '%s\\n' encoder-unavailable.identity ;; esac")

            scripts_dir = root / "scripts"
            scripts_dir.mkdir()
            signing_script = scripts_dir / "sign-notarize.sh"
            signing_script.write_text(source("scripts/sign-notarize.sh"))
            signing_script.chmod(0o755)
            (scripts_dir / "verify-release-bundle.py").write_text("#!/usr/bin/env python3\n")
            (scripts_dir / "verify-release-bundle.py").chmod(0o755)
            worker_dir = root / "worker"
            worker_dir.mkdir()
            (worker_dir / "build_manifest.py").write_text("#!/usr/bin/env python3\n")
            (worker_dir / "build_manifest.py").chmod(0o755)
            entitlements_dir = root / "apps/desktop/src-tauri"
            entitlements_dir.mkdir(parents=True)
            (entitlements_dir / "python-entitlements.plist").write_text("fixture")
            (entitlements_dir / "capture-entitlements.plist").write_text("fixture")

            app = root / "Yawn.app/Contents/Resources"
            app.mkdir(parents=True)
            (app / "Info.plist").write_text("fixture")
            (app / "app-runtime.json").write_text('{"encoder":{"path":"encoder-unavailable.identity"},"schema":"app-runtime/1"}')
            environment = os.environ.copy()
            environment["PATH"] = f"{bin_dir}:{environment['PATH']}"
            result = subprocess.run(
                [str(signing_script), "local", "--admission", "product", str(root / "Yawn.app")],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                env=environment,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn(
                "Developer ID timestamping may contact Apple; notarization and packaging are disabled",
                result.stdout,
            )
            calls = log.read_text()
            self.assertIn("codesign --force --options runtime", calls)
            self.assertIn("verify-release-bundle.py", calls)
            self.assertNotIn("notarytool", calls)
            self.assertNotIn("stapler", calls)
            self.assertNotIn("build-dmg", calls)

        # Every binary that asks macOS for the microphone must be signed with the
        # audio-input entitlement. Shipping a requester without it is not a
        # cosmetic miss: the requester never appears in System Settings, so the
        # operator has no way to grant what the app just asked for. That defect
        # shipped once. These pins are why it cannot ship silently again.
        self.assertIn(
            '|| "$path" == "$APP/Contents/Resources/bin/permission-probe" ]]', signing
        )
        preview = source("scripts/prepare-preview-bundle.sh")
        self.assertIn('PROBE="$RESOURCES/bin/permission-probe"', preview)
        self.assertIn(
            'codesign --force --sign "$identity" --entitlements "$ENTITLEMENTS" "$PROBE"',
            preview,
        )
        self.assertIn('has_audio_input_entitlement "$PROBE"', preview)
        verifier = source("scripts/verify-release-bundle.py")
        self.assertIn(
            'PROBE_EXECUTABLE = Path("Contents/Resources/bin/permission-probe")', verifier
        )
        self.assertIn(
            "relative in {MAIN_EXECUTABLE, CAPTURE_EXECUTABLE, PROBE_EXECUTABLE}", verifier
        )
        runtime_build = source("worker/build_runtime.sh")
        self.assertIn('[[ -x "$STAGE/bin/permission-probe" ]]', runtime_build)
        self.assertIn("--product permission-probe", runtime_build)
        # The probe is a manifest resource, so it is digest-verified before the app
        # spawns it, the same as every other child. Bound in every admission rather
        # than only internal-alpha, because first run exists in every build.
        self.assertIn(
            '(manifest.get("permission_probe") or {}).get("path") == "bin/permission-probe"',
            verifier,
        )
        self.assertIn(
            '"permission_probe": Path("bin/permission-probe")',
            source("worker/build_manifest.py"),
        )
        self.assertIn('"permission_probe",', source("worker/main.py"))

        entitlements = plistlib.loads(
            (ROOT / "apps/desktop/src-tauri/python-entitlements.plist").read_bytes()
        )
        self.assertEqual(
            entitlements,
            {"com.apple.security.cs.allow-unsigned-executable-memory": True},
        )
        capture_entitlements = plistlib.loads(
            (ROOT / "apps/desktop/src-tauri/capture-entitlements.plist").read_bytes()
        )
        self.assertEqual(
            capture_entitlements,
            {"com.apple.security.device.audio-input": True},
        )

    def test_frozen_verifier_covers_app_dmg_layout_and_runtime(self) -> None:
        verifier = source("scripts/verify-signed-release.sh")
        for required in (
            'codesign --verify --deep --strict "$APP"',
            'stapler validate "$APP"',
            'spctl --assess --type execute --verbose=4 "$APP"',
            'codesign --verify --strict "$DMG"',
            'stapler validate "$DMG"',
            "context:primary-signature",
            "verify-dmg-layout.sh",
            'verify-release-bundle.py" "$APP" --signed',
            'shasum -a 256 "$DMG"',
        ):
            self.assertIn(required, verifier)

    def test_bundle_verifier_refuses_unselected_admission_and_extra_entitlements(self) -> None:
        verifier = source("scripts/verify-release-bundle.py")
        for required in (
            'manifest.get("admission") == admission',
            'choices=("product", "internal-alpha")',
            'default="product"',
            '"whisper-large-v3-turbo-weights"',
            '"bin/meeting-capture"',
            "NSMicrophoneUsageDescription",
            "NSAudioCaptureUsageDescription",
            'EXPECTED_TEAM_ID = "34VZ63G58M"',
            '"arm64" in architecture_set',
            'architecture_set <= {"arm64", "x86_64"}',
            'if "executable" in kind.stdout',
            "np.linalg.svd(np.eye(2))",
            "np.fft.fft(np.ones(4))",
            "entitlements(path) == expected_entitlements",
            "CAPTURE_EXECUTABLE",
            '"com.apple.security.device.audio-input": True',
            'EXPECTED_PYTHON_IDENTIFIER = f"{EXPECTED_IDENTIFIER}.python-runtime"',
            "CODE_SIGNATURE_RUNTIME = 0x00010000",
            "expected_designated_requirement(identifier)",
            "entitlements(app) == CAPTURE_ENTITLEMENTS",
            "verify_note_runtime_resources_present(resources)",
            "REQUIRED_NOTE_RUNTIME_RESOURCES",
            'Path("note-bridge.py")',
            'Path("note-generator-mlx.py")',
            '"note-runtime-generate.json"',
            '"note-runtime-project.json"',
            '"note-validator.zip"',
        ):
            self.assertIn(required, verifier)

    def test_note_generate_manifest_pins_the_mlx_generator_and_is_written(self) -> None:
        """The `generate` sibling builds, pins the real generator, and is written.

        The builder must produce exactly the bytes Rust re-derives in
        `canonical_manifest`, and `main()`'s note-runtime lane writes it
        pinned to the `NOTE_MODELS` catalog entry — the signed catalog now
        carries the note-model role, so the manifest names a real model set.
        """
        builder = source("worker/build_manifest.py")
        self.assertIn('NOTE_GENERATE_MANIFEST = Path("note-runtime-generate.json")', builder)
        self.assertIn('NOTE_GENERATOR = Path("note-generator-mlx.py")', builder)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixtures = {
                "python-runtime/bin/python3.12": b"runtime",
                "worker/main.py": b"worker",
                "bin/audiotee": b"tap",
                "bin/permission-probe": b"probe",
                "encoder-unavailable.identity": b"encoder",
                "note-bridge.py": source("worker/note_bridge.py").encode(),
                "note-generator-mlx.py": source("worker/note_generator_mlx.py").encode(),
            }
            for relative, contents in fixtures.items():
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(contents)
            completed = subprocess.run(
                [str(ROOT / "worker/build_manifest.py"), str(root)],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            module = load_build_manifest()
            self.assertEqual(
                (root / "note-runtime-generate.json").read_bytes(),
                module.canonical_note_manifest(
                    module.note_generate_manifest(root, module.note_model_pins())
                ),
            )
            models = [
                {"id": "note-generator-weights", "sha256": "b" * 64},
                {"id": "note-generator-config", "sha256": "a" * 64},
            ]
            document = module.note_generate_manifest(root, models)
            self.assertEqual(list(document), [
                "schema", "role", "runtime", "bridge", "validator", "generator", "models"
            ])
            self.assertEqual(document["schema"], "note-runtime/1")
            self.assertEqual(document["role"], "generate")
            self.assertEqual(document["generator"], {
                "relative_path": "note-generator-mlx.py",
                "sha256": hashlib.sha256(
                    source("worker/note_generator_mlx.py").encode()
                ).hexdigest(),
            })
            # Sorted by id, because Rust sorts the pinned set before comparing
            # it to a catalog entry's derived one.
            self.assertEqual([model["id"] for model in document["models"]], [
                "note-generator-config", "note-generator-weights"
            ])
            raw = module.canonical_note_manifest(document)
            self.assertEqual(raw, json.dumps(document, ensure_ascii=False, indent=2).encode())
            self.assertNotIn(b"\\", raw)
            # An empty model set is refused here, not deferred to Rust.
            with self.assertRaises(SystemExit):
                module.note_generate_manifest(root, [])

    def test_note_model_pins_mirror_the_rust_identifier_derivation(self) -> None:
        """The pins must equal what Rust's `note_runtime_models` derives.

        The expected list is the same one
        `note_runtime_model_ids_name_every_file_of_a_sharded_model_distinctly`
        pins in `crates/session-core/src/note_projector_process.rs`; the two
        tests drifting apart is the failure this pair exists to catch.
        """
        module = load_build_manifest()
        self.assertEqual(
            [pin["id"] for pin in module.note_model_pins()],
            [
                "note-generator-config",
                "note-generator-tokenizer",
                "note-generator-tokenizer-config",
                "note-generator-weights-00001-of-00002",
                "note-generator-weights-00002-of-00002",
                "note-generator-weights-index",
            ],
        )
        self.assertEqual(
            module.note_runtime_model_id("weights", "model.safetensors"),
            "note-generator-weights",
        )
        catalog = module.model_catalog()
        self.assertEqual(list(catalog), ["schema", "models", "note_models"])
        entry = catalog["note_models"][0]
        self.assertEqual(entry["downloadBytes"], entry["installedBytes"])
        self.assertEqual(
            entry["downloadBytes"], sum(file["bytes"] for file in entry["files"])
        )
        for file in entry["files"]:
            self.assertTrue(file["url"].startswith("https://"))
            self.assertIn(entry["revision"], file["url"])
            self.assertTrue(file["url"].endswith(file["name"]))

    def test_note_project_runtime_manifest_and_validator_zip_are_generated_canonically(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixtures = {
                "python-runtime/bin/python3.12": b"runtime",
                "worker/main.py": b"worker",
                "bin/audiotee": b"tap",
                "bin/permission-probe": b"probe",
                "encoder-unavailable.identity": b"encoder",
                "note-bridge.py": source("worker/note_bridge.py").encode(),
                "note-generator-mlx.py": source("worker/note_generator_mlx.py").encode(),
            }
            for relative, contents in fixtures.items():
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(contents)
            completed = subprocess.run(
                [str(ROOT / "worker/build_manifest.py"), str(root)],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            raw = (root / "note-runtime-project.json").read_bytes()
            document = json.loads(raw)
            self.assertEqual(
                raw,
                json.dumps(document, ensure_ascii=False, indent=2).encode(),
            )
            self.assertEqual(list(document), [
                "schema", "role", "runtime", "bridge", "validator", "generator", "models"
            ])
            self.assertEqual(document["schema"], "note-runtime/1")
            self.assertEqual(document["role"], "project")
            self.assertIsNone(document["generator"])
            self.assertEqual(document["models"], [])
            self.assertEqual(document["bridge"]["relative_path"], "note-bridge.py")
            self.assertEqual(document["validator"]["relative_path"], "note-validator.zip")
            with zipfile.ZipFile(root / "note-validator.zip") as archive:
                # Order-sensitive on purpose: `verify_note_runtime` compares the
                # namelist to `VALIDATOR_SOURCES` positionally, so a module may
                # be appended but never reordered.
                self.assertEqual(
                    archive.namelist(),
                    [
                        "note_validator.py",
                        "summarize.py",
                        "transcript.py",
                        "capture_health.py",
                        "candidate_first.py",
                    ],
                )

    def test_bundle_contract_ships_the_complete_note_runtime(self) -> None:
        # Flipped from an exclusion contract when the signed catalog gained
        # the note-model role: every bundle now ships the five note runtime
        # resources, and the preview preparer refuses a bundle missing any.
        required = {
            "../runtime/note-bridge.py",
            "../runtime/note-generator-mlx.py",
            "../runtime/note-runtime-generate.json",
            "../runtime/note-runtime-project.json",
            "../runtime/note-validator.zip",
        }
        for config_path in (
            "apps/desktop/src-tauri/tauri.conf.json",
            "apps/desktop/src-tauri/tauri.preview.conf.json",
        ):
            resources = json.loads(source(config_path))["bundle"]["resources"]
            self.assertTrue(required.issubset(resources), config_path)

        preview_preparer = source("scripts/prepare-preview-bundle.sh")
        self.assertIn("require_note_runtime_complete", preview_preparer)
        self.assertIn('[[ -f "$path" && ! -L "$path" ]]', preview_preparer)
        for name in required:
            self.assertIn(name.removeprefix("../runtime/"), preview_preparer)

    def test_bundle_manifest_rebuild_excludes_and_refuses_note_runtime_resources(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixtures = {
                "python-runtime/bin/python3.12": b"runtime",
                "worker/main.py": b"worker",
                "bin/meeting-capture": b"tap",
                "bin/permission-probe": b"probe",
                "encoder-unavailable.identity": b"encoder",
                "models/whisper-large-v3-turbo/config.json": b"config",
                "models/whisper-large-v3-turbo/weights.safetensors": b"weights",
                "models/all-MiniLM-L6-v2/config.json": b"embedder-config",
                "models/all-MiniLM-L6-v2/sentence_bert_config.json": b"sentence-config",
                "models/all-MiniLM-L6-v2/tokenizer.json": b"tokenizer",
                "models/all-MiniLM-L6-v2/model.safetensors": b"embedder-weights",
            }
            for relative, contents in fixtures.items():
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(contents)
            completed = subprocess.run(
                [
                    str(ROOT / "worker/build_manifest.py"),
                    str(root),
                    "--admission",
                    "internal-alpha",
                    "--exclude-note-runtime",
                ],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertTrue((root / "app-runtime.json").is_file())
            for name in (
                "note-bridge.py",
                "note-runtime-project.json",
                "note-validator.zip",
            ):
                self.assertFalse((root / name).exists(), name)

            forbidden = root / "note-bridge.py"
            forbidden.write_bytes(b"unadmitted")
            blocked = subprocess.run(
                [
                    str(ROOT / "worker/build_manifest.py"),
                    str(root),
                    "--admission",
                    "internal-alpha",
                    "--exclude-note-runtime",
                ],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertNotEqual(blocked.returncode, 0)
            self.assertIn("test-only note runtime resource is present", blocked.stderr)

    def test_external_model_manifests_bind_optional_native_helper_without_whisper(self) -> None:
        for native in (False, True):
            with self.subTest(native=native), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                fixtures = {
                    "python-runtime/bin/python3.12": b"runtime",
                    "worker/main.py": b"worker",
                    "bin/meeting-capture": b"tap",
                    "bin/permission-probe": b"probe",
                    "encoder-unavailable.identity": b"encoder",
                    "models/all-MiniLM-L6-v2/config.json": b"embedder-config",
                    "models/all-MiniLM-L6-v2/sentence_bert_config.json": b"sentence-config",
                    "models/all-MiniLM-L6-v2/tokenizer.json": b"tokenizer",
                    "models/all-MiniLM-L6-v2/model.safetensors": b"embedder-weights",
                }
                if native:
                    fixtures["bin/apple-speech"] = b"native-helper"
                for relative, contents in fixtures.items():
                    target = root / relative
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(contents)
                completed = subprocess.run(
                    [
                        str(ROOT / "worker/build_manifest.py"),
                        str(root),
                        "--admission",
                        "internal-alpha",
                        "--exclude-note-runtime",
                        "--external-transcript-models",
                        *(["--apple-speech"] if native else []),
                    ],
                    stdin=subprocess.DEVNULL,
                    timeout=30,
                    text=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    check=False,
                )
                self.assertEqual(completed.returncode, 0, completed.stderr)
                manifest = json.loads((root / "app-runtime.json").read_text())
                self.assertEqual(manifest["schema"], "app-runtime/3" if native else "app-runtime/2")
                if native:
                    self.assertEqual(manifest["apple_speech"], {
                        "path": "bin/apple-speech",
                        "sha256": hashlib.sha256(b"native-helper").hexdigest(),
                    })
                else:
                    self.assertNotIn("apple_speech", manifest)
                self.assertEqual(manifest["model_catalog"]["path"], "model-catalog.json")
                self.assertFalse(
                    any(model["id"].startswith("whisper-") for model in manifest["models"])
                )

    def test_release_verifier_and_builder_agree_on_the_exact_model_catalog(self) -> None:
        from worker.build_manifest import model_catalog

        verifier = load_release_verifier()
        with tempfile.TemporaryDirectory() as temporary:
            resources = Path(temporary)
            raw = (json.dumps(model_catalog(), indent=2) + "\n").encode()
            (resources / "model-catalog.json").write_bytes(raw)
            manifest = {
                "model_catalog": {
                    "path": "model-catalog.json",
                    "sha256": hashlib.sha256(raw).hexdigest(),
                }
            }
            verifier.verify_model_catalog(resources, manifest)

    def test_release_verifier_requires_the_complete_note_runtime(self) -> None:
        verifier = load_release_verifier()
        with tempfile.TemporaryDirectory() as temporary:
            resources = Path(temporary)
            for relative in verifier.REQUIRED_NOTE_RUNTIME_RESOURCES:
                (resources / relative).write_bytes(b"resource")
            verifier.verify_note_runtime_resources_present(resources)
            for relative in verifier.REQUIRED_NOTE_RUNTIME_RESOURCES:
                path = resources / relative
                path.unlink()
                with self.assertRaises(verifier.VerificationError):
                    verifier.verify_note_runtime_resources_present(resources)
                path.write_bytes(b"resource")

            # A symlink where a resource should be is unsafe, not present.
            linked = resources / verifier.REQUIRED_NOTE_RUNTIME_RESOURCES[0]
            linked.unlink()
            linked.symlink_to("elsewhere")
            with self.assertRaises(verifier.VerificationError):
                verifier.verify_note_runtime_resources_present(resources)

    def test_release_identifiers_have_the_exact_developer_id_requirement_relationship(self) -> None:
        verifier = load_release_verifier()
        outer = verifier.expected_designated_requirement(verifier.EXPECTED_IDENTIFIER)
        nested = verifier.expected_designated_requirement(
            verifier.EXPECTED_PYTHON_IDENTIFIER
        )
        self.assertEqual(
            verifier.EXPECTED_PYTHON_IDENTIFIER,
            f"{verifier.EXPECTED_IDENTIFIER}.python-runtime",
        )
        self.assertEqual(
            nested,
            outer.replace(verifier.EXPECTED_IDENTIFIER, verifier.EXPECTED_PYTHON_IDENTIFIER),
        )
        self.assertEqual(
            verifier.normalized_requirement(nested.replace(" exists", " /* exists */")),
            nested,
        )

    def test_encoder_candidate_lane_is_pinned_and_refuses_the_wrong_artifact(self) -> None:
        verifier = load_release_verifier()
        build_script = source("worker/build_runtime.sh")
        self.assertIn("build-alpha-encoder", build_script)
        self.assertIn("requirements-encoder.lock", build_script)
        digest_lines = [
            line
            for line in build_script.splitlines()
            if line.startswith("ENCODER_ONNX_SHA256=")
        ]
        # The build script and the closed verifier pin the same deterministic
        # export; a candidate that drifts from either is refused, not renamed.
        self.assertEqual(
            digest_lines,
            [f"ENCODER_ONNX_SHA256='{verifier.EXPECTED_ENCODER_ONNX_SHA256}'"],
        )
        signing = source("scripts/sign-notarize.sh")
        self.assertIn('--encoder "$ENCODER_PATH"', signing)
        # The classification-agreement harness measures the same registered
        # export; a drifted pin would let it silently measure another artifact.
        agreement_harness = source("spike/encoder-packaging/bench_gate_agreement.py")
        self.assertIn(
            f'EXPECTED_ONNX_SHA256 = "{verifier.EXPECTED_ENCODER_ONNX_SHA256}"',
            agreement_harness,
        )

        with tempfile.TemporaryDirectory() as temporary:
            resources = Path(temporary)
            (resources / "models/speaker-encoder").mkdir(parents=True)
            artifact = resources / "models/speaker-encoder/ecapa-tdnn.onnx"
            artifact.write_bytes(b"not the pinned export")
            python = resources / "python3.12"  # the refusal paths never reach it

            verifier.verify_encoder_candidate(
                resources,
                {"encoder": {"path": "encoder-unavailable.identity"}},
                python,
            )
            with self.assertRaises(verifier.VerificationError):
                verifier.verify_encoder_candidate(
                    resources,
                    {"encoder": {"path": "models/speaker-encoder/ecapa-tdnn.onnx"}},
                    python,
                )
            with self.assertRaises(verifier.VerificationError):
                verifier.verify_encoder_candidate(
                    resources, {"encoder": {"path": "worker/main.py"}}, python
                )

    def test_sitting_derivation_mirrors_the_store_and_respects_the_frozen_admission(
        self,
    ) -> None:
        # The Python producer and the Rust sitting evidence store are two
        # authorities over the same artifacts; these mirrors are how they stay
        # one contract. A drifted name or bound would let the worker write what
        # the store refuses — or worse, what it silently accepts unbounded.
        store = source("crates/session-core/src/sitting_evidence.rs")
        producer = source("worker/sitting_derivation.py")
        for rust_line, python_line in [
            ('const RAW_AUDIO_NAME: &str = "audio.raw";', 'RAW_AUDIO_NAME = "audio.raw"'),
            ('const SEGMENTS_FILE: &str = "segments.json";', 'SEGMENTS_NAME = "segments.json"'),
            ('const EMBEDDINGS_FILE: &str = "embeddings.bin";', 'EMBEDDINGS_NAME = "embeddings.bin"'),
            ("const SEGMENTS_MAX_BYTES: u64 = 1_048_576;", "SEGMENTS_MAX_BYTES = 1_048_576"),
            ("const EMBEDDINGS_MAX_BYTES: u64 = 16_777_216;", "EMBEDDINGS_MAX_BYTES = 16_777_216"),
            ("const RAW_MAX_BYTES: u64 = 1_073_741_824;", "RAW_MAX_BYTES = 1_073_741_824"),
        ]:
            self.assertIn(rust_line, store)
            self.assertIn(python_line, producer)
        guidance = source("crates/session-core/src/enrollment_guidance.rs")
        self.assertIn("MIN_SCORABLE_SECONDS: f64 = 2.0", guidance)
        self.assertIn("MIN_SCORABLE_SECONDS = 2.0", producer)

        # The packaged internal-alpha operation set is frozen on both sides of
        # the process boundary. Pinned as an AGREEMENT rather than as a literal
        # list. The earlier literal named three operations and asserted that
        # sitting.derive stayed boundary-lane; the operator registered it on
        # 2026-08-04 and the profile family on 2026-08-05, so the pin went red
        # for being out of date rather than for catching the drift it exists to
        # catch, and a red-for-staleness pin is how real drift gets ignored.
        worker_main = source("worker/main.py")
        supervision = source("crates/session-core/src/supervision.rs")
        python_alpha = set(
            re.findall(
                r'"([a-z]+\.[a-z]+)"',
                worker_main.split("ALPHA_OPERATIONS = frozenset(", 1)[1]
                .split("{", 1)[1]
                .split("}", 1)[0],
            )
        )
        rust_alpha = set(
            re.findall(
                r"(?m)^\s*([A-Z][A-Za-z]+),\s*$",
                supervision.split("pub fn internal_alpha_operations", 1)[1]
                .split("[", 1)[1]
                .split("]", 1)[0],
            )
        )
        flat = lambda name: name.replace(".", "").lower()
        self.assertTrue(python_alpha)
        self.assertEqual(
            {flat(name) for name in python_alpha},
            {flat(name) for name in rust_alpha},
        )
        # Profile adoption stays outside internal alpha because it changes the
        # active profile rather than producing inspectable candidate evidence.
        self.assertNotIn("profile.adopt", python_alpha)

    def test_manifest_encoder_argument_binds_the_named_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixtures = {
                "python-runtime/bin/python3.12": b"runtime",
                "worker/main.py": b"worker",
                "bin/meeting-capture": b"tap",
                "bin/permission-probe": b"probe",
                "encoder-unavailable.identity": b"encoder",
                "models/whisper-large-v3-turbo/config.json": b"config",
                "models/whisper-large-v3-turbo/weights.safetensors": b"weights",
                "models/all-MiniLM-L6-v2/config.json": b"embedder-config",
                "models/all-MiniLM-L6-v2/sentence_bert_config.json": b"sentence-config",
                "models/all-MiniLM-L6-v2/tokenizer.json": b"tokenizer",
                "models/all-MiniLM-L6-v2/model.safetensors": b"embedder-weights",
                "models/speaker-encoder/ecapa-tdnn.onnx": b"candidate",
            }
            for relative, contents in fixtures.items():
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(contents)
            arguments = [
                str(ROOT / "worker/build_manifest.py"),
                str(root),
                "--admission",
                "internal-alpha",
                "--exclude-note-runtime",
            ]
            completed = subprocess.run(
                [*arguments, "--encoder", "models/speaker-encoder/ecapa-tdnn.onnx"],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            manifest = json.loads((root / "app-runtime.json").read_text())
            self.assertEqual(
                manifest["encoder"]["path"], "models/speaker-encoder/ecapa-tdnn.onnx"
            )
            self.assertEqual(
                manifest["encoder"]["sha256"], hashlib.sha256(b"candidate").hexdigest()
            )
            escaped = subprocess.run(
                [*arguments, "--encoder", "../outside-the-root"],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertNotEqual(escaped.returncode, 0)
            self.assertIn("escapes the runtime root", escaped.stderr)

    def test_frozen_alpha_verification_selects_alpha_admission(self) -> None:
        runbook = source("docs/distribution-runbook.md")
        frozen = runbook.split("## Recheck a frozen artifact", 1)[1]
        self.assertIn(
            '"target/release/bundle/macos/Yawn-<version>-macos-arm64.dmg" \\\n  internal-alpha',
            frozen,
        )


if __name__ == "__main__":
    unittest.main()
