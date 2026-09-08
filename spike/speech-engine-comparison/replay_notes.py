#!/usr/bin/env python3
"""Replay existing capture filters and note validator against research ASR output.

Writes only under an explicit private research root. No app IPC, new approval,
or mutation of the existing audio, transcript, note, or evaluation ledger.
"""
from __future__ import annotations

import argparse
import contextlib
import ctypes
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import resource
import re
import sys
import time
import wave

ROOT = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(ROOT), str(ROOT / 'notes'), str(ROOT / 'spike')]


def sha(path: Path) -> str:
    with path.open('rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()


def deny_network() -> None:
    library = ctypes.CDLL('/usr/lib/libsandbox.dylib')
    library.sandbox_init.argtypes = [ctypes.c_char_p, ctypes.c_uint64,
                                    ctypes.POINTER(ctypes.c_char_p)]
    library.sandbox_init.restype = ctypes.c_int
    error = ctypes.c_char_p()
    if library.sandbox_init(b'no-network', 1, ctypes.byref(error)) != 0:
        raise RuntimeError('research process network sandbox failed')


def note_segments(result: dict) -> tuple[list[dict], str]:
    run = result['runs'][0]
    if run.get('utterances'):
        return run['utterances'], 'native final utterances'
    if result['engine'] != 'parakeet':
        return run['segments'], 'native decoded segments'
    # Yawn consumes utterances, whereas FluidAudio exposes words. This explicit
    # research adapter preserves native endpoints and never invents word times.
    groups, current = [], None
    for word in run['segments']:
        if current and (word['start'] - current['end'] > 1.0 or word['end'] - current['start'] > 15.0):
            groups.append(current)
            current = None
        if current is None:
            current = dict(word)
        else:
            current['text'] += ' ' + word['text']
            current['end'] = word['end']
        if re.search(r'[.!?][\"\u201d\u2019]*$', word['text']):
            groups.append(current)
            current = None
    if current:
        groups.append(current)
    return groups, 'research word grouping: sentence punctuation, gap >1s, span >15s; native endpoints'


def _read_json(path: Path, label: str) -> dict:
    if path.is_symlink() or not path.is_file():
        raise ValueError(f'{label} is missing or unsafe')
    try:
        value = json.loads(path.read_text())
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ValueError(f'{label} is unreadable ({exc})') from None
    if not isinstance(value, dict):
        raise ValueError(f'{label} must contain one JSON object')
    return value


def _wav_duration(path: Path) -> float:
    try:
        with wave.open(str(path), 'rb') as audio:
            if audio.getframerate() <= 0:
                raise ValueError('sample rate is invalid')
            return audio.getnframes() / audio.getframerate()
    except (EOFError, OSError, wave.Error, ValueError) as exc:
        raise ValueError(f'{path.name} duration is unreadable ({exc})') from None


def _bound_native_segments(
    result_path: Path,
    receipt_path: Path,
    audio_path: Path,
    *,
    engine: str,
    grouping: str = 'current',
) -> tuple[list[dict], str, list[dict]]:
    """Bind one native result to its exact audio before transcript creation."""
    from worker.speech_results import validated_native_segments

    receipt = _read_json(receipt_path, 'native attempt receipt')
    result = _read_json(result_path, 'native speech result')
    if receipt.get('schema') != 'speech-engine-attempt/1':
        raise ValueError('native attempt receipt has an unrecognized schema')
    if receipt.get('status') != 'ok' or receipt.get('source_unchanged') is not True:
        raise ValueError('native attempt receipt is not a successful unchanged run')
    if receipt.get('engine') != engine or result.get('engine') != engine:
        raise ValueError('native result and receipt engines must match the requested engine')
    if receipt.get('result_sha256') != sha(result_path):
        raise ValueError('native attempt receipt does not match its result bytes')
    if receipt.get('audio_sha256') != sha(audio_path):
        raise ValueError('native attempt receipt does not match its audio bytes')
    duration = _wav_duration(audio_path)
    current = validated_native_segments(result, engine=engine, duration_seconds=duration)
    if grouping == 'current':
        if engine == 'apple':
            return current, 'native final utterances', []
        return current, 'research word grouping: sentence punctuation, gap >1s, span >15s; native endpoints', []
    if grouping != 'bounded':
        raise ValueError(f'unsupported grouping {grouping!r}')

    # `validated_native_segments` above is the gate. Bounded grouping then
    # deliberately starts from the selected run's raw timed words, not Apple's
    # final utterances or Parakeet's current punctuation/gap grouping.
    from compare_segmentation import (
        _validate_derived_groups,
        bounded_groups,
        validated_segments,
    )

    raw_words, _ = validated_segments(result['runs'][0], duration)
    groups, exceptions = bounded_groups(raw_words)
    _validate_derived_groups(raw_words, groups, 'bounded_40_tokens_max_15s')
    turns = [
        {'text': group['text'], 'start': group['start'], 'end': group['end']}
        for group in groups
    ]
    return (
        turns,
        'research bounded grouping: 40 lexical tokens, span <=15s; raw validated timed words and native endpoints',
        exceptions,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--capture', type=Path, required=True)
    parser.add_argument('--engine', choices=['whisper', 'apple', 'parakeet'], required=True)
    parser.add_argument('--mic-result', type=Path, required=True)
    parser.add_argument('--system-result', type=Path, required=True)
    parser.add_argument('--mic-receipt', type=Path)
    parser.add_argument('--system-receipt', type=Path)
    parser.add_argument('--speech-model', type=Path)
    parser.add_argument('--grouping', choices=['current', 'bounded'], default='current')
    parser.add_argument('--note-model', type=Path, required=True)
    parser.add_argument('--catalog', type=Path, required=True)
    parser.add_argument('--generate-site-packages', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--transcript-only', action='store_true')
    args = parser.parse_args()
    if args.engine == 'whisper' and args.grouping == 'bounded':
        parser.error('--grouping bounded is only available for native replay')
    os.umask(0o077)
    out = args.out.resolve()
    protected = [ROOT, args.capture.resolve(), Path('/Applications'),
                 Path.home() / 'Library/Application Support/com.ninochavez.local-meeting-notes',
                 Path.home() / 'Library/Application Support/com.ninochavez.local-meeting-notes.preview']
    if any(out.is_relative_to(p) for p in protected):
        raise ValueError('replay output cannot be inside product or source storage')
    sources = [args.capture / name for name in ('mic.wav', 'system.wav', 'session.json')]
    from verify_capture import verify_acquisition
    verify_acquisition(args.capture)
    hashes = {str(p): sha(p) for p in sources}
    if args.engine == 'whisper':
        if args.speech_model is None:
            parser.error('--speech-model is required for whisper replay')
        results = [_read_json(p, 'speech result') for p in (args.mic_result, args.system_result)]
        if any(r.get('status') != 'ok' or not r.get('runs') or r.get('engine') != 'whisper' for r in results):
            raise ValueError('both successful Whisper results are required')
        adapted = [(segments, provenance, []) for segments, provenance in (note_segments(r) for r in results)]
    else:
        if args.mic_receipt is None or args.system_receipt is None:
            parser.error('--mic-receipt and --system-receipt are required for native replay')
        adapted = [
            _bound_native_segments(args.mic_result, args.mic_receipt, args.capture / 'mic.wav', engine=args.engine, grouping=args.grouping),
            _bound_native_segments(args.system_result, args.system_receipt, args.capture / 'system.wav', engine=args.engine, grouping=args.grouping),
        ]
    out.mkdir(parents=True, exist_ok=False, mode=0o700)
    segment_sets = [item[0] for item in adapted]
    turn_counts = [len(x) for x in segment_sets]
    from worker.transcription import create_transcript_revision, create_transcript_revision_from_segments
    started = time.monotonic()
    with contextlib.redirect_stdout(sys.stderr):
        if args.engine == 'whisper':
            remaining = iter(segment_sets)
            transcript_id, transcript_path = create_transcript_revision(
                args.capture, out / 'meetings/research/transcript', args.speech_model,
                transcribe_audio=lambda *_: next(remaining),
            )
        else:
            transcript_id, transcript_path = create_transcript_revision_from_segments(
                args.capture,
                out / 'meetings/research/transcript',
                mic_segments=segment_sets[0],
                system_segments=segment_sets[1],
            )
    filter_seconds = time.monotonic() - started
    result = {'schema': 'speech-note-replay/1', 'engine': args.engine, 'grouping': args.grouping,
              'speech_source_hashes': [sha(args.mic_result), sha(args.system_result)],
              'source_audio_hashes': list(hashes.values()),
              'input_native_turn_counts': turn_counts,
              'segmentation': [item[1] for item in adapted],
              'grouping_exceptions': [item[2] for item in adapted],
              'transcript_id': transcript_id, 'filter_seconds': filter_seconds,
              'transcript_path': str(transcript_path),
              'scope': 'research generation with current filters/validator; not packaged app or human acceptance',
              'gate_filter': 'no voice profile supplied; current voicing and bleed filters reused'}
    if args.engine != 'whisper':
        result['speech_attempt_receipt_hashes'] = [sha(args.mic_receipt), sha(args.system_receipt)]
    if not args.transcript_only:
        catalog = json.loads(args.catalog.read_text())
        entry = next(m for m in catalog['note_models'] if m['revision'] == args.note_model.name)
        for item in entry['files']:
            path = args.note_model / item['name']
            if path.stat().st_size != item['bytes'] or sha(path) != item['sha256']:
                raise ValueError('note model does not match installed catalog')
        sys.path.insert(0, str(args.generate_site_packages))
        os.environ.update(HF_HUB_OFFLINE='1', TRANSFORMERS_OFFLINE='1')
        deny_network()
        from worker import note_generator_mlx, note_validator
        setup = time.monotonic()
        session = note_generator_mlx._Session()
        session.resolve(str(args.note_model))
        setup_seconds = time.monotonic() - setup
        request_metrics = []
        def ask(request):
            call_start = time.monotonic()
            answer = note_generator_mlx._answer(session, {
                **request, 'model_directory': str(args.note_model)})
            request_metrics.append({'kind': request.get('schema', 'classification'),
                                    'seconds': time.monotonic() - call_start})
            (out / 'progress.json').write_text(json.dumps({'completed_calls': len(request_metrics),
                                                         'call_seconds': sum(r['seconds'] for r in request_metrics)}))
            return answer
        root_fd = os.open(out, os.O_RDONLY | os.O_DIRECTORY)
        generation_start = time.monotonic()
        try:
            generated = note_validator.generate(root_fd, {
                'meeting_id': 'research', 'transcript_id': transcript_id}, ask=ask)
            result['status'] = 'validated-note-output'
            (out / 'generated.json').write_text(json.dumps(generated, indent=2))
            result['claims'] = len(generated['claims'])
            result['generated_sha256'] = sha(out / 'generated.json')
        except (note_validator.GenerationRefused, note_validator.ArtifactFailure) as exc:
            result.update(status='transcript-only', refusal=exc.code)
        finally:
            os.close(root_fd)
        result.update(note_model={'id': entry['id'], 'revision': entry['revision']},
                      note_setup_seconds=setup_seconds,
                      note_generation_seconds=time.monotonic() - generation_start,
                      note_requests=request_metrics,
                      runtime={'mlx_lm': importlib.metadata.version('mlx-lm'),
                               'mlx': importlib.metadata.version('mlx')},
                      network='Seatbelt no-network profile applied before model load')
    else:
        result['status'] = 'transcript-replay-only'
    result['process_peak_rss_bytes'] = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    result['source_unchanged'] = all(sha(p) == hashes[str(p)] for p in sources)
    result['source_code_hashes'] = {name: sha(ROOT / name) for name in (
        'worker/transcription.py', 'worker/note_validator.py', 'worker/note_generator_mlx.py',
        'notes/candidate_first.py', 'notes/transcript.py', 'spike/dual_capture.py',
        'spike/speech-engine-comparison/replay_notes.py',
        'spike/speech-engine-comparison/compare_segmentation.py',
        'worker/speech_results.py')}
    (out / 'receipt.json').write_text(json.dumps(result, indent=2, allow_nan=False))
    print(json.dumps({k:result[k] for k in ('engine','status','source_unchanged')}))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
