#!/bin/bash
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
VENDOR="$REPO/apps/desktop/vendor"
STAGE="$REPO/apps/desktop/runtime"
DOWNLOADS="$VENDOR/downloads"
PYTHON_URL='https://github.com/astral-sh/python-build-standalone/releases/download/20260718/cpython-3.12.13%2B20260718-aarch64-apple-darwin-install_only.tar.gz'
PYTHON_SHA256='62aeee6161d57303a71a138b75fd5cc6fb8c89c4b1d9c7f0a052d89fa0b6652b'
WHISPER_REVISION='a4aaeec0636e6fef84abdcbe3544cb2bf7e9f6fb'
WHISPER_CONFIG_SHA256='b34fc29e4e11e0a25e812775dd67f4dd16fc2c8eb43d28ae25ff7d660ecb6379'
WHISPER_WEIGHTS_SHA256='951ed3fc1203e6a62467abb2144a96ce7eafca8fa77e3704fdb8635ff3e7f8a6'
WHISPER_DEFAULT="$HOME/.cache/huggingface/hub/models--mlx-community--whisper-large-v3-turbo/snapshots/$WHISPER_REVISION"
WHISPER_SOURCE="${LMN_WHISPER_MODEL_DIR:-$WHISPER_DEFAULT}"
Q4_REVISION='660c343bbf4e52ac257f0b7d952e5388e6f93bef'
Q4_BASE="https://huggingface.co/mlx-community/whisper-large-v3-turbo-q4/resolve/$Q4_REVISION"
Q4_DEFAULT="$VENDOR/downloads/whisper-large-v3-turbo-q4"
Q4_SOURCE="${YAWN_Q4_MODEL_DIR:-$Q4_DEFAULT}"
Q4_FILES=(config.json weights.npz)
Q4_SHA256=(
  '538e24557b8f9bc504700add5e7bbe32087c2353001ff563e64772ad4398671a'
  '862bbc832b05f3f4ec19dd632b701d61a6d3f5c7906360a10d72a79870642a80'
)
# The corpus embedding model, at the immutable revision every measurement in
# `notes/SEMANTIC_RETRIEVAL.md` was taken against. Downloaded if absent and
# digest-checked either way, like the CPython archive above — the pins were
# recorded from the Hugging Face metadata endpoint before the first download and
# have now been re-verified on three separate fetches.
#
# It stages in build-alpha rather than a mode of its own. build-alpha-encoder
# exists because the ECAPA encoder is a candidate under an admission check with
# alternatives; this model is chosen and measured, and what is unjudged about it
# — whether 7 of 10 is useful — is not a question a build mode can settle.
EMBEDDER_REVISION='1110a243fdf4706b3f48f1d95db1a4f5529b4d41'
EMBEDDER_BASE="https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/$EMBEDDER_REVISION"
EMBEDDER_DEFAULT="$VENDOR/downloads/all-MiniLM-L6-v2"
EMBEDDER_SOURCE="${LMN_EMBEDDER_MODEL_DIR:-$EMBEDDER_DEFAULT}"
EMBEDDER_STAGE_RELATIVE='models/all-MiniLM-L6-v2'
EMBEDDER_FILES=(config.json sentence_bert_config.json tokenizer.json model.safetensors)
EMBEDDER_SHA256=(
  '953f9c0d463486b10a6871cc2fd59f223b2c70184f49815e7efbcab5d8908b41'
  'fc1993fde0a95c24ec6c022539d41cf6e2f7c9721e5415d6fb6897472a9cd4b7'
  'be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037'
  '53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db'
)
# The converted ECAPA encoder is a deterministic export from the pinned
# checkpoint (spike/encoder-packaging/export_onnx.py); two independent exports
# reproduce this digest byte-identically. build-alpha-encoder packages it as a
# candidate for admission check 2 — packaging it admits nothing.
ENCODER_ONNX_SHA256='1d5e288b1037410fd0c98f618e94523a6b7ca8a99c7069f076efb40aa95759cd'
ENCODER_DEFAULT="$VENDOR/downloads/ecapa-tdnn.onnx"
ENCODER_SOURCE="${LMN_ENCODER_ONNX_SOURCE:-$ENCODER_DEFAULT}"
ENCODER_STAGE_RELATIVE='models/speaker-encoder/ecapa-tdnn.onnx'

