"""Transcription progress, worker half.

The Apple speech helper writes the end time of each final result to stdout, one
number per line.  These tests pin the bridge that turns those lines into
cumulative, throttled ``transcription.progress`` events, and pin the rule that
progress is best effort: nothing about it may change a transcript or fail a
request.  Every helper here is a fake that is launched like the real one.
"""

from __future__ import annotations

import contextlib
import hashlib
import io
import json
import math
import os
import sys
import tempfile
import threading
import time
import unittest
import uuid
import wave
from pathlib import Path
from unittest import mock

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(REPO / "spike"))
sys.path.insert(0, str(REPO / "notes"))

import test_transcript_retry as retry_fixture  # noqa: E402  (reuses its real capture fixture)

from worker import adapters  # noqa: E402
from worker.apple_speech import (  # noqa: E402
    AppleSpeechRefused,
    TranscriptionProgress,
    _drain_progress,
    leg_timeout_seconds,
    make_producer,
    transcribe_leg,
    transcribe_legs,
    write_provenance,
)
from worker.main import (  # noqa: E402
    dispatch_without_protocol_output,
    make_transcription_progress_emitter,
)

PYTHON = sys.executable

RESULT = {
    "schema": "speech-engine-result/1",
    "status": "ok",
    "engine": "apple",
    "runs": [{
        "text": "hello",
        "segments": [{"text": "hello", "start": 0.0, "end": 0.1}],
        "utterances": [{"text": "hello", "start": 0.0, "end": 0.1}],
        "timing_issue_count": 0,
    }],
}
EXPECTED_SEGMENTS = [{"start": 0.0, "end": 0.1, "text": "hello"}]
RESULT_SHA256 = hashlib.sha256(json.dumps(RESULT).encode()).hexdigest()


def write_wav(path: Path, seconds: float) -> None:
    frames = int(seconds * 16_000)
    with wave.open(str(path), "wb") as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(16_000)
        remaining = frames
        while remaining > 0:
            count = min(remaining, 16_000)
            output.writeframes(b"\0\0" * count)
            remaining -= count


def write_helper(path: Path, *, before: str = "", after: str = "", write_result: bool = True) -> None:
    """A fake helper.  ``before`` runs first (progress output), ``after`` last."""
    source = (
        f"#!{PYTHON}\n"
        "import json, os, sys, time\n"
        "out = sys.argv[sys.argv.index('--out') + 1]\n"
        "audio = sys.argv[sys.argv.index('--audio') + 1]\n"
        "def put(data):\n"
        "    os.write(1, data if isinstance(data, bytes) else data.encode())\n"
        f"{before}\n"
        + (f"open(out, 'w').write(json.dumps({RESULT!r})); os.chmod(out, 0o600)\n" if write_result else "")
        + f"{after}\n"
    )
    path.write_text(source, encoding="utf-8")
    path.chmod(0o700)


def producer_for(helper: Path):
    return make_producer(
        helper=helper,
        helper_sha256=hashlib.sha256(helper.read_bytes()).hexdigest(),
        locale="en-US",
        os_version="26.0",
    )


class FakeClock:
    """Deterministic monotonic clock; ``step`` is added on every read."""

    def __init__(self, step: float = 0.0) -> None:
        self.now = 1000.0
        self.step = step
        self.slept: list[float] = []

    def __call__(self) -> float:
        self.now += self.step
        return self.now

    def sleep(self, seconds: float) -> None:
        self.slept.append(seconds)
        self.now += seconds


class LegHarness(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name).resolve()
        os.chmod(self.root, 0o700)
        self.addCleanup(self.temporary.cleanup)
        self.audio = self.root / "mic.wav"
        write_wav(self.audio, 10.0)
        self.helper = self.root / "helper.py"

    def run_leg(self, before: str = "", after: str = "", *, audio: Path | None = None, **options):
        write_helper(self.helper, before=before, after=after)
        return transcribe_leg(
            producer_for(self.helper), audio or self.audio, temporary_parent=self.root, **options
        )

    def assert_transcript_unchanged(self, result, audio: Path | None = None) -> None:
        segments, receipt = result
        self.assertEqual(segments, EXPECTED_SEGMENTS)
        self.assertEqual(
            receipt,
            {
                "audio_sha256": hashlib.sha256((audio or self.audio).read_bytes()).hexdigest(),
                "result_sha256": RESULT_SHA256,
            },
        )


