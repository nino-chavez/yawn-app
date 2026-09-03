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

# Identify the app by pid resolved from its bundle path, never by process name.
#
# The name is ambiguous twice over. System Events calls it "Yawn Preview" right
# after launch and "local-meeting-notes-desktop" once settled, which an earlier
# version handled by trying both -- and that fixed the symptom while leaving the
# real problem: `local-meeting-notes-desktop` is ALSO the executable name of the
# shipped `/Applications/Yawn.app`, which holds the operator's real meetings.
# Measured on this Mac with both running: the name lookup answered with the
# shipped app and reported 0 windows while the preview had one. A capture would
# have read the wrong window's geometry, or fallen back to fixed bounds and
# photographed whatever sat there.
#
# The executable *path* is unique where the name is not, so the pid is the
# unambiguous handle and every accessibility call addresses it by unix id.
cap_pid() {
  pgrep -f "${CAP_APP%/}/Contents/MacOS/" 2>/dev/null | head -1
}

# A locked screen looks exactly like a missing window, and the difference
# decides whether the right move is to wait or to kill something.
#
# Measured 2026-09-02: with the screen locked, accessibility reports 0 windows
# for every app, `first process whose frontmost is true` errors with "Invalid
# index", and `screencapture` writes no file. On that reading `cap_launch`
# concluded the app was running window-less and quit it -- twice -- when
# nothing was wrong with the app at all. Had a note generation been in flight
# it would have been killed mid-run.
#
# So: prove the session is usable before believing anything accessibility says,
# and never take a destructive step on a reading this check has not cleared.
cap_session_usable() {
  # `grep -c`, not `grep -q`. With `set -o pipefail` (top of this file) `-q`
  # exits on the first match, SIGPIPEs `ioreg`, and the non-zero pipeline makes
  # this read as "not locked" -- a guard that fails open, silently, in exactly
  # the state it exists to catch. `-c` consumes all input, so the pipeline ends
  # cleanly. Verified: this branch now fires on a locked screen.
  local locked
  locked=$(ioreg -n Root -d1 -a 2>/dev/null | grep -c CGSSessionScreenIsLocked || true)
  if [ "${locked:-0}" -gt 0 ]; then
    cap_log "SESSION: screen is locked -- no capture is possible, and a 0-window reading means nothing"
    return 1
  fi
  if ! osascript -e 'tell application "System Events" to get name of first process whose frontmost is true' >/dev/null 2>&1; then
    cap_log "SESSION: no frontmost process -- the login session is not interactive"
    return 1
  fi
  # Assistive access is a SEPARATE permission from everything above, and until
  # 2026-09-03 nothing here noticed it was missing. Both probes above are
  # answered without it: a process NAME and a bundle path are ordinary
  # attributes, while `count of windows` is accessibility. So the guard passed
  # while every window query in this file failed with -25211, and `cap_bounds`
  # quietly served its fallback rectangle -- the same fail-open shape as the
  # locked-screen bug, one permission over.
  #
  # A real window count is the probe because it is the thing that has to work.
  # It returns "0" rather than erroring when the frontmost app genuinely has no
  # window, so this separates "not allowed to ask" from "nothing to see".
  if ! osascript -e 'tell application "System Events" to return (count of windows of first process whose frontmost is true) as text' >/dev/null 2>&1; then
    cap_log "SESSION: no assistive access -- window geometry, frontmost checks and clicks all fail"
    cap_log "SESSION: grant Accessibility to the host app in System Settings > Privacy & Security > Accessibility"
    return 1
  fi
  return 0
}

cap_window_count() {
  local pid
  pid=$(cap_pid)
  [ -n "$pid" ] || { printf '0'; return 1; }
  osascript -e "tell application \"System Events\" to tell (first process whose unix id is $pid) to return (count of windows) as text" 2>/dev/null | head -1
}

