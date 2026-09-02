# Experience brief — Yawn desktop, rethink phase 1

2026-09-02 · draft for the operator · judged-screen pattern § 2b

This brief follows the blind cold review of the installed build
(`docs/evidence/screen-reviews/all-surfaces-bfa0a80-installed-cold.md`) and
the comparables composition record
(`docs/evidence/comparables-window-composition-2026-09-02.md`). It does not
change the product brief. It writes down the one thing the prior direction
never did: the platform strategy. The object/action/state matrix and the
review ledger stay in `DIRECTION.md`.

## The user's situation

One person at a Mac, with other work on screen, in one of three moments:

- **Before.** A meeting is about to start. They want to be recording in
  under three seconds, with the consent check done, and get back to the
  call. The window is a stop on the way, not a place to stay.
- **During.** The call is on. They want to type reminders without looking
  at the app, know it is still recording, and stop with one control.
- **After.** They want the finished note, and when a detail matters, the
  transcript that proves it. Later, they want to find a past meeting by
  what it was about.

They know how to use a Mac. They should never have to learn Yawn.

## The five job questions, per moment

| Moment | What is happening now | What is next | Who | When and where | What can I do |
|---|---|---|---|---|---|
| Before | Not recording; audio ready or not | Record | The operator alone | Now, this Mac | Record; open a past meeting |
| During | Recording, since when, paused or not | Stop | Speakers as they appear | Elapsed time | Type notes; pause; stop |
| After | Note ready, transcript ready, or needs attention | Read the note; generate it if absent | Speakers in the transcript | Meeting date and length | Read, check a claim, retry, export, record another |

## Hierarchy

1. The meeting is the loudest thing on screen. Never the app.
2. The recording control is always one place, in the chrome, and is the
   only element that spends the record color.
3. The list is beside the document, not a page of its own, so the operator
   never leaves a note to find another.
4. Status is small, near its object, in the toolbar or the row. A full-width
   banner is reserved for a condition that blocks or endangers the record.
5. The transcript is disclosed at the point of doubt, never in the main
   reading path (brief rule; D5 already builds this).

## Content strategy

- Say the on-device promise once, in Settings, in the three-fact card the
  cold review called "the clearest, most complete copy hierarchy in the
  set." Everywhere else, carry it as a caption ("On this Mac") or not at all.
  Four phrasings today; one after.
- No product pitch inside the product. "Capture the conversation. Keep your
  own judgment." is marketing register; the cold review found it "outstays
  its welcome." It goes. The first-run sheet may keep one sentence of
  orientation.
- Status words are the brief's: recording, preparing, finishing, transcript
  ready, needs attention. The same fact is stated once per screen, not three
  times (the cold review counted three on the launch view).
- One severity vocabulary: a fact ("paused once, 0:03") is a caption; an
  integrity problem ("could not verify whether this recording was paused")
  is a needs-attention state with a next action. Today both are the same
  amber banner.
- A row shows the meeting's title, date, and note excerpt. A raw transcript
  fragment is never a title; an untitled meeting says "Meeting · date."

## Motion and feedback

State changes are legible and short (the existing 120 ms / 200 ms tokens).
Recording start and stop confirm in the toolbar and the menu-bar pill.
Nothing performs.

## Platform strategy

The surface is a macOS document-style utility. Conventions adopted and
declined, with the reason for each decline, as § 2b requires.

**Adopt**