class TranscribeLegProgressTests(LegHarness):
    def test_normal_progress_is_reported_in_order(self) -> None:
        seen: list[float] = []
        result = self.run_leg("put('1.000\\n2.500\\n9.250\\n')", on_progress=seen.append)
        self.assertEqual(seen, [1.0, 2.5, 9.25])
        self.assert_transcript_unchanged(result)

    def test_progress_lines_are_split_across_writes_and_flushes(self) -> None:
        seen: list[float] = []
        result = self.run_leg(
            "put('1.0'); time.sleep(0.05); put('00\\n2.'); time.sleep(0.05); put('000\\n')",
            on_progress=seen.append,
        )
        self.assertEqual(seen, [1.0, 2.0])
        self.assert_transcript_unchanged(result)

    def test_non_numeric_and_malformed_lines_are_ignored(self) -> None:
        noise = [
            b"hello world\n", b"nan\n", b"NaN\n", b"inf\n", b"-inf\n", b"-3.000\n", b"+5.000\n",
            b"1e3\n", b"1_0.000\n", b" 4.000\n", b"4.000 \n", b"\n", b"0x10\n", b"5.\n", b".5\n",
            b"1.2.3\n", b"1,5\n", b"4.000\r\n", "\u0663.000\n".encode(), b"\xff\xfe\n",
            b"The quarterly numbers look good\n",
        ]
        seen: list[float] = []
        before = f"put({b''.join(noise)!r})\nput('3.000\\n')"
        result = self.run_leg(before, on_progress=seen.append)
        self.assertEqual(seen, [3.0])
        self.assert_transcript_unchanged(result)

    def test_an_unterminated_last_line_is_not_a_value(self) -> None:
        seen: list[float] = []
        result = self.run_leg("put('2.000\\n7.000')", on_progress=seen.append)
        self.assertEqual(seen, [2.0])
        self.assert_transcript_unchanged(result)

    def test_values_that_go_backwards_never_lower_progress(self) -> None:
        seen: list[float] = []
        result = self.run_leg("put('3.000\\n2.000\\n5.000\\n4.000\\n5.000\\n1.000\\n')", on_progress=seen.append)
        self.assertEqual(seen, [3.0, 5.0])
        self.assert_transcript_unchanged(result)

    def test_values_above_the_duration_are_clamped_to_it(self) -> None:
        seen: list[float] = []
        result = self.run_leg("put('9.500\\n12.000\\n99999.000\\n')", on_progress=seen.append)
        self.assertEqual(seen, [9.5, 10.0])
        self.assert_transcript_unchanged(result)

    def test_a_flood_of_identical_lines_reports_once_and_never_blocks_the_helper(self) -> None:
        seen: list[float] = []
        # About 1.8 MB, far more than a 64 KB pipe holds: it only finishes if the
        # worker keeps draining.
        result = self.run_leg("put('5.000\\n' * 300000)", on_progress=seen.append, timeout_seconds=60)
        self.assertEqual(seen, [5.0])
        self.assert_transcript_unchanged(result)

    def test_a_flood_of_rising_lines_stays_monotone(self) -> None:
        seen: list[float] = []
        result = self.run_leg(
            "put(''.join('%.3f\\n' % (i / 2000.0) for i in range(1, 20001)))",
            on_progress=seen.append,
            timeout_seconds=60,
        )
        self.assertEqual(seen, sorted(set(seen)))
        self.assertEqual(seen[-1], 10.0)
        self.assert_transcript_unchanged(result)

    def test_an_overlong_line_is_discarded_even_when_its_tail_looks_numeric(self) -> None:
        # 65 * 1000 + 7 bytes: a thousand full 65-byte reads, then the 7-byte tail
        # "111111\n".  Read on its own that tail is a valid number, and it would
        # clamp to the duration and falsely mark the leg finished.
        seen: list[float] = []
        before = (
            "put('1' * (65 * 1000 + 6) + '\\n')\n"
            "put('2.000\\n')\n"
            "put('9' * 64 + '\\n')\n"      # exactly 65 bytes with its newline: complete, not numeric
            "put('7' * 65 + '\\n')\n"      # 66 bytes: the lone newline is the tail of the overlong line
            "put('3.000\\n')\n"
        )
        result = self.run_leg(before, on_progress=seen.append)
        self.assertEqual(seen, [2.0, 3.0])
        self.assert_transcript_unchanged(result)

    def test_a_silent_leg_reports_nothing_and_succeeds(self) -> None:
        seen: list[float] = []
        result = self.run_leg("", on_progress=seen.append)
        self.assertEqual(seen, [])
        self.assert_transcript_unchanged(result)
        self.assert_transcript_unchanged(self.run_leg(""))

    def test_a_callback_that_raises_stops_reporting_but_not_the_transcription(self) -> None:
        calls: list[float] = []

        def failing(seconds: float) -> None:
            calls.append(seconds)
            raise RuntimeError("progress sink is broken")

        # The second burst is larger than a pipe, so a reader that gave up after
        # the exception would leave the helper blocked until the timeout.
        result = self.run_leg(
            "put('1.000\\n2.000\\n')\nput('3.000\\n' * 100000)",
            on_progress=failing,
            timeout_seconds=60,
        )
        self.assertEqual(calls, [1.0])
        self.assert_transcript_unchanged(result)

    def test_a_reader_that_cannot_read_closes_its_end_instead_of_leaving_the_helper_to_stall(self) -> None:
        class BrokenPipe:
            def __init__(self) -> None:
                self.lines = [b"1.000\n", b"2.000\n"]
                self.closed = False

            def readline(self, _limit: int) -> bytes:
                if self.lines:
                    return self.lines.pop(0)
                raise OSError("read failed")

            def close(self) -> None:
                self.closed = True

        seen: list[float] = []
        stream = BrokenPipe()
        _drain_progress(stream, 10.0, seen.append, threading.Event())
        self.assertEqual(seen, [1.0, 2.0])
        self.assertTrue(stream.closed)

    def test_no_progress_thread_outlives_a_leg(self) -> None:
        self.run_leg("put('1.000\\n')", on_progress=lambda _seconds: None)
        self.assertFalse([t for t in threading.enumerate() if t.name == "apple-speech-progress"])

    def test_a_helper_that_closes_stdout_but_keeps_running_is_killed_at_the_deadline(self) -> None:
        pid_file = self.root / "pid"
        seen: list[float] = []
        started = time.monotonic()
        with self.assertRaisesRegex(AppleSpeechRefused, "timed out"):
            self.run_leg(
                f"open({str(pid_file)!r}, 'w').write(str(os.getpid()))\nput('1.000\\n')\nos.close(1)\ntime.sleep(120)",
                on_progress=seen.append,
                timeout_seconds=3.0,
            )
        self.assertLess(time.monotonic() - started, 30)
        self.assertEqual(seen, [1.0])
        pid = int(pid_file.read_text())
        with self.assertRaises(ProcessLookupError):
            os.kill(pid, 0)  # killed and reaped, not left running or as a zombie
        self.assertFalse([t for t in threading.enumerate() if t.name == "apple-speech-progress"])

    def test_a_helper_that_keeps_stdout_open_and_hangs_is_killed_at_the_deadline(self) -> None:
        pid_file = self.root / "pid"
        with self.assertRaisesRegex(AppleSpeechRefused, "timed out"):
            self.run_leg(
                f"open({str(pid_file)!r}, 'w').write(str(os.getpid()))\ntime.sleep(120)",
                timeout_seconds=3.0,
            )
        with self.assertRaises(ProcessLookupError):
            os.kill(int(pid_file.read_text()), 0)

    def test_the_timeout_grows_with_the_leg_duration(self) -> None:
        self.assertEqual(leg_timeout_seconds(0.2), 600)
        self.assertEqual(leg_timeout_seconds(600.0), 600)
        self.assertEqual(leg_timeout_seconds(600.2), 601)
        self.assertEqual(leg_timeout_seconds(3600.0), 3600)
        self.assertEqual(leg_timeout_seconds(7920.4), 7921)
        short, long = self.root / "short.wav", self.root / "long.wav"
        write_wav(short, 20.0)
        write_wav(long, 700.2)
        write_helper(self.helper)
        producer = producer_for(self.helper)
        for audio, expected in ((short, 600.0), (long, 701.0)):
            with self.subTest(audio=audio.name):
                with mock.patch("worker.apple_speech._run_transcribe_helper", return_value=1) as run:
                    with self.assertRaises(AppleSpeechRefused):
                        transcribe_leg(producer, audio, temporary_parent=self.root)
                self.assertEqual(run.call_args.kwargs["timeout_seconds"], expected)
        with mock.patch("worker.apple_speech._run_transcribe_helper", return_value=1) as run:
            with self.assertRaises(AppleSpeechRefused):
                transcribe_leg(producer, long, temporary_parent=self.root, timeout_seconds=1.5)
        self.assertEqual(run.call_args.kwargs["timeout_seconds"], 1.5)

    def test_existing_checks_survive_progress(self) -> None:
        seen: list[float] = []
        with self.assertRaisesRegex(AppleSpeechRefused, "helper failed"):
            self.run_leg("put('1.000\\n')", "sys.exit(3)", on_progress=seen.append)
        self.assertEqual(seen, [1.0])
        with self.assertRaisesRegex(AppleSpeechRefused, "changed while Apple speech read it"):
            self.run_leg("put('1.000\\n')", f"open({str(self.audio)!r}, 'ab').write(b'tamper')", on_progress=seen.append)
        write_wav(self.audio, 10.0)
        write_helper(self.helper)
        producer = producer_for(self.helper)
        self.helper.write_text(self.helper.read_text() + "# changed\n")
        with self.assertRaisesRegex(AppleSpeechRefused, "helper changed after verification"):
            transcribe_leg(producer, self.audio, temporary_parent=self.root, on_progress=seen.append)
        write_helper(self.helper, write_result=False)
        with self.assertRaisesRegex(AppleSpeechRefused, "result was invalid"):
            transcribe_leg(producer_for(self.helper), self.audio, temporary_parent=self.root)


