#!/usr/bin/env bash
# Driver for the note-surfaces capture pass. Sources the primitives; owns only
# the navigation between states.
#
# This file exists because `capture-installed-screens.sh` says it should:
# navigation is per-pass and would rot inside the primitives, and every earlier
# pass rebuilt its own driver by hand and then lost it. The 2026-09-03 pass was
# driven entirely by hand from a terminal, which is why it is written down here
# the first time it worked.
#
#   CAPTURE_OPERATOR_AWAY=1 CAP_OUT=<dir> bash scripts/capture-pass-note-surfaces.sh
#
# THE FRAGILE PART IS THE COORDINATES. They are points read off the 1000px-wide
# downscaled frame (`<label>-s.png`), which is how a point gets read off a
# screenshot in a transcript. `cap_click` converts to screen coordinates. They
# are correct for build 21 (d0c5d86) at the 1080x900 default window, and the
# visual refit now in flight WILL move them. When a step lands in the wrong
# place, do not nudge the number blindly: take a frame, read the new position
# off it, and update the constant here with the build it was re-derived from.
set -uo pipefail

CAP_SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
source "$CAP_SELF_DIR/capture-installed-screens.sh"

# Points on the 1000px-wide downscaled frame. Re-derived 2026-09-03, build 21.
P_ROW_FAILED="130 100"     # sidebar row 1, the note-not-created meeting
P_ROW_NOTE="130 265"       # sidebar row 4, the meeting that already has a note
P_MANAGE="797 82"
P_MENU_LOCK="703 169"
P_MENU_TRASH="703 224"
P_SHEET_CANCEL_TRASH="581 505"
P_SHEET_CANCEL_LOCK="591 505"
P_FULL_TRANSCRIPT="348 473"
P_GENERATE="358 301"

step() { cap_log "STEP $*"; }

cap_require_operator_away || exit 2
cap_launch                || exit 1

# --- the meeting that already has a note -----------------------------------
step "note document"
cap_click $P_ROW_NOTE "sidebar row: meeting with a note" || exit 1
sleep 3
cap_frame 02-dark-note && cap_downscale 02-dark-note

# --- the two sheets. Both are OPENED and CANCELLED, never confirmed. --------
# Move to Trash is the only irreversible action reachable in this pass. It is
# photographed because no review had ever seen it, and cancelled immediately.
# If you edit this file, the cancel step is not optional and not reorderable.
step "manage menu"
cap_click $P_MANAGE "Manage" || exit 1
sleep 1
cap_frame 03-dark-manage-menu && cap_downscale 03-dark-manage-menu

step "trash confirmation (opened, then cancelled)"
cap_click $P_MENU_TRASH "Move to Trash..." || exit 1
sleep 2
cap_frame 04-dark-trash-confirm && cap_downscale 04-dark-trash-confirm
cap_click $P_SHEET_CANCEL_TRASH "Cancel" || exit 1
sleep 2
cap_frame 05-dark-after-cancel && cap_downscale 05-dark-after-cancel

step "lock sheet (opened, then cancelled)"
cap_click $P_MANAGE "Manage" && sleep 1
cap_click $P_MENU_LOCK "Lock meeting..." || exit 1
sleep 2
cap_frame 06-dark-lock-sheet && cap_downscale 06-dark-lock-sheet
cap_click $P_SHEET_CANCEL_LOCK "Cancel" || exit 1
sleep 2

# --- transcript -------------------------------------------------------------
step "full transcript"
cap_click $P_FULL_TRANSCRIPT "Full transcript disclosure" || exit 1
sleep 3
cap_frame 10-dark-transcript && cap_downscale 10-dark-transcript

# --- settings, reached by menu rather than by coordinate --------------------
# The menu path is stable across layout changes in a way a point is not, so it
# is preferred wherever the app exposes one.
step "settings"
_pid=$(cap_pid)
cap_guard && osascript -e "tell application \"System Events\" to tell (first process whose unix id is $_pid) to click menu item \"Settings…\" of menu 1 of menu bar item 2 of menu bar 1" >/dev/null 2>&1
sleep 3
cap_frame 11-dark-settings && cap_downscale 11-dark-settings
cap_guard && osascript -e "tell application \"System Events\" to tell (first process whose unix id is $_pid) to click button 1 of window 1" >/dev/null 2>&1
sleep 2

# --- generation, last, because it is the only step that changes storage -----
# Ordered last deliberately: everything above is read-only, so a failure here
# cannot cost the frames already banked.
step "generating state"
cap_click $P_ROW_FAILED "sidebar row: note-not-created meeting" && sleep 3
cap_frame 01-dark-open && cap_downscale 01-dark-open
cap_click $P_GENERATE "Generate note" || exit 1
sleep 3
cap_frame 08-dark-generating && cap_downscale 08-dark-generating

cap_log "DONE. Frames in $CAP_OUT. Read every one before filing the set."
cap_log "Generation was started and is still running; capture its outcome separately."