if [[ "$(uname -s)-$(uname -m)" != "Darwin-arm64" ]]; then
  echo "boundary runtime build requires macOS arm64" >&2
  exit 1
fi

verify() {
  # Freshness first: a staging that predates its sources fails here with the
  # rebuild instruction rather than passing content checks that only prove the
  # staging is self-consistent, not current.
  python3 "$REPO/worker/source_digest.py" check "$STAGE"
  [[ -x "$STAGE/python-runtime/bin/python3.12" ]]
  [[ -x "$STAGE/bin/audiotee" ]]
  [[ -x "$STAGE/bin/permission-probe" ]]
  [[ -f "$STAGE/app-runtime.json" ]]
  [[ -f "$STAGE/note-runtime-project.json" ]]
  [[ -f "$STAGE/note-runtime-generate.json" ]]
  [[ -f "$STAGE/note-bridge.py" ]]
  [[ -f "$STAGE/note-generator-mlx.py" ]]
  [[ -f "$STAGE/note-validator.zip" ]]
  PYTHONPATH="$REPO" python3 -c \
    'import sys; from pathlib import Path; from worker.build_manifest import verify_note_runtime; verify_note_runtime(Path(sys.argv[1]))' \
    "$STAGE"
  (cd "$STAGE" && "$STAGE/python-runtime/bin/python3.12" -E -s -B -c \
    'import json, numpy; import worker.main; doc=json.load(open("app-runtime.json")); print(doc["admission"], numpy.__version__)' \
    1>/dev/null)
  # The note.create assembler's import closure, resolved the way the worker
  # resolves it (worker.adapters puts the staged notes/ and spike/ on the path).
  # A module missing here is refused at the user's Generate press otherwise.
  (cd "$STAGE" && "$STAGE/python-runtime/bin/python3.12" -E -s -B -c \
    'import worker.adapters; import candidate_first, summarize, transcript, capture_health' 1>/dev/null)
  local admission
  admission="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["admission"])' "$STAGE/app-runtime.json")"
  if [[ "$admission" == "internal-alpha" ]]; then
    [[ -x "$STAGE/bin/meeting-capture" ]]
    local schema
    schema="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["schema"])' "$STAGE/app-runtime.json")"
    if [[ "$schema" == "app-runtime/2" ]]; then
      [[ -f "$STAGE/model-catalog.json" ]]
      [[ ! -e "$STAGE/models/whisper-large-v3-turbo" ]]
      PYTHONPATH="$REPO" python3 -c \
        'import json,sys; from pathlib import Path; from worker.build_manifest import model_catalog; actual=json.load(open(Path(sys.argv[1]) / "model-catalog.json")); assert actual == model_catalog()' \
        "$STAGE"
    else
      echo "$WHISPER_CONFIG_SHA256  $STAGE/models/whisper-large-v3-turbo/config.json" | shasum -a 256 -c - >/dev/null
      echo "$WHISPER_WEIGHTS_SHA256  $STAGE/models/whisper-large-v3-turbo/weights.safetensors" | shasum -a 256 -c - >/dev/null
    fi
    (cd "$STAGE" && "$STAGE/python-runtime/bin/python3.12" -E -s -B -c \
      'import mlx.core, mlx_whisper, worker.transcription' 1>/dev/null)
    # Verify the isolated generate-site-packages tree with mlx_lm accessible.
    # Replicates the bootstrap pattern from note_bridge.py _GENERATOR_BOOTSTRAP.
    (cd "$STAGE" && "$STAGE/python-runtime/bin/python3.12" -E -s -B -c \
      'import sys, os; sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(sys.executable)), "lib", "python%d.%d" % sys.version_info[:2], "generate-site-packages")); import mlx_lm' 1>/dev/null)
    # Verify the shared tree still has mlx 0.29.3 and can import mlx_whisper independently.
    # Without the isolated dir, the shared mlx is used.
    (cd "$STAGE" && "$STAGE/python-runtime/bin/python3.12" -E -s -B -c \
      'from importlib.metadata import version; v = version("mlx"); assert v.startswith("0.29.3"), f"shared mlx must stay 0.29.3, got {v}"; import mlx.core, mlx_whisper' 1>/dev/null)
    for index in "${!EMBEDDER_FILES[@]}"; do
      echo "${EMBEDDER_SHA256[$index]}  $STAGE/$EMBEDDER_STAGE_RELATIVE/${EMBEDDER_FILES[$index]}" \
        | shasum -a 256 -c - >/dev/null
    done
    (cd "$STAGE" && "$STAGE/python-runtime/bin/python3.12" -E -s -B -c \
      'import tokenizers, worker.embedding' 1>/dev/null)
    # Named, not defaulted. Without it the five model-backed embedding tests skip
    # silently, and a suite that skips the only tests touching the model would
    # report the same green as one that ran them.
    LMN_PACKAGED_RUNTIME_ROOT="$STAGE" \
      LMN_TAP_TEST_BINARY="$STAGE/bin/audiotee" \
      LMN_MEETING_CAPTURE_TEST_BINARY="$STAGE/bin/meeting-capture" \
      LMN_EMBEDDING_MODEL_DIR="$STAGE/$EMBEDDER_STAGE_RELATIVE" \
      LMN_TRANSCRIPT_MODEL_DIR="$Q4_SOURCE" \
      "$STAGE/python-runtime/bin/python3.12" -E -s -B -m unittest discover \
        -s "$REPO/worker/tests" -v
  else
    LMN_PACKAGED_RUNTIME_ROOT="$STAGE" \
      LMN_TAP_TEST_BINARY="$STAGE/bin/audiotee" \
      "$STAGE/python-runtime/bin/python3.12" -E -s -B -m unittest discover \
        -s "$REPO/worker/tests" -v
  fi
  local encoder_path
  encoder_path="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["encoder"]["path"])' "$STAGE/app-runtime.json")"
  if [[ "$encoder_path" != "encoder-unavailable.identity" ]]; then
    echo "$ENCODER_ONNX_SHA256  $STAGE/$encoder_path" | shasum -a 256 -c - >/dev/null
    (cd "$STAGE" && "$STAGE/python-runtime/bin/python3.12" -E -s -B -c "
import numpy
import onnxruntime
from worker.fbank import fbank_features
session = onnxruntime.InferenceSession('$encoder_path', providers=['CPUExecutionProvider'])
features = fbank_features(numpy.zeros(16000, dtype=numpy.float32))
embedding = session.run(None, {'features': features[numpy.newaxis, ...],
                               'lengths': numpy.ones(1, dtype=numpy.float32)})[0]
assert embedding.shape[-1] == 192, embedding.shape
" 1>/dev/null)
  fi
}

mode="${1:-build}"
case "$mode" in
  verify)
    verify
    exit 0
    ;;
  build|build-alpha|build-alpha-encoder|build-alpha-external) ;;
  *)
    echo "usage: worker/build_runtime.sh [build|build-alpha|build-alpha-encoder|build-alpha-external|verify]" >&2
    exit 64
    ;;
