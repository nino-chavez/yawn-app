#!/bin/zsh
set -euo pipefail

SCRIPT_DIR="${0:A:h}"
OUT_DIR="$SCRIPT_DIR/out"
PORT=8792
BASE_URL="http://127.0.0.1:$PORT"

mkdir -p "$OUT_DIR"
preview "$SCRIPT_DIR" "$PORT" >/dev/null
trap 'preview --stop 8792 >/dev/null 2>&1 || true' EXIT

export BROWSE_SESSION="yawn-visual-treatment-capture"
browse-start >/dev/null
browse-cdp Emulation.setDeviceMetricsOverride '{"width":1080,"height":900,"deviceScaleFactor":2,"mobile":false}' >/dev/null

for treatment in native-editorial private-notebook precision-utility; do
  for theme in dark light; do
    for state in sparse dense attention; do
      output="$OUT_DIR/${treatment}--${theme}--${state}.png"
      url="$BASE_URL/shared/frame.html?treatment=${treatment}&theme=${theme}&state=${state}"
      browse-shot "$url" --wait --out "$output" >/dev/null
      echo "$output"
    done
  done
done

echo "Comparison: $BASE_URL/compare.html"
