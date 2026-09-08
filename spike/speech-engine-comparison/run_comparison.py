#!/usr/bin/env python3
"""Sequential local replay with source hashes and bounded child processes.

All output belongs outside source/product trees. Raw engine text and logs are
owner-private. This measures engine replay, not packaged-app end-to-end time.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent


def digest(path: Path) -> str:
    with path.open('rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()


def run(manifest_path: Path, out: Path, engine: str, native: Path | None,
        python: Path, model: Path, catalog: Path, timeout: int, repeats: int,
        only_case: str | None) -> int:
    manifest = json.loads(manifest_path.read_text())
    if manifest.get('schema') != 'speech-engine-corpus/1':
        raise ValueError('unrecognized corpus manifest')
    out = out.resolve()
    forbidden = [HERE.parents[1], Path('/Applications'),
                 Path.home() / 'Library/Application Support/com.ninochavez.local-meeting-notes',
                 Path.home() / 'Library/Application Support/com.ninochavez.local-meeting-notes.preview']
    if any(out.is_relative_to(p) for p in forbidden):
        raise ValueError('results must be outside repository and product storage')
    os.umask(0o077)
    out.mkdir(parents=True, exist_ok=True, mode=0o700)
    failures = 0
    for case in manifest['cases']:
        if only_case and case['id'] != only_case:
            continue
        source = Path(case['audio'])
        if digest(source) != case['audio_sha256']:
            raise ValueError('source does not match frozen corpus')
        target = out / (case['id'] + '.' + engine + '.json')
        receipt = target.with_suffix('.receipt.json')
        if target.exists() or receipt.exists():
            raise ValueError(f'refusing to overwrite existing attempt: {target.name}')
        args = ['--audio', str(source), '--out', str(target), '--repeats', str(repeats)]
        if engine == 'whisper':
            command = [str(python), str(HERE / 'whisper_probe.py'), *args,
                       '--model', str(model), '--catalog', str(catalog)]
        else:
            if not native:
                raise ValueError('native probe executable required')
            command = [str(native), '--engine', engine, *args]
        environment = dict(os.environ, HF_HUB_OFFLINE='1', TRANSFORMERS_OFFLINE='1',
                           PYTHONUNBUFFERED='1')
        print(f"running {case['id']} {engine}", flush=True)
        started = time.monotonic()
        reason = None
        with target.with_suffix('.log').open('x') as log:
            try:
                process = subprocess.run(command, stdin=subprocess.DEVNULL,
                                         stdout=log, stderr=log, env=environment,
                                         timeout=timeout)
                code = process.returncode
            except subprocess.TimeoutExpired:
                code, reason = -1, 'timeout'
        elapsed = time.monotonic() - started
        unchanged = digest(source) == case['audio_sha256']
        success = code == 0 and unchanged and target.exists()
        if success:
            data = json.loads(target.read_text())
            success = data.get('status') == 'ok' and len(data.get('runs', [])) == repeats
        receipt.write_text(json.dumps({
            'schema': 'speech-engine-attempt/1', 'case': case['id'], 'engine': engine,
            'audio_sha256': case['audio_sha256'], 'source_unchanged': unchanged,
            'manifest_sha256': digest(manifest_path), 'status': 'ok' if success else 'error',
            'returncode': code, 'reason': reason, 'process_wall_seconds': elapsed,
            'result_sha256': digest(target) if target.exists() else None,
            'runner_sha256': digest(Path(__file__)),
            'probe_sha256': digest(HERE / 'whisper_probe.py' if engine == 'whisper' else native),
            'os': platform.platform(), 'python': sys.version.split()[0],
            'memory_caveat': 'process RSS excludes Apple speech service; not total engine memory',
        }, indent=2))
        print(f"{'ok' if success else 'error'} {case['id']} {engine} {elapsed:.2f}s", flush=True)
        failures += not success
    return int(bool(failures))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--engine', choices=['whisper', 'apple', 'parakeet'], required=True)
    parser.add_argument('--native', type=Path)
    parser.add_argument('--python', type=Path, required=True)
    parser.add_argument('--model', type=Path, required=True)
    parser.add_argument('--catalog', type=Path, required=True)
    parser.add_argument('--timeout', type=int, default=1200)
    parser.add_argument('--repeats', type=int, default=2)
    parser.add_argument('--case')
    args = parser.parse_args()
    return run(args.manifest, args.out, args.engine, args.native, args.python,
               args.model, args.catalog, args.timeout, args.repeats, args.case)


if __name__ == '__main__':
    raise SystemExit(main())
