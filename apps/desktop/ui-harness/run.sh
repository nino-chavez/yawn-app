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
ln -s "$PWD/../ui/main.js" "$PWD/../ui/view-model.mjs" "$PWD/../ui/dom-patch.mjs" "$PWD/../ui/styles.css" "$PWD/../ui/tokens.css" "$PWD/../ui/settings.js" "$PWD/../ui/settings.html" "$PWD/../ui/settings-window.css" "$serve/"
ln -s "$PWD/harness.html" "$PWD/tauri-stub.js" "$PWD/tonal-ledger-canvas.contract.json" "$PWD/settings-retirement.html" "$serve/"
python3 -m http.server "$port" --directory "$serve" --bind 127.0.0.1 >/dev/null 2>&1 &
server_pid=$!

swiftc -O runner.swift -o "$build_dir/runner"
for _ in $(seq 1 50); do
  curl -sf "http://127.0.0.1:$port/harness.html" >/dev/null && break
  sleep 0.1
done

# Each runner launch gets a fresh nonce served through the same HTTP cache as
# the UI files. The file is backdated so that any WebKit cache that survives
# between launches would treat the previous launch's copy as fresh (or get a
# 304 for it), and the runner refuses to run the scenario when the page's nonce
# does not match. This is what catches a runner that tests stale UI code.
nonce_file="$serve/harness-nonce.js"
stamp_nonce() {
  HARNESS_NONCE="$(uuidgen)"
  export HARNESS_NONCE
  printf 'window.__harnessNonce = "%s";\n' "$HARNESS_NONCE" > "$nonce_file"
  touch -t 202001010000 "$nonce_file"
}

run() {
  stamp_nonce
  if [ -n "${HARNESS_CAPTURE_DIR:-}" ]; then mkdir -p "$HARNESS_CAPTURE_DIR"; export HARNESS_CAPTURE_PATH="$HARNESS_CAPTURE_DIR/$1-${HARNESS_APPEARANCE:-dark}.png"; fi
  "$build_dir/runner" "http://127.0.0.1:$port/harness.html?mode=$1${3:-}" "$PWD/$2"; }
run_settings() {
  stamp_nonce
  if [ -n "${HARNESS_CAPTURE_DIR:-}" ]; then mkdir -p "$HARNESS_CAPTURE_DIR"; export HARNESS_CAPTURE_PATH="$HARNESS_CAPTURE_DIR/settings-${HARNESS_APPEARANCE:-dark}.png"; fi
  "$build_dir/runner" "http://127.0.0.1:$port/settings-retirement.html?mode=settings-retirement${2:-}" "$PWD/$1"; }
case "$mode" in
  capture) run capture scenario.js ;;
  stop-status) run stop-status scenario.js ;;
  library) run library scenario.js ;;
  search)
    echo "== search: cross-meeting exact-match focus and honest result states =="
    run search-results search.js
    echo "== search: capture blocks the probe affordance =="
    run search-capture search.js
    ;;
  smoke) run library smoke.js ;;
  sheets) run library sheets.js ;;
  fidelity) run fidelity fidelity.js "&width=1080&height=900&appearance=${HARNESS_APPEARANCE:-dark}" ;;
  note-retirement-negative) run transcript-retirement note-retirement.js "&width=1080&height=900&negative-control=1" ;;
  note-retirement)
    run transcript-retirement note-retirement.js "&width=1080&height=900&appearance=${HARNESS_APPEARANCE:-dark}"
    run saved-draft note-retirement.js "&width=1080&height=900&appearance=${HARNESS_APPEARANCE:-dark}"
    run summary-failed note-retirement.js "&width=1080&height=900&appearance=${HARNESS_APPEARANCE:-dark}"
    run saved-draft-unreadable note-retirement.js "&width=1080&height=900&appearance=${HARNESS_APPEARANCE:-dark}"
    run_settings settings-retirement.js "&width=1080&height=900&appearance=${HARNESS_APPEARANCE:-dark}"
    ;;
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
  *) echo "usage: run.sh [capture|stop-status|library|search|smoke|sheets|fidelity|note-retirement|all]" >&2; exit 2 ;;
esac