esac

mkdir -p "$DOWNLOADS"
if [[ "$mode" == "build-alpha-external" ]]; then
  mkdir -p "$Q4_SOURCE"
  for index in "${!Q4_FILES[@]}"; do
    target="$Q4_SOURCE/${Q4_FILES[$index]}"
    if [[ ! -f "$target" ]] || ! echo "${Q4_SHA256[$index]}  $target" | shasum -a 256 -c - >/dev/null 2>&1; then
      curl -fL --max-time 1200 -o "$target" "$Q4_BASE/${Q4_FILES[$index]}"
    fi
    echo "${Q4_SHA256[$index]}  $target" | shasum -a 256 -c - >/dev/null
  done
fi
ARCHIVE="$DOWNLOADS/cpython-3.12.13-arm64.tar.gz"
if [[ ! -f "$ARCHIVE" ]] || ! echo "$PYTHON_SHA256  $ARCHIVE" | shasum -a 256 -c - >/dev/null 2>&1; then
  curl -fL --max-time 300 -o "$ARCHIVE" "$PYTHON_URL"
fi
echo "$PYTHON_SHA256  $ARCHIVE" | shasum -a 256 -c -

rm -rf "$VENDOR/python-runtime" "$STAGE"
mkdir -p "$STAGE/bin" "$STAGE/worker" "$STAGE/spike" "$STAGE/notes" "$STAGE/models"
tar -xzf "$ARCHIVE" -C "$VENDOR"
mv "$VENDOR/python" "$VENDOR/python-runtime"
if [[ "$mode" == build-alpha* ]]; then
  "$VENDOR/python-runtime/bin/python3" -m pip install --quiet --require-hashes \
    --only-binary=:all: \
    -r "$REPO/worker/requirements-alpha.lock"
  "$VENDOR/python-runtime/bin/python3" -m pip install --quiet --require-hashes \
    --only-binary=:all: --no-deps \
    -r "$REPO/worker/requirements-mlx-whisper.lock"
  "$VENDOR/python-runtime/bin/python3" -m pip install --quiet --require-hashes \
    --only-binary=:all: --no-deps \
    -r "$REPO/worker/requirements-embedder.lock"
  if [[ "$mode" == "build-alpha-encoder" ]]; then
    "$VENDOR/python-runtime/bin/python3" -m pip install --quiet --require-hashes \
      --only-binary=:all: --no-deps \
      -r "$REPO/worker/requirements-encoder.lock"
  fi
