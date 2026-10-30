"""Stale Apple speech leg directories are swept before the next run.

A worker that is killed mid-leg (a supervisor timeout, a crash, a quit before the
worker took its helper down with it) cannot remove its ``.apple-speech-*``
directory, which holds that leg's raw result text.  Only one transcription runs
per meeting at a time, so the next Apple speech run for the meeting removes what
it finds in the meeting's transcript folder before its first leg.
"""

from __future__ import annotations

import hashlib
import os
import shutil
import stat
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(REPO / "spike"))
sys.path.insert(0, str(REPO / "notes"))

import test_transcription_progress as progress_tests  # noqa: E402  (its fake helpers and capture fixture)

from worker import adapters  # noqa: E402
from worker.apple_speech import sweep_stale_leg_directories  # noqa: E402


class SweepStaleLegDirectoriesTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.dir = Path(self.temporary.name).resolve()
        os.chmod(self.dir, 0o700)

    def stale(self, name: str = ".apple-speech-abcd1234") -> Path:
        path = self.dir / name
        path.mkdir(mode=0o700)
        (path / "result.json").write_text('{"raw":"transcript text"}')
        os.chmod(path / "result.json", 0o600)
        return path

    def test_it_removes_stale_leg_directories_with_their_results(self) -> None:
        first, second = self.stale(), self.stale(".apple-speech-z9_y8x7w")
        self.assertEqual(sweep_stale_leg_directories(self.dir), 2)
        self.assertFalse(first.exists() or second.exists())

    def test_it_removes_a_directory_named_by_tempfile_itself(self) -> None:
        path = Path(tempfile.mkdtemp(prefix=".apple-speech-", dir=self.dir))
        (path / "result.json").write_text("{}")
        self.assertEqual(sweep_stale_leg_directories(self.dir), 1)
        self.assertFalse(path.exists())

    def test_it_leaves_everything_else_alone(self) -> None:
        keep_names = [
            f"{'a' * 64}.json",                       # a transcript revision
            f"{'a' * 64}.apple-speech.json",          # its provenance
            "notes.txt",
            ".apple-speech.json",
            ".apple-speech-",                         # the bare prefix is not a leg directory
            ".apple-speech-capability-abcd1234",      # the capability check's directory
            ".apple-speech-Upper1",                   # not tempfile's alphabet
            ".other-abcd1234",
        ]
        for name in keep_names:
            if "capability" in name:
                (self.dir / name).mkdir()
                (self.dir / name / "capability.json").write_text("{}")
            else:
                (self.dir / name).write_text("kept")
        (self.dir / "subdirectory").mkdir()
        (self.dir / "subdirectory" / ".apple-speech-abcd1234").mkdir()   # only direct children are swept
        self.assertEqual(sweep_stale_leg_directories(self.dir), 0)
        for name in keep_names:
            self.assertTrue((self.dir / name).exists(), name)
        self.assertTrue((self.dir / "subdirectory" / ".apple-speech-abcd1234").is_dir())

    def test_a_regular_file_with_a_leg_name_is_not_removed(self) -> None:
        file = self.dir / ".apple-speech-abcd1234"
        file.write_text("not a directory")
        self.assertEqual(sweep_stale_leg_directories(self.dir), 0)
        self.assertEqual(file.read_text(), "not a directory")

    def test_a_symlink_with_a_leg_name_is_neither_followed_nor_removed(self) -> None:
        outside = Path(tempfile.mkdtemp(prefix="outside-"))
        self.addCleanup(lambda: shutil.rmtree(outside, ignore_errors=True))
        (outside / "precious.txt").write_text("keep me")
        link = self.dir / ".apple-speech-linked"
        os.symlink(outside, link)
        dangling = self.dir / ".apple-speech-dangling"
        os.symlink(self.dir / "missing", dangling)
        self.assertEqual(sweep_stale_leg_directories(self.dir), 0)
        self.assertTrue(link.is_symlink() and dangling.is_symlink())
        self.assertEqual((outside / "precious.txt").read_text(), "keep me")

    def test_a_symlink_inside_a_stale_directory_is_removed_without_touching_its_target(self) -> None:
        outside = Path(tempfile.mkdtemp(prefix="outside-"))
        self.addCleanup(lambda: shutil.rmtree(outside, ignore_errors=True))
        (outside / "precious.txt").write_text("keep me")
        stale = self.stale()
        os.symlink(outside, stale / "escape")
        self.assertEqual(sweep_stale_leg_directories(self.dir), 1)
        self.assertFalse(stale.exists())
        self.assertEqual((outside / "precious.txt").read_text(), "keep me")

    def test_a_directory_owned_by_someone_else_is_not_removed(self) -> None:
        stale = self.stale()
        with mock.patch("worker.apple_speech.os.geteuid", return_value=os.geteuid() + 1):
            self.assertEqual(sweep_stale_leg_directories(self.dir), 0)
        self.assertTrue(stale.is_dir())

    def test_failures_are_skipped_and_never_raised(self) -> None:
        first, second = self.stale(), self.stale(".apple-speech-z9_y8x7w")
        with mock.patch("worker.apple_speech.shutil.rmtree", side_effect=PermissionError("cannot remove")):
            self.assertEqual(sweep_stale_leg_directories(self.dir), 0)
        self.assertTrue(first.is_dir() and second.is_dir())
        self.assertEqual(sweep_stale_leg_directories(self.dir / "no-such-folder"), 0)
        os.chmod(self.dir, stat.S_IRUSR | stat.S_IXUSR)   # cannot be modified, only read
        self.addCleanup(lambda: os.chmod(self.dir, 0o700))
        self.assertEqual(sweep_stale_leg_directories(self.dir), 0)