class TranscriptionProgressTests(unittest.TestCase):
    def setUp(self) -> None:
        self.clock = FakeClock()
        self.events: list[tuple[int, int]] = []
        self.times: list[float] = []

    def emit(self, done: int, total: int) -> None:
        self.events.append((done, total))
        self.times.append(self.clock.now)

    def progress(self, total: int = 100) -> TranscriptionProgress:
        return TranscriptionProgress(self.emit, total=total, clock=self.clock, sleep=self.clock.sleep)

    def at(self, offset: float) -> None:
        self.clock.now = 1000.0 + offset

    def test_the_first_event_is_done_zero(self) -> None:
        progress = self.progress()
        progress.start()
        self.assertEqual(self.events, [(0, 100)])
        progress.start()
        self.assertEqual(self.events, [(0, 100)])

    def test_events_are_throttled_to_one_per_second_and_only_when_done_changed(self) -> None:
        progress = self.progress()
        report = progress.leg(0, 60)
        self.at(0.0); progress.start()
        self.at(0.2); report(5.0)       # inside the window
        self.at(0.99); report(7.9)      # inside the window
        self.assertEqual(self.events, [(0, 100)])
        self.at(1.0); report(7.9)       # window over: the latest value goes out
        self.assertEqual(self.events, [(0, 100), (7, 100)])
        self.at(1.5); report(7.9)       # unchanged
        self.at(1.6); report(9.0)       # inside the window
        self.assertEqual(len(self.events), 2)
        self.at(2.0); report(9.0)
        self.assertEqual(self.events[-1], (9, 100))
        self.at(2.9); report(30.0)
        self.assertEqual(len(self.events), 3)
        gaps = [b - a for a, b in zip(self.times, self.times[1:])]
        self.assertTrue(all(gap >= 1.0 for gap in gaps), gaps)

    def test_done_never_goes_down_and_total_never_changes(self) -> None:
        progress = self.progress()
        report = progress.leg(0, 60)
        self.at(0.0); progress.start()
        self.at(2.0); report(20.0)
        self.at(4.0); report(10.0)      # a lower value is not progress
        self.at(6.0); report(21.0)
        self.assertEqual(self.events, [(0, 100), (20, 100), (21, 100)])

    def test_the_second_leg_adds_the_first_legs_ceiling_and_clamps_to_its_own(self) -> None:
        progress = self.progress()
        mic, system = progress.leg(0, 60), progress.leg(60, 40)
        self.at(0.0); progress.start()
        self.at(2.0); mic(10.0)
        self.at(4.0); system(5.9)
        self.at(6.0); system(500.0)
        self.assertEqual(self.events, [(0, 100), (10, 100), (65, 100), (100, 100)])

    def test_unusable_reports_are_ignored(self) -> None:
        progress = self.progress()
        report = progress.leg(0, 60)
        self.at(0.0); progress.start()
        for value in (float("nan"), float("inf"), -1.0, -0.0001, "7", None, [3]):
            self.at(self.clock.now - 1000.0 + 5.0)
            report(value)  # type: ignore[arg-type]
        self.assertEqual(self.events, [(0, 100)])

    def test_a_finished_leg_waits_out_the_window_instead_of_dropping_its_update(self) -> None:
        progress = self.progress()
        self.at(0.0); progress.start()
        self.at(0.3); progress.leg_complete(60)
        self.assertEqual(len(self.clock.slept), 1)
        self.assertAlmostEqual(self.clock.slept[0], 0.7)
        self.assertEqual(self.events, [(0, 100), (60, 100)])
        self.assertGreaterEqual(self.times[1] - self.times[0], 1.0)

    def test_a_finished_leg_does_not_wait_when_nothing_is_pending(self) -> None:
        progress = self.progress()
        self.at(0.0); progress.start()
        self.at(5.0); progress.leg_complete(60)
        self.assertEqual(self.clock.slept, [])
        self.at(5.1); progress.leg_complete(60)    # already sent
        self.assertEqual(self.clock.slept, [])
        self.assertEqual(self.events, [(0, 100), (60, 100)])

    def test_the_final_event_is_always_total_and_is_not_throttled(self) -> None:
        progress = self.progress()
        self.at(0.0); progress.start()
        self.at(0.1); progress.finish()
        self.assertEqual(self.events, [(0, 100), (100, 100)])
        self.at(0.2); progress.finish()
        self.assertEqual(self.events, [(0, 100), (100, 100)])  # no duplicate

    def test_finish_is_not_repeated_when_the_last_report_already_reached_total(self) -> None:
        progress = self.progress()
        self.at(0.0); progress.start()
        self.at(2.0); progress.leg(60, 40)(40.0)
        self.assertEqual(self.events[-1], (100, 100))
        self.at(2.1); progress.finish()
        self.assertEqual(self.events, [(0, 100), (100, 100)])

    def test_a_failing_emitter_turns_progress_off_and_is_never_raised(self) -> None:
        calls = []

        def failing(done: int, total: int) -> None:
            calls.append((done, total))
            raise BrokenPipeError("protocol stream closed")

        progress = TranscriptionProgress(failing, total=100, clock=self.clock, sleep=self.clock.sleep)
        report = progress.leg(0, 60)
        self.at(0.0); progress.start()
        self.at(2.0); report(10.0)
        self.at(4.0); progress.leg_complete(60)
        progress.finish()
        self.assertEqual(calls, [(0, 100)])

    def test_the_total_must_be_a_positive_whole_number(self) -> None:
        for bad in (0, -5, 3.5, True, "9"):
            with self.subTest(total=bad), self.assertRaises(ValueError):
                TranscriptionProgress(self.emit, total=bad)  # type: ignore[arg-type]

    def test_reports_from_several_threads_stay_ordered_on_the_wire(self) -> None:
        clock = FakeClock(step=1.0)     # every attempt is outside the window
        events: list[tuple[int, int]] = []
        progress = TranscriptionProgress(lambda d, t: events.append((d, t)), total=10_000, clock=clock, sleep=clock.sleep)
        reports = [progress.leg(0, 10_000)] * 4

        def work(report) -> None:
            for second in range(1, 400):
                report(float(second))

        threads = [threading.Thread(target=work, args=(report,)) for report in reports]
        for thread in threads: thread.start()
        for thread in threads: thread.join()
        values = [done for done, _ in events]
        self.assertEqual(values, sorted(set(values)))
        self.assertEqual({total for _, total in events}, {10_000})


