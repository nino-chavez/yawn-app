---
surface: mac-rail (an interrupted recording; a meeting whose saved note cannot be read)
kind: cold
build: main's installed Yawn Fixture app (UI as of 9f44753); data seeded by eb3336f's rendered-review-fixture
device: installed Yawn Fixture app on this Mac, 1080 by 900 at 2x, the app's own window captured with screencapture -l; Dark (the Mac's appearance) and Light (the in-app choice)
reviewer: one fresh subagent (Claude) with the four frames under shuffled neutral names and the five job questions only
implementer: the session that seeded and captured it (Claude), not the reviewer
cold: true
states: [interrupted-dark, interrupted-light, unreadable-note-dark, unreadable-note-light]
verdict: revise (all four); nothing fixed on this branch, because it changes only the fixture seeder. Findings sorted below into fixture data, decisions already kept, and new product findings for a follow-up
---

# Yawn for Mac — interrupted and unreadable-note pages, installed build, 2026-10-16

**Question.** Now that the fixture can reach them on the installed build, do the
interrupted-recording page and the unreadable-saved-note page answer the five job
questions in both appearances? Before this, the interrupted page existed only as
WKWebView harness frames (`mac-rail-2026-10-16-cold.md`), and the unreadable-note page
had never been reviewed.

**Captures.** `mac-fixture-interrupted-2026-10-16/` with its `MANIFEST.md`. Four judged
frames and one launch receipt. Every frame was checked for a painted window and for the
Computer Use indicator before it was kept.

**One pass.** The reviewer gave all four frames revise. The interrupted page's
"This recording did not finish" was read correctly as the current state in both
appearances. The window was read as a native Mac app (traffic lights, unified title bar,
sidebar, inactive-window styling).

## Findings and what happens to each

**Fixture data, not product.**

- *"Error-as-title"* (unreadable-note frames). The heading "Fixture: this meeting's
  saved note cannot be read" is the fixture transcript's first line, which the app
  derives as the title. The reviewer flagged that this could be test data. It is.
- *Sidebar titles indistinguishable* ("cur0 cur1 cur2…", "Fixture: this is the launch
  checklis…") and *every row showing only a time*. All seeded meetings share one day and
  carry invented text. The installed record already covered ellipsis.

**Already decided; kept.**

- *Move to Trash is the interrupted banner's only action, and it is destructive.* This
  matches the harness record (`mac-rail-2026-10-16-cold.md`, "The interrupted state's one
  action"). There is no transcription of a recovered recording's kept audio, so no
  positive action exists to offer. The primary action waits on a product decision. It
  remains an operator item.
- *The 6:00 stop mark on an empty rail.* It is the Rail's stop mark (`DESIGN.md`). It
  marks where the recording ended, not a note.
- *"Full transcript" collapsed while the row says "transcript available".* It is
  collapsed by design, and the transcript is one click away.
- *The share button absent on the interrupted meeting.* There is nothing verified to
  export without a transcript. This is by design.

**New product findings (follow-up; not fixed here).**

1. **"Yawn could not verify whether this recording was paused."** It appears above the
   player on the interrupted page (`crates/session-core/src/capture_quality.rs`, the
   pause projection's unavailable text). An interruption receipt carries no pause ledger,
   so the sentence is true, but it reads as a stray quote with no consequence. The
   smallest remedy is to say what it means ("6:00 may include paused time"), or to drop
   it on an interrupted meeting, whose banner already says the recording did not finish.
2. **"Any retained audio remains available."** This is the banner detail
   (`apps/desktop/src-tauri/src/library_reader.rs`). It hedges, while the meta line says
   "Audio kept" and the player shows 6:00. The nothing-kept case already has its own
   state (`recovered-interrupted-nothing-kept`), so this copy can be definite.
3. **The unreadable-note block gives no next step**, and it is the lowest-contrast text
   on the page ("Yawn could not read this meeting's personal note, so it was left
   unchanged.", `apps/desktop/ui/main.js`). The smallest remedy is one clause saying the
   file is kept untouched and what the reader can do, at body contrast.

All three are copy on judged surfaces. They need entries in `DIRECTION.md`'s
content-reads table, an installed recapture, and a cold pass of their own.

## Removed, combined, demoted or hidden

Nothing on the screens: this branch changes only the fixture seeder. The fixture lost the
tampered-transcript meeting's installed state. That state could only ever render as the
whole-app recovery wall, and its export withholding stays covered by
`meeting_export.rs`'s unit test.