else
  "$VENDOR/python-runtime/bin/python3" -m pip install --quiet --require-hashes \
    --only-binary=:all: \
    -r "$REPO/worker/requirements-runtime.lock"
fi

cp -R "$VENDOR/python-runtime" "$STAGE/python-runtime"
# Stage the isolated mlx-lm tree after copy, separate from the shared site-packages so
# only the generate role imports it (via generate-site-packages on sys.path).
# This keeps mlx_whisper using the shared mlx==0.29.3 unchanged.
if [[ "$mode" == build-alpha* ]]; then
  mkdir -p "$STAGE/python-runtime/lib/python3.12/generate-site-packages"
  "$VENDOR/python-runtime/bin/python3" -m pip install --quiet --require-hashes \
    --only-binary=:all: \
    --target "$STAGE/python-runtime/lib/python3.12/generate-site-packages" \
    -r "$REPO/worker/requirements-generate.lock"
fi
cp "$REPO/worker/__init__.py" "$REPO/worker/main.py" \
  "$REPO/worker/adapters.py" "$REPO/worker/product_contracts.py" \
  "$REPO/worker/storage.py" "$REPO/worker/fbank.py" \
  "$REPO/worker/transcription.py" "$REPO/worker/embedding.py" "$STAGE/worker/"
cp "$REPO/worker/note_bridge.py" "$STAGE/note-bridge.py"
cp "$REPO/worker/note_generator_mlx.py" "$STAGE/note-generator-mlx.py"
cp "$REPO/spike/verify_capture.py" "$REPO/spike/capture_health.py" \
  "$REPO/spike/dual_capture.py" "$REPO/spike/speaker_gate.py" \
  "$REPO/spike/aec_bound.py" "$STAGE/spike/"
# mlx_minilm.py holds the forward pass every measured vector came from.
# `worker/embedding.py` imports it rather than restating it, so it is staged
# beside the other notes modules and not copied into the worker package.
# candidate_first.py is the assembler's registered product contract:
# `worker/adapters.py` imports it under note.create to turn the generate
# child's kept claims into a note document. Without it every generated note
# is refused as a protocol failure after the model has already run.
cp "$REPO/notes/transcript.py" "$REPO/notes/summarize.py" \
  "$REPO/notes/mlx_minilm.py" "$REPO/notes/candidate_first.py" "$STAGE/notes/"

swift build -c release --product audiotee --package-path "$REPO/capture/audiotee"
cp "$REPO/capture/audiotee/.build/arm64-apple-macosx/release/audiotee" \
  "$STAGE/bin/audiotee"
chmod 0755 "$STAGE/bin/audiotee"

# The fallback requester is staged in every mode. Internal-alpha uses
# meeting-capture's exact preflight path so it clears the executable that records;
# boundary and product admissions still need this smaller helper for microphone
# status because they do not package meeting-capture.
swift build -c release --product permission-probe \
  --package-path "$REPO/capture/permission-probe"