class TranscribeLegsProgressTests(LegHarness):
    MIC_SECONDS, SYSTEM_SECONDS = 40.0, 25.5
    TOTAL = 40 + 26

    def setUp(self) -> None:
        super().setUp()
        self.mic = self.root / "mic.wav"
        self.system = self.root / "system.wav"
        write_wav(self.mic, self.MIC_SECONDS)
        write_wav(self.system, self.SYSTEM_SECONDS)
        self.events: list[tuple[int, int]] = []
        self.times: list[float] = []
        self.speech = (
            "n = 40 if 'mic' in os.path.basename(audio) else 25.5\n"
            "put(''.join('%.3f\\n' % i for i in range(1, int(n) + 1)))\n"
            "put('%.3f\\n' % n)"
        )

    def run_legs(self, clock: FakeClock, before: str | None = None, emit=None):
        write_helper(self.helper, before=self.speech if before is None else before)

        def record(done: int, total: int) -> None:
            self.events.append((done, total))
            self.times.append(clock.now)

        return transcribe_legs(
            producer_for(self.helper), self.mic, self.system,
            temporary_parent=self.root, emit_progress=emit or record,
            clock=clock, sleep=clock.sleep,
        )

    def test_with_a_frozen_clock_only_the_first_leg_boundary_and_final_events_go_out(self) -> None:
        clock = FakeClock(step=0.0)
        (mic, _), (system, _) = self.run_legs(clock)
        self.assertEqual(self.events, [(0, self.TOTAL), (40, self.TOTAL), (self.TOTAL, self.TOTAL)])
        self.assertEqual(mic, EXPECTED_SEGMENTS)
        self.assertEqual(system, EXPECTED_SEGMENTS)

    def test_a_running_clock_gives_monotone_throttled_events_from_zero_to_total(self) -> None:
        clock = FakeClock(step=0.3)
        self.run_legs(clock)
        self.assertEqual(self.events[0], (0, self.TOTAL))
        self.assertEqual(self.events[-1], (self.TOTAL, self.TOTAL))
        self.assertEqual({total for _, total in self.events}, {self.TOTAL})
        done = [d for d, _ in self.events]
        self.assertEqual(done, sorted(set(done)))
        self.assertIn(40, done)                 # the mic leg's whole ceiling
        self.assertGreater(len(done), 5)        # real in-leg progress, not only boundaries
        self.assertLess(len(done), self.TOTAL)  # and throttled below one per second of audio
        gaps = [b - a for a, b in zip(self.times[:-1], self.times[1:-1])]
        self.assertTrue(all(gap >= 1.0 for gap in gaps), gaps)

    def test_silent_legs_still_count_as_fully_done_when_the_helper_exits_cleanly(self) -> None:
        clock = FakeClock(step=0.3)
        self.run_legs(clock, before="")
        self.assertEqual(self.events, [(0, self.TOTAL), (40, self.TOTAL), (self.TOTAL, self.TOTAL)])

    def test_a_silent_system_leg_after_speech_jumps_to_total_at_the_end(self) -> None:
        clock = FakeClock(step=0.3)
        mic_only = "if 'mic' in os.path.basename(audio): put(''.join('%.3f\\n' % i for i in range(1, 41)))"
        self.run_legs(clock, before=mic_only)
        done = [d for d, _ in self.events]
        self.assertEqual(done[-2:], [40, self.TOTAL])
        self.assertEqual(done, sorted(set(done)))

    def test_progress_changes_nothing_about_the_transcript(self) -> None:
        clock = FakeClock(step=0.3)
        with_progress = self.run_legs(clock)
        write_helper(self.helper, before=self.speech)
        without = transcribe_legs(producer_for(self.helper), self.mic, self.system, temporary_parent=self.root)
        self.assertEqual(with_progress, without)

    def test_a_failing_emitter_does_not_fail_the_legs(self) -> None:
        calls = []

        def failing(done: int, total: int) -> None:
            calls.append((done, total))
            raise BrokenPipeError("closed")

        clock = FakeClock(step=0.3)
        (mic, _), (system, _) = self.run_legs(clock, emit=failing)
        self.assertEqual(calls, [(0, self.TOTAL)])
        self.assertEqual((mic, system), (EXPECTED_SEGMENTS, EXPECTED_SEGMENTS))

    def test_an_unmeasurable_wav_skips_progress_and_fails_exactly_as_before(self) -> None:
        with wave.open(str(self.system), "wb") as output:
            output.setnchannels(1); output.setsampwidth(2); output.setframerate(8_000)
            output.writeframes(b"\0\0" * 8_000)
        write_helper(self.helper, before=self.speech)
        producer = producer_for(self.helper)
        with self.assertRaises(AppleSpeechRefused) as plain:
            transcribe_legs(producer, self.mic, self.system, temporary_parent=self.root)
        events: list[tuple[int, int]] = []
        with self.assertRaises(AppleSpeechRefused) as measured:
            transcribe_legs(producer, self.mic, self.system, temporary_parent=self.root, emit_progress=lambda d, t: events.append((d, t)))
        self.assertEqual(str(plain.exception), str(measured.exception))
        self.assertEqual(events, [])

    def test_a_leg_that_fails_never_reports_completion(self) -> None:
        clock = FakeClock(step=0.0)
        with self.assertRaisesRegex(AppleSpeechRefused, "helper failed"):
            self.run_legs(clock, before=self.speech + "\nsys.exit(2)")
        self.assertEqual(self.events, [(0, self.TOTAL)])


