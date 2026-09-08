"""Render palette studies through the existing native WebKit harness."""
import sys,json,threading,subprocess,os
from pathlib import Path
from http.server import ThreadingHTTPServer
from urllib.parse import urlparse,urlencode
from PIL import Image
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE.parent/'layout'))
from serve import Handler,ROOT,UI
OUT=HERE/'captures';OUT.mkdir(exist_ok=True)
class StudyHandler(Handler):
 def do_GET(self):
  name=urlparse(self.path).path.lstrip('/')
  if name=='main.js':data=(UI/name).read_text().replace('void initialize();','await initialize(); await import("/study.js").then(m=>m.apply(state,render));')
  elif name in ['study.js','palettes.json']:data=(HERE/name).read_text()
  elif name=='settings-scenes.js':data='import("/study.js").then(m=>m.color());'
  else:return super().do_GET()
  self.send_response(200);self.send_header('Content-Type','application/json' if name.endswith('.json') else 'text/javascript');self.end_headers();self.wfile.write(data.encode())
server=ThreadingHTTPServer(('127.0.0.1',0),StudyHandler)
threading.Thread(target=server.serve_forever,daemon=True).start()
runner=ROOT/'.artifacts/palette-study-runner'
runner.parent.mkdir(parents=True,exist_ok=True)
subprocess.run(['swiftc','-O',str(HERE.parent/'runner.swift'),'-o',str(runner)],stdin=subprocess.DEVNULL,check=True,timeout=60)
scenario=HERE/'capture.js'
try:
 for palette in ['current','lavender','graphite','blue']:
  for scene in ['meeting','settings','attention']:
   w,h=(720,720) if scene=='settings' else (1080,900)
   page='settings-harness' if scene=='settings' else 'harness'
   query=urlencode(dict(mode='library',scene=scene,palette=palette,width=w,height=h,appearance='dark'))
   dest=OUT/f'{palette}-{scene}.png'
   r=subprocess.run([str(runner),f'http://127.0.0.1:{server.server_port}/{page}.html?{query}',str(scenario)],env=dict(os.environ,HARNESS_CAPTURE_PATH=str(dest)),stdin=subprocess.DEVNULL,capture_output=True,text=True,timeout=55)
   if r.returncode:raise RuntimeError(r.stdout+r.stderr)
   with Image.open(dest) as im:im.resize((w,h)).save(dest)
   (OUT/f'{palette}-{scene}.json').write_text(r.stdout)
   print(palette,scene,flush=True)
finally:server.shutdown();server.server_close()
