#!/usr/bin/env bash
# Capture primitives for the installed `Yawn Preview.app`.
#
# This is the reusable half of a capture pass: launching, appearance, finding
# the window, shooting a named frame, and proving the frame is real. The
# navigation between states is per-pass and belongs in a driver script that
# sources this one -- encoding "click the second row, then Manage" here would
# rot against every UI change, which is why earlier passes each rebuilt their
# own driver and lost it.
#
#   source scripts/capture-installed-screens.sh
#   cap_require_operator_away          # refuses without CAPTURE_OPERATOR_AWAY=1
#   cap_launch
#   cap_appearance dark
#   cap_frame 01-dark-transcript-only
#   cap_appearance reset
#
# THE RULE THIS FILE EXISTS TO ENFORCE, in `cap_require_operator_away`: this
# script drives the GUI with synthetic events. Twice on 2026-09-02 a run fired
# while the operator was using the Mac and its clicks landed in his browser.
# It runs only on an explicit statement that he is away from the machine, never
# on "the screen is unlocked".
set -uo pipefail

CAP_BUNDLE_ID="${CAP_BUNDLE_ID:-com.ninochavez.local-meeting-notes.preview}"
# `${BASH_SOURCE[0]:-$0}` rather than `${BASH_SOURCE[0]}`: sourcing this from
# zsh (the interactive shell here) leaves BASH_SOURCE unset, and under `set -u`
# that aborts with a parameter error that reads like a broken script.
CAP_SELF="${BASH_SOURCE[0]:-$0}"
CAP_APP="${CAP_APP:-$(cd "$(dirname "$CAP_SELF")/.." && pwd)/target/release/bundle/macos/Yawn Preview.app}"
CAP_OUT="${CAP_OUT:-$PWD/captures}"
CAP_LOG="${CAP_LOG:-$CAP_OUT/capture.log}"
# The window's default size, used only when accessibility cannot report bounds.
CAP_FALLBACK_BOUNDS="${CAP_FALLBACK_BOUNDS:-0 0 1080 900}"

cap_log() { printf '%s %s\n' "$(date +%H:%M:%S)" "$*" | tee -a "$CAP_LOG" >&2; }

cap_require_operator_away() {
  if [ "${CAPTURE_OPERATOR_AWAY:-}" != "1" ]; then
    cat >&2 <<'MSG'
refusing to run: set CAPTURE_OPERATOR_AWAY=1 only when the operator has said
he is away from the Mac. This script sends synthetic clicks and keystrokes to
whatever is frontmost. An unlocked screen is not the same as an empty chair --
that mistake has already sent clicks into his browser twice.
MSG
    return 2
  fi
  mkdir -p "$CAP_OUT"
}

# System Events names this process "Yawn Preview" right after launch and
# "local-meeting-notes-desktop" once it has settled, and which one answers is
# a race. Ask for both rather than picking one and calling a miss "no window".
cap_process_name() {
  local name
  for name in "Yawn Preview" "local-meeting-notes-desktop"; do
    if osascript -e "tell application \"System Events\" to exists process \"$name\"" 2>/dev/null | grep -q true; then
      printf '%s' "$name"
      return 0
    fi
  done
  return 1
}

cap_launch() {
  [ -d "$CAP_APP" ] || { cap_log "FATAL no bundle at $CAP_APP"; return 1; }
  open -a "$CAP_APP"
  local waited=0 name
  while [ "$waited" -lt 40 ]; do
    if name=$(cap_process_name); then
      cap_log "process is \"$name\" after ${waited}s"
      return 0
    fi
    sleep 1
    waited=$((waited + 1))
  done
  cap_log "FATAL no process after ${waited}s"
  return 1
}

# Appearance is set on the app's own defaults domain, not system-wide, so the
# operator's desktop is never switched out from under him. `reset` removes the
# key rather than writing Dark back, so the app returns to following the system.
cap_appearance() {
  case "$1" in
    light) defaults write "$CAP_BUNDLE_ID" AppleInterfaceStyle -string Light ;;
    dark)  defaults delete "$CAP_BUNDLE_ID" AppleInterfaceStyle 2>/dev/null || true ;;
    reset) defaults delete "$CAP_BUNDLE_ID" AppleInterfaceStyle 2>/dev/null || true ;;
    *) cap_log "FATAL unknown appearance $1"; return 1 ;;
  esac
  cap_log "appearance $1"
}

