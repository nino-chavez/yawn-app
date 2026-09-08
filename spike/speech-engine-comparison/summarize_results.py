#!/usr/bin/env python3
"""Produce a content-free receipt from private speech-comparison artifacts."""
import argparse
import hashlib
import json
from pathlib import Path

from scoring import local_span_survival, score, segment_records, tokens


def sha(path):
    with path.open('rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()


def read(path):
    return json.loads(path.read_text())


def compact_score(value):
    if isinstance(value, dict):
        return {k: compact_score(v) for k, v in value.items() if k != 'by_span'}
    if isinstance(value, list):
        return [compact_score(v) for v in value]
    return value


def summarize(root):
    manifest = read(root / 'corpus.json')
    reference_path = root / 'public/ami-reference-v2.json'
    reference = read(reference_path)
    private_path = root / 'private-reference-v2.json'
    private = read(private_path)
    result = {
        'schema': 'speech-comparison-public-receipt/1', 'date': '2026-09-07',
        'scope': 'local research; no app migration, human note acceptance, or ship-gate pass',
        'machine': {'chip': 'Apple M3 Max', 'ram_bytes': 38654705664, 'macos': '26.6.2', 'build': '25G83'},
        'manifest_sha256': sha(root / 'corpus.json'),
        'reference': {'sha256': sha(reference_path), 'manual_lexical_tokens': len(tokens(reference['reference_text'])),
                      'correction': 'v2 fixes NITE range parsing; original annotations unchanged; v1 span scores discarded',
                      'original_derived_reference_sha256': sha(root / 'public/ami-reference.json')},
        'private_reference': {'sha256': sha(private_path), 'kind': private['kind'],
                              'approval': private['approval'], 'original_lock_validated': private['original_lock_validated'],
                              'timing_used_fallback': private['timing_used_fallback'], 'counts': private['counts']},
        'native_model_identity': read(root / 'native-model-identity.json'),
        'silence_after_current_voicing_filter': read(root / 'silence-filter.json'),
        'engines': {},
    }
    for engine in ('whisper', 'apple', 'parakeet'):
        item = {'cases': {}}
        for case in manifest['cases']:
            path = root / (engine + '-run') / (case['id'] + '.' + engine + '.json')
            raw = read(path)
            attempt = read(path.with_suffix('.receipt.json'))
            assert attempt['result_sha256'] == sha(path)
            assert attempt['audio_sha256'] == sha(Path(case['audio']))
            assert attempt['status'] == 'ok' and attempt['source_unchanged']
            rows = []
            for run in raw['runs']:
                _, timing = segment_records(run, case['duration'])
                rows.append({k: run.get(k) for k in ('repeat', 'process_seconds', 'total_seconds', 'process_peak_rss_bytes', 'timing_granularity', 'timing_issue_count')} |
                            {'output_tokens': len(tokens(run['text'])), 'timing': timing})
            item['cases'][case['id']] = {
                'duration_seconds': case['duration'], 'audio_sha256': case['audio_sha256'],
                'result_sha256': sha(path), 'attempt_sha256': sha(path.with_suffix('.receipt.json')),
                'source_unchanged': attempt['source_unchanged'], 'setup_seconds': raw['setup_seconds'],
                'probe_sha256': attempt['probe_sha256'], 'runner_sha256': attempt['runner_sha256'],
                'whole_process_seconds_two_repeats': attempt['process_wall_seconds'], 'runs': rows,
            }
            if case['id'] == 'ami-full':
                item['ami_manual_diagnostics'] = compact_score(score(reference, raw, case['duration']))
        # Existing event text is a Whisper-derived draft reference, not ASR truth.
        replay = read(root / (engine + '-transcript-ledger') / 'receipt.json')
        transcript = read(Path(replay['transcript_path']))
        records, _ = segment_records({'segments': transcript['turns']}, 736.589375)
        spans = local_span_survival(private['important_spans'], records)
        best = {}
        for span, row in zip(private['important_spans'], spans['by_span']):
            fraction = row['survived_terms'] / row['terms'] if row['terms'] else 0
            best[span['event_id']] = max(best.get(span['event_id'], 0), fraction)
        item['private_event_proxy'] = {
            'events': len(best), 'mean_best_alternative_local_term_fraction': sum(best.values()) / len(best),
            'events_with_complete_lexical_alternative': sum(v == 1 for v in best.values()),
            'retained_transcript_turns': len(transcript['turns']),
            'transcript_sha256': sha(Path(replay['transcript_path'])),
            'limitation': 'draft Whisper-derived local token proxy after capture filters; not semantic recall or an approved gate',
        }
        note = read(root / (engine + '-notes-short') / 'receipt.json')
        item['short_note_replay'] = {k: note.get(k) for k in (
            'status', 'claims', 'refusal', 'source_unchanged', 'filter_seconds', 'note_setup_seconds',
            'note_generation_seconds', 'segmentation', 'transcript_id', 'generated_sha256', 'note_model',
            'runtime', 'network', 'source_code_hashes')}
        item['short_note_replay']['request_count'] = len(note.get('note_requests', []))
        item['short_note_replay']['receipt_sha256'] = sha(root / (engine + '-notes-short') / 'receipt.json')
        item['short_note_replay']['whole_process_seconds'] = read(root / (engine + '-notes-short.attempt.json'))['process_wall_seconds']
        result['engines'][engine] = item
    result['research_source_sha256'] = {str(p.relative_to(Path(__file__).parent)): sha(p)
        for p in sorted(Path(__file__).parent.rglob('*')) if p.is_file() and p.suffix in {'.py', '.swift', '.resolved'} and '.build' not in p.parts}
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    report = summarize(args.root)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2, allow_nan=False) + '\n')
    print('Wrote content-free research receipt.')
