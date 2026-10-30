"""A quit app must not leave the Apple speech helper running.

The desktop runs no exit handler when it quits: its process ends, the worker's
liveness pipe closes, and the worker's watchdog ends the worker.  The speech
helper is the worker's child, so before this fix it carried on, finished its
track and wrote raw result text into
``meetings/<id>/transcript/.apple-speech-*/result.json``.

The watchdog now stops the helpers the worker started.  It deliberately signals
no process group: the desktop also puts the meeting capture helper in the
worker's group, and that helper must be left to finalize a recording from its
own liveness pipe.  What a killed worker leaves behind is swept by the next run
(see test_stale_leg_directories.py).
"""

from __future__ import annotations

import os
import signal
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest import mock

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(REPO / "spike"))
sys.path.insert(0, str(REPO / "notes"))

import test_transcription_progress as progress_tests  # noqa: E402  (its fake helpers and capture fixture)

from worker import apple_speech  # noqa: E402
from worker import main as worker_main  # noqa: E402
from worker.apple_speech import (  # noqa: E402
    AppleSpeechRefused,
    kill_running_helpers,
    sweep_stale_leg_directories,
    transcribe_leg,
)

PYTHON = sys.executable

# Runs as the worker: it leads its own group when asked (the desktop's
# OwnedChild does setpgid(0, 0) before exec), starts the real watchdog, and then
# either runs a real leg against a fake helper or just waits.
CHILD = r"""
import hashlib, os, sys, time
from pathlib import Path
repo, fd, leader, mode, helper, audio, parent, pidfile = sys.argv[1:9]
sys.path.insert(0, repo)
if leader == "1":
    os.setpgid(0, 0)
from worker import main as wm
from worker.apple_speech import make_producer, transcribe_leg
wm.start_parent_liveness_watchdog(int(fd))
open(pidfile, "w").write(str(os.getpid()))
if mode == "wait":
    time.sleep(60)
else:
    producer = make_producer(
        helper=Path(helper), helper_sha256=hashlib.sha256(Path(helper).read_bytes()).hexdigest(),
        locale="en-US", os_version="test",
    )
    try:
        transcribe_leg(producer, Path(audio), temporary_parent=Path(parent))
    except Exception as exc:
        print("leg ended:", type(exc).__name__, flush=True)
"""

# A helper that lives well past the test.
SLOW_HELPER = (
    "#!%(python)s\n"
    "import os, sys, time\n"
    "out = sys.argv[sys.argv.index('--out') + 1]\n"
    "open(%(helper_pid)r, 'w').write(str(os.getpid()))\n"
    "os.write(1, b'1.000\\n')\n"
    "time.sleep(60)\n"
    "open(out, 'w').write('{}')\n"
)


def alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    return True


def wait_for(condition, seconds: float) -> bool:
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if condition():
            return True
        time.sleep(0.05)
    return condition()


