from http.server import ThreadingHTTPServer, BaseHTTPRequestHandler
from pathlib import Path
from urllib.parse import urlparse
import mimetypes
import hashlib,json,subprocess
ROOT=Path(__file__).resolve().parents[4]
UI=ROOT/'apps/desktop/ui'
HARNESS=ROOT/'apps/desktop/ui-harness'
AUDIT=Path(__file__).resolve().parent
def provenance():
 files=[*UI.glob('*.js'),*UI.glob('*.mjs'),*UI.glob('*.css'),*UI.glob('*.html'),HARNESS/'tauri-stub.js',HARNESS/'harness.html',HARNESS/'runner.swift',UI/'review/harness.js',UI/'review/settings-harness.html',*AUDIT.glob('*.js'),*AUDIT.glob('*.py'),AUDIT/'cases.json']
 commit=subprocess.run(['git','-C',str(ROOT),'rev-parse','HEAD'],stdin=subprocess.DEVNULL,capture_output=True,text=True,check=True,timeout=5).stdout.strip()
 return {'synthetic':True,'sourceCommit':commit,'sha256':{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(set(files))}}

class Handler(BaseHTTPRequestHandler):
 def do_GET(self):
  name=urlparse(self.path).path.lstrip('/')
  if name=='layout-provenance.json':
   data=json.dumps(self.server.layout_provenance)
  elif name=='main.js':
   data=(UI/name).read_text().replace('void initialize();','await initialize();\nwindow.__layoutReview={state,render};\nawait import("/scenes.js").then(m=>m.applyScene(state,render));')
  elif name=='browser-harness.html':
   data=(UI/'index.html').read_text()
  elif name=='harness.html':
   data=(HARNESS/name).read_text().replace('<script src="tauri-stub.js">','<script>window.setInterval=()=>0;</script><script src="tauri-stub.js">')
  elif name=='settings-harness.html':
   data=(UI/'review/settings-harness.html').read_text().replace('../settings.html','/settings.html').replace('../settings.js','/settings.js').replace('../tokens.css','/tokens.css').replace('../settings-window.css','/settings-window.css').replace('<script src="harness.js"></script>','<script>window.setInterval=()=>0;</script><script src="harness.js"></script><script src="settings-scenes.js"></script>')
  else:
   choices=[AUDIT/name,UI/name,HARNESS/name,UI/'review'/name]
   p=next((p for p in choices if p.is_file() and '..' not in Path(name).parts),None)
   if not p:self.send_error(404);return
   data=p.read_bytes()
  if isinstance(data,str):data=data.encode()
  self.send_response(200)
  self.send_header('Content-Type',mimetypes.guess_type(name)[0] or 'application/octet-stream')
  self.send_header('Cache-Control','no-store')
  self.end_headers();self.wfile.write(data)
 def log_message(self,*args):pass
if __name__=='__main__':
 import sys
 server=ThreadingHTTPServer(('127.0.0.1',int(sys.argv[1]) if len(sys.argv)>1 else 8816),Handler)
 server.layout_provenance=provenance()
 server.serve_forever()