class ProgressWireTests(unittest.TestCase):
    def setUp(self) -> None:
        self.request_id = str(uuid.uuid4())
        self.meeting_id = str(uuid.uuid4())
        self.protocol = io.StringIO()

    def emitter(self, operation: str = "transcript.create", arguments=None):
        return make_transcription_progress_emitter(
            self.request_id,
            operation,
            {"meeting_id": self.meeting_id} if arguments is None else arguments,
            protocol_output=self.protocol,
        )

    def frames(self) -> list[str]:
        return self.protocol.getvalue().splitlines(keepends=True)

    def test_the_frame_is_exactly_the_contract_line(self) -> None:
        self.emitter()(1234, 7920)
        self.assertEqual(
            self.protocol.getvalue(),
            '{"audio_seconds_done":1234,"audio_seconds_total":7920,"event":"transcription.progress",'
            f'"meeting_id":"{self.meeting_id}","request_id":"{self.request_id}","schema":"worker-event/2"}}\n',
        )

    def test_it_is_built_for_create_and_retry_only(self) -> None:
        for operation in ("transcript.create", "transcript.retry"):
            self.assertIsNotNone(self.emitter(operation), operation)
        for operation in ("note.create", "capture.inspect", "capture.finalize", "transcript.restore", "sitting.derive"):
            self.assertIsNone(self.emitter(operation), operation)

    def test_it_needs_uuid_ids_and_object_arguments(self) -> None:
        self.assertIsNone(self.emitter(arguments={"meeting_id": "not-a-uuid"}))
        self.assertIsNone(self.emitter(arguments={"meeting_id": 7}))
        self.assertIsNone(self.emitter(arguments={}))
        self.assertIsNone(self.emitter(arguments=["x"]))
        self.assertIsNone(make_transcription_progress_emitter(
            "nope", "transcript.create", {"meeting_id": self.meeting_id}, protocol_output=self.protocol))
        self.assertEqual(self.frames(), [])

    def test_an_invalid_value_is_never_sent(self) -> None:
        emit = self.emitter()
        emit(0, 100)
        for done, total in (
            (True, 100), (5.0, 100), (5, 100.0), ("5", 100), (None, 100), (-1, 100),
            (101, 100), (5, 0), (5, -3), (5, True), (50, 99),   # total changed
        ):
            with self.subTest(done=done, total=total):
                emit(done, total)  # type: ignore[arg-type]
        emit(50, 100)
        emit(49, 100)                                           # went down
        emit(50, 100)                                           # equal is allowed
        emit(100, 100)
        sent = [json.loads(line) for line in self.frames()]
        self.assertEqual(
            [(f["audio_seconds_done"], f["audio_seconds_total"]) for f in sent],
            [(0, 100), (50, 100), (50, 100), (100, 100)],
        )

    def test_the_event_rides_the_protocol_stream_not_the_discarded_stdout(self) -> None:
        emit = self.emitter()

        def fake_dispatch(*_args, **kwargs):
            print("adapter noise that must never reach the protocol")
            kwargs["progress"](0, 5)
            kwargs["progress"](5, 5)
            return {"transcript": "a" * 64}

        with mock.patch("worker.main.dispatch", side_effect=fake_dispatch):
            with contextlib.redirect_stdout(io.StringIO()) as captured:
                result = dispatch_without_protocol_output(
                    Path("/nonexistent"), "transcript.create", {"meeting_id": self.meeting_id},
                    encoder_digest="e" * 64, admission="internal-alpha", model_dir=None, progress=emit,
                )
        self.assertEqual(result, {"transcript": "a" * 64})
        self.assertEqual(captured.getvalue(), "")
        self.assertEqual([json.loads(line)["audio_seconds_done"] for line in self.frames()], [0, 5])
        self.assertNotIn("noise", self.protocol.getvalue())

    def test_the_transcription_heartbeat_is_unchanged_and_independent(self) -> None:
        from worker.main import start_transcription_heartbeat

        with mock.patch("worker.main.TRANSCRIPTION_HEARTBEAT_SECONDS", 0.01):
            heartbeat = start_transcription_heartbeat(
                self.request_id, "transcript.create", {"meeting_id": self.meeting_id}, protocol_output=self.protocol,
            )
            stopped, thread = heartbeat
            try:
                for _ in range(1000):
                    if self.protocol.getvalue():
                        break
                    time.sleep(0.01)
            finally:
                stopped.set()
                thread.join()
        first = json.loads(self.frames()[0])
        self.assertEqual((first["event"], first["state"]), ("capture.state", "transcribing"))
        self.assertNotIn("audio_seconds_done", first)


