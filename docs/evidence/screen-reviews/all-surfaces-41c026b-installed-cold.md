---
surface: all (41c026b-installed set 01-08)
build: 41c026b
commit: 41c026b (R8-R13 refit, R14 document-pane port 1d19b27 + 55c3343, interrupted-state presentation 70a068e, reader message 4180458, D-OPEN 41c026b)
device: this Mac, packaged Yawn Preview.app, runtime build-alpha-external, real storage with four real meetings; dark and light appearance
reviewer: blind reviewer agent (fresh session; eight frames and section 3a of the judged-screen pattern only)
implementer: refit-ui-r8-r13, port-document-pane-r14, reader agents; pane finish, D-OPEN, captures by the rethink session
kind: cold
cold: true
release_marker: rethink-phase3-port-2026-09-02
states: launch (transcript-only meeting selected), interrupted meeting with released audio, back to the first meeting, settings; the same in light; first run
verdict: 01 revise, 02 revise, 03 revise, 04 revise, 05 revise, 06 revise, 07 revise, 08 revise
category_read: main window reads as an installed native Mac app (traffic lights, grouped sidebar rows with selection block, toolbar search plus one action, bezel buttons); Settings still wobbles (bordered cards on a backdrop read as a web settings page)
not_reviewed: during, paused, meeting with generated note and inspector (no real note on this Mac), withheld turn, large text, increased contrast, first run in light
protocol: judged-screen pattern section 3a, five job questions, plus the category read
---

# Cold review — 41c026b installed captures

Reviewer had no access to product brief, design rationale, code, or MANIFEST.md.
Judged from the eight PNG frames only, against the five job questions (what is
happening now, what is next, who, when and where, what can I do).

## 1. Category read

