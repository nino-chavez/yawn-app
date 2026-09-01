"""Window text to vector, in the packaged runtime.

# What this is, and what it deliberately is not

It is a tokenizer, a batch loop, and a base64 encoder. **The forward pass is not
here.** `notes/mlx_minilm.py` holds it, was verified against the reference
implementation to 1e-6, and produced every number in
`notes/semantic_scale_receipt.json`. This module imports `MiniLMEncoder` and
`encode` from it rather than restating them, because a second forward pass would
be a second set of vectors that nothing measured — and it would look right.
`the_forward_pass_is_imported_and_not_restated` fails if that changes.

The one thing that does change from the measured setup is how text becomes token
IDs. The scale run used `transformers.AutoTokenizer`, which is not in the
packaged runtime; this uses the `tokenizers` wheel, which reads the same
`tokenizer.json` the pinned revision publishes. That substitution was measured
before it was written: `notes/packaged_tokenizer_receipt.json` records 0 token-ID
mismatches over 810 texts and 0 differences over 264 scored fields with only the
tokenizer swapped.

# Two silent defaults that have to be turned off

`tokenizer.json` at revision `1110a243fdf4706b3f48f1d95db1a4f5529b4d41` bakes in
**truncation at 128 tokens** and **fixed padding to 128**. Both are wrong here
and neither announces itself: truncation would cut a 128-word window roughly in
half and embed the remainder as if it were the whole thing, and fixed padding
would pad a nine-word question out to 128 positions. `transformers` overrides
both from `sentence_bert_config.json`; a bare `Tokenizer.from_file` does not.

`the_two_silent_defaults_are_off` pins both, because the failure is invisible —
the vectors would have the right shape and the wrong content.

# Why the caller supplies the text

Windowing lives in `crates/session-core/src/corpus_window.rs`, pinned to the
boundaries the measurement produced. Re-deriving windows here would be a third
implementation of the same cut. So this receives text and returns vectors, and
owns neither the segmentation nor the storage.
"""

from __future__ import annotations

import base64
import hashlib
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
if str(REPO / "notes") not in sys.path:
    sys.path.insert(0, str(REPO / "notes"))

VECTOR_DIMENSION = 384

# `MAX_FRAME_BYTES` is 64 KiB and one vector is 384 little-endian float32s —
# 1,536 bytes, 2,048 base64 characters, call it 2.2 KB with its JSON envelope.
# Thirty would fit and twenty-four leaves room for the digests beside them. This
# is a protocol limit, not a model one: a caller with more windows sends more
# requests.
MAX_WINDOWS_PER_REQUEST = 24


class EmbeddingRefused(ValueError):
    """The request cannot be answered as asked. Never a partial answer."""


class PackagedTokenizer:
    """The three `transformers` methods `mlx_minilm.encode` calls.

    Deliberately thin, and deliberately the same shape as the adapter in
    `notes/packaged_tokenizer_parity.py`. That file is frozen against a committed
    receipt, so the two are not merged into one import; what matters is that the
    configuration below is the configuration the receipt was produced with, and
    the test pins that rather than this sentence.
    """

    def __init__(self, model_directory: Path):
        from tokenizers import Tokenizer

        path = model_directory / "tokenizer.json"
        if not path.is_file():
            raise EmbeddingRefused("packaged tokenizer is missing")
        self._tokenizer = Tokenizer.from_file(str(path))
        self._tokenizer.no_truncation()
        self._tokenizer.enable_padding(pad_id=0, pad_token="[PAD]")

    @property
    def truncation(self):
        return self._tokenizer.truncation

    @property
    def padding(self):
        return self._tokenizer.padding

    def __call__(self, texts, padding=True, truncation=False, return_tensors=None):
        if not padding or truncation or return_tensors is not None:
            raise EmbeddingRefused("the forward pass calls this one way; anything else is unverified")
        encodings = self._tokenizer.encode_batch(texts)
        return {
            "input_ids": [encoding.ids for encoding in encodings],
            "attention_mask": [encoding.attention_mask for encoding in encodings],
        }

    def encode(self, text, add_special_tokens=True):
        return self._tokenizer.encode(text, add_special_tokens=add_special_tokens).ids


def load(model_directory: Path):
    """The encoder and tokenizer for one model directory.

    Loading is the expensive part, so a caller that embeds repeatedly holds the
    pair rather than calling this per request.
    """
    from mlx_minilm import MiniLMEncoder

    return MiniLMEncoder(model_directory), PackagedTokenizer(model_directory)


def embed_windows(encoder, tokenizer, windows: list[dict]) -> dict[str, str]:
    """Digest to vector, for every window, or nothing.

    Each window is `{"text_sha256": <64 hex>, "text": <str>}`. The digest is
    **verified against the text and then used as the key**, not recomputed and
    substituted: the store's binding check compares what it gets against what the
    window currently holds, so a worker that quietly digested whatever text
    arrived would turn a transport corruption into a vector the store accepts for
    the wrong words.

    A digest map, not a list, because that is the shape `worker-result/2` carries
    — `artifact_digests` is `dict[str, str]`, so this needs no protocol change.
    One consequence: two windows with identical text collapse to one entry. That
    is correct rather than lossy — identical text has an identical vector, and
    the caller keys by digest — but it means the reply may be shorter than the
    request, and `identical_windows_collapse_to_one_entry` pins it.

    Refuses rather than truncates. `mlx_minilm.encode` raises when an input
    exceeds the 256-token ceiling; a 128-word window should never reach it, which
    is exactly why a silent truncation here would go unnoticed.
    """
    if not isinstance(windows, list) or not windows:
        raise EmbeddingRefused("embedding request carries no windows")
    if len(windows) > MAX_WINDOWS_PER_REQUEST:
        raise EmbeddingRefused(
            f"embedding request exceeds {MAX_WINDOWS_PER_REQUEST} windows"
        )

    texts = []
    digests = []
    for window in windows:
        if not isinstance(window, dict) or set(window) != {"text_sha256", "text"}:
            raise EmbeddingRefused("window does not match the closed schema")
        text = window["text"]
        digest = window["text_sha256"]
        if not isinstance(text, str) or not isinstance(digest, str):
            raise EmbeddingRefused("window fields have the wrong type")
        if len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest):
            raise EmbeddingRefused("window digest is not a lowercase sha256")
        if hashlib.sha256(text.encode("utf-8")).hexdigest() != digest:
            raise EmbeddingRefused("window text does not match the digest sent with it")
        texts.append(text)
        digests.append(digest)

    # Imported after validation, not before it: a malformed request is refused
    # identically in every lane, including the boundary runtime that does not
    # package the mlx wheel. Importing first made five pure-validation tests
    # error with ModuleNotFoundError in the boundary lane's verify.
    from mlx_minilm import SequenceTooLong, encode

    try:
        vectors = encode(encoder, tokenizer, texts)
    except SequenceTooLong as exc:
        raise EmbeddingRefused(f"window exceeds the model window: {exc}") from None

    import numpy

    matrix = numpy.asarray(vectors)
    if matrix.shape != (len(texts), VECTOR_DIMENSION):
        raise EmbeddingRefused("the forward pass returned an unexpected shape")
    # Little-endian float32 — the byte layout `corpus_window_vector` stores, so
    # the store writes what the model produced rather than a re-encoding of it.
    rows = matrix.astype("<f4")
    return {
        digest: base64.b64encode(row.tobytes()).decode("ascii")
        for digest, row in zip(digests, rows)
    }