class AppleAdapterProgressTests(unittest.TestCase):
    """Real transcript.create and transcript.retry code on a real capture fixture."""

    def setUp(self) -> None:
        self.fixture = retry_fixture.TranscriptRetryTests("test_retry_is_closed_source_bound_idempotent_and_uncommitted")
        self.fixture.setUp()
        self.addCleanup(self.fixture.tearDown)
        self.root = self.fixture.root
        self.meeting_id = self.fixture.meeting_id
        self.transcript_dir = self.fixture.transcript_dir
        self.helper = self.root / "apple-helper.py"
        self.markers = self.root / "markers"
        self.markers.mkdir()
        write_helper(
            self.helper,
            before=(
                f"open(os.path.join({str(self.markers)!r}, os.path.basename(audio)), 'w').close()\n"
                "put('0.100\\n0.190\\n')"
            ),
        )
        self.producer = producer_for(self.helper)
        self.clock = FakeClock(step=0.0)
        real = TranscriptionProgress
        self.patches = [
            mock.patch(
                "worker.apple_speech.TranscriptionProgress",
                lambda emit, **kw: real(emit, **{**kw, "clock": self.clock, "sleep": self.clock.sleep}),
            ),
            mock.patch("worker.adapters._installed_voiceprint_gate", return_value=None),
        ]
        for patch in self.patches:
            patch.start()
            self.addCleanup(patch.stop)
        self.log: list[object] = []
        self.candidate_bytes = b'{"fixture":"apple candidate"}\n'
        self.candidate = hashlib.sha256(self.candidate_bytes).hexdigest()

    def candidate_writer(self, capture_dir, transcript_dir, *_positional, **_keywords):
        self.log.append("revision")
        path = transcript_dir / f"{self.candidate}.json"
        if not path.exists():
            retry_fixture.private_file(path, self.candidate_bytes)
        return self.candidate, path

    def progress(self, done: int, total: int) -> None:
        self.log.append(("progress", done, total))
        self.markers_seen.append((done, len(list(self.markers.iterdir()))))

    def run_create(self, progress, engine: str = "apple-native"):
        import verify_capture

        real_verify = verify_capture.verify_acquisition

        def verify(capture_dir):
            self.log.append("verify")
            return real_verify(capture_dir)

        self.markers_seen: list[tuple[int, int]] = []
        with mock.patch("verify_capture.verify_acquisition", verify), \
                mock.patch("worker.transcription.create_transcript_revision_from_segments", self.candidate_writer), \
                mock.patch("worker.transcription.create_transcript_revision", self.candidate_writer):
            return adapters.transcript_create(
                self.root, {"meeting_id": self.meeting_id},
                admission="internal-alpha", model_dir=self.fixture.model, encoder_digest="e" * 64,
                transcription_engine=engine, speech_locale="en-US",
                apple_speech_helper=self.helper, apple_speech_helper_sha256=self.producer.helper_sha256,
                apple_speech_os_version="26.0", progress=progress,
            )

    def test_create_emits_zero_after_verification_and_before_the_first_leg(self) -> None:
        result = self.run_create(self.progress)
        self.assertEqual(result, {"transcript": self.candidate})
        # 0.2 s of audio per leg: ceil(0.2) + ceil(0.2) == 2.
        self.assertEqual(self.log, ["verify", ("progress", 0, 2), ("progress", 1, 2), ("progress", 2, 2), "revision"])
        self.assertEqual(self.markers_seen[0], (0, 0))   # no helper had started at done == 0
        self.assertEqual(self.markers_seen[1], (1, 1))   # the mic leg had, the system leg had not
        self.assertEqual(self.markers_seen[2], (2, 2))

    def test_a_progress_callback_that_raises_changes_nothing(self) -> None:
        def broken(done: int, total: int) -> None:
            self.log.append(("progress", done, total))
            raise BrokenPipeError("worker protocol stream closed")

        result = self.run_create(broken)
        self.assertEqual(result, {"transcript": self.candidate})
        self.assertEqual([entry for entry in self.log if entry != "verify"], [("progress", 0, 2), "revision"])
        provenance = list(self.transcript_dir.glob("*.apple-speech.json"))
        self.assertEqual([p.name for p in provenance], [f"{self.candidate}.apple-speech.json"])

    def test_create_without_a_callback_is_unchanged(self) -> None:
        self.assertEqual(self.run_create(None), {"transcript": self.candidate})
        self.assertEqual(self.log, ["verify", "revision"])

    def test_the_whisper_path_emits_no_progress(self) -> None:
        events: list[tuple[int, int]] = []
        result = self.run_create(lambda d, t: events.append((d, t)), engine="whisper")
        self.assertEqual(result, {"transcript": self.candidate})
        self.assertEqual(events, [])
        self.assertEqual(self.log, ["revision"])

    def test_the_whisper_retry_path_emits_no_progress(self) -> None:
        events: list[tuple[int, int]] = []
        calls: list[tuple[Path, Path]] = []
        with mock.patch("worker.transcription.create_transcript_revision", self.fixture._candidate_writer(calls)):
            adapters.transcript_retry(
                self.root, self.fixture._arguments(),
                admission="internal-alpha", model_dir=self.fixture.model, encoder_digest="e" * 64,
                progress=lambda d, t: events.append((d, t)),
            )
        self.assertEqual(len(calls), 1)
        self.assertEqual(events, [])

    def test_retry_on_the_apple_path_reports_the_same_progress(self) -> None:
        receipt = {"audio_sha256": "a" * 64, "result_sha256": "b" * 64}
        write_provenance(self.transcript_dir, self.fixture.source_digest, self.producer, mic=receipt, system=receipt)
        events: list[tuple[int, int]] = []
        with mock.patch("worker.transcription.create_transcript_revision_from_segments", self.candidate_writer):
            digests = adapters.transcript_retry(
                self.root, self.fixture._arguments(),
                admission="internal-alpha", model_dir=None, encoder_digest="e" * 64,
                transcription_engine="apple-native", speech_locale="en-US",
                apple_speech_helper=self.helper, apple_speech_helper_sha256=self.producer.helper_sha256,
                apple_speech_os_version="26.0", progress=lambda d, t: events.append((d, t)),
            )
        self.assertEqual(digests["candidate-transcript"], self.candidate)
        self.assertEqual(events, [(0, 2), (1, 2), (2, 2)])

    def test_dispatch_hands_the_callback_to_create_and_retry(self) -> None:
        for operation in ("transcript.create", "transcript.retry"):
            with self.subTest(operation=operation):
                target = "transcript_create" if operation.endswith("create") else "transcript_retry"
                sentinel = object()
                with mock.patch(f"worker.adapters.{target}", return_value={"ok": "x"}) as adapter:
                    adapters.dispatch(
                        self.root, operation, {"meeting_id": self.meeting_id},
                        encoder_digest="e" * 64, admission="internal-alpha", progress=sentinel,  # type: ignore[arg-type]
                    )
                self.assertIs(adapter.call_args.kwargs["progress"], sentinel)


