from __future__ import annotations

import importlib.util
import json
import re
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def load_source_digest():
    path = ROOT / "worker/source_digest.py"
    spec = importlib.util.spec_from_file_location("runtime_source_digest", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class SourceDigestCoverageContract(unittest.TestCase):
    """The covered set must track build_runtime.sh's actual staging steps."""

    def setUp(self):
        self.module = load_source_digest()
        self.covered = set(self.module.runtime_source_files(ROOT))
        self.script = (ROOT / "worker/build_runtime.sh").read_text(encoding="utf-8")

    def test_every_python_source_the_script_copies_is_covered(self):
        referenced = set(re.findall(r'"\$REPO/([A-Za-z0-9_./-]+\.py)"', self.script))
        self.assertTrue(referenced, "no $REPO/*.py references found — parser broke")
        uncovered = sorted(referenced - self.covered)
        self.assertEqual(
            uncovered,
            [],
            f"build_runtime.sh stages sources the freshness digest misses: {uncovered}",
        )

    def test_every_swift_package_the_script_builds_is_covered(self):
        packages = set(
            re.findall(r'--package-path "\$REPO/([A-Za-z0-9_./-]+)"', self.script)
        )
        self.assertTrue(packages, "no swift package paths found — parser broke")
        for package in packages:
            with self.subTest(package=package):
                self.assertTrue(
                    any(path.startswith(f"{package}/Sources/") for path in self.covered),
                    f"no {package}/Sources files in the freshness digest",
                )

    def test_note_validator_sources_are_covered(self):
        spec = importlib.util.spec_from_file_location(
            "bm_for_contract", ROOT / "worker/build_manifest.py"
        )
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        for source in module.VALIDATOR_SOURCES.values():
            relative = str(source.relative_to(ROOT))
            with self.subTest(source=relative):
                self.assertIn(relative, self.covered)

    def test_requirement_locks_and_staging_tooling_are_covered(self):
        for lock in (ROOT / "worker").glob("requirements-*.lock"):
            self.assertIn(f"worker/{lock.name}", self.covered)
        for tool in (
            "worker/build_runtime.sh",
            "worker/build_manifest.py",
            "worker/source_digest.py",
        ):
            self.assertIn(tool, self.covered)


class SourceDigestStampTests(unittest.TestCase):
    def setUp(self):
        self.module = load_source_digest()

    def stamped_stage(self, directory: str) -> Path:
        stage = Path(directory)
        self.module.write_stamp(stage, ROOT)
        return stage

    def test_stamp_is_canonical_and_round_trips_as_fresh(self):
        with tempfile.TemporaryDirectory() as directory:
            stage = self.stamped_stage(directory)
            raw = (stage / self.module.STAMP_NAME).read_text(encoding="utf-8")
            self.assertTrue(raw.endswith("\n"))
            document = json.loads(raw)
            self.assertEqual(document["schema"], self.module.SCHEMA)
            self.assertEqual(list(document["files"]), sorted(document["files"]))
            self.assertEqual(self.module.check_stamp(stage, ROOT), [])

    def test_never_staged_is_named_as_such(self):
        problems = self.module.check_stamp(Path("/nonexistent/runtime-stage"), ROOT)
        self.assertEqual(len(problems), 1)
        self.assertIn("not staged", problems[0])
        self.assertNotIn("predates source", problems[0])

    def test_unstamped_staging_is_distinguished_from_stale(self):
        with tempfile.TemporaryDirectory() as directory:
            problems = self.module.check_stamp(Path(directory), ROOT)
        self.assertEqual(len(problems), 1)
        self.assertIn("no source stamp", problems[0])
        self.assertIn("run worker/build_runtime.sh", problems[0])
        self.assertNotIn("predates source", problems[0])

    def rewrite(self, stage: Path, mutate) -> list[str]:
        stamp = stage / self.module.STAMP_NAME
        document = json.loads(stamp.read_text(encoding="utf-8"))
        mutate(document)
        stamp.write_text(json.dumps(document) + "\n", encoding="utf-8")
        return self.module.check_stamp(stage, ROOT)

    def test_changed_source_reports_the_file_and_the_remedy(self):
        with tempfile.TemporaryDirectory() as directory:
            stage = self.stamped_stage(directory)
            problems = self.rewrite(
                stage,
                lambda document: document["files"].__setitem__(
                    "worker/main.py", "0" * 64
                ),
            )
        self.assertIn("source changed since staging: worker/main.py", problems)
        self.assertEqual(
            problems[-1], "runtime staging predates source — run worker/build_runtime.sh"
        )

    def test_added_and_removed_sources_are_both_stale(self):
        with tempfile.TemporaryDirectory() as directory:
            stage = self.stamped_stage(directory)

            def mutate(document):
                document["files"].pop("worker/embedding.py")
                document["files"]["worker/retired_module.py"] = "1" * 64

            problems = self.rewrite(stage, mutate)
        self.assertIn("source added since staging: worker/embedding.py", problems)
        self.assertIn("source removed since staging: worker/retired_module.py", problems)
        self.assertIn("predates source", problems[-1])

    def test_unrecognized_schema_is_refused_without_a_stale_claim(self):
        with tempfile.TemporaryDirectory() as directory:
            stage = self.stamped_stage(directory)
            problems = self.rewrite(
                stage,
                lambda document: document.__setitem__("schema", "something-else/9"),
            )
        self.assertEqual(len(problems), 1)
        self.assertIn("unrecognized schema", problems[0])
        self.assertNotIn("predates source", problems[0])


class BundledStampResourceContract(unittest.TestCase):
    """Every enumerating tauri conf must carry the stamp into the bundle."""

    def test_confs_bundle_the_stamp_beside_the_runtime_manifest(self):
        for name in (
            "tauri.conf.json",
            "tauri.preview.conf.json",
            "tauri.fixture.conf.json",
        ):
            with self.subTest(conf=name):
                config = json.loads(
                    (ROOT / "apps/desktop/src-tauri" / name).read_text(encoding="utf-8")
                )
                resources = config["bundle"]["resources"]
                self.assertEqual(
                    resources.get("../runtime/source-digest.json"),
                    "source-digest.json",
                )


if __name__ == "__main__":
    unittest.main()
