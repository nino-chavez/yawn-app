#!/bin/zsh
set -euo pipefail

SCRIPT_DIR="${0:A:h}"
OUT_DIR="$SCRIPT_DIR/out"
PORT="${PORT:-8792}"
BASE_URL="http://127.0.0.1:$PORT"

mkdir -p "$OUT_DIR"
preview "$SCRIPT_DIR" "$PORT" >/dev/null
trap 'preview --stop 8792 >/dev/null 2>&1 || true' EXIT

export BROWSE_SESSION="yawn-visual-treatment-capture"
if ! start_output="$(browse-start 2>&1)"; then
  existing_port="$(printf '%s\n' "$start_output" | sed -n 's/.*BROWSE_PORT=\([0-9][0-9]*\).*/\1/p' | tail -1)"
  if [[ -z "$existing_port" ]]; then
    printf '%s\n' "$start_output" >&2
    exit 1
  fi
  export BROWSE_PORT="$existing_port"
fi

for treatment in tonal-ledger-refined marquee-document-refined tonal-ledger marquee-document; do
  for theme in dark light; do
  for state in sparse no-note dense attention; do
      output="$OUT_DIR/${treatment}--${theme}--${state}.png"
      url="$BASE_URL/shared/frame.html?treatment=${treatment}&theme=${theme}&state=${state}"
      browse-nav "$url" --wait >/dev/null
      browse-cdp Emulation.setDeviceMetricsOverride '{"width":1080,"height":900,"deviceScaleFactor":1,"mobile":false}' >/dev/null
      browse-screenshot --out "$output" >/dev/null
      echo "$output"
    done
  done
done

echo "Comparison: $BASE_URL/compare.html"