cp "$REPO/capture/permission-probe/.build/arm64-apple-macosx/release/permission-probe" \
  "$STAGE/bin/permission-probe"
chmod 0755 "$STAGE/bin/permission-probe"
if [[ "$mode" == build-alpha* ]]; then
  if [[ "$mode" != "build-alpha-external" ]]; then
    [[ -f "$WHISPER_SOURCE/config.json" && -f "$WHISPER_SOURCE/weights.safetensors" ]] || {
      echo "fixed Whisper snapshot $WHISPER_REVISION is unavailable" >&2
      exit 1
    }
    echo "$WHISPER_CONFIG_SHA256  $WHISPER_SOURCE/config.json" | shasum -a 256 -c -
    echo "$WHISPER_WEIGHTS_SHA256  $WHISPER_SOURCE/weights.safetensors" | shasum -a 256 -c -
    mkdir -p "$STAGE/models/whisper-large-v3-turbo"
    cp -L "$WHISPER_SOURCE/config.json" "$WHISPER_SOURCE/weights.safetensors" \
      "$STAGE/models/whisper-large-v3-turbo/"
  fi
  mkdir -p "$EMBEDDER_SOURCE"
  for index in "${!EMBEDDER_FILES[@]}"; do
    name="${EMBEDDER_FILES[$index]}"
    want="${EMBEDDER_SHA256[$index]}"
    if [[ ! -f "$EMBEDDER_SOURCE/$name" ]] \
      || ! echo "$want  $EMBEDDER_SOURCE/$name" | shasum -a 256 -c - >/dev/null 2>&1; then
      curl -fL --max-time 600 -o "$EMBEDDER_SOURCE/$name" "$EMBEDDER_BASE/$name"
    fi
    echo "$want  $EMBEDDER_SOURCE/$name" | shasum -a 256 -c -
  done
  mkdir -p "$STAGE/$EMBEDDER_STAGE_RELATIVE"
  for name in "${EMBEDDER_FILES[@]}"; do
    cp -L "$EMBEDDER_SOURCE/$name" "$STAGE/$EMBEDDER_STAGE_RELATIVE/"
  done
  swift build -c release --product meeting-capture --package-path "$REPO/capture/audiotee"
  cp "$REPO/capture/audiotee/.build/arm64-apple-macosx/release/meeting-capture" \
    "$STAGE/bin/meeting-capture"
  chmod 0755 "$STAGE/bin/meeting-capture"
fi
if [[ "$mode" == "build-alpha-encoder" ]]; then
  [[ -f "$ENCODER_SOURCE" ]] || {
    echo "converted ECAPA encoder is unavailable at $ENCODER_SOURCE" >&2
    echo "produce it: .venv/bin/python spike/encoder-packaging/export_onnx.py ~/.cache/speaker-gate <path>" >&2
    exit 1
  }
  echo "$ENCODER_ONNX_SHA256  $ENCODER_SOURCE" | shasum -a 256 -c -
  mkdir -p "$STAGE/models/speaker-encoder"
  cp -L "$ENCODER_SOURCE" "$STAGE/$ENCODER_STAGE_RELATIVE"
fi
# The placeholder identity file is a fixed bundle resource in every mode; the
# manifest's encoder entry, not this file, is what every consumer reads.
printf '%s\n' 'phase-2-boundary-no-encoder-model' > "$STAGE/encoder-unavailable.identity"
if [[ "$mode" == "build-alpha-encoder" ]]; then
  python3 "$REPO/worker/build_manifest.py" "$STAGE" --admission internal-alpha \
    --encoder "$ENCODER_STAGE_RELATIVE"
elif [[ "$mode" == "build-alpha-external" ]]; then
  python3 "$REPO/worker/build_manifest.py" "$STAGE" --admission internal-alpha \
    --external-transcript-models
elif [[ "$mode" == "build-alpha" ]]; then
  python3 "$REPO/worker/build_manifest.py" "$STAGE" --admission internal-alpha
else
  python3 "$REPO/worker/build_manifest.py" "$STAGE"
fi
python3 "$REPO/worker/source_digest.py" stamp "$STAGE"

verify
