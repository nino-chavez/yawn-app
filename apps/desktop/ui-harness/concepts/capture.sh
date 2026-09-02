#!/bin/zsh
# Renders every concept x state x theme at 1080x900 @2x into out/.
# Each frame: navigate, apply the viewport override to that tab, screenshot.
set -e
export BROWSE_SESSION=yawn-concept-capture
cd "$(dirname "$0")"
PORT=8791
preview . $PORT >/dev/null 2>&1 || true
sleep 1
for c in a b c; do for s in 1 2 3 4; do for t in dark light; do
  browse-nav "http://localhost:$PORT/$c/index.html?state=$s&theme=$t" >/dev/null
  browse-cdp Emulation.setDeviceMetricsOverride '{"width":1080,"height":900,"deviceScaleFactor":2,"mobile":false}' >/dev/null
  sleep 0.4
  browse-screenshot --out "out/$c-$s-$t.png" >/dev/null
  echo "out/$c-$s-$t.png"
done; done; done
