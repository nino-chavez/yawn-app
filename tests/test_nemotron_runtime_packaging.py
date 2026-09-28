from __future__ import annotations

import hashlib
import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]


def load(name: str, relative: str):
    path = ROOT / relative
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class NemotronRuntimePackagingTests(unittest.TestCase):
    def setUp(self):
        self.manifest = load("yawn_build_manifest_for_nemotron", "worker/build_manifest.py")
        self.packager = load("yawn_nemotron_packager_for_tests", "scripts/package_nemotron_diarizer.py")

    def write_native_runtime(self, root: Path) -> Path:
        stage = root / "nemotron-diarization"
        (stage / "bin").mkdir(parents=True)
        (stage / "lib").mkdir()
        files = {
            "bin/nemo-speech": b"native-cli",
            "lib/libnemo_speech_asr.dylib": b"native-library",
        }
        for relative, contents in files.items():
            (stage / relative).write_bytes(contents)
        document = {
            "schema": self.packager.SCHEMA,
            "source": {"commit": self.packager.SOURCE_COMMIT, "tree": "a" * 40, "patch_sha256": self.packager.PATCH_SHA256},
            "platform": {"os": "macos", "arch": "arm64", "backend": "metal"},
            "command": {
                "path": "bin/nemo-speech",
                "sha256": hashlib.sha256(files["bin/nemo-speech"]).hexdigest(),
                "argv": list(self.packager.COMMAND),
            },
            "dylibs": [{
                "path": "lib/libnemo_speech_asr.dylib",
                "sha256": hashlib.sha256(files["lib/libnemo_speech_asr.dylib"]).hexdigest(),
            }],
            "assets": [],
            "licenses": [],
        }
        license_path = stage / "licenses/LICENSE"
        license_path.parent.mkdir()
        license_path.write_bytes(b"Apache-2.0")
        document["licenses"] = [{
            "path": "licenses/LICENSE",
            "sha256": hashlib.sha256(license_path.read_bytes()).hexdigest(),
        }]
        (stage / "runtime.json").write_text(json.dumps(document), encoding="utf-8")
        return stage

    def test_ordinary_runtime_records_native_runtime_as_explicitly_unavailable(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "nemotron-diarization").mkdir()
            self.manifest.ensure_nemotron_runtime(root)
            actual = json.loads((root / "nemotron-diarization/runtime.json").read_text())
        self.assertEqual(actual, self.manifest.NEMOTRON_ABSENT)

    def test_unavailable_receipt_cannot_hide_native_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            stage = root / "nemotron-diarization"
            stage.mkdir()
            (stage / "nemo-speech").write_bytes(b"unexpected")
            with self.assertRaisesRegex(SystemExit, "without a receipt"):
                self.manifest.ensure_nemotron_runtime(root)
            (stage / "runtime.json").write_text(json.dumps(self.manifest.NEMOTRON_ABSENT))
            with self.assertRaisesRegex(SystemExit, "unexpected files"):
                self.manifest.ensure_nemotron_runtime(root)

    def test_native_receipt_pins_source_command_and_each_resource(self):
        with tempfile.TemporaryDirectory() as directory:
            stage = self.write_native_runtime(Path(directory))
            self.manifest.ensure_nemotron_runtime(stage.parent)
            document = self.packager.verify_staged_runtime(stage)
        self.assertEqual(document["command"]["argv"], list(self.packager.COMMAND))
        self.assertEqual(document["source"]["commit"], self.packager.SOURCE_COMMIT)
        self.assertEqual(document["source"]["patch_sha256"], self.packager.PATCH_SHA256)

    def test_changed_native_resource_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            stage = self.write_native_runtime(Path(directory))
            (stage / "lib/libnemo_speech_asr.dylib").write_bytes(b"changed")
            with self.assertRaises(SystemExit):
                self.manifest.ensure_nemotron_runtime(stage.parent)

    def test_missing_private_dylib_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            stage = self.write_native_runtime(Path(directory))
            (stage / "lib/libnemo_speech_asr.dylib").unlink()
            with self.assertRaisesRegex(SystemExit, "missing or unsafe"):
                self.manifest.ensure_nemotron_runtime(stage.parent)

    def test_post_sign_refresh_refuses_unsigned_native_files(self):
        with tempfile.TemporaryDirectory() as directory:
            stage = self.write_native_runtime(Path(directory))
            unsigned = subprocess.CompletedProcess(("codesign",), 1, "", "unsigned")
            with mock.patch.object(self.packager, "run", return_value=unsigned):
                with self.assertRaisesRegex(self.packager.AdmissionError, "not signed"):
                    self.packager.refresh_staged_runtime(stage)

    def test_absolute_non_system_dylib_is_an_explicit_hold(self):
        result = subprocess.CompletedProcess(
            ("otool", "-L", "fixture"),
            0,
            "fixture:\n\t/opt/homebrew/lib/libsentencepiece.0.dylib (compatibility version 0.0.0)\n",
            "",
        )
        with mock.patch.object(self.packager, "run", return_value=result):
            with self.assertRaisesRegex(self.packager.AdmissionError, "pinned static"):
                self.packager.linked_dylibs(Path("fixture"))

    def test_missing_license_accounting_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            stage = self.write_native_runtime(Path(directory))
            document = json.loads((stage / "runtime.json").read_text())
            document["licenses"] = []
            (stage / "runtime.json").write_text(json.dumps(document))
            with self.assertRaisesRegex(self.packager.AdmissionError, "license accounting"):
                self.packager.verify_staged_runtime(stage)

    def test_dedicated_build_mode_and_resource_mapping_are_declared(self):
        runtime_build = (ROOT / "worker/build_runtime.sh").read_text(encoding="utf-8")
        self.assertIn("build-alpha-diarization", runtime_build)
        self.assertIn("YAWN_NEMO_SOURCE_DIR", runtime_build)
        self.assertIn("YAWN_NEMO_DEPENDENCY_PREFIX", runtime_build)
        self.assertIn("package_nemotron_diarizer.py", runtime_build)
        config = json.loads((ROOT / "apps/desktop/src-tauri/tauri.conf.json").read_text())
        self.assertEqual(
            config["bundle"]["resources"].get("../runtime/nemotron-diarization"),
            "nemotron-diarization",
        )


if __name__ == "__main__":
    unittest.main()
