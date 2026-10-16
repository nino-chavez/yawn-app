# DESIGN.md — Yawn visual system, Mac and iPad

Status: **Rail selected 2026-10-15** by the operator
([decisions/family-direction-2026-10-15.md](decisions/family-direction-2026-10-15.md)).
This replaces Tonal Ledger — Canvas (2026-09-03) and its Lavender Haze dark
palette (2026-09-08), which stay on disk as archaeology. Not yet implemented:
the Mac app still renders the prior system until the implementation wave
lands, and the iPad app (`apps/ipad/`) is at build-plan slice 1.

The static reference is the Rail concept in the family harness:
`apps/desktop/ui-harness/family-concepts/2-rail/` (`styles.css`,
`concept.js`) and its frames in `docs/design/concepts/family-2026-10-15/`
(`2-<state>-<appearance>.png`). The product implementation
(`packages/design-tokens/tokens.json`, `apps/desktop/ui/styles.css`, and the
SwiftUI app) must match the reference rather than reinterpret it. Frames are
comparison evidence, not installed acceptance; each app's acceptance is a cold
review of device captures.

## Thesis

A private meeting notepad with a recorder attached. The operator's own
writing and the retained transcript are the destination. On both devices the
page is the product and the app is its margin. Down that margin runs a rail:
each line the operator wrote carries the moment it was written, the rail is
how they move through the audio, and after the meeting the transcript turn
spoken nearest each note sits level with it.

## What this system keeps from the prior decisions

- The Mac window frame (Concept A, 2026-09-02): a meeting list beside a
  document pane, one Record control in a unified toolbar, Trash last.
- The iPad structure (Concept C, 2026-10-14): the page is the screen, the
  rail in its left margin is the scrubber, the transcript folds beside the
  ink, the list is a sidebar hidden while recording, consent is a full-screen
  step of its own.
- The product rules: no generated notes, a withheld turn renders as withheld,
  the storage fact is said once, status words are the brief's, no explanatory
  chrome in steady state.
- Copy truth, keyboard paths, focus order, ARIA, and reduced-motion behavior.

## Character

Paper-first, plain, settled, candid, kin. The page is a warm sheet; the chrome
is graphite and recedes. The operator's words are the largest type on screen.
State lives in words and position, near its object. Red means recording,
amber means a condition needs attention, blue ink means a moment in the
recording, and nothing else carries color. The two apps share the rail, the
palette, the type rules and the status words; they differ only where the hand
differs.

## Color

Direct light and dark values. `packages/design-tokens/tokens.json` is the
authority; `apps/desktop/ui/tokens.css` is generated from it; the SwiftUI app
carries the same values as named colors in its asset catalog.

| Role | Light | Dark | Job |
|---|---:|---:|---|
| Window chrome | `#ecebe6` | `#1b1a17` | Toolbar, nav bar, status bar |
| Sidebar | `#e5e3dc` | `#1f1e1a` | Meeting list, Settings sidebar |
| Page | `#faf9f5` | `#262521` | The document pane and the iPad page |
| Panel | `#f3f2ed` | `#2b2a25` | The iPad transcript column, Settings groups |
| Hairline | `rgba(50,42,28,0.12)` | `rgba(255,250,235,0.10)` | Separators |
| Hairline, strong | `rgba(50,42,28,0.22)` | `rgba(255,250,235,0.20)` | Scrub track, increased contrast |
| Label | `#201e1a` | `#ece7db` | Primary text, ink |
| Label 2 | `#645f55` | `#a39d8f` | Secondary text, transcript turns |
| Label 3 | `#67625a` | `#999487` | Captions, times, group labels |
| Label 4 | `#cfcbc1` | `#44413a` | Disabled, placeholders |
| Graphite (accent) | `#3d3a34` | `#d6cfbf` | Selection locator, primary push button, active control |
| On graphite | `#faf9f5` | `#1b1a17` | Text on a graphite fill |
| Selection | `rgba(50,42,28,0.07)` | `rgba(255,250,235,0.07)` | Selected row surface, hover |
| Control | `#fffefb` | `#35332d` | Push buttons, search field, capsules |
| Control border | `rgba(50,42,28,0.18)` | `rgba(255,250,235,0.14)` | Their one border |
| Rail | `#cfcabd` | `#4d4a42` | The rail line |
| Rail mark | `#201e1a` | `#ece7db` | A note's dot on the rail |
| Record | `#c7392c` | `#ff6a5c` | Recording only |
| On record | `#fffaf7` | `#1b0c09` | Text on a record fill |
| Record, soft | `rgba(199,57,44,0.11)` | `rgba(255,106,92,0.16)` | The iPad recording badge, paused |
| Attention | `#8f5d12` | `#d9a24a` | A condition that needs attention |
| Attention, soft | `rgba(190,130,40,0.12)` | `rgba(217,162,74,0.14)` | The bounded attention surface |
| Moment | `#2f5fa8` | `#8fb4ff` | The playhead, a tapped line, the current turn, the active pen |
| On moment | `#ffffff` | `#0f1a2e` | Text on a moment fill |
| Moment, soft | `rgba(47,95,168,0.12)` | `rgba(143,180,255,0.18)` | The current turn's surface, the tapped line's fill |
| Ink, dim | `rgba(31,29,25,0.28)` | `rgba(239,233,220,0.30)` | Strokes written after the playhead during replay |

