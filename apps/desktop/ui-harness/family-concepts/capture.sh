#!/bin/zsh
# Renders concept x state x theme frames at 2x into an output directory.
# Mac states (1-5) at 1080x900, iPad states (6-9) at 1180x820 points.
#
#   ./capture.sh <outdir> [concepts="1 2 3"] [states="1 2 3 4 5 6 7 8 9"] [themes="light dark"]
#
# Each frame: navigate, apply the viewport override to that tab, then
# Page.captureScreenshot with a 2x clip (browse-screenshot returns 1x even
# under the override). The override does not survive a navigation, so it is
# re-applied per frame. Uses the session's own browse-tool tab; run
# `browse-tabs close-owned` when done.
set -e
OUT=${1:?outdir}
CONCEPTS=${2:-"1 2 3"}
STATES=${3:-"1 2 3 4 5 6 7 8 9"}
THEMES=${4:-"light dark"}
PORT=${PORT:-8793}
cd "$(dirname "$0")"
mkdir -p "$OUT"
preview . $PORT >/dev/null 2>&1 || true
sleep 1
TMPJ=$(mktemp -t yawn-capture).json
for c in ${=CONCEPTS}; do
  dir=$(ls -d ${c}-*/ | head -1); dir=${dir%/}
  for s in ${=STATES}; do
    if (( s <= 5 )); then page=mac; w=1080; h=900; else page=ipad; w=1180; h=820; fi
    for t in ${=THEMES}; do
      browse-nav "http://localhost:$PORT/$dir/$page.html?state=$s&theme=$t" >/dev/null
      browse-cdp Emulation.setDeviceMetricsOverride "{\"width\":$w,\"height\":$h,\"deviceScaleFactor\":2,\"mobile\":false}" >/dev/null
      sleep 0.5
      browse-cdp Page.captureScreenshot "{\"format\":\"png\",\"clip\":{\"x\":0,\"y\":0,\"width\":$w,\"height\":$h,\"scale\":2}}" > "$TMPJ"
      python3 -c "import json,base64,sys; d=json.load(open(sys.argv[1])); open(sys.argv[2],'wb').write(base64.b64decode(d.get('data') or d['result']['data']))" "$TMPJ" "$OUT/$c-$s-$t.png"
      echo "$OUT/$c-$s-$t.png"
    done
  done
done
rm -f "$TMPJ"
