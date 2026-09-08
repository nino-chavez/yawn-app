#!/usr/bin/env python3
"""Capture production UI in native WebKit with synthetic, render-only scenes.

python3 apps/desktop/ui-harness/layout/run.py [--scenes transcript,trash]
Requires macOS, Swift, and Pillow (used only to normalize Retina screenshots).
Does not invoke the app backend or read product storage.
"""
import argparse,json,os,subprocess,threading
from pathlib import Path
from urllib.parse import urlencode
from http.server import ThreadingHTTPServer
from PIL import Image
from serve import Handler,ROOT,provenance

HERE=Path(__file__).resolve().parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--scenes',help='Comma-separated scene filter')
parser.add_argument('--output',type=Path,default=ROOT/'.artifacts/layout-review')
args=parser.parse_args()
args.output=args.output.resolve();args.output.mkdir(parents=True,exist_ok=True)
runner=args.output/'runner'
subprocess.run(['swiftc','-O',str(HERE.parent/'runner.swift'),'-o',str(runner)],stdin=subprocess.DEVNULL,check=True,timeout=60)
server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
server.layout_provenance=provenance()
threading.Thread(target=server.serve_forever,daemon=True).start()
results=[]
try:
 for case in json.loads((HERE/'cases.json').read_text()):
  if args.scenes and case['scene'] not in args.scenes.split(','):continue
  name=f"{case['scene']}-{case['appearance']}-{case['width']}x{case['height']}"+('-'+case['section'] if 'section' in case else '')+('-bottom' if case.get('bottom') else '')
  page='settings-harness' if case['scene'].startswith('settings') else 'browser-harness' if case['scene']=='browser-notice' else 'harness'
  url=f'http://127.0.0.1:{server.server_port}/{page}.html?{urlencode(case)}'
  env=dict(os.environ,HARNESS_CAPTURE_PATH=str(args.output/(name+'.png')))
  completed=subprocess.run([str(runner),url,str(HERE/'native-scene.js')],env=env,stdin=subprocess.DEVNULL,capture_output=True,text=True,timeout=55)
  try:receipt=json.loads(completed.stdout)
  except ValueError:receipt={'error':completed.stdout+completed.stderr}
  receipt.update(case=case,exit=completed.returncode)
  (args.output/(name+'.json')).write_text(json.dumps(receipt,indent=2)+'\n')
  if completed.returncode==0:
   with Image.open(args.output/(name+'.png')) as frame:
    frame.resize((case['width'],case['height'])).save(args.output/(name+'.png'))
  results.append({'name':name,'exit':completed.returncode})
  print(name,completed.returncode,flush=True)
finally:
 server.shutdown();server.server_close()
 (args.output/'results.json').write_text(json.dumps(results,indent=2)+'\n')
raise SystemExit(1 if any(row['exit'] for row in results) else 0)
