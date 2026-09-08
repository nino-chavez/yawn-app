import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("release_notes", Path(__file__).resolve().parents[1] / "scripts/prepare-release-notes.py")
notes = importlib.util.module_from_spec(spec)
spec.loader.exec_module(notes)


class ReleaseCorpusTests(unittest.TestCase):
    def test_range_excludes_previous_release_and_keeps_commit_body(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            def git(*args):
                return subprocess.check_output(["git", "-C", str(repo), *args], stdin=subprocess.DEVNULL, text=True, timeout=10).strip()
            git("init", "-q")
            git("config", "user.name", "Release test")
            git("config", "user.email", "test@example.invalid")
            git("config", "core.hooksPath", "/dev/null")
            (repo / "screen.txt").write_text("before")
            git("add", ".")
            git("commit", "-qm", "Previous release")
            git("tag", "v0.6.3")
            (repo / "screen.txt").write_text("after")
            git("commit", "-qam", "Fix wrapping\n\nLong meeting titles stay inside the pane.")
            result = notes.collect("v0.6.3", "HEAD", "0.6.4", repo)
            self.assertEqual(len(result["commits"]), 1)
            self.assertEqual(result["commits"][0]["subject"], "Fix wrapping")
            self.assertIn("Long meeting titles", result["commits"][0]["body"])
            self.assertEqual(result["commits"][0]["files"], ["screen.txt"])
            self.assertEqual(result["status"], "input-for-review")
            with self.assertRaises(subprocess.CalledProcessError):
                notes.collect("HEAD", "v0.6.3", "0.6.4", repo)
            with self.assertRaises(ValueError):
                notes.collect("v0.6.3", "HEAD", "latest", repo)
