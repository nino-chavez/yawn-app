from __future__ import annotations

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


class MeetingCaptureHandshakeTests(unittest.TestCase):
    def sidecar_binary(self) -> str:
        sidecar = os.environ.get("LMN_MEETING_CAPTURE_TEST_BINARY")
        if not sidecar:
            self.skipTest("LMN_MEETING_CAPTURE_TEST_BINARY is not set")
        return sidecar

    def test_permission_status_runs_without_capture_descriptors(self) -> None:
        sidecar = self.sidecar_binary()
        result = subprocess.run(
            [sidecar, "--permission-preflight", "status"],
            capture_output=True,
            check=False,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        document = json.loads(result.stdout)
        self.assertEqual(
            {key: value for key, value in document.items() if key != "microphone"},
            {
                "action": "status",
                "schema": "permission-probe/1",
                "system_audio": "unmeasured",
                "system_audio_detail": "run request-system-audio; setup uses the real capture helper",
            },
        )
        self.assertIn(
            document["microphone"],
            {"authorized", "denied", "restricted", "not-determined", "unknown"},
        )

    def test_paused_and_parent_eof_do_not_open_capture_hardware_or_files(self) -> None:
        sidecar = self.sidecar_binary()
        with tempfile.TemporaryDirectory(prefix="meeting-capture-handshake-") as temporary:
            capture_dir = Path(temporary)
            capture_dir.chmod(0o700)
            directory_fd = os.open(capture_dir, os.O_RDONLY)
            control_read, control_write = os.pipe()
            event_read, event_write = os.pipe()
            liveness_read, liveness_write = os.pipe()
            process = subprocess.Popen(
                [
                    sidecar,
                    "--capture-dir-fd",
                    str(directory_fd),
                    "--control-fd",
                    str(control_read),
                    "--event-fd",
                    str(event_write),
                    "--parent-liveness-fd",
                    str(liveness_read),
                ],
                pass_fds=(directory_fd, control_read, event_write, liveness_read),
                stdout=subprocess.DEVNULL,
                stderr=subprocess.PIPE,
            )
            os.close(control_read)
            os.close(event_write)
            os.close(liveness_read)
            try:
                with os.fdopen(event_read, "rb", closefd=True) as events:
                    self.assertEqual(
                        json.loads(events.readline()),
                        {"schema": "capture-event/1", "event": "paused"},
                    )
                    self.assertEqual(list(capture_dir.iterdir()), [])
                    os.close(liveness_write)
                    liveness_write = -1
                    process.wait(timeout=30)
                    self.assertEqual(process.returncode, 1, process.stderr.read().decode())
                    self.assertEqual(
                        [json.loads(line) for line in events],
                        [{"schema": "capture-event/1", "event": "interrupted"}],
                    )
                    self.assertEqual(list(capture_dir.iterdir()), [])
            finally:
                if liveness_write >= 0:
                    os.close(liveness_write)
                os.close(control_write)
                os.close(directory_fd)
                if process.poll() is None:
                    process.kill()
                    process.wait()
                process.stderr.close()

    def test_unknown_control_fails_once_without_opening_hardware_or_files(self) -> None:
        sidecar = self.sidecar_binary()
        with tempfile.TemporaryDirectory(prefix="meeting-capture-control-") as temporary:
            capture_dir = Path(temporary)
            capture_dir.chmod(0o700)
            directory_fd = os.open(capture_dir, os.O_RDONLY)
            control_read, control_write = os.pipe()
            event_read, event_write = os.pipe()
            liveness_read, liveness_write = os.pipe()
            process = subprocess.Popen(
                [
                    sidecar,
                    "--capture-dir-fd",
                    str(directory_fd),
                    "--control-fd",
                    str(control_read),
                    "--event-fd",
                    str(event_write),
                    "--parent-liveness-fd",
                    str(liveness_read),
                ],
                pass_fds=(directory_fd, control_read, event_write, liveness_read),
                stdout=subprocess.DEVNULL,
                stderr=subprocess.PIPE,
            )
            os.close(control_read)
            os.close(event_write)
            os.close(liveness_read)
            try:
                with os.fdopen(event_read, "rb", closefd=True) as events:
                    self.assertEqual(json.loads(events.readline())["event"], "paused")
                    os.write(control_write, b"Q")
                    process.wait(timeout=30)
                    self.assertEqual(process.returncode, 2, process.stderr.read().decode())
                    self.assertEqual(
                        [json.loads(line) for line in events],
                        [
                            {
                                "schema": "capture-event/1",
                                "event": "failed",
                                "code": "invalid_control",
                                "detail": "unknown capture control byte",
                            }
                        ],
                    )
                    self.assertEqual(list(capture_dir.iterdir()), [])
            finally:
                os.close(liveness_write)
                os.close(control_write)
                os.close(directory_fd)
                if process.poll() is None:
                    process.kill()
                    process.wait()
                process.stderr.close()
