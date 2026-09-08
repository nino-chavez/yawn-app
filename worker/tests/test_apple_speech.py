from __future__ import annotations

import json
import os
import tempfile
import unittest
import wave
from pathlib import Path

from worker.apple_speech import (
    AppleSpeechRefused,
    make_producer,
    require_matching_provenance,
    transcribe_leg,
    write_provenance,
)


class AppleSpeechBridgeTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        os.chmod(self.root, 0o700)
        self.audio = self.root / "mic.wav"
        with wave.open(str(self.audio), "wb") as output:
            output.setnchannels(1); output.setsampwidth(2); output.setframerate(16_000)
            output.writeframes(b"\0\0" * 16_000)
        self.helper = self.root / "helper.py"
        self.helper.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, sys\n"
            "out=sys.argv[sys.argv.index('--out')+1]\n"
            "d={'schema':'speech-engine-result/1','status':'ok','engine':'apple','runs':[{'text':'hello','segments':[{'text':'hello','start':0.0,'end':0.5}],'utterances':[{'text':'hello','start':0.0,'end':0.5}],'timing_issue_count':0}]}\n"
            "open(out,'w').write(json.dumps(d)); os.chmod(out,0o600)\n",
            encoding="utf-8",
        )
        self.helper.chmod(0o700)
        self.producer = make_producer(helper=self.helper, helper_sha256=__import__("hashlib").sha256(self.helper.read_bytes()).hexdigest(), locale="en-US", os_version="26.0")

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def test_valid_result_is_private_validated_and_source_bound(self) -> None:
        segments, receipt = transcribe_leg(self.producer, self.audio, temporary_parent=self.root)
        self.assertEqual(segments, [{"start": 0.0, "end": 0.5, "text": "hello"}])
        self.assertEqual(receipt["audio_sha256"], __import__("hashlib").sha256(self.audio.read_bytes()).hexdigest())
        provenance = write_provenance(self.root, "b" * 64, self.producer, mic=receipt, system=receipt)
        self.assertEqual(len(provenance), 64)
        require_matching_provenance(self.root, "b" * 64, self.producer)

    def test_retry_refuses_changed_producer(self) -> None:
        receipt = {"audio_sha256": "a" * 64, "result_sha256": "b" * 64}
        write_provenance(self.root, "b" * 64, self.producer, mic=receipt, system=receipt)
        changed = make_producer(helper=self.helper, helper_sha256="c" * 64, locale="en-US", os_version="26.0")
        with self.assertRaises(AppleSpeechRefused):
            require_matching_provenance(self.root, "b" * 64, changed)

    def test_rejects_unsafe_or_missing_helper(self) -> None:
        with self.assertRaises(AppleSpeechRefused):
            make_producer(helper=None, helper_sha256=__import__("hashlib").sha256(self.helper.read_bytes()).hexdigest(), locale="en-US", os_version="26.0")


class NativeWorkerStartupTests(unittest.TestCase):
    def test_native_worker_starts_without_a_whisper_receipt_and_exits_with_parent(self):
        import hashlib
        import select
        import subprocess
        import sys
        from worker.main import load_manifest

        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary).resolve()
            root = base / "app"
            root.mkdir(mode=0o700)
            resources = base / "resources"
            resources.mkdir(mode=0o700)
            helper = resources / "apple-speech"
            helper.write_text(
                "#!/usr/bin/env python3\nimport json,os,sys\n"
                "out=sys.argv[sys.argv.index('--out')+1]\n"
                "d={'schema':'apple-speech-capability/1','state':'ready','locale':'en-US','os_version':'26.0 fixture','asset_identity':'os-managed'}\n"
                "open(out,'w').write(json.dumps(d));os.chmod(out,0o600)\n"
            )
            helper.chmod(0o700)
            manifest = {"schema": "app-runtime/3", "admission": "internal-alpha", "models": []}
            for field in ("runtime", "worker", "tap", "encoder", "permission_probe", "model_catalog", "apple_speech"):
                path = helper if field == "apple_speech" else resources / field
                if field != "apple_speech":
                    path.write_text("{}" if field == "model_catalog" else field)
                manifest[field] = {"path": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
            manifest_path = resources / "app-runtime.json"
            manifest_path.write_text(json.dumps(manifest))
            self.assertEqual(load_manifest(manifest_path)["schema"], "app-runtime/3")
            reader, writer = os.pipe()
            child = subprocess.Popen(
                [sys.executable, "-m", "worker.main", "--app-data-root", str(root),
                 "--runtime-manifest", str(manifest_path), "--transcription-engine", "apple-native",
                 "--parent-liveness-fd", str(reader)],
                cwd=Path(__file__).resolve().parents[2], stdin=subprocess.PIPE,
                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, pass_fds=(reader,), text=True,
            )
            os.close(reader)
            try:
                self.assertTrue(select.select([child.stdout], [], [], 10)[0], "native worker did not answer")
                ready = json.loads(child.stdout.readline())
                self.assertEqual(ready["event"], "worker.ready")
                self.assertEqual(ready["models"], [])
                os.close(writer)
                writer = None
                child.wait(timeout=5)
            finally:
                if writer is not None:
                    os.close(writer)
                if child.poll() is None:
                    child.kill()
                    child.wait(timeout=5)
                child.stdin.close()
                child.stdout.close()
            helper.write_text("changed")
            with self.assertRaises(ValueError):
                load_manifest(manifest_path)
