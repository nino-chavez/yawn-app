from __future__ import annotations

import json
import os
import sys
import subprocess
import tempfile
import unittest
from pathlib import Path


class TapHandshakeTests(unittest.TestCase):
    def tap_binary(self) -> str:
        tap = os.environ.get("LMN_TAP_TEST_BINARY")
        if not tap:
            self.skipTest("LMN_TAP_TEST_BINARY is not set")
        return tap

    def test_paused_ack_precedes_hardware_and_parent_eof_exits(self) -> None:
        tap = self.tap_binary()
        control_read, control_write = os.pipe()
        ready_read, ready_write = os.pipe()
        liveness_read, liveness_write = os.pipe()
        process = subprocess.Popen(
            [
                tap,
                "--sample-rate",
                "16000",
                "--app-control-fd",
                str(control_read),
                "--app-ready-fd",
                str(ready_write),
                "--parent-liveness-fd",
                str(liveness_read),
            ],
            pass_fds=(control_read, ready_write, liveness_read),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        os.close(control_read)
        os.close(ready_write)
        os.close(liveness_read)
        try:
            with os.fdopen(ready_read, "rb", closefd=True) as ready:
                self.assertEqual(
                    json.loads(ready.readline()),
                    {"schema": "tap-ready/1", "state": "paused"},
                )
            os.close(liveness_write)
            process.wait(timeout=30)
            self.assertEqual(process.returncode, 0, process.stderr.read().decode())
        finally:
            os.close(control_write)
            if process.poll() is None:
                process.kill()
                process.wait()
            process.stderr.close()

    def test_liveness_only_parent_eof_exits_before_hardware(self) -> None:
        tap = self.tap_binary()
        liveness_read, liveness_write = os.pipe()
        os.close(liveness_write)
        process = subprocess.Popen(
            [tap, "--sample-rate", "16000", "--parent-liveness-fd", str(liveness_read)],
            pass_fds=(liveness_read,),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        os.close(liveness_read)
        try:
            process.wait(timeout=30)
            self.assertEqual(process.returncode, 0, process.stderr.read().decode())
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            process.stderr.close()

    def test_cli_tap_leg_passes_parent_liveness_fd(self) -> None:
        repo = Path(__file__).resolve().parents[2]
        sys.path.insert(0, str(repo / "spike"))
        import dual_capture

        with tempfile.TemporaryDirectory(prefix="tap-liveness-") as temporary:
            fake_tap = Path(temporary) / "fake-tap"
            fake_tap.write_text(
                "#!/usr/bin/env python3\n"
                "import os, sys\n"
                "fd = int(sys.argv[sys.argv.index('--parent-liveness-fd') + 1])\n"
                "while os.read(fd, 1):\n"
                "    pass\n",
                encoding="utf-8",
            )
            fake_tap.chmod(0o700)
            original = dual_capture.TAP_BIN
            dual_capture.TAP_BIN = fake_tap
            tap = dual_capture.TapLeg(Path(temporary) / "system.wav")
            try:
                tap.start()
                liveness = tap._liveness_write_fd
                self.assertIsNotNone(liveness)
                os.close(liveness)
                tap._liveness_write_fd = None
                tap.proc.wait(timeout=30)
                self.assertEqual(tap.proc.returncode, 0)
            finally:
                tap.stop()
                dual_capture.TAP_BIN = original