DRIVER = r"""
import hashlib, os, sys
from pathlib import Path
from unittest import mock
sys.path.insert(0, %(spike)r)
sys.path.insert(0, %(notes)r)
import worker.main as main

def revision(capture_dir, transcript_dir, **_keywords):
    data = b'{"fixture":"end to end"}\n'
    digest = hashlib.sha256(data).hexdigest()
    path = Path(transcript_dir) / (digest + ".json")
    if not path.exists():
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(data)
    return digest, path

with mock.patch("worker.transcription.create_transcript_revision_from_segments", revision), \
        mock.patch("worker.adapters._installed_voiceprint_gate", return_value=None):
    raise SystemExit(main.main())
"""

E2E_HELPER = (
    "#!%(python)s\n"
    "import json, os, sys\n"
    "out = sys.argv[sys.argv.index('--out') + 1]\n"
    "if '--capabilities' in sys.argv:\n"
    "    d = {'schema': 'apple-speech-capability/1', 'state': 'ready', 'locale': 'en-US', 'os_version': '26.0 fixture', 'asset_identity': 'os-managed'}\n"
    "else:\n"
    "    os.write(1, b'0.100\\n0.190\\n')\n"
    "    d = %(result)r\n"
    "open(out, 'w').write(json.dumps(d)); os.chmod(out, 0o600)\n"
)