| Convention | Why |
|---|---|
| Native title bar with a unified toolbar (title as toolbar) | Every comparable on this Mac carries its primary action, search, and status in the chrome. The current hidden-title overlay with a custom topbar is the hybrid the skill audit called "neither native nor custom." |
| Record as a toolbar control, with ⌘R and a File menu item | One obvious way to start (brief). The cold review found the "Allow system audio" setup button outranking the recurring task. |
| List beside document: a meetings list on the left, the note on the right, collapsing to one pane below about 900 pt | The width is unused today ("right two-thirds of the lower page empty"). The list stops being a page you leave. |
| Sidebar toggle, ⌘⇧S, and a View menu | Standard for every list-beside-document app. |
| Native search field in the toolbar | Title search is the list's only filter and belongs where every Mac app puts it. |
| System font, system semantic colors, and the user's accent color | The brief says quiet and Mac-native. The current green Settings eyebrow and coral Record are bespoke accents the cold review flagged as inconsistent. |
| Standard list rows with a selection highlight | The cold review read the current rows as native; keep that. |
| Window state restoration: open to the list with the last note selected | The app currently opens into a stale "Your meeting is ready to read" terminal view in both appearances (cold review 03, 08). |
| A Settings window that behaves like Preferences: standard size, closable, not modal | Disabled minimize and zoom on a 720-pt window read as a defect. |
| Native Edit, File, View, Window menus with the full command set | D9 already required native text behavior; the menu is where the rest of it lives. |
| Trash as a list item at the bottom of the sidebar | Matches Notes and Bear; it is a place, not a link. |

**Decline**

| Convention | Seen in | Why declined |
|---|---|---|
| Folders, tags, or a tree in the sidebar | Notes, Bear, Craft, Agenda, Obsidian | Product brief anti-goal. The sidebar is one flat list, grouped by date. |
| A document grid | Craft | The list is organized by meeting and time, not by card. |
| Tabs | Obsidian | One meeting at a time is the product; D5's split view is the second pane, not a second document. |
| A calendar card or "coming up" | Granola | No calendar integration exists and none is promised. The before moment is a Record control, not a schedule. |
| A chat bar over meetings | Granola | Anti-goal: chat over every meeting. |
| A permanent third pane for the transcript | Bear's tag pane, three-pane Notes | The transcript discloses at the point of doubt (D5: hover, split, synced scroll). A permanent pane puts it in the reading path. |
| A hero headline or pitch line on Home | Current Yawn | See content strategy. |
| Tinted content panels | Current Yawn | Color only for recording or attention (brief). The note card is content, not attention. |
| Seeded sample content | Granola's demo meeting | Granola's demo has real provenance (a founder's real two-minute recording), which the brief amendment of 2026-09-01 did not anticipate. The amendment's rule still holds for Yawn because Yawn ships no recording of anyone; a sample would have to be fabricated. Revisit only if a real, licensed, disposable recording is shipped with the app. |

**Undecided, for the concepts to test**

- Whether the During moment lives in the main window's document pane, in a
  compact separate window, or only in the menu-bar pill with the main
  window optional. The brief allows any; the concepts must compare them on
  the same state.
- Whether the note and transcript split (D5) is a resizable split in the
  document pane or an inspector on the right.

## Surfaces

- Launch state: what the window opens to with meetings present
- Home, first run: zero meetings, orientation sheet
- Home, first run: sheet dismissed, empty list
- Library: list with meetings and the selected note
- During capture: note canvas, recording state, pause and stop
- After: the note with overview and outcomes, source one click away
- A withheld transcript turn
- Needs attention: a real failure state with a next action
- Settings with the active model
- Trash
- Platform accessibility: large text, increased contrast (operator-run)

## Representative states for the concept comparison (§ 2c)

Each of the three concepts renders these four, on this Mac, in both
appearances, with the real storage of four meetings:

1. Home with meetings, nothing selected
2. During capture, two minutes in, one pause
3. After: a meeting with a generated note and one claim's source open
4. Needs attention: the recovered-interrupted meeting

## Provenance

Cold review bfa0a80-installed (ten frames, blind). Comparables record of
six installed apps, device captures. Product brief (2026-08-10, amended
2026-09-01). Granola first-run doc fetched 2026-09-02 for the demo-content
fact. The adopt/decline table is the author's proposal; the operator's
selection in the concept ADR is what makes it direction.