class WatchdogProcessTests(unittest.TestCase):
    """Real processes: a worker-like child, its helper, and a pipe the test closes."""

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        os.chmod(self.root, 0o700)
        self.children: list[subprocess.Popen] = []
        self.pids: list[int] = []
        self.addCleanup(self.reap)

    def reap(self) -> None:
        for pid in self.pids:
            try:
                os.kill(pid, signal.SIGKILL)
            except OSError:
                pass
        for child in self.children:
            if child.poll() is None:
                child.kill()
            try:
                child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                pass
            if child.stdout is not None:
                child.stdout.close()

    def launch(self, *, leader: bool, mode: str, descriptor: int | None = None):
        """Start the worker-like child; returns (child, write end of its liveness pipe, pid files)."""
        read_fd, write_fd = os.pipe()
        self.addCleanup(lambda: os.close(write_fd) if self.is_open(write_fd) else None)
        helper = self.root / "helper.py"
        audio = self.root / "leg.wav"
        progress_tests.write_wav(audio, 1.0)
        pids = {"helper": self.root / "helper.pid", "worker": self.root / "worker.pid"}
        helper.write_text(SLOW_HELPER % {"python": PYTHON, "helper_pid": str(pids["helper"])})
        helper.chmod(0o700)
        child = subprocess.Popen(
            [PYTHON, "-c", CHILD, str(REPO), str(read_fd if descriptor is None else descriptor),
             "1" if leader else "0", mode, str(helper), str(audio), str(self.root), str(pids["worker"])],
            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            pass_fds=(read_fd,),
        )
        self.children.append(child)
        os.close(read_fd)
        self.assertTrue(wait_for(lambda: pids["worker"].exists(), 20), "the worker-like child did not start")
        return child, write_fd, pids

    @staticmethod
    def is_open(descriptor: int) -> bool:
        try:
            os.fstat(descriptor)
        except OSError:
            return False
        return True

    def read_pid(self, path: Path) -> int:
        pid = int(path.read_text())
        self.pids.append(pid)
        return pid

    def test_when_the_parent_goes_away_the_worker_and_its_helper_are_gone(self) -> None:
        child, write_fd, pids = self.launch(leader=True, mode="leg")
        self.assertTrue(wait_for(lambda: pids["helper"].exists(), 20), "the fake helper did not start")
        helper_pid = self.read_pid(pids["helper"])
        self.assertTrue(alive(helper_pid) and child.poll() is None)

        os.close(write_fd)  # the app quits: the liveness pipe closes

        child.wait(timeout=15)
        self.assertTrue(wait_for(lambda: not alive(helper_pid), 10), "the speech helper kept running after the worker went away")
        # Whatever a killed worker left behind, the next run sweeps.
        sweep_stale_leg_directories(self.root)
        self.assertEqual(list(self.root.glob(".apple-speech-*")), [])

    def test_a_process_the_desktop_put_in_the_workers_group_is_left_alone(self) -> None:
        # The meeting capture helper joins the worker's process group (setpgid) and
        # finalizes a recording when its own liveness pipe closes.  Quitting must
        # not kill it just because it shares the worker's group.
        child, write_fd, pids = self.launch(leader=True, mode="leg")
        self.assertTrue(wait_for(lambda: pids["helper"].exists(), 20), "the fake helper did not start")
        helper_pid = self.read_pid(pids["helper"])
        capture_stand_in = subprocess.Popen(
            ["sleep", "60"], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            process_group=child.pid,
        )
        self.pids.append(capture_stand_in.pid)
        self.addCleanup(lambda: (capture_stand_in.kill(), capture_stand_in.wait()))
        self.assertEqual(os.getpgid(capture_stand_in.pid), child.pid)    # it really is in the worker's group

        os.close(write_fd)

        child.wait(timeout=15)
        self.assertTrue(wait_for(lambda: not alive(helper_pid), 10), "the speech helper kept running after the worker went away")
        time.sleep(0.5)
        self.assertIsNone(capture_stand_in.poll(), "a process in the worker's group was killed with the worker")

    def test_every_exit_path_keeps_its_exit_code_whether_or_not_the_worker_leads_a_group(self) -> None:
        for leader in (True, False):
            for path, expected in (("eof", 0), ("bad-descriptor", 2)):
                with self.subTest(leader=leader, path=path):
                    # A number the child never opens: reading it fails with EBADF.
                    bad = 777 if path == "bad-descriptor" else None
                    child, write_fd, _ = self.launch(leader=leader, mode="wait", descriptor=bad)
                    if path == "eof":
                        os.close(write_fd)
                    self.assertEqual(child.wait(timeout=15), expected)


class ExitAfterStoppingHelpersTests(unittest.TestCase):
    def patched(self):
        stop = mock.Mock()
        exit_ = mock.Mock()
        killpg = mock.Mock(side_effect=AssertionError("a process group was signalled"))
        for patch in (
            mock.patch("worker.main.kill_running_helpers", stop),
            mock.patch("worker.main.os._exit", exit_),
            mock.patch("worker.main.os.killpg", killpg),
        ):
            patch.start()
            self.addCleanup(patch.stop)
        return stop, exit_, killpg

    def test_it_stops_the_helpers_then_exits_with_the_given_code(self) -> None:
        stop, exit_, killpg = self.patched()
        worker_main.exit_after_stopping_helpers(2)
        stop.assert_called_once_with()
        exit_.assert_called_once_with(2)
        killpg.assert_not_called()

    def test_it_exits_even_if_stopping_the_helpers_fails(self) -> None:
        stop, exit_, _ = self.patched()
        stop.side_effect = RuntimeError("registry broke")
        with self.assertRaises(RuntimeError):
            worker_main.exit_after_stopping_helpers(0)
        exit_.assert_called_once_with(0)

    def test_the_watchdog_uses_it_on_eof_and_on_a_read_error_with_the_existing_codes(self) -> None:
        for path, code in (("eof", 0), ("error", 2)):
            with self.subTest(path=path):
                called = threading.Event()
                with mock.patch("worker.main.exit_after_stopping_helpers", side_effect=lambda value: called.set()) as exit_helpers:
                    read_fd, write_fd = os.pipe()
                    if path == "eof":
                        os.close(write_fd)
                    else:
                        os.close(read_fd)
                        os.close(write_fd)
                    thread = worker_main.start_parent_liveness_watchdog(read_fd)
                    self.assertTrue(called.wait(10))
                    thread.join(timeout=10)
                    self.assertFalse(thread.is_alive())
                    exit_helpers.assert_called_once_with(code)
                    if path == "eof":
                        os.close(read_fd)


