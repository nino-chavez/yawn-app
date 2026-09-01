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
ln -s "$PWD/../ui/main.js" "$PWD/../ui/view-model.mjs" "$PWD/../ui/dom-patch.mjs" "$PWD/../ui/styles.css" "$serve/"
ln -s "$PWD/harness.html" "$PWD/tauri-stub.js" "$serve/"
python3 -m http.server "$port" --directory "$serve" --bind 127.0.0.1 >/dev/null 2>&1 &
server_pid=$!

swiftc -O runner.swift -o "$build_dir/runner"
for _ in $(seq 1 50); do
  curl -sf "http://127.0.0.1:$port/harness.html" >/dev/null && break
  sleep 0.1
done

run() { "$build_dir/runner" "http://127.0.0.1:$port/harness.html?mode=$1" "$PWD/$2"; }
case "$mode" in
  capture) run capture scenario.js ;;
  library) run library scenario.js ;;
  smoke) run library smoke.js ;;
  all)
    echo "== capture: operator-note undo across poll ticks =="
    run capture scenario.js
    echo "== library: transcript-search undo and focus per keystroke =="
    run library scenario.js
    echo "== smoke: interactive flows under the in-place patcher =="
    run library smoke.js
    ;;
  *) echo "usage: run.sh [capture|library|smoke|all]" >&2; exit 2 ;;
esac
