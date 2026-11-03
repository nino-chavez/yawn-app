"""Preview the source frontend with synthetic meetings; optionally recapture native WebKit frames."""
from pathlib import Path
import argparse, functools, hashlib, http.server, json, os, signal, subprocess, tempfile, threading, uuid
root = Path(__file__).resolve().parents[4]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--port', type=int, default=8876)
parser.add_argument('--out', type=Path, required=True, help='Persistent native capture directory')
parser.add_argument('--capture', action='store_true', help='Regenerate all 28 native content captures')
args = parser.parse_args()
out = args.out.resolve()
out.mkdir(parents=True, exist_ok=True)
port = args.port
nonce = str(uuid.uuid4())

def capture(binary):
    records = []
    for study in ['current', 'refined']:
        for appearance in ['light', 'dark']:
            for mode, probe in [('first-run', 'none'), ('capture', 'none'), ('ledger', 'none'), ('ledger', 'related'), ('ledger', 'expanded'), ('interrupted', 'none'), ('settings-retirement', 'none')]:
                settings = mode == 'settings-retirement'
                page = 'settings-retirement.html' if settings else 'harness.html'
                name = 'settings' if settings else mode
                name += '-' + probe if probe != 'none' else ''
                width, height = (580, 640) if settings else (1080, 900)
                dest = out / study
                dest.mkdir(exist_ok=True)
                url = f'http://127.0.0.1:{port}/{page}?mode={mode}&study={study}&probe={probe}&appearance={appearance}&width={width}&height={height}'
                env = {**os.environ, 'HARNESS_NONCE': nonce, 'HARNESS_CAPTURE_PATH': str(dest / f'{name}-{appearance}.png')}
                result = subprocess.run([str(binary), url, str(root / 'apps/desktop/ui-harness/revisit/capture-state.js')], stdin=subprocess.DEVNULL, timeout=65, capture_output=True, text=True, env=env)
                (dest / f'{name}-{appearance}.log').write_text(result.stdout + result.stderr)
                if result.returncode:
                    raise RuntimeError(f'{study} {name} {appearance} failed: ' + result.stdout[-1600:] + result.stderr[-500:])
                data = json.loads(result.stdout)
                assert data.get('pass'), data
                records.append({'study': study, 'mode': mode, 'probe': probe, 'appearance': appearance, 'viewport': {'width': width, 'height': height}, 'frame': f'{study}/{name}-{appearance}.png', 'result': data})
                print(study, name, appearance, 'PASS', flush=True)
    for appearance in ['light', 'dark']:
        for mode in ['capture', 'ledger', 'interrupted']:
            a = next((r for r in records if r['study'] == 'current' and r['mode'] == mode and (r['probe'] == 'none') and (r['appearance'] == appearance)))
            b = next((r for r in records if r['study'] == 'refined' and r['mode'] == mode and (r['probe'] == 'none') and (r['appearance'] == appearance)))
            assert a['result']['notes'] == b['result']['notes']
    sourceFiles = ['apps/desktop/ui/main.js', 'apps/desktop/ui/styles.css', 'apps/desktop/ui/tokens.css', 'apps/desktop/ui-harness/revisit/refined.css', 'apps/desktop/ui-harness/revisit/relationships.js', 'apps/desktop/ui-harness/revisit/runner.swift', 'apps/desktop/ui-harness/revisit/study-stub.js', 'apps/desktop/ui-harness/revisit/harness.html', 'apps/desktop/ui-harness/revisit/capture-state.js', 'apps/desktop/ui-harness/revisit/settings-retirement.html', 'apps/desktop/ui/settings.js', 'apps/desktop/ui/settings-window.js', 'apps/desktop/ui/settings-window.css', 'apps/desktop/ui/settings-standalone.css']
    manifest = {'renderer': 'Offscreen native WKWebView; same frontend, stubbed Tauri bridge; synthetic meetings', 'limits': 'Not installed-app/backend acceptance. Related/expanded native frames pose the relationship state explicitly because an offscreen window has no physical hover/keyboard focus; real interaction is tested separately in the browser.', 'sourceHead': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, stdin=subprocess.DEVNULL, timeout=10, text=True).strip(), 'sourceHashes': {p: hashlib.sha256((root / p).read_bytes()).hexdigest() for p in sourceFiles}, 'records': records}
    (out / 'native-results.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print('Completed', len(records), 'native captures', flush=True)

def stop(_signum, _frame):
    raise KeyboardInterrupt
signal.signal(signal.SIGTERM, stop)
with tempfile.TemporaryDirectory(prefix='yawn-review-') as scratch:
    serve = Path(scratch)
    for file in (root / 'apps/desktop/ui').iterdir():
        if file.suffix in ['.js', '.mjs', '.css', '.html'] and file.name != 'index.html' and (not file.name.endswith('.test.mjs')):
            (serve / file.name).symlink_to(file)
    harness = root / 'apps/desktop/ui-harness'
    for file in [harness / 'tauri-stub.js', harness / 'rail.contract.json', *[harness / 'revisit' / name for name in ['index.html', 'harness.html', 'settings-retirement.html', 'refined.css', 'relationships.js', 'study-stub.js']]]:
        (serve / file.name).symlink_to(file)
    (serve / 'captures').symlink_to(out, target_is_directory=True)
    (serve / 'harness-nonce.js').write_text('window.__harnessNonce = ' + json.dumps(nonce) + ';\n')
    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(serve))
    server = http.server.ThreadingHTTPServer(('127.0.0.1', port), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        if args.capture:
            binary = serve / 'native-runner'
            subprocess.run(['swiftc', '-O', str(harness / 'revisit/runner.swift'), '-o', str(binary)], stdin=subprocess.DEVNULL, timeout=120, check=True)
            capture(binary)
        manifest = json.loads((out / 'native-results.json').read_text())
        for name, digest in manifest['sourceHashes'].items():
            if hashlib.sha256((root / name).read_bytes()).hexdigest() != digest:
                raise RuntimeError('Native frames are stale; restart with --capture: ' + name)
        print(f'Yawn comparison ready: http://127.0.0.1:{port}/', flush=True)
        thread.join()
    except KeyboardInterrupt:
        pass
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)
