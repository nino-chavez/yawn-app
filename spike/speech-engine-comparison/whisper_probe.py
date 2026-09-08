#!/usr/bin/env python3
"""Research replay of the installed speech model; never writes product storage."""
from __future__ import annotations

import argparse
import contextlib
import hashlib
import importlib
import importlib.metadata
import json
import os
from pathlib import Path
import resource
import sys
import time


def digest(path: Path) -> str:
    with path.open('rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--audio', type=Path, required=True)
    parser.add_argument('--model', type=Path, required=True)
    parser.add_argument('--catalog', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--repeats', type=int, default=2)
    args = parser.parse_args()
    os.umask(0o077)
    os.environ.update(HF_HUB_OFFLINE='1', TRANSFORMERS_OFFLINE='1')
    repo = Path(__file__).resolve().parents[2]
    sys.path[:0] = [str(repo), str(repo / 'spike')]
    model = args.model.resolve(strict=True)
    catalog = json.loads(args.catalog.read_text())
    entry = next(m for m in catalog['models'] if m['revision'] == model.name)
    for item in entry['files']:
        path = model / item['name']
        if path.stat().st_size != item['bytes'] or digest(path) != item['sha256']:
            raise ValueError('installed model does not match catalog')
    source_hash = digest(args.audio)
    setup_start = time.monotonic()
    import mlx.core as mx
    from worker.transcription import _read_leg
    from dual_capture import transcribe
    holder = importlib.import_module('mlx_whisper.transcribe').ModelHolder
    holder.get_model(str(model), mx.float16)
    mx.eval(holder.model.parameters())
    setup_seconds = time.monotonic() - setup_start
    runs = []
    for repeat in range(args.repeats):
        mx.random.seed(0)
        started = time.monotonic()
        # File read is inside the decode clock, as for the native file adapters.
        audio = _read_leg(args.audio)
        with contextlib.redirect_stdout(sys.stderr):
            segments = transcribe(audio, str(model), 'en')
        elapsed = time.monotonic() - started
        runs.append({
            'repeat': repeat, 'process_seconds': elapsed,
            'total_seconds': elapsed + (setup_seconds if repeat == 0 else 0),
            'text': ' '.join(row['text'] for row in segments), 'segments': segments,
            'process_peak_rss_bytes': resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
            'mlx_peak_memory_bytes': mx.get_peak_memory(),
            'timing_granularity': 'Whisper decoded segment ranges; product defaults',
        })
    if digest(args.audio) != source_hash:
        raise ValueError('source audio changed during replay')
    result = {
        'schema': 'speech-engine-result/1', 'status': 'ok', 'engine': 'whisper',
        'engine_version': importlib.metadata.version('mlx-whisper'),
        'mlx_version': importlib.metadata.version('mlx'),
        'model_identity': {'id': entry['id'], 'revision': entry['revision'],
                           'files': [{k: f[k] for k in ('name', 'bytes', 'sha256')}
                                     for f in entry['files']]},
        'locale': 'en', 'setup_seconds': setup_seconds, 'runs': runs,
        'audio_sha256': source_hash,
        'decode': {'condition_on_previous_text': False, 'word_timestamps': False,
                   'random_seed': 0, 'other_options': 'installed mlx-whisper defaults'},
        'decode_source_sha256': digest(repo / 'spike/dual_capture.py'),
        'memory_scope': 'process highwater; MLX allocated peak reported separately',
    }
    with args.out.open('x') as handle:
        json.dump(result, handle, indent=2, allow_nan=False)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