Main-window frames (01, 02, 03, 05, 06) read as an installed native Mac app, not
a web page in a window. The cues that decide it: real macOS traffic-light window
controls; a sidebar with grouped section headers ("YESTERDAY", "PREVIOUS 30
DAYS") and two-line list rows with a solid blue selection block, matching the
Mail/Notes sidebar convention; a toolbar built from a search field plus a single
right-aligned text-only action ("Record"), not a web nav bar; and button shapes
(Rename/Manage) that read as system bezel buttons rather than CSS-styled
`<button>` elements. There is no browser chrome, no URL bar, no drop-shadowed
web card pattern in the main window.

The Settings window (04, 07) wobbles on this read. Its outer chrome is native
(small traffic lights, "Yawn Settings" title), but the content is built from
big-radius bordered cards floating on a gray/dark backdrop ("What happens with
your meeting", "Speech model") — a layout metaphor closer to a modern web
settings/SaaS dashboard page than to System Settings' plain table rows. It is
the one place in the set where the interior surface reads more "web" than
"native."

## 2. Per-frame job-question answers

**01 (dark, launch — meeting detail)**
- What is happening now: "Sep 1, 2026 · Transcript Only"; "Recording was paused
  1 time, for 0:03 in total."; "The audio was already deleted."; "No meeting
  note yet." — answered.
- What is next: "Generate note" button + "Runs the downloaded note model on
  this Mac. It usually takes several minutes..." — answered.
- Who: not answered anywhere on the frame.
- When and where: "Sep 1, 2026" answers when; where is not answered.
- What can I do: Rename, Manage, Generate note, type into "Your notes", expand
  "Full transcript" — answered.

**02 (dark, interrupted)**
- What is happening now: "This recording did not finish." + "This recording
  was interrupted before it could be transcribed. Its audio has since been
  deleted." — answered.
- What is next: only "Move to Trash..." is offered — answered, but narrowly;
  no retry or re-record path is shown.
- Who: not answered.
- When and where: the pane itself carries no date/time; "8:48 AM" only exists
  in the sidebar row to the left. Partially answered, and only by looking away
  from the pane.
- What can I do: "Move to Trash..." only.

**03 (dark, back to first)** — pixel-identical to 01 (same content, same
answers). Returning to the first meeting after visiting the interrupted one
reproduces the exact same frame, with nothing visibly lost or reset.

**04 (dark, settings)**
- What is happening now: "Smaller download" carries an "In use" badge —
  answered for the speech model; the "Note model" section is cut off at the
  bottom of the frame, so its current state is unknown from this capture.
- What is next: "Download and use" tells you what switching models does —
  answered.
- Who: not answered.
- When and where: not answered — no timestamp, no version string.
- What can I do: switch to the full model; presumably scroll for note-model
  choices, but that isn't visible in-frame.

**05 (light, launch)** — same content and same answers as 01.

**06 (light, interrupted)** — same content and same answers as 02.

**07 (light, settings)** — same content and same answers as 04, including the
same bottom cutoff at "Note model."

**08 (dark, first-run)**
- What is happening now: two different "now" states are stacked and both
  visible — the sheet's "Before, during, after." onboarding explainer, and,
  dimmed behind it, "Microphone access is ready. Let Yawn verify its capture
  helper before recording." They compete rather than sequence.
- What is next: the sheet says "Got it" dismisses it; separately, "Allow
  system audio" is the actual next required step, but the sheet's own copy
  never mentions it.
- Who: not answered.
- When and where: not answered.
- What can I do: dismiss the sheet ("Got it"); once dismissed, presumably
  "Allow system audio" — but that action is not surfaced by the thing asking
  for the user's attention.

## 3. Type

Frame 01's document pane uses roughly four distinct visual sizes: a large bold
title ("Meeting · Sep 1, 2026"), body-regular text (paragraphs, placeholder,
"Full transcript" label), a bold section label ("Your notes", "No meeting note
yet."), and small gray labels ("Stored on this Mac", "Sep 1, 2026 · Transcript
Only", the Rename/Manage button labels). Most of the pane's text clusters at
about the same 14–16px size, differentiated mainly by weight and color rather
than size.

Settings (04) carries more levels: an H1 ("Settings"), bold section headers
("Speech model"), bold card sub-labels ("What leaves this Mac", "Smaller
download"), regular body/description text, and two smaller labels — the "In
use" pill and the "1.61 GB" gray tag beside "Full model." That's five to six
distinct sizes, a deeper type hierarchy than the main document pane uses for
comparable information.

Hardest to read at this scale: the "1.61 GB" gray-on-dark label next to "Full
model" and the "In use" pill text in frame 04 — both small and low-contrast.
Also hard to read: "Try it: record a 30-second note to yourself." in frame 08's
dimmed sidebar — very low-contrast dark gray on near-black, and it is
competing for attention with an active modal at the same time.

## 4. Controls

- Filled blue rounded-rect, white label: "Generate note", "Move to Trash...",
  "Allow system audio", "Download and use" — one consistent family.
- Gray/outline bezel buttons: "Rename", "Manage" — a second, secondary family.
- Bare red text, no fill or border: "Record" — a third, minimal style.
- Near-white filled pill: "Got it" (frame 08) — the only light-colored filled
  button in the whole set, sitting inside an otherwise all-dark sheet next to
  blue-family buttons everywhere else in the app.
- Small blue-tinted pill badge: "In use" — same hue as the primary-button
  family but a different shape (badge, not action).
- Search field: gray rounded-rect input, placeholder "Search meetings."
- Disclosure row: "Full transcript ⌄" — text plus chevron.
- List-row selection: solid blue block with rounded corners at the row edges.

Four distinct button shapes appear across the set (blue-filled, gray-outline,
bare-text, and the one-off near-white "Got it"), not one family — "Got it" in
particular breaks the pattern the rest of the app establishes.

## 5. Hierarchy (frame 01)

Loudest element: the large bold title "Meeting · Sep 1, 2026" — biggest,
boldest text on the frame, and a reasonable choice to lead a detail view.

Second: not clearly the "No meeting note yet." line that visually follows it —
the saturated blue "Generate note" button competes for that spot, because it's
the only strongly colored element on an otherwise grayscale frame. Color
contrast pulls the eye about as hard as the title's size contrast does, so the
button and the title effectively tie for attention rather than the title
clearly leading into a calmer second tier.

## 6. Copy

No sentence directly restates the one immediately above it. Two related but
independent claims about local-only processing exist in different places —
"Nothing leaves your computer" under Generate note (01) and "What leaves this
Mac / Nothing. Recording, transcription, and notes run and stay here." in
Settings (04) — consistent messaging, not an on-screen restatement, but worth
knowing the claim is made twice in different words.

"Before, during, after." (08) is a rule-of-three tagline, mildly marketing in
register, sitting inside an otherwise plainly functional consent/workflow
explainer — it stands out against the rest of the app's plain declarative
copy.

"Saved separately from the transcript. These are your notes, not generated
claims." (01/05) reads slightly like promotional differentiation copy ("not
generated claims") rather than a plain instruction, and it sits far below the
empty field it describes, separated by a large gap.

State words map cleanly where checked: "interrupted" (sidebar) matches "This
recording did not finish." (detail pane). "Transcript Only" (01) is presented
as prose metadata with no visible badge or defined vocabulary elsewhere in the
set to confirm it's a real state category versus one meeting's incidental
description.

## 7. Layout

Frame 01: text blocks share one left margin and one right-aligned column for
Rename/Manage and "Stored on this Mac" — internally consistent. But the content
column stops well short of the pane's actual width, leaving a large unused
right-hand gutter with nothing to justify it (no secondary content, no rail).

Vertical rhythm in the same frame is uneven: tight spacing between "No meeting
note yet." and the button beneath it, then a large fixed empty region — roughly
200px — between the notes placeholder text at the top of its box and the
caption ("Saved separately...") near the bottom of the same box, with nothing
between them. That empty region reads as a layout gap, not an intentional
empty state.

Frame 02/06: the "This recording did not finish." message sits left-of-center
and roughly 48% down the pane, in a field of otherwise-empty space — it does
not read as a deliberately, symmetrically centered empty state.

Hairline use is inconsistent between windows: Settings (04/07) divides
liberally, separating every fact inside its cards; the main document pane (01)
uses exactly one hairline, above "Full transcript," nowhere else.

Settings (04/07) is cut off at the bottom in both appearances — "Note model" is
visible only as a header with its body text truncated by the frame edge, with
no visible scrollbar or "more below" cue to say whether the window scrolls or
is simply clipped.

## 8. Dark and light parity

The notes-field placeholder ("Write down the detail you will want to verify
later.") is close in contrast to real body text in dark mode (01) but clearly
lighter/washed in light mode (05) — the light version signals "this is an
empty placeholder" much more clearly than the dark version does, for the
identical state.

The blue accent shifts saturation between appearances: dark mode's blue
(Generate note, Move to Trash, Allow system audio, In use) reads as a bright,
punchy blue; the light-mode equivalents (05/06/07) read more muted, closer to
periwinkle, especially the "In use" pill and "Download and use" button in 07.

Settings' card-on-background contrast is stronger in dark (04, distinctly
lighter charcoal card against near-black) than in light (07, white card
against light gray — present, but a visibly smaller delta).

Frame 08 (first-run) has no light-mode counterpart in this set, so its parity
can't be checked either way.

## 9. First run (08)

The sheet itself reads as native — attached to the top of the window, dark
rounded card, dimmed backdrop, single action button — standard macOS sheet
presentation.

The content behind it competes rather than helps. Two things are visible
simultaneously through the dim: sidebar copy that already covers similar
ground ("Press Record to start a private meeting...", "Try it: record a
30-second note to yourself."), and a live, actionable, unrelated permission
request ("Allow system audio" with its own button) that the sheet's own text
never references. A first-run person reads "Before, during, after," clicks
"Got it," and only then discovers a second, unexplained gate was sitting the
whole time.

## 10. Verdict per frame

| Frame | Verdict | Reason |
|---|---|---|
| 01 | revise | large unexplained empty gap in the notes box; unused right-hand gutter |
| 02 | revise | empty-state message reads off-center in a lot of dead space; title bar drops the meeting's identity |
| 03 | revise | identical to 01 — same issue |
| 04 | revise | content cut off at bottom with no scroll affordance; card layout reads web, not System Settings |
| 05 | revise | same as 01, plus placeholder contrast is weaker signal than in 05's own light-mode counterpart reads it (i.e. dark mode is the weaker one) |
| 06 | revise | same as 02 |
| 07 | revise | same as 04, plus washed-out blue accent relative to dark |
| 08 | revise | onboarding sheet doesn't disclose the permission gate sitting behind it |

## 11. Top three findings, ranked

1. **Frame 08 — two unrelated asks stacked on one screen, one of them
   unexplained.** The "Before, during, after." sheet offers only "Got it," while
   directly behind it (dimmed but legible) sits "Allow system audio" /
   "Microphone access is ready. Let Yawn verify its capture helper before
   recording." with its own blue button — a real, required action the sheet's
   copy never mentions. Smallest fix: fold the permission ask into the sheet,
   or sequence the two (onboarding, then permission) instead of showing both
   at once.

2. **Frame 02/06 — the interrupted-recording state loses its own bearings.**
   The window title collapses to the bare app name "Yawn" instead of retaining
   the meeting's date/time (compare frame 01's "Meeting · Sep 1, 2026" in the
   same title-bar position), and "This recording did not finish." sits
   left-of-center around 48% down the pane rather than reading as a
   deliberately centered empty state. Smallest fix: keep the meeting's
   date/time in the title bar even on failure, and center the message on both
   axes.

3. **Frame 01/03/05 — the notes field carries a large fixed dead zone.** Roughly
   200px of empty space separates the placeholder text ("Write down the detail
   you will want to verify later.") from its own caption ("Saved separately
   from the transcript...") at the bottom of the same box, and the surrounding
   content column ignores the pane's actual width, leaving an unused gutter on
   the right. Smallest fix: size the notes box to its content, or move the
   caption directly under the placeholder instead of anchoring it near the
   bottom of a fixed-height box.

A stronger version of this screen would commit to one content-column width and
hold every element to it — title, body text, buttons, the notes box — instead
of letting text hug a narrow measure while a wide gutter sits unused beside it.
It would carry identifying context (the meeting's date/time) into every state
of the title bar, including failure, instead of dropping to the bare app name.
It would sequence the two first-run asks instead of showing an onboarding sheet
that talks about the workflow while a real, unexplained permission gate waits
behind it. And it would settle on one button family — one shape, one color, one
weight for a primary action — rather than mixing a blue-filled family with a
lone near-white "Got it" and a bare red-text "Record" link.
