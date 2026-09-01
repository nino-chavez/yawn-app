"""What `corpus.embed` has to get right.

Most of these run without weights, because most of what can go wrong here is
argument handling and the two silent tokenizer defaults rather than the model.

The rest need `LMN_EMBEDDING_MODEL_DIR` to name a directory holding the pinned
revision. **`build_runtime.sh verify` sets it explicitly** to the staged model,
so they run in the packaged runtime rather than skipping — a suite that skipped
the only tests touching the model would report the same green as one that ran
them. Outside that, they skip, which is why this file states the variable rather
than defaulting to a path that happens to exist on one machine.
"""

from __future__ import annotations

import base64
import hashlib
import json
import os
import sys
import unittest
from pathlib import Path
from unittest import mock

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "notes"))

from worker import adapters, embedding, main

MODEL_DIRECTORY = os.environ.get("LMN_EMBEDDING_MODEL_DIR")
WITH_MODEL = unittest.skipIf(
    not MODEL_DIRECTORY or not Path(MODEL_DIRECTORY).is_dir(),
    "set LMN_EMBEDDING_MODEL_DIR to the pinned MiniLM directory",
)


def window(text: str) -> dict:
    return {
        "text_sha256": hashlib.sha256(text.encode("utf-8")).hexdigest(),
        "text": text,
    }


class RegistrationTests(unittest.TestCase):
    def test_the_operation_is_registered_in_every_lane(self) -> None:
        """Replaced rather than deleted, and the note is the point.

        Until 2026-08-08 this asserted the *opposite* for internal-alpha, on the
        ground that the shipped worker must not advertise what it can only
        refuse. `build-alpha` now stages the model, so the ground is gone. The
        boundary lane keeps it too — a boundary runtime carries neither the wheel
        nor the model, and `corpus_embed` refuses on the manifest there.
        """
        for admission in ("boundary-test", "product", "internal-alpha"):
            self.assertIn("corpus.embed", main.operations_for(admission))

    def test_no_packaged_model_refuses_rather_than_failing_to_import(self) -> None:
        with self.assertRaises(adapters.AdapterRefused) as refused:
            adapters.dispatch(
                REPO,
                "corpus.embed",
                {"windows": [window("alpha beta")]},
                encoder_digest="0" * 64,
                embedding_dir=None,
            )
        self.assertIn("no embedding model is packaged", str(refused.exception))

    def test_a_manifest_without_the_model_names_no_directory(self) -> None:
        """A boundary manifest lists no models at all, and that has to read as
        absence rather than as an error at request time."""
        manifest = {"admission": "boundary-test", "models": []}
        self.assertIsNone(main.embedding_model_dir(REPO / "app-runtime.json", manifest))

    def test_a_partial_model_directory_is_not_a_model_directory(self) -> None:
        manifest = {
            "admission": "internal-alpha",
            "models": [{"id": "all-minilm-l6-v2-weights", "path": "x", "sha256": "y"}],
        }
        self.assertIsNone(main.embedding_model_dir(REPO / "app-runtime.json", manifest))


class ArgumentTests(unittest.TestCase):
    """These reach `embed_windows` directly; none of them get as far as a model."""

    def refuses(self, windows, fragment: str) -> None:
        with self.assertRaises(embedding.EmbeddingRefused) as refused:
            embedding.embed_windows(None, None, windows)
        self.assertIn(fragment, str(refused.exception))

    def test_an_empty_request_is_refused(self) -> None:
        self.refuses([], "carries no windows")

    def test_more_windows_than_one_frame_can_answer_is_refused(self) -> None:
        many = [window(f"text {index}") for index in range(embedding.MAX_WINDOWS_PER_REQUEST + 1)]
        self.refuses(many, "exceeds")

    def test_a_window_with_extra_fields_is_refused(self) -> None:
        extra = window("alpha")
        extra["meeting_id"] = "not-yours-to-send"
        self.refuses([extra], "closed schema")

    def test_a_digest_that_does_not_describe_the_text_is_refused(self) -> None:
        mismatched = window("alpha beta")
        mismatched["text"] = "gamma delta"
        self.refuses([mismatched], "does not match the digest")

    def test_a_digest_that_is_not_a_lowercase_sha256_is_refused(self) -> None:
        for bad in ("0" * 63, "0" * 65, "A" * 64, "z" * 64):
            broken = window("alpha")
            broken["text_sha256"] = bad
            self.refuses([broken], "lowercase sha256")

    def test_argument_refusal_does_not_need_the_model_runtime(self) -> None:
        """The class docstring says none of these get as far as a model; this
        makes that claim mechanical. The boundary runtime packages no mlx wheel,
        and until 2026-09-01 an import above the validation made these five
        tests error there with ModuleNotFoundError — invisible on a dev machine
        where mlx happens to be importable, which is why this pins it by
        poisoning the import rather than trusting the environment."""
        with mock.patch.dict(sys.modules, {"mlx_minilm": None}):
            self.refuses([], "carries no windows")
            self.refuses([{"unexpected": "field"}], "closed schema")

    def test_a_reply_fits_one_protocol_frame(self) -> None:
        """The batch cap is derived from `MAX_FRAME_BYTES`; this checks the
        arithmetic rather than trusting the comment that states it."""
        entry = "0" * 64
        vector = base64.b64encode(bytes(embedding.VECTOR_DIMENSION * 4)).decode("ascii")
        reply = {
            "schema": "worker-result/2",
            "request_id": "11111111-1111-4111-8111-111111111111",
            "ok": True,
            "code": None,
            "recoverable": None,
            "artifact_digests": {
                entry[:-2] + f"{index:02x}": vector
                for index in range(embedding.MAX_WINDOWS_PER_REQUEST)
            },
        }
        encoded = json.dumps(reply, separators=(",", ":"), sort_keys=True)
        self.assertLessEqual(len(encoded.encode("utf-8")), main.MAX_FRAME_BYTES)