# Waits for a WINDOW, not just a process. A running app with no window is the
# state that makes every later capture silently wrong: `cap_bounds` finds no
# window, falls back to the default rectangle, and `screencapture -R`
# photographs whatever occupies that part of the screen. Measured tonight --
# the preview sat frontmost with zero windows and reported itself launched.
#
# `open -a` on an already-running instance does not always restore a closed
# window (this app has no Window-menu entry to reopen one either), so a
# window-less instance is quit and relaunched rather than nudged.
cap_launch() {
  [ -d "$CAP_APP" ] || { cap_log "FATAL no bundle at $CAP_APP"; return 1; }
  cap_session_usable || return 1
  local pid
  pid=$(cap_pid)
  if [ -n "$pid" ] && [ "$(cap_window_count)" = "0" ]; then
    cap_log "pid $pid is running with no window; quitting it to get a fresh one"
    kill "$pid" 2>/dev/null
    local gone=0
    while [ "$gone" -lt 15 ] && [ -n "$(cap_pid)" ]; do sleep 1; gone=$((gone + 1)); done
  fi
  open -a "$CAP_APP"
  local waited=0 count
  while [ "$waited" -lt 40 ]; do
    pid=$(cap_pid)
    if [ -n "$pid" ]; then
      count=$(cap_window_count)
      if [ -n "$count" ] && [ "$count" != "0" ]; then
        cap_log "pid $pid, $count window(s) after ${waited}s"
        return 0
      fi
    fi
    sleep 1
    waited=$((waited + 1))
  done
  cap_log "FATAL no window after ${waited}s (pid ${pid:-none})"
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

# Returns real window bounds or fails. It used to serve `CAP_FALLBACK_BOUNDS`
# whenever the query came back empty, which reads as defensive and is not: the
# window is not at the fallback origin (it sits at 360,80 on this Mac), so a
# frame taken on those numbers photographs a rectangle of desktop and whatever
# else is under it. `cap_frame` reads every frame back from disk, and a valid
# PNG of the wrong rectangle passes that check -- so the fallback converted a
# hard failure into a plausible, wrong, permanently-filed piece of evidence.
#
# Same lesson as the two guards above: when a check cannot get a true answer it
# must say so, not substitute a default and continue.
cap_bounds() {
  local pid bounds
  pid=$(cap_pid)
  [ -n "$pid" ] || { cap_log "BOUNDS: no process for $CAP_APP"; return 1; }
  bounds=$(osascript <<EOF 2>/dev/null
tell application "System Events" to tell (first process whose unix id is $pid)
  if (count of windows) is 0 then error "no window"
  set p to position of window 1
  set s to size of window 1
  return ((item 1 of p) as text) & " " & ((item 2 of p) as text) & " " & ((item 1 of s) as text) & " " & ((item 2 of s) as text)
end tell
EOF
)
  if [ -z "$bounds" ]; then
    cap_log "BOUNDS: unreadable for pid $pid -- refusing to guess a rectangle"
    return 1
  fi
  printf '%s' "$bounds"
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
  # `frame_path`, never `path`. In zsh -- which the header says this file is
  # sourced from -- `path` is the array tied to `PATH`, so `local path=...`
  # empties PATH for the rest of the function and `screencapture`, `file` and
  # `sips` all stop resolving by name. Harmless in bash, which is why it
  # survived: the same line is correct in one shell and destroys the function
  # in the other.
  local frame_path="$CAP_OUT/$label.png"
  # `screencapture -R` takes a screen RECTANGLE, so it photographs whatever is
  # on top of that rectangle -- not the app. Without this the read-back below
  # passes happily on a valid PNG of the wrong window, which is the one failure
  # it cannot see. Same reason `cap_click` guards.
  cap_guard || { cap_log "FAILED $label (guard)"; return 1; }
  local rect
  rect=$(cap_bounds) || { cap_log "FAILED $label (bounds)"; return 1; }
  read -r x y w h <<<"$rect"
  screencapture -x -R"$x,$y,$w,$h" "$frame_path" 2>/dev/null
  if [ ! -s "$frame_path" ]; then
    cap_log "FAILED $label (no file)"
    return 1
  fi
  local kind dims
  kind=$(file -b "$frame_path")
  case "$kind" in
    *PNG*) ;;
    *) cap_log "FAILED $label (not a PNG: $kind)"; rm -f "$frame_path"; return 1 ;;
  esac
  dims=$(sips -g pixelWidth -g pixelHeight "$frame_path" 2>/dev/null | awk '/pixel/{printf "%s ", $2}')
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
  cap_session_usable || return 1
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
  local rect
  rect=$(cap_bounds) || { cap_log "FAILED click ${label:-($sx,$sy)} (bounds)"; return 1; }
  read -r wx wy _ _ <<<"$rect"
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