Rules:

- Red appears only on the Record control, the recording badge, the live
  rail mark, and the menu-bar pill. Amber only on the needs-attention dot,
  the attention surface, and the stop mark on the rail. Blue ink only on the
  moment link. Graphite is not a brand color; it is the chrome.
- A fact is a caption; a problem is a bounded attention surface with one next
  action. Never a banner.
- The page has no enclosing card. The operator's notes, the ledger and the
  transcript column sit directly on it.

Contrast is measured in `docs/evidence/design-contrast-2026-10-15.md`. Every
text pair meets 4.5:1 in both appearances after Label 3 and light Attention
were darkened on 2026-10-15 (the concept frames' first values failed). The
rail line and the scrub track are guides, not meaning: the dots, the playhead
and the ticks on them carry the meaning and use Label or Moment.

## Type

System font throughout, no second family. Sizes are fixed pixels on the Mac
and Dynamic Type styles on the iPad.

| Element | Mac | iPad |
|---|---|---|
| The operator's note | 16px / 1.5, Label | Ink, the hand's own size; the handwriting stand-in is never shipped |
| Meeting title | 20px / 1.25, weight 650, Label | Nav bar title 17pt semibold |
| Meta line (date · time · length · audio kept) | 12px, Label 3 | Nav bar subtitle 12pt, Label 3 |
| Transcript turn | 13px / 1.45, Label 2; the current turn in Label | 14pt / 1.4, Label; the current turn on Moment soft |
| Turn time and speaker | 11px, tabular, Label 3 | 11pt tabular, Label 3 |
| Rail time | 11px, tabular, Label 3; the live mark in Record; the playhead in Moment | 11pt tabular, Label 3; the same two exceptions |
| Fold count ("+ 7 turns") | 11px, Label 3 | 12pt, Label 3 |
| Sidebar row title · caption · excerpt | 13px semibold · 11px · 11px | 15pt semibold · 12pt · the ink first line at 16pt |
| Group label | 10px, 0.08em tracking, uppercase, Label 3; the only uppercase on the Mac besides Settings section labels | 12pt semibold, Label 3, sentence case |
| Toolbar title, search, push buttons | 13px semibold · 12px · 12px | Capsule 15pt semibold |
| Needs-attention headline and detail | 13px semibold · 12px / 1.45 | 17pt semibold · 15pt |
| Settings section label · row · detail · state | 10px uppercase tracked · 13px · 11px · 12px, Label 2 | not in scope |
| First-run line and its caption | 20px weight 650 · 15px Label 2 · 11px Label 3 | title3 · subheadline, Label 3 |

Tabular numerals on every time. Large text follows the system setting; the
ledger's note column is a measure in characters, so it holds.

## Composition, Mac

One main window, default 1080 by 900, minimum 900 by 600. The unified
toolbar (52pt) carries, left to right: traffic lights, sidebar toggle, window
title, export and More (ellipsis) icon buttons when a meeting is open, the
search field (170 by 24), and the Record control. Nothing else enters the
toolbar.

Sidebar 262pt: meetings grouped by day; each row title · caption (time ·
length · status) · one excerpt line, 6px by 10px padding, 6px radius. The
selected row is the Selection surface with a 3px Graphite locator inset at
its left edge; text does not invert. A needs-attention row carries an amber
dot; "Recording now" is the first row during capture with a red dot. Trash is
last, above a hairline.

Document pane: the page. Inner padding 28px top, 40px right, 28px left. The
header and the transport are indented 76px, so they share the notes' left
edge past the rail gutter.

1. **Header.** The meta line (date · time · length · audio kept until), then
   the title. Rename is in the More menu, not a button on the page.
2. **Transport.** A 24px play button, "11:30 / 24:18" in Label 3 tabular, a
   3px scrub track up to 300px wide with the elapsed part in Label 2 and an
   11px Moment knob, and at the right a "Full transcript" text button with a
   chevron. The track carries a 1.5px tick per transcript turn (the graft
   from Record): You above the center line in Graphite, Them below in
   Label 3.
3. **The ledger.** A grid of rail | note | nearest turn: 64px, 1fr, 300px,
   with 24px column gaps and 7px vertical padding per row. The rail is a 2px
   line at 52px from the ledger's left edge, running the ledger's height. A
   mark is a 40px right-aligned time, an 8px gap, and a 9px dot centered on
   the rail. The note is one paragraph at 16px. The nearest turn is its time ·
   speaker in 11px over its text in 13px, with 2px by 6px padding and a 6px
   radius. Between rows, a fold row shows "+ N turns" in the turn column.
   The current turn sits on Moment soft with a 3px Moment locator; the
   playhead's mark is an 11px Moment dot with its time in Moment.
4. **A line typed after Stop** sits below the last timed line under an
   "After the meeting" divider (12px Label 3, a hairline to the right) with no
   mark on the rail.

During capture the ledger is one column (rail | note) with no turn column.
The live mark, the last line's, is a red dot with its time in Record. The
toolbar shows the live Record badge with Pause and Stop. Nothing else is on
the page; the live transcript is not shown.

Needs attention: the header, then a bounded attention surface indented with
the notes (12px by 16px padding, 8px radius, Attention soft, a 3px Attention
locator), with the headline, the detail, a Graphite primary and a secondary
push button. Below it the ledger with whatever was typed, and the rail ends
with a square Attention mark at the stop time and "Recording stopped here" in
12px Attention.

First run: the page with an empty rail at 80px, one line at 20px ("Record a
meeting"), one sentence at 15px, and "⌘R, or the Record button" at 11px,
centered. The sidebar carries one caption: "Meetings you record appear here,
newest first."

The rail stays on first run, bare, as the page's margin (operator decision,
2026-10-16). Every cold reviewer of the Rail builds, on both platforms, read
the empty line as a stray divider; the operator kept it because the rail is
the page's structure, not a mark, and a page that gains its margin only once
something is written would change shape under the first note. A cold review
that names the bare rail is answered by this paragraph, not by a revision.
The same holds on the iPad.

Settings (the graft from Record; placement amended by the operator,
2026-10-16): a page in the main window's document pane, not a window of its
own. It opens from the gear at the right of the sidebar's foot, from the
Settings menu item and from ⌘,; the toolbar title reads "Settings" and opening
a meeting, Trash or Record leaves it. The page is one 580px column of groups,
centered in the pane. Each group is a section label, then rows of
label and detail with the state or control at the right, separated by
hairlines, no inset cards. States are 12px Label 2 text; actions are push
buttons; a switch is the system switch. The one privacy sentence closes the
column. Every group's first row is visible in one window height.

Appearance (operator decision, 2026-10-16): the first Settings group offers
System, Light or Dark as three toggle buttons, System by default. System
follows macOS; Light or Dark holds regardless of it, on the page and on the
native window chrome. Every dark value keys on the app's theme attribute,
never the system media query, so the choice governs both appearances.

Spacing scale: 4, 8, 12, 16, 24, 32, 48, 64 px. Radii: 5px controls, 6px rows
and turns, 8px the attention surface, 18px capsules.

## Composition, iPad

Status bar 24pt; nav bar 52pt on Window chrome with a hairline below; the
page fills the rest. Landscape and portrait, Split View and Stage Manager.

- **The rail** is a 2px line at 64pt from the page's left edge, drawn on
  first run too, before any mark (see "First run" under Composition, Mac). A mark is a
  9pt dot on the rail with its time to the left; the playhead is an 11pt
  Moment dot with its time in a Moment pill; the live mark is Record.
- **Ink** starts 92pt from the left edge and keeps 90pt clear at the right
  for the tool column. Lines are the hand's own size; the concept's 64pt
  pitch is the fixture's, not a rule.
- **The tool column** (pen, eraser, undo) is 56pt wide at the right edge
  while recording, 18pt apart; after the meeting the tools move to the nav
  bar. The active tool sits in a Moment soft capsule in Moment.
- **The recording badge** is a 32pt capsule on Record soft in Record with a
  dot, "Recording" and the elapsed time; Stop is a 36pt Record capsule at
  the right of the nav bar.
- **After the meeting**, the page keeps 60 percent of the width and the
  transcript column takes the rest on Panel with a hairline at its left: the
  nearest turn level with each note (its time over its text), the fold counts
  between, the current turn on Moment soft with a 3px Moment locator. The
  nav bar carries "‹ Meetings", the transport capsule ("▶ 11:30 / 23:05"), the
  title and subtitle, the tools, Full transcript, Search and Export.
- **Strokes written after the playhead** during replay are Ink dim; the
  tapped line gets a 2px Moment outline on Moment soft. A stroke added after
  Stop sits under the "After the meeting" divider with no mark.
- **Consent** is a full-screen step: a 420pt card on Page with a hairline and
  an 18pt radius over Window chrome. Title, one sentence, the consent switch,
  the keep-period segmented control, "Stays on this iPad" once, a 50pt
  Record "Start recording" button, and "Not now" in Label 2.
- **The meeting list** is a floating sidebar, 320pt, 20pt radius, 8pt inset,
  on Sidebar. A row is day, time · length, a mini rail of when notes were
  written, and the first ink line. The selected row is Selection with a 3px
  Graphite locator.

## Components and states

| Component | States | Rule |
|---|---|---|
| Push button | default, primary, pressed, disabled, focused | 22px high, 10px side padding, 12px type, 5px radius, one Control border on Control. Primary is Graphite with On graphite text. One primary per surface; a destructive action is never the primary |
| Record control | idle, live (badge with elapsed, Pause, Stop), paused | The only red on the Mac. Toolbar and menu-bar pill only |
| Icon button | default, hover | Sidebar toggle, Export, More. 28 by 24, 6px radius, Selection on hover |
| Search field | empty, typing, filtered-empty | 170 by 24, Control with its border, 12px. Title search only |
| Sidebar row | default, hover, selected, needs-attention, recording-now | Selection surface and Graphite locator when selected; text never inverts |
| Rail mark | timed, live, playhead, stop, after-stop (none) | Time left of the dot; Record for live, Moment for the playhead, Attention square for a stop |
| Ledger row | timed with nearest turn, timed without turn (during), fold, after-the-meeting | Rows never reflow the note; the turn column is secondary type |
| Transport | idle, playing, paused | One per meeting view; ticks per turn; the knob is Moment |
| Transcript turn | default, current, dim, withheld | Withheld renders as withheld text with Restore, never as missing |
| Fold row | collapsed, expanded | "+ N turns" in Label 3; expanding shows the turns in place |
| Attention surface | — | Attention soft with a 3px locator, headline, detail, primary and secondary actions. No banner |
| Settings group | — | Section label, hairline rows, state text or control at the right |
| Sheet | Start sheet only | Record disabled until the attestations and a retention choice |
| Popover and menu | More menu, speech-model picker | Never the meetings list |
| Toast | one line, optional action | Bottom center, 4 s, never for a fact a row already shows |
| iPad capsule | default, record, active | 36pt, 18pt radius, Control with border; Record for Stop; Moment soft for the active tool |

## Motion and feedback

State changes are short and legible (120 ms / 200 ms). Recording start and
stop confirm in the toolbar and the menu-bar pill. During replay the playhead
moves down the rail and the current turn's surface follows it; on the iPad,
strokes dim and brighten as it passes. A fold opens in place. Nothing
performs. Reduced motion sets every transition to 0 ms and keeps the state.

## Focus and keyboard

Focus ring 2px Moment, 1px offset, on every control and row. Tab order:
toolbar left to right, sidebar rows top to bottom, the page, the transcript.
⌘R Record · ⌘. Stop · ⌘⇧P Pause · ⌘⇧S Sidebar · ⌘F Search · ⌘, Settings ·
↑↓ selection · ⏎ open · ⌫ Move to Trash · ⌘⌥T full transcript · ← → in the
transport move the playhead by one note · Esc closes a sheet or a fold.
Every shortcut is also a menu item.

## Accessibility

Large text: the Mac sizes follow the system setting and the note column's
measure is in characters. Increased contrast: separators and Label 3 darken
(`prefers-contrast: more`, token override), no new colors. Reduced motion: as
above. Reduce transparency: the sheet dim is an opaque panel. VoiceOver: a
sidebar row is a button named "title, status, date"; a rail mark is a button
named "note at 11:30, play from here"; the live Record control announces
elapsed time on demand; a fold is a disclosure named "7 more turns". On the
iPad, Dynamic Type governs the chrome and the transcript; ink is the hand's.
Captures of large text and increased contrast on both apps belong in the cold
review set of the implemented build.

## Icons

SF Symbols only, via the system: sidebar (`sidebar.left`), trash (`trash`),
record (`record.circle`), pause (`pause.fill`), stop (`stop.fill`), play
(`play.fill`), search (`magnifyingglass`), export (`square.and.arrow.up`),
more (`ellipsis`), transcript (`text.alignleft`), pen (`pencil.tip`), eraser
(`eraser`), undo (`arrow.uturn.backward`), back (`chevron.left`). No custom
glyphs, no emoji, no illustration. The harness's gray squares stand in for
these in the frames only.

## What the implementation must prove

- The Mac page at 1080 by 900 shows the operator's notes and at least eight
  nearest turns without a click (brief density target).
- The rail's marks come from stored note moments, never from line position.
- A note saved before note moments existed renders as a plain page with no
  marks and no fold rows.
- Both appearances, large text and increased contrast, cold-reviewed from
  device captures of the installed build; on the iPad, from the device.

## Family invariants (shared with Minder)

Yawn and Minder share no palette, type, radii, or components. They share the
rules below, stated once here and once in Minder's DIRECTION.md and
DESIGN.md.

| Invariant | Minder | Yawn |
|---|---|---|
| The five job questions in under five seconds are the thesis and the review protocol | Today screen thesis | Every surface; the blind review's protocol |
| State is carried in words and structure, never color alone | Accessibility contract | Status words plus the dot or the mark; no color-only meaning |
| No explanatory chrome in steady state | Copy about the app appears only when something is broken and actionable | No pitch line; the storage promise said once, in Settings and the consent step |
| Attention is not completion; a fact is not a problem | Attention, completion, freshness are distinct | A fact is a caption; a problem is a needs-attention surface with an action |
| Semantic tones with the same names, different values | violet, rose, gold, neutral | record, attention, moment, neutral |
| Source authority is shown, never restated | Source facts panel vs local details | A note and its nearest turn; the transcript is the record |
| Customer terminology, no internal jargon | Minder, activity, handoff, source calendar | meeting, note, transcript, recording, moment; never queue, claim gate, router |
| Tokens are a schema, not a stylesheet | packages/design-tokens/tokens.json | The same schema, Yawn's values, for both apps |