class RunningHelperRegistryTests(unittest.TestCase):
    """transcribe_leg and kill_running_helpers, in one process."""

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        os.chmod(self.root, 0o700)
        registry = apple_speech._HelperRegistry()
        patch = mock.patch("worker.apple_speech._HELPERS", registry)
        patch.start()
        self.addCleanup(patch.stop)
        self.registry = registry
        self.audio = self.root / "leg.wav"
        progress_tests.write_wav(self.audio, 1.0)
        self.helper_pid_file = self.root / "helper.pid"
        self.helper = self.root / "helper.py"
        self.helper.write_text(SLOW_HELPER % {"python": PYTHON, "helper_pid": str(self.helper_pid_file)})
        self.helper.chmod(0o700)
        self.producer = progress_tests.producer_for(self.helper)
        self.pids: list[int] = []
        self.addCleanup(self.reap)

    def reap(self) -> None:
        for pid in self.pids:
            try:
                os.kill(pid, signal.SIGKILL)
            except OSError:
                pass

    def start_leg(self):
        outcome: list[BaseException | str] = []

        def run() -> None:
            try:
                transcribe_leg(self.producer, self.audio, temporary_parent=self.root)
                outcome.append("finished")
            except BaseException as exc:  # noqa: BLE001  (recorded for the test to inspect)
                outcome.append(exc)

        thread = threading.Thread(target=run, daemon=True)
        thread.start()
        self.assertTrue(wait_for(self.helper_pid_file.exists, 20), "the fake helper did not start")
        pid = int(self.helper_pid_file.read_text())
        self.pids.append(pid)
        return thread, outcome, pid

    def test_kill_running_helpers_stops_the_helper_and_the_leg_fails_cleanly(self) -> None:
        thread, outcome, pid = self.start_leg()
        self.assertEqual(len(self.registry._running), 1)

        kill_running_helpers()

        thread.join(timeout=15)
        self.assertFalse(thread.is_alive())
        self.assertTrue(wait_for(lambda: not alive(pid), 10))
        self.assertIsInstance(outcome[0], AppleSpeechRefused)
        self.assertEqual(self.registry._running, set())
        self.assertEqual(list(self.root.glob(".apple-speech-*")), [])   # the leg cleaned up after itself

    def test_no_helper_starts_once_the_worker_is_stopping(self) -> None:
        kill_running_helpers()
        with self.assertRaisesRegex(AppleSpeechRefused, "app is closing"):
            transcribe_leg(self.producer, self.audio, temporary_parent=self.root)
        self.assertFalse(self.helper_pid_file.exists())
        self.assertEqual(list(self.root.glob(".apple-speech-*")), [])

    def test_a_finished_or_timed_out_helper_leaves_the_registry(self) -> None:
        progress_tests.write_helper(self.helper)
        producer = progress_tests.producer_for(self.helper)
        transcribe_leg(producer, self.audio, temporary_parent=self.root)
        self.assertEqual(self.registry._running, set())
        self.helper.write_text(SLOW_HELPER % {"python": PYTHON, "helper_pid": str(self.helper_pid_file)})
        producer = progress_tests.producer_for(self.helper)
        with self.assertRaisesRegex(AppleSpeechRefused, "timed out"):
            transcribe_leg(producer, self.audio, temporary_parent=self.root, timeout_seconds=3.0)
        self.assertEqual(self.registry._running, set())
        self.pids.append(int(self.helper_pid_file.read_text()))

    def test_killing_with_nothing_running_is_harmless(self) -> None:
        kill_running_helpers()
        kill_running_helpers()


if __name__ == "__main__":
    unittest.main()