class SweepBeforeTheLegsTests(unittest.TestCase):
    """Real transcript.create code on a real capture fixture, with a fake helper."""

    def setUp(self) -> None:
        self.fixture = progress_tests.retry_fixture.TranscriptRetryTests(
            "test_retry_is_closed_source_bound_idempotent_and_uncommitted"
        )
        self.fixture.setUp()
        self.addCleanup(self.fixture.tearDown)
        self.root = self.fixture.root
        self.meeting_id = self.fixture.meeting_id
        self.transcript_dir = self.fixture.transcript_dir
        self.listings = self.root / "listings"
        self.listings.mkdir()
        self.helper = self.root / "apple-helper.py"
        # Each leg records what its meeting's transcript folder held when it started.
        progress_tests.write_helper(
            self.helper,
            before=(
                f"open(os.path.join({str(self.listings)!r}, os.path.basename(audio)), 'w').write("
                "'\\n'.join(sorted(os.listdir(os.path.dirname(os.path.dirname(out))))))"
            ),
        )
        self.producer = progress_tests.producer_for(self.helper)
        self.candidate_bytes = b'{"fixture":"sweep candidate"}\n'
        self.candidate = hashlib.sha256(self.candidate_bytes).hexdigest()
        patch = mock.patch("worker.adapters._installed_voiceprint_gate", return_value=None)
        patch.start()
        self.addCleanup(patch.stop)

    def writer(self, capture_dir, transcript_dir, *_positional, **_keywords):
        path = transcript_dir / f"{self.candidate}.json"
        if not path.exists():
            progress_tests.retry_fixture.private_file(path, self.candidate_bytes)
        return self.candidate, path

    def stale(self, name: str) -> Path:
        path = self.transcript_dir / name
        path.mkdir(mode=0o700)
        (path / "result.json").write_text('{"raw":"left by a killed worker"}')
        return path

    def create(self) -> dict:
        with mock.patch("worker.transcription.create_transcript_revision_from_segments", self.writer):
            return adapters.transcript_create(
                self.root, {"meeting_id": self.meeting_id},
                admission="internal-alpha", model_dir=None, encoder_digest="e" * 64,
                transcription_engine="apple-native", speech_locale="en-US",
                apple_speech_helper=self.helper, apple_speech_helper_sha256=self.producer.helper_sha256,
                apple_speech_os_version="26.0",
            )

    def test_stale_leg_directories_are_gone_before_the_first_leg_starts(self) -> None:
        stale = [self.stale(".apple-speech-abcd1234"), self.stale(".apple-speech-wxyz_789")]
        self.assertEqual(self.create(), {"transcript": self.candidate})
        for leg in ("mic.wav", "system.wav"):
            seen = (self.listings / leg).read_text().splitlines()
            for path in stale:
                self.assertNotIn(path.name, seen, f"{path.name} was still there when the {leg} leg started")
        self.assertFalse(any(path.exists() for path in stale))

    def test_other_entries_and_the_capability_directory_stay(self) -> None:
        capability = self.transcript_dir / ".apple-speech-capability-abcd1234"
        capability.mkdir(mode=0o700)
        (capability / "capability.json").write_text("{}")
        other = self.transcript_dir / "notes.txt"
        other.write_text("kept")
        source = self.fixture.source
        self.stale(".apple-speech-abcd1234")
        self.create()
        self.assertTrue((capability / "capability.json").is_file())
        self.assertEqual(other.read_text(), "kept")
        self.assertTrue(source.is_file())
        self.assertTrue((self.transcript_dir / f"{self.candidate}.json").is_file())
        self.assertTrue((self.transcript_dir / f"{self.candidate}.apple-speech.json").is_file())

    def test_a_symlink_named_like_a_leg_directory_is_not_followed(self) -> None:
        outside = Path(tempfile.mkdtemp(prefix="outside-"))
        self.addCleanup(lambda: shutil.rmtree(outside, ignore_errors=True))
        (outside / "precious.txt").write_text("keep me")
        link = self.transcript_dir / ".apple-speech-linked"
        os.symlink(outside, link)
        self.assertEqual(self.create(), {"transcript": self.candidate})
        self.assertTrue(link.is_symlink())
        self.assertEqual((outside / "precious.txt").read_text(), "keep me")

    def test_a_removal_failure_does_not_fail_or_change_the_transcript(self) -> None:
        stale = self.stale(".apple-speech-abcd1234")
        clean = self.create()
        self.assertFalse(stale.exists())
        stale = self.stale(".apple-speech-abcd1234")
        real_rmtree = shutil.rmtree

        def refuse_the_stale_directory(path, *args, **kwargs):
            if Path(path) == stale:          # the legs' own directories still clean up normally
                raise PermissionError("cannot remove")
            return real_rmtree(path, *args, **kwargs)

        with mock.patch("shutil.rmtree", refuse_the_stale_directory):
            refused = self.create()
        self.assertEqual(refused, clean)
        self.assertTrue(stale.is_dir())      # left for the next run, not fatal now
        self.assertEqual(self.create(), clean)
        self.assertFalse(stale.exists())

    def test_a_refused_capture_leaves_the_folder_untouched(self) -> None:
        stale = self.stale(".apple-speech-abcd1234")
        (self.fixture.capture / "mic.wav").unlink()
        with self.assertRaises((Exception, SystemExit)):
            self.create()
        self.assertTrue(stale.is_dir())


if __name__ == "__main__":
    unittest.main()