class ForwardPassProvenanceTests(unittest.TestCase):
    def test_the_forward_pass_is_imported_and_not_restated(self) -> None:
        """The measured vectors came from `mlx_minilm`. A second forward pass
        here would produce vectors nothing measured, and they would look right."""
        source = Path(embedding.__file__).read_text(encoding="utf-8")
        self.assertIn("from mlx_minilm import MiniLMEncoder", source)
        self.assertIn("from mlx_minilm import SequenceTooLong, encode", source)
        # Calls, not words. An earlier version banned "attention" and tripped on
        # `attention_mask`, which this module legitimately builds — the same
        # mistake a `tanh` ban made in `mlx_minilm`'s own tests. What would
        # actually mean the math was inlined is an `mx.` call or a layer
        # definition.
        for restated in (
            "import mlx",
            "mx.",
            "def forward",
            "def _feed_forward",
            "softmax",
            "layer_norm",
        ):
            self.assertNotIn(
                restated,
                source,
                f"{restated!r} suggests the forward pass was inlined rather than imported",
            )


@WITH_MODEL
class ModelTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.directory = Path(MODEL_DIRECTORY)
        cls.encoder, cls.tokenizer = embedding.load(cls.directory)

    def test_the_two_silent_defaults_are_off(self) -> None:
        """`tokenizer.json` at this revision carries truncation at 128 and fixed
        padding to 128. Both would produce vectors of the right shape from the
        wrong tokens, which nothing downstream can detect."""
        self.assertIsNone(self.tokenizer.truncation)
        # `length` is the whole check: `None` is pad-to-longest-in-batch, and the
        # value this file ships with is 128. Asserting the number it must not be
        # as well as the value it must have, because the two are one keystroke
        # apart and only one of them is detectable downstream.
        self.assertIsNone(self.tokenizer.padding["length"])
        self.assertNotEqual(self.tokenizer.padding["length"], 128)

    def test_a_vector_is_the_bytes_the_store_holds(self) -> None:
        import numpy

        text = "the lease renews in March and nobody has told the landlord"
        produced = embedding.embed_windows(self.encoder, self.tokenizer, [window(text)])
        self.assertEqual(list(produced), [window(text)["text_sha256"]])
        raw = base64.b64decode(produced[window(text)["text_sha256"]])
        self.assertEqual(len(raw), embedding.VECTOR_DIMENSION * 4)
        values = numpy.frombuffer(raw, dtype="<f4")
        self.assertEqual(values.shape, (embedding.VECTOR_DIMENSION,))
        self.assertTrue(numpy.isfinite(values).all())
        # Unnormalized mean-pooled hidden state, as `mlx_minilm.embed` returns
        # and as the probe normalises at comparison time. A unit-length vector
        # here would mean someone normalised on the way out and the store's
        # cosine would be computed twice.
        self.assertNotAlmostEqual(float(numpy.linalg.norm(values)), 1.0, places=3)

    def test_identical_windows_collapse_to_one_entry(self) -> None:
        text = "two meetings said exactly this"
        produced = embedding.embed_windows(
            self.encoder, self.tokenizer, [window(text), window(text)]
        )
        self.assertEqual(len(produced), 1)

    def test_a_window_over_the_ceiling_refuses_rather_than_truncating(self) -> None:
        over = " ".join(f"word{index}" for index in range(400))
        with self.assertRaises(embedding.EmbeddingRefused) as refused:
            embedding.embed_windows(self.encoder, self.tokenizer, [window(over)])
        self.assertIn("exceeds the model window", str(refused.exception))

    def test_batching_moves_a_vector_only_below_what_a_ranking_can_see(self) -> None:
        """Padding is masked out of both the attention and the mean, so batching
        must not *change* a window's meaning. It does move the numbers.

        Measured rather than hoped: a 100-word window batched beside a two-word
        one differs from the same window alone in 3 of 384 components, by up to
        1.4e-6 — MLX's batched matmul is not bitwise the unbatched one. The
        registered name for this test said "agrees" and asserted 1e-6; it failed,
        and widening the tolerance quietly would have hidden the fact.

        So it asserts the quantity retrieval actually uses. Cosine similarity
        between the two must be 1 to within 1e-6, which is two orders below the
        0.02 density band and four below the smallest margin any measured
        question turned on."""
        import numpy

        def vector(payload: str) -> numpy.ndarray:
            return numpy.frombuffer(base64.b64decode(payload), dtype="<f4").astype(
                numpy.float64
            )

        texts = ["short one", " ".join(f"w{index}" for index in range(100))]
        batched = embedding.embed_windows(
            self.encoder, self.tokenizer, [window(text) for text in texts]
        )
        for text in texts:
            alone = embedding.embed_windows(self.encoder, self.tokenizer, [window(text)])
            digest = window(text)["text_sha256"]
            left, right = vector(batched[digest]), vector(alone[digest])
            cosine = float(
                left @ right / (numpy.linalg.norm(left) * numpy.linalg.norm(right))
            )
            self.assertAlmostEqual(cosine, 1.0, places=6)
            # Stated as a bound rather than an equality, because the exact
            # componentwise delta is a property of the MLX build.
            self.assertLess(float(numpy.abs(left - right).max()), 1e-4)


if __name__ == "__main__":
    unittest.main()