class NativeWorkerProgressEndToEndTests(unittest.TestCase):
    """The real worker process, protocol stream and all, with a fake helper."""

    def test_a_transcript_create_request_streams_progress_then_its_result(self) -> None:
        import select
        import subprocess

        fixture = retry_fixture.TranscriptRetryTests("test_retry_is_closed_source_bound_idempotent_and_uncommitted")
        fixture.setUp()
        self.addCleanup(fixture.tearDown)
        with tempfile.TemporaryDirectory() as resources_name:
            resources = Path(resources_name).resolve()
            os.chmod(resources, 0o700)
            helper = resources / "apple-speech"
            helper.write_text(E2E_HELPER % {"python": PYTHON, "result": RESULT})
            helper.chmod(0o700)
            manifest = {"schema": "app-runtime/3", "admission": "internal-alpha", "models": []}
            for field in ("runtime", "worker", "tap", "encoder", "permission_probe", "model_catalog", "apple_speech"):
                path = helper if field == "apple_speech" else resources / field
                if field != "apple_speech":
                    path.write_text("{}" if field == "model_catalog" else field)
                manifest[field] = {"path": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
            manifest_path = resources / "app-runtime.json"
            manifest_path.write_text(json.dumps(manifest))
            request_id = str(uuid.uuid4())
            command = {
                "schema": "worker-command/2", "request_id": request_id, "operation": "transcript.create",
                "arguments": {"meeting_id": fixture.meeting_id},
            }
            liveness_read, liveness_write = os.pipe()
            child = subprocess.Popen(
                [PYTHON, "-c", DRIVER % {"spike": str(REPO / "spike"), "notes": str(REPO / "notes")},
                 "--app-data-root", str(fixture.root), "--runtime-manifest", str(manifest_path),
                 "--transcription-engine", "apple-native", "--parent-liveness-fd", str(liveness_read)],
                cwd=REPO, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                pass_fds=(liveness_read,), text=True,
            )
            os.close(liveness_read)
            lines: list[str] = []
            try:
                child.stdin.write(json.dumps(command) + "\n")
                child.stdin.flush()
                deadline = time.monotonic() + 60
                while time.monotonic() < deadline:
                    if not select.select([child.stdout], [], [], 1)[0]:
                        continue
                    line = child.stdout.readline()
                    self.assertTrue(line, f"worker closed its stream early after {lines}")
                    lines.append(line)
                    if json.loads(line).get("schema") == "worker-result/2":
                        break
                else:
                    self.fail(f"no result from the worker; saw {lines}")
            finally:
                os.close(liveness_write)
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=10)
                child.stdin.close()
                child.stdout.close()
        frames = [json.loads(line) for line in lines]
        self.assertEqual(frames[0]["event"], "worker.ready")
        self.assertEqual(frames[-1]["schema"], "worker-result/2")
        self.assertTrue(frames[-1]["ok"], frames[-1])
        progress = [line for line in lines if '"transcription.progress"' in line]
        self.assertEqual(
            progress,
            [
                '{"audio_seconds_done":%d,"audio_seconds_total":2,"event":"transcription.progress",'
                '"meeting_id":"%s","request_id":"%s","schema":"worker-event/2"}\n' % (done, fixture.meeting_id, request_id)
                for done in (0, 1, 2)
            ],
        )
        # Nothing but the ready event, optional 3 s heartbeats, progress, and one result.
        for frame in frames[1:-1]:
            self.assertIn(frame["event"], {"capture.state", "transcription.progress"})


if __name__ == "__main__":
    unittest.main()
