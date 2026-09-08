#!/usr/bin/env bash
# Runs the WKWebView editor-preservation harness against apps/desktop/ui.
# See README.md in this directory for what each mode verifies.
set -euo pipefail
cd "$(dirname "$0")"

mode="${1:-all}"
port="${HARNESS_PORT:-8749}"
build_dir="$(mktemp -d)"
server_pid=""
cleanup() {
  if [ -n "$server_pid" ]; then
    { kill "$server_pid" && wait "$server_pid"; } 2>/dev/null || true
  fi
  rm -rf "$build_dir"
}
trap cleanup EXIT

# One origin serving the real UI files plus the harness page and stub, so the
# module imports in main.js load without file:// CORS trouble.
serve="$build_dir/serve"
mkdir "$serve"
ln -s "$PWD/../ui/main.js" "$PWD/../ui/view-model.mjs" "$PWD/../ui/dom-patch.mjs" "$PWD/../ui/styles.css" "$PWD/../ui/tokens.css" "$serve/"
ln -s "$PWD/harness.html" "$PWD/tauri-stub.js" "$PWD/tonal-ledger-canvas.contract.json" "$serve/"
python3 -m http.server "$port" --directory "$serve" --bind 127.0.0.1 >/dev/null 2>&1 &
server_pid=$!

swiftc -O runner.swift -o "$build_dir/runner"
for _ in $(seq 1 50); do
  curl -sf "http://127.0.0.1:$port/harness.html" >/dev/null && break
  sleep 0.1
done

run() { "$build_dir/runner" "http://127.0.0.1:$port/harness.html?mode=$1${3:-}" "$PWD/$2"; }
case "$mode" in
  capture) run capture scenario.js ;;
  library) run library scenario.js ;;
  search)
    echo "== search: cross-meeting exact-match focus and honest result states =="
    run search-results search.js
    echo "== search: capture blocks the probe affordance =="
    run search-capture search.js
    ;;
  smoke) run library smoke.js ;;
  sheets) run library sheets.js ;;
  fidelity) run fidelity fidelity.js "&width=1080&height=900" ;;
  all)
    echo "== capture: operator-note undo across poll ticks =="
    run capture scenario.js
    echo "== library: transcript-search undo and focus per keystroke =="
    run library scenario.js
    echo "== smoke: interactive flows under the in-place patcher =="
    run library smoke.js
    echo "== sheets: entrance animation fires once across repeated render ticks =="
    run library sheets.js
    ;;
  *) echo "usage: run.sh [capture|library|search|smoke|sheets|fidelity|all]" >&2; exit 2 ;;
esac