cap_bounds() {
  local name bounds
  name=$(cap_process_name) || { printf '%s' "$CAP_FALLBACK_BOUNDS"; return 0; }
  bounds=$(osascript <<EOF 2>/dev/null
tell application "System Events" to tell process "$name"
  if (count of windows) is 0 then error "no window"
  set p to position of window 1
  set s to size of window 1
  return ((item 1 of p) as text) & " " & ((item 2 of p) as text) & " " & ((item 1 of s) as text) & " " & ((item 2 of s) as text)
end tell
EOF
)
  if [ -z "$bounds" ]; then
    cap_log "bounds unreadable, using fallback"
    printf '%s' "$CAP_FALLBACK_BOUNDS"
  else
    printf '%s' "$bounds"
  fi
}

# Every frame is read back before it counts. A screencapture that wrote nothing,
# wrote a zero-byte file, or wrote a PNG of the wrong window is indistinguishable
# from a successful capture until something opens the file -- and a capture set
# is evidence, so an unverified frame in it is worse than a missing one.
cap_frame() {
  # Two statements, not `local a=$1 b=.../$a.png`: bash expands every word of a
  # simple command before running it, so the second assignment would read
  # `label` while it is still unset and abort under `set -u`.
  local label="$1"
  local path="$CAP_OUT/$label.png"
  read -r x y w h <<<"$(cap_bounds)"
  screencapture -x -R"$x,$y,$w,$h" "$path" 2>/dev/null
  if [ ! -s "$path" ]; then
    cap_log "FAILED $label (no file)"
    return 1
  fi
  local kind dims
  kind=$(file -b "$path")
  case "$kind" in
    *PNG*) ;;
    *) cap_log "FAILED $label (not a PNG: $kind)"; rm -f "$path"; return 1 ;;
  esac
  dims=$(sips -g pixelWidth -g pixelHeight "$path" 2>/dev/null | awk '/pixel/{printf "%s ", $2}')
  cap_log "captured $label  ${dims}(bounds $x $y $w $h)"
}

# Yawn is frontmost, or nothing is clicked. This is the control for the accident
# that shaped this file: on 2026-09-02 two runs sent clicks while the app was
# NOT frontmost and they landed in the operator's browser. A click into a
# frontmost Yawn window can only reach Yawn.
# Checks the frontmost app's BUNDLE, not its process name. The name is not
# unique and cannot be: the shipped `/Applications/Yawn.app` and this preview
# bundle both run an executable called `local-meeting-notes-desktop`, and the
# shipped one holds the operator's real meetings. A name check passes on either,
# so a capture run could have clicked -- including into a delete confirmation --
# inside the wrong app against real data. Only the bundle path separates them.
cap_guard() {
  local front
  front=$(osascript -e 'tell application "System Events" to get POSIX path of (file of first process whose frontmost is true)' 2>/dev/null)
  local want="${CAP_APP%/}"
  case "${front%/}" in
    "$want") return 0 ;;
    *) cap_log "ABORT: frontmost bundle is \"$front\", not \"$want\" -- refusing to click"; return 1 ;;
  esac
}

# Click at a point given in the coordinates of the 1000px-wide downscaled frame,
# which is how a point gets read off a screenshot. Converted to screen
# coordinates here so call sites use the numbers they can see.
#
# `osascript ... to click at {x, y}` is deliberately not used: at a bare
# coordinate it does nothing and reports success. See capture-click.swift.
CAP_CLICK_BIN="${CAP_CLICK_BIN:-$(dirname "$CAP_SELF")/.bin/capture-click}"
cap_click() {
  local sx="$1"
  local sy="$2"
  local label="${3:-}"
  cap_guard || return 1
  if [ ! -x "$CAP_CLICK_BIN" ]; then
    mkdir -p "$(dirname "$CAP_CLICK_BIN")"
    swiftc -O "$(dirname "$CAP_SELF")/capture-click.swift" -o "$CAP_CLICK_BIN" || {
      cap_log "FATAL could not build $CAP_CLICK_BIN"; return 1; }
    cap_log "built $CAP_CLICK_BIN"
  fi
  read -r wx wy _ _ <<<"$(cap_bounds)"
  local x
  local y
  x=$(python3 -c "print(int($wx + $sx * ${CAP_SCALE:-1.08}))")
  y=$(python3 -c "print(int($wy + $sy * ${CAP_SCALE:-1.08}))")
  "$CAP_CLICK_BIN" "$x" "$y" || { cap_log "FAILED click ${label:-($sx,$sy)}"; return 1; }
  cap_log "clicked ${label:-($sx,$sy)} -> screen $x,$y"
}

# A downscaled sibling for reading in a transcript. The full frame stays as the
# evidence; the -s copy is what gets opened, because a full-resolution capture
# costs several times the image tokens of a 1000px one and a capture pass reads
# every frame it takes.
cap_downscale() {
  local label="$1"
  local src="$CAP_OUT/$label.png"
  local dst="$CAP_OUT/$label-s.png"
  [ -s "$src" ] || return 1
  sips -Z "${2:-1000}" "$src" --out "$dst" >/dev/null 2>&1
}
